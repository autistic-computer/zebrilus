use std::io::Write as _;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value, json};
use zz_protocol::{
    AgentProvider, AgentQuestionAnswer, PaneId,
    agent_stream::{AgentImage, AgentQuestion},
};

use zz_protocol::chat::ChatEvent;

use super::{
    AgentPrompt, AgentPromptOutcome, AgentStreamPayload, Claude, ClaudeFiles, Lens, LensPane,
    LensSpec, Link, RuntimeCommand, RuntimeControl, Shadow, Source, shadow_lens,
};
use crate::agent::lens_channel::{Channel, Hub, request};

const SESSION: &str = "4f1d8a52-2b7e-4c1a-9d3e-6a0b5c7e9f21";
const MOD: &str = "mod-1";
const TURN: &str = "a141d466-429e-4f3a-8241-e860401a43a0";
const PANE: PaneId = PaneId(7);

#[derive(Default)]
struct Pane {
    sent: Mutex<Vec<String>>,
    keys: Mutex<Vec<String>>,
    hub: Option<Hub>,
}

impl LensPane for Pane {
    fn pid(&self) -> Option<u32> {
        Some(std::process::id())
    }

    fn submit(&self, text: &str) -> Result<(), String> {
        self.sent.lock().push(text.to_owned());
        Ok(())
    }

    fn keys(&self, keys: &[&str]) -> Result<(), String> {
        self.keys
            .lock()
            .extend(keys.iter().map(|key| (*key).to_owned()));
        Ok(())
    }

    fn attach(&self, images: &[AgentImage]) -> Result<(), String> {
        self.sent
            .lock()
            .extend(images.iter().map(|image| format!("image:{}", image.format)));
        Ok(())
    }

    fn screen(&self) -> Option<String> {
        None
    }

    fn channel(&self) -> Option<Channel> {
        self.hub.as_ref()?.attach(PANE)
    }
}

struct Rig {
    root: tempfile::TempDir,
    transcript: PathBuf,
    pane: Arc<Pane>,
}

impl Rig {
    fn new(channel: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("work");
        std::fs::create_dir_all(&cwd).unwrap();
        let cwd = std::fs::canonicalize(cwd).unwrap();
        let project = root
            .path()
            .join("config/projects")
            .join(cwd.to_string_lossy().replace(['/', '.'], "-"));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(root.path().join("registry")).unwrap();
        let transcript = project.join(format!("{SESSION}.jsonl"));
        std::fs::write(&transcript, "").unwrap();
        let pane = Arc::new(Pane {
            hub: channel.then(|| Hub::bind(root.path().join("lens.sock")).unwrap()),
            ..Pane::default()
        });
        let rig = Self {
            root,
            transcript,
            pane,
        };
        rig.record("idle");
        rig
    }

    fn socket(&self) -> PathBuf {
        self.root.path().join("lens.sock")
    }

    fn hub(&self) -> &Hub {
        self.pane.hub.as_ref().unwrap()
    }

    fn open_poll(&self) -> UnixStream {
        let mut stream = UnixStream::connect(self.socket()).unwrap();
        write!(
            stream,
            "GET /commands?pane=7&instance={MOD} HTTP/1.1\r\nHost: zz\r\nContent-Length: 0\r\n\r\n"
        )
        .unwrap();
        stream
    }

    fn record(&self, status: &str) {
        let pid = std::process::id();
        let cwd = std::fs::canonicalize(self.root.path().join("work")).unwrap();
        std::fs::write(
            self.root.path().join(format!("registry/{pid}.json")),
            json!({
                "pid": pid,
                "sessionId": SESSION,
                "cwd": cwd,
                "status": status,
                "updatedAt": 1,
                "waitingFor": if status == "waiting" { "permission prompt" } else { "" },
            })
            .to_string(),
        )
        .unwrap();
    }

