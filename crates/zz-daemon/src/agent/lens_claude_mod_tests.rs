use std::io::Write as _;
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
    AgentPrompt, AgentStreamPayload, Claude, ClaudeFiles, Lens, LensPane, LensSpec, Link,
    RuntimeCommand, RuntimeControl, Shadow, Source, shadow_lens,
};
use crate::agent::lens_channel::request;

const SESSION: &str = "4f1d8a52-2b7e-4c1a-9d3e-6a0b5c7e9f21";
const MOD: &str = "mod-1";
const TURN: &str = "a141d466-429e-4f3a-8241-e860401a43a0";

#[derive(Default)]
struct Pane {
    sent: Mutex<Vec<String>>,
    keys: Mutex<Vec<String>>,
    channel: Option<PathBuf>,
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

    fn channel(&self) -> Option<PathBuf> {
        self.channel.clone()
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
            channel: channel.then(|| root.path().join("lens.sock")),
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
            .and_then(|path| Link::open(path, Arc::clone(&ids), io));
        let spec = LensSpec {
            pane: PaneId(7),
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
            "/events",
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
            request(&socket, "GET", &format!("/commands?instance={MOD}"), "").1
        })
    }

    fn ask(&self, body: Value) -> JoinHandle<Value> {
        let socket = self.socket();
        let mut body = body;
        body["i"] = Value::from(MOD);
        std::thread::spawn(move || request(&socket, "POST", "/ask", &body.to_string()).1)
    }
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
            "/ask",
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