    fn line(&self, entry: &Value) {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.transcript)
            .unwrap();
        writeln!(file, "{entry}").unwrap();
    }

    fn lens(&self) -> Lens {
        let ids = Arc::new(AtomicU64::new(1));
        let mut claude = Claude::new(Some(ClaudeFiles {
            registry: self.root.path().join("registry"),
            config: self.root.path().join("config"),
        }));
        let io: Arc<dyn LensPane> = self.pane.clone();
        claude.link = self
            .pane
            .channel()
            .map(|channel| Link::open(channel, Arc::clone(&ids), io));
        let spec = LensSpec {
            pane: PANE,
            provider: AgentProvider::ClaudeCode,
            io: self.pane.clone(),
        };
        Lens::with_source(spec, Source::Claude(Box::new(claude)), ids)
    }

    fn post(&self, events: &[Value]) {
        self.post_as(MOD, events);
    }

    fn post_as(&self, instance: &str, events: &[Value]) {
        let events = events
            .iter()
            .map(|event| {
                let mut event = event.clone();
                event["i"] = Value::from(instance);
                event
            })
            .collect::<Vec<_>>();
        let (status, _) = request(
            &self.socket(),
            "POST",
            "/events?pane=7",
            &Value::Array(events).to_string(),
        );
        assert_eq!(status, 200);
    }

    fn hello(&self, turn: Option<&str>) {
        self.post(&[json!({
            "t": "hello", "session": SESSION, "model": "claude-haiku-5-5", "turn": turn,
        })]);
    }

    fn poll(&self) -> JoinHandle<Value> {
        let socket = self.socket();
        std::thread::spawn(move || {
            request(
                &socket,
                "GET",
                &format!("/commands?pane=7&instance={MOD}"),
                "",
            )
            .1["commands"]
                .clone()
        })
    }

    fn ask(&self, body: Value) -> JoinHandle<Value> {
        let socket = self.socket();
        let mut body = body;
        body["i"] = Value::from(MOD);
        std::thread::spawn(move || request(&socket, "POST", "/ask?pane=7", &body.to_string()).1)
    }
}

fn until(mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn exited_pid() -> u32 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

fn ask_prompt(lens: &mut Lens, turn_id: u64, text: &str) -> Vec<AgentStreamPayload> {
    lens.command(RuntimeCommand::Prompt {
        turn_id,
        prompt: AgentPrompt {
            text: text.to_owned(),
            ..AgentPrompt::default()
        },
    })
}

fn failure(payloads: &[AgentStreamPayload], turn: u64) -> Option<String> {
    payloads.iter().find_map(|payload| match payload {
        AgentStreamPayload::PromptFinished {
            turn_id,
            outcome: AgentPromptOutcome::Failed { message },
        } if *turn_id == turn => Some(message.clone()),
        _ => None,
    })
}

fn live(lens: &mut Lens) {
    let started = Instant::now();
    while lens.source.live_link().is_none() {
        assert!(started.elapsed() < Duration::from_secs(5));
        lens.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn text(turn: &str, step: u64, index: u64, off: usize, text: &str) -> Value {
    json!({ "t": "text", "turn": turn, "step": step, "index": index, "off": off, "text": text, "at": 1 })
}

fn texts(payloads: &[AgentStreamPayload], kind: &str) -> Vec<String> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::Update { update } if update["sessionUpdate"] == kind => {
                update["content"]["text"].as_str().map(str::to_owned)
            }
            _ => None,
        })
        .collect()
}

fn prompt(uuid: &str, text: &str) -> Value {
    json!({
        "type": "user", "uuid": uuid, "parentUuid": null, "isSidechain": false,
        "message": { "role": "user", "content": text },
    })
}

fn said(uuid: &str, parent: &str, index: u64, kind: &str, text: &str) -> Value {
    let block = if kind == "thinking" {
        json!({ "type": "thinking", "thinking": text, "signature": "sig" })
    } else {
        json!({ "type": "text", "text": text })
    };
    json!({
        "type": "assistant", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
        "apiBlockIndex": index,
        "message": { "id": "msg_011CgbAhE9ejr4xi5zFySBx8", "role": "assistant", "content": [block] },
    })
}

fn busy(payloads: &[AgentStreamPayload]) -> Vec<bool> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::Activity { busy } => Some(*busy),
            _ => None,
        })
        .collect()
}

fn requested(payloads: &[AgentStreamPayload]) -> Option<(u64, Value, Value, Vec<AgentQuestion>)> {
    payloads.iter().find_map(|payload| match payload {
        AgentStreamPayload::PermissionRequested {
            request_id,
            tool_call,
            options,
            questions,
        } => Some((
            *request_id,
            tool_call.clone(),
            options.clone(),
            questions.clone(),
        )),
        _ => None,
    })
}

fn resolved(payloads: &[AgentStreamPayload]) -> Vec<u64> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::PermissionResolved { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .collect()
}

#[test]
fn live_text_reaches_the_lens_before_the_transcript_and_never_twice() {
    let rig = Rig::new(true);
    rig.line(&prompt("u1", "count to two"));
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(None);
    live(&mut lens);

    rig.record("busy");
    rig.post(&[
        json!({ "t": "turn", "turn": TURN }),
        json!({ "t": "step", "turn": TURN, "step": 0 }),
        json!({ "t": "thinking", "turn": TURN, "step": 0, "index": 0, "off": 0, "text": "Counting.", "at": 1 }),
        text(TURN, 0, 1, 0, "one\n"),
        text(TURN, 0, 1, 4, "tw"),
    ]);
    let streamed = lens.tick();
    assert_eq!(busy(&streamed), [true]);
    assert_eq!(texts(&streamed, "agent_thought_chunk"), ["Counting."]);
    assert_eq!(texts(&streamed, "agent_message_chunk"), ["one\n", "tw"]);

    rig.line(&said("a1", "u1", 0, "thinking", ""));
    rig.line(&said("a2", "a1", 1, "text", "one\ntwo"));
    let recorded = lens.tick();
    assert!(texts(&recorded, "agent_thought_chunk").is_empty());
    assert_eq!(texts(&recorded, "agent_message_chunk"), ["o"]);
    let rest = recorded
        .iter()
        .find_map(|payload| match payload {
            AgentStreamPayload::Update { update }
                if update["sessionUpdate"] == "agent_message_chunk" =>
            {
                update["messageId"].as_str().map(str::to_owned)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(rest, format!("live:{TURN}:0:1"));

    rig.post(&[json!({ "t": "done", "turn": TURN, "reason": "answer" })]);
    let done = lens.tick();
    assert_eq!(busy(&done), [false]);
    assert!(texts(&done, "agent_message_chunk").is_empty());
}

#[test]
fn a_turn_heard_only_through_its_steps_still_shows_busy() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(None);
    live(&mut lens);
    rig.post(&[
        json!({ "t": "step", "turn": "agent-turn", "step": 0, "agent": "a1" }),
        json!({ "t": "step", "turn": TURN, "step": 0 }),
        text(TURN, 0, 0, 0, "The subagent replied pong."),
    ]);
    let started = lens.tick();
    assert_eq!(busy(&started), [true]);
    assert_eq!(
        texts(&started, "agent_message_chunk"),
        ["The subagent replied pong."]
    );
    rig.post(&[json!({ "t": "done", "turn": TURN, "reason": "answer" })]);
    assert_eq!(busy(&lens.tick()), [false]);

    rig.post(&[
        json!({ "t": "step", "turn": "short", "step": 0 }),
        text("short", 0, 0, 0, "Done."),
        json!({ "t": "done", "turn": "short", "reason": "answer" }),
    ]);
    let short = lens.tick();
    assert_eq!(busy(&short), [true]);
    assert_eq!(texts(&short, "agent_message_chunk"), ["Done."]);
    assert_eq!(busy(&lens.tick()), [false]);
}

#[test]
fn a_turn_that_follows_a_lens_turn_at_once_keeps_its_text_in_one_message() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    let mut shadow = Shadow::default().attached();
    let mut events = shadow_lens(&mut shadow, &lens.open());
    let poll = rig.poll();
    rig.hello(None);
    live(&mut lens);
    lens.command(RuntimeCommand::Prompt {
        turn_id: 3,
        prompt: AgentPrompt {
            text: "start a background agent".to_owned(),
            ..AgentPrompt::default()
        },
    });
    poll.join().unwrap();
    let _poll = rig.poll();
    rig.post(&[json!({ "t": "turn", "turn": TURN })]);
    events.extend(shadow_lens(&mut shadow, &lens.tick()));
    rig.post(&[
        json!({ "t": "done", "turn": TURN, "reason": "answer" }),
        json!({ "t": "step", "turn": "notified", "step": 0 }),
    ]);
    let settled = lens.tick();
    assert!(settled.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PromptFinished { turn_id: 3, .. }
    )));
    assert_eq!(busy(&settled).last(), Some(&true));
    events.extend(shadow_lens(&mut shadow, &settled));
    rig.post(&[text("notified", 0, 0, 0, "The subagent ")]);
    events.extend(shadow_lens(&mut shadow, &lens.tick()));
    rig.post(&[text("notified", 0, 0, 13, "replied ping.")]);
    events.extend(shadow_lens(&mut shadow, &lens.tick()));
    let started = events
        .iter()
        .filter(|event| {
            matches!(event, ChatEvent::ItemStarted { item, .. } if item.0.contains("notified"))
        })
        .count();
    assert_eq!(started, 1);
}

#[test]
fn a_block_joined_partway_or_with_a_gap_is_finished_by_the_transcript() {
    let rig = Rig::new(true);
    rig.line(&prompt("u1", "go"));
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    rig.record("busy");

    rig.post(&[
        text(TURN, 0, 0, 5, "late"),
        text(TURN, 1, 0, 0, "abc"),
        text(TURN, 1, 0, 9, "lost"),
        text(TURN, 1, 0, 3, "def"),
    ]);
    assert_eq!(texts(&lens.tick(), "agent_message_chunk"), ["abc"]);

    rig.line(&said("a1", "u1", 0, "text", "early late"));
    rig.line(&said("a2", "a1", 0, "text", "abcdefghi lost"));
    assert_eq!(
        texts(&lens.tick(), "agent_message_chunk"),
        ["early late", "defghi lost"]
    );
}

#[test]
fn after_a_reload_a_block_shown_live_comes_back_whole_from_the_transcript() {
    let rig = Rig::new(true);
    rig.line(&prompt("u1", "go"));
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    rig.record("busy");
    rig.post(&[text(TURN, 0, 0, 0, "hello")]);
    assert_eq!(texts(&lens.tick(), "agent_message_chunk"), ["hello"]);

    rig.line(&prompt("u2", "rewound"));
    let reloaded = lens.tick();
    assert!(
        reloaded
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::SessionReset { .. }))
    );
    rig.post(&[text(TURN, 0, 0, 5, " there")]);
    rig.line(&said("a1", "u2", 0, "text", "hello there"));
    assert_eq!(texts(&lens.tick(), "agent_message_chunk"), ["hello there"]);
}

#[test]
fn escape_in_the_terminal_ends_the_lens_turn_at_once() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let poll = rig.poll();
    rig.hello(None);
    live(&mut lens);

    let finished = lens.command(RuntimeCommand::Prompt {
        turn_id: 9,
        prompt: AgentPrompt {
            text: "write sixty lines".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(finished.is_empty());
    assert_eq!(
        poll.join().unwrap(),
        json!([{ "id": 1, "op": "prompt", "text": "write sixty lines" }])
    );
    assert!(rig.pane.sent.lock().is_empty());

    let _poll = rig.poll();
    rig.record("busy");
    rig.post(&[
        json!({ "t": "ack", "id": 1, "ok": true }),
        json!({ "t": "turn", "turn": TURN }),
    ]);
    assert_eq!(busy(&lens.tick()), [true]);

    rig.post(&[json!({ "t": "done", "turn": TURN, "reason": "aborted" })]);
    let ended = lens.tick();
    assert!(ended.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PromptFinished { turn_id: 9, .. }
    )));
    assert!(busy(&ended).contains(&false));
    assert!(lens.turn.is_none());
}

#[test]
fn a_prompt_the_cli_refuses_is_typed_into_the_terminal_instead() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let poll = rig.poll();
    rig.hello(None);
    live(&mut lens);
    lens.command(RuntimeCommand::Prompt {
        turn_id: 4,
        prompt: AgentPrompt {
            text: "hi".to_owned(),
            ..AgentPrompt::default()
        },
    });
    poll.join().unwrap();
    assert!(rig.pane.sent.lock().is_empty());
    let _poll = rig.poll();
    rig.post(&[json!({
        "t": "ack", "id": 1, "ok": false,
        "error": "the session was stopped and nobody has sent input since",
    })]);
    let ticked = lens.tick();
    assert_eq!(*rig.pane.sent.lock(), ["hi"]);
    assert!(
        !ticked
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. }))
    );
    assert!(lens.turn.is_some());
}

#[test]
fn slash_commands_and_images_are_still_typed_while_the_mod_is_live() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(None);
    live(&mut lens);
    lens.command(RuntimeCommand::Prompt {
        turn_id: 1,
        prompt: AgentPrompt {
            text: "/compact".to_owned(),
            ..AgentPrompt::default()
        },
    });
    lens.turn = None;
    lens.command(RuntimeCommand::Prompt {
        turn_id: 2,
        prompt: AgentPrompt {
            text: "what is this".to_owned(),
            images: vec![AgentImage {
                format: "image/png".to_owned(),
                data: b"hi".to_vec(),
            }],
            ..AgentPrompt::default()
        },
    });
    assert_eq!(
        *rig.pane.sent.lock(),
        ["/compact", "image:image/png", "what is this"]
    );
}

#[test]
fn without_the_mod_the_lens_types_and_answers_with_keys_as_before() {
    for channel in [false, true] {
        let rig = Rig::new(channel);
        let mut lens = rig.lens();
        lens.open();
        assert!(lens.source.live_link().is_none());
        lens.command(RuntimeCommand::Prompt {
            turn_id: 1,
            prompt: AgentPrompt {
                text: "hello".to_owned(),
                ..AgentPrompt::default()
            },
        });
        assert_eq!(*rig.pane.sent.lock(), ["hello"]);
        lens.control(RuntimeControl::Cancel {
            turn_id: 1,
            session_id: None,
        });
        assert_eq!(*rig.pane.keys.lock(), ["Escape"]);
        lens.command(RuntimeCommand::SetConfigOption {
            option_id: "model".to_owned(),
            value: "sonnet".to_owned(),
        });
        assert_eq!(rig.pane.sent.lock().last().unwrap(), "/model sonnet");
    }
}

#[test]
fn a_bash_approval_from_the_lens_answers_the_waiting_hook() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    rig.record("waiting");

    let input =
        json!({ "command": "touch modperm.txt", "description": "Create empty file modperm.txt" });
    let hook = rig.ask(json!({
        "t": "ask", "key": "mod-1:1", "tool": "Bash", "input": input,
        "id": "toolu_01VeqgawTY9cGMo7xgn67arJ", "suggestions": [{ "type": "addRules" }],
    }));
    let started = Instant::now();
    let asked = loop {
        let payloads = lens.tick();
        if let Some(asked) = requested(&payloads) {
            break asked;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    let (request_id, tool_call, options, _) = asked;
    assert_eq!(tool_call["toolCallId"], "toolu_01VeqgawTY9cGMo7xgn67arJ");
    assert_eq!(tool_call["rawInput"], input);
    let ids = options
        .as_array()
        .unwrap()
        .iter()
        .map(|option| option["optionId"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["allow", "allow_always", "reject"]);
    assert!(rig.pane.keys.lock().is_empty());
    assert!(
        lens.tick()
            .iter()
            .all(|payload| !matches!(payload, AgentStreamPayload::PermissionRequested { .. }))
    );

    let answered = lens.control(RuntimeControl::RespondPermission {
        request_id,
        option_id: Some("allow".to_owned()),
    });
    assert_eq!(resolved(&answered), [request_id]);
    assert_eq!(
        hook.join().unwrap(),
        json!({ "decision": { "behavior": "allow", "updatedInput": input } })
    );
    assert!(rig.pane.keys.lock().is_empty());
}

#[test]
fn answering_in_the_terminal_withdraws_the_card_and_frees_the_hook() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    let hook = rig.ask(json!({
        "t": "ask", "key": "mod-1:1", "tool": "Bash", "input": { "command": "ls" }, "id": "toolu_1",
    }));
    let started = Instant::now();
    let request_id = loop {
        if let Some((request_id, ..)) = requested(&lens.tick()) {
            break request_id;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    rig.post(&[json!({ "t": "used", "id": "toolu_1" })]);
    assert_eq!(resolved(&lens.tick()), [request_id]);
    assert_eq!(hook.join().unwrap(), json!({ "pass": true }));
}

#[test]
fn questions_answered_from_the_lens_reach_the_hook_with_every_answer() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    let input = json!({ "questions": [
        { "question": "Which fruit?", "header": "Fruit", "multiSelect": false,
          "options": [{ "label": "apple", "description": "" }, { "label": "pear", "description": "" }] },
        { "question": "Which colors?", "header": "Colors", "multiSelect": true,
          "options": [{ "label": "red", "description": "" }, { "label": "blue", "description": "" }] },
    ]});
    let hook = rig.ask(json!({
        "t": "ask", "key": "mod-1:2", "tool": "AskUserQuestion", "input": input, "id": "toolu_q",
    }));
    let started = Instant::now();
    let (request_id, tool_call, _, questions) = loop {
        if let Some(asked) = requested(&lens.tick()) {
            break asked;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(tool_call["toolCallId"], "toolu_q");
    assert_eq!(questions.len(), 2);
    assert!(questions[1].multi_select);

    let answered = lens.control(RuntimeControl::AnswerQuestion {
        request_id,
        answers: vec![
            AgentQuestionAnswer {
                id: "q0".to_owned(),
                answers: vec!["pear".to_owned()],
            },
            AgentQuestionAnswer {
                id: "q1".to_owned(),
                answers: vec!["red".to_owned(), "blue".to_owned()],
            },
        ],
    });
    assert_eq!(resolved(&answered), [request_id]);
    let decision = hook.join().unwrap();
    assert_eq!(decision["decision"]["behavior"], "allow");
    assert_eq!(
        decision["decision"]["updatedInput"]["answers"],
        json!({ "Which fruit?": "pear", "Which colors?": "red, blue" })
    );
    assert_eq!(
        decision["decision"]["updatedInput"]["questions"],
        input["questions"]
    );
}

#[test]
fn a_single_question_picked_by_option_answers_with_its_label() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    let input = json!({ "questions": [{ "question": "Pick one?", "header": "Fruit", "multiSelect": false,
        "options": [{ "label": "Apple", "description": "red" }, { "label": "Pear", "description": "green" }] }] });
    let hook =
        rig.ask(json!({ "t": "ask", "key": "mod-1:3", "tool": "AskUserQuestion", "input": input }));
    let started = Instant::now();
    let (request_id, tool_call, options, _) = loop {
        if let Some(asked) = requested(&lens.tick()) {
            break asked;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        tool_call["toolCallId"]
            .as_str()
            .unwrap()
            .starts_with("lens-permission-")
    );
    assert_eq!(options[1]["optionId"], "answer-1");
    lens.control(RuntimeControl::RespondPermission {
        request_id,
        option_id: Some("answer-1".to_owned()),
    });
    assert_eq!(
        hook.join().unwrap()["decision"]["updatedInput"]["answers"],
        json!({ "Pick one?": "Pear" })
    );
}

#[test]
fn a_card_whose_cli_stopped_polling_is_withdrawn() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    let _hook = rig
        .ask(json!({ "t": "ask", "key": "mod-1:9", "tool": "Bash", "input": { "command": "ls" } }));
    let started = Instant::now();
    let request_id = loop {
        if let Some((request_id, ..)) = requested(&lens.tick()) {
            break request_id;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    lens.command(RuntimeCommand::SetConfigOption {
        option_id: "model".to_owned(),
        value: "opus".to_owned(),
    });
    poll.join().unwrap();
    let started = Instant::now();
    let withdrawn = loop {
        let withdrawn = resolved(&lens.tick());
        if !withdrawn.is_empty() {
            break withdrawn;
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(withdrawn, [request_id]);
    assert!(lens.source.live_link().is_none());
}

#[test]
fn a_model_pick_is_set_through_the_mod_and_shows_at_once() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let poll = rig.poll();
    rig.hello(None);
    live(&mut lens);
    let changed = lens.command(RuntimeCommand::SetConfigOption {
        option_id: "model".to_owned(),
        value: "sonnet".to_owned(),
    });
    assert!(changed.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::ConfigOptionsChanged { option_id, value, .. }
            if option_id == "model" && value == "sonnet"
    )));
    assert_eq!(
        poll.join().unwrap(),
        json!([{ "id": 1, "op": "model", "value": "sonnet" }])
    );
    assert!(rig.pane.sent.lock().is_empty());

    lens.command(RuntimeCommand::SetConfigOption {
        option_id: "effort".to_owned(),
        value: "high".to_owned(),
    });
    assert_eq!(*rig.pane.sent.lock(), ["/effort high"]);

    rig.post(&[json!({ "t": "model", "model": "claude-sonnet-5-5" })]);
    lens.tick();
    assert_eq!(
        lens.source.link().and_then(|link| link.model()),
        Some("claude-sonnet-5-5")
    );
}

#[test]
fn stop_aborts_the_turn_and_cancels_what_it_waits_on() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    let hook = rig.ask(
        json!({ "t": "ask", "key": "mod-1:4", "tool": "Bash", "input": { "command": "rm x" } }),
    );
    let started = Instant::now();
    let request_id = loop {
        if let Some((request_id, ..)) = requested(&lens.tick()) {
            break request_id;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    let stopped = lens.control(RuntimeControl::Cancel {
        turn_id: 1,
        session_id: None,
    });
    assert_eq!(resolved(&stopped), [request_id]);
    assert_eq!(poll.join().unwrap(), json!([{ "id": 1, "op": "abort" }]));
    assert_eq!(
        hook.join().unwrap()["decision"],
        json!({ "behavior": "deny", "message": "The user cancelled the request.", "interrupt": true })
    );
    assert!(rig.pane.keys.lock().is_empty());
}

#[test]
fn another_claude_in_the_pane_is_ignored_and_never_blocked() {
    let rig = Rig::new(true);
    rig.line(&prompt("u1", "hi"));
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);

    rig.post_as(
        "nested",
        &[
            json!({ "t": "hello", "session": "another-session", "turn": "t2" }),
            text("t2", 0, 0, 0, "not ours"),
        ],
    );
    let socket = rig.socket();
    let nested = std::thread::spawn(move || {
        request(
            &socket,
            "POST",
            "/ask?pane=7",
            &json!({ "t": "ask", "i": "nested", "key": "nested:1", "tool": "Bash", "input": {} })
                .to_string(),
        )
        .1
    });
    let started = Instant::now();
    while !nested.is_finished() {
        assert!(requested(&lens.tick()).is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(nested.join().unwrap(), json!({ "pass": true }));
    assert!(texts(&lens.tick(), "agent_message_chunk").is_empty());
}

#[test]
fn a_background_agents_card_outlives_the_main_turn_and_closes_with_its_own() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    let hook = rig.ask(json!({
        "t": "ask", "key": "mod-1:5", "tool": "Bash", "input": { "command": "ls" },
        "id": "toolu_bg", "agent": "a1",
    }));
    let started = Instant::now();
    let request_id = loop {
        if let Some((request_id, ..)) = requested(&lens.tick()) {
            break request_id;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    rig.post(&[json!({ "t": "done", "turn": TURN, "reason": "answer" })]);
    assert!(resolved(&lens.tick()).is_empty());
    assert!(!hook.is_finished());
    rig.post(&[json!({ "t": "done", "turn": "agent-turn", "agent": "a1", "reason": "answer" })]);
    assert_eq!(resolved(&lens.tick()), [request_id]);
    assert_eq!(hook.join().unwrap(), json!({ "pass": true }));
}

#[test]
fn a_hello_that_beat_the_record_to_its_session_is_adopted_once_the_record_catches_up() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.post(&[json!({
        "t": "hello", "session": "resumed-session", "model": "claude-haiku-5-5", "turn": null,
    })]);
    for _ in 0..5 {
        lens.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(lens.source.live_link().is_none());

    let pid = std::process::id();
    std::fs::write(
        rig.root.path().join(format!("registry/{pid}.json")),
        json!({
            "pid": pid, "sessionId": "resumed-session", "cwd": rig.root.path().join("work"),
            "status": "idle", "updatedAt": 2,
        })
        .to_string(),
    )
    .unwrap();
    live(&mut lens);
}

#[test]
fn a_dialog_answered_in_the_terminal_withdraws_its_card_before_the_tool_ends() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let _poll = rig.poll();
    rig.hello(Some(TURN));
    live(&mut lens);
    rig.record("busy");
    let hook = rig.ask(json!({
        "t": "ask", "key": "mod-1:6", "tool": "Bash", "input": { "command": "sleep 8" },
        "id": "toolu_slow",
    }));
    let started = Instant::now();
    let request_id = loop {
        if let Some((request_id, ..)) = requested(&lens.tick()) {
            break request_id;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(resolved(&lens.tick()).is_empty());

    rig.record("waiting");
    assert!(resolved(&lens.tick()).is_empty());
    rig.record("busy");
    assert_eq!(resolved(&lens.tick()), [request_id]);
    assert_eq!(hook.join().unwrap(), json!({ "pass": true }));
}

#[test]
fn a_plugin_heard_before_the_lens_opened_takes_the_very_first_prompt() {
    let rig = Rig::new(true);
    let early = rig.poll();
    rig.hello(Some(TURN));
    until(|| rig.hub().polling(super::PaneId(7), MOD));
    rig.post(&[
        text(TURN, 0, 0, 0, "dropped while nobody watched"),
        json!({ "t": "done", "turn": TURN, "reason": "answer" }),
    ]);

    let mut lens = rig.lens();
    assert_eq!(early.join().unwrap(), json!([]));
    let opened = lens.open();
    assert!(lens.source.live_link().is_some());
    assert!(!lens.source.live_link().unwrap().busy());
    assert!(texts(&opened, "agent_message_chunk").is_empty());

    assert!(ask_prompt(&mut lens, 1, "first").is_empty());
    assert!(rig.pane.sent.lock().is_empty());
    assert_eq!(
        rig.poll().join().unwrap(),
        json!([{ "id": 1, "op": "prompt", "text": "first" }])
    );
}

#[test]
fn a_prompt_the_plugin_took_as_the_cli_exited_fails_instead_of_vanishing() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let first = rig.poll();
    rig.hello(None);
    live(&mut lens);
    ask_prompt(&mut lens, 5, "hi");
    assert_eq!(
        first.join().unwrap(),
        json!([{ "id": 1, "op": "prompt", "text": "hi" }])
    );
    let again = rig.open_poll();
    until(|| rig.hub().polling(super::PaneId(7), MOD));

    std::fs::remove_file(
        rig.root
            .path()
            .join(format!("registry/{}.json", std::process::id())),
    )
    .unwrap();
    if let Source::Claude(claude) = &mut lens.source {
        claude.record.as_mut().unwrap().pid = exited_pid();
    }
    drop(again);

    let started = Instant::now();
    let message = loop {
        if let Some(message) = failure(&lens.tick(), 5) {
            break message;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(message, "Claude Code exited before it took the prompt");
    assert!(rig.pane.sent.lock().is_empty());
    assert!(lens.turn.is_none());
}

#[test]
fn a_prompt_left_with_a_plugin_that_hung_up_is_typed_while_the_cli_runs() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let first = rig.poll();
    rig.hello(None);
    live(&mut lens);
    ask_prompt(&mut lens, 6, "still here");
    first.join().unwrap();
    let again = rig.open_poll();
    until(|| rig.hub().polling(super::PaneId(7), MOD));
    drop(again);

    until(|| {
        lens.tick();
        !rig.pane.sent.lock().is_empty()
    });
    assert_eq!(*rig.pane.sent.lock(), ["still here"]);
    assert!(lens.turn.is_some());
    assert!(lens.source.live_link().is_none());
}

#[test]
fn a_prompt_after_the_cli_left_the_pane_fails_instead_of_reaching_the_shell() {
    let rig = Rig::new(true);
    rig.record("idle");
    let mut lens = rig.lens();
    lens.open();
    let polling = rig.open_poll();
    rig.hello(None);
    live(&mut lens);
    drop(polling);
    std::fs::remove_file(
        rig.root
            .path()
            .join(format!("registry/{}.json", std::process::id())),
    )
    .unwrap();
    until(|| {
        lens.tick();
        lens.source.live_link().is_none()
    });

    let payloads = ask_prompt(&mut lens, 8, "rm -rf build");
    assert_eq!(
        failure(&payloads, 8).as_deref(),
        Some("Claude Code is not running in this pane")
    );
    let steered = lens.command(RuntimeCommand::Verb {
        prompt: AgentPrompt {
            text: "/steer git push".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(matches!(
        steered.as_slice(),
        [AgentStreamPayload::PromptsReclaimed { prompts }] if prompts[0].text == "git push"
    ));
    assert!(rig.pane.sent.lock().is_empty());
    assert!(lens.turn.is_none());
}

#[test]
fn a_prompt_sent_as_a_turn_ends_is_not_ended_by_that_turns_transcript_line() {
    let rig = Rig::new(true);
    let mut lens = rig.lens();
    lens.open();
    let poll = rig.poll();
    rig.hello(None);
    live(&mut lens);
    assert!(ask_prompt(&mut lens, 1, "ping nine").is_empty());
    poll.join().unwrap();
    let poll = rig.poll();
    rig.post(&[
        json!({ "t": "turn", "turn": TURN }),
        json!({ "t": "step", "turn": TURN, "step": 0 }),
        text(TURN, 0, 0, 0, "ping nine"),
        json!({ "t": "done", "turn": TURN, "reason": "answer" }),
    ]);
    let first = lens.tick();
    assert!(first.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PromptFinished { turn_id: 1, .. }
    )));

    assert!(ask_prompt(&mut lens, 2, "ping ten").is_empty());
    assert_eq!(
        poll.join().unwrap(),
        json!([{ "id": 2, "op": "prompt", "text": "ping ten" }])
    );
    let _poll = rig.poll();
    rig.line(&prompt("u1", "ping nine"));
    rig.line(&said("a1", "u1", 0, "text", "ping nine"));
    rig.line(&json!({
        "type": "system", "subtype": "turn_duration", "uuid": "d1", "parentUuid": "a1",
        "isSidechain": false, "durationMs": 700,
    }));
    let late = lens.tick();
    assert!(
        !late
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. })),
        "{late:?}"
    );

    let next = "b2c3d4e5-0000-4000-8000-000000000002";
    rig.post(&[
        json!({ "t": "turn", "turn": next }),
        json!({ "t": "step", "turn": next, "step": 0 }),
        text(next, 0, 0, 0, "ping ten"),
        json!({ "t": "done", "turn": next, "reason": "answer" }),
    ]);
    let second = lens.tick();
    let reply = second.iter().position(|payload| {
        matches!(payload, AgentStreamPayload::Update { update }
            if update["content"]["text"] == "ping ten")
    });
    let done = second.iter().position(|payload| {
        matches!(
            payload,
            AgentStreamPayload::PromptFinished { turn_id: 2, .. }
        )
    });
    assert!(reply.is_some() && reply < done, "{second:?}");
}
