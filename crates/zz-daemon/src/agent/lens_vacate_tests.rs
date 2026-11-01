use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use parking_lot::Mutex;
use serde_json::{Value, json};
use zz_protocol::chat::{ChatView, ItemKind};
use zz_protocol::{AgentProvider, PaneId, agent_stream::AgentImage};

use super::{
    AgentPrompt, AgentPromptOutcome, AgentStreamPayload, Claude, ClaudeFiles, Lens, LensPane,
    LensSpec, RuntimeCommand, Shadow, Source, shadow_lens,
};

const OLD: &str = "5b0e6f43-7c1d-4a2e-9f80-1d2c3b4a5e6f";
const NEW: &str = "c2d4e6f8-0a1b-4c3d-8e5f-6a7b8c9d0e1f";

struct Pane {
    pid: u32,
    sent: Mutex<Vec<String>>,
}

impl LensPane for Pane {
    fn pid(&self) -> Option<u32> {
        Some(self.pid)
    }

    fn submit(&self, text: &str) -> Result<(), String> {
        self.sent.lock().push(text.to_owned());
        Ok(())
    }

    fn keys(&self, _keys: &[&str]) -> Result<(), String> {
        Ok(())
    }

    fn attach(&self, _images: &[AgentImage]) -> Result<(), String> {
        Ok(())
    }

    fn screen(&self) -> Option<String> {
        None
    }
}

struct Rig {
    root: tempfile::TempDir,
    project: PathBuf,
    pane: Arc<Pane>,
}

impl Rig {
    fn new() -> Self {
        Self::in_pane(std::process::id())
    }

    fn in_pane(pid: u32) -> Self {
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
        Self {
            root,
            project,
            pane: Arc::new(Pane {
                pid,
                sent: Mutex::default(),
            }),
        }
    }

    fn record(&self, session: &str, status: &str) {
        let pid = self.pane.pid;
        let cwd = std::fs::canonicalize(self.root.path().join("work")).unwrap();
        std::fs::write(
            self.root.path().join(format!("registry/{pid}.json")),
            json!({
                "pid": pid, "sessionId": session, "cwd": cwd, "status": status,
                "updatedAt": 1, "waitingFor": "",
            })
            .to_string(),
        )
        .unwrap();
    }

    fn line(&self, session: &str, entry: &Value) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.project.join(format!("{session}.jsonl")))
            .unwrap();
        writeln!(file, "{entry}").unwrap();
    }

    fn lens(&self) -> Lens {
        Lens::with_source(
            LensSpec {
                pane: PaneId(7),
                provider: AgentProvider::ClaudeCode,
                io: self.pane.clone(),
            },
            Source::Claude(Box::new(Claude::new(Some(ClaudeFiles {
                registry: self.root.path().join("registry"),
                config: self.root.path().join("config"),
            })))),
            Arc::new(AtomicU64::new(1)),
        )
    }
}

fn prompt(uuid: &str, parent: Option<&str>, text: &str) -> Value {
    json!({
        "type": "user", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
        "message": { "role": "user", "content": text },
    })
}

fn answer(uuid: &str, parent: &str, text: &str) -> Value {
    json!({
        "type": "assistant", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
        "message": {
            "id": format!("msg_{uuid}"), "role": "assistant", "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": text }],
        },
    })
}

fn turn_duration(uuid: &str, parent: &str) -> Value {
    json!({
        "type": "system", "subtype": "turn_duration", "uuid": uuid, "parentUuid": parent,
        "isSidechain": false, "durationMs": 900,
    })
}

fn send(lens: &mut Lens, turn_id: u64, text: &str) -> Vec<AgentStreamPayload> {
    lens.command(RuntimeCommand::Prompt {
        turn_id,
        prompt: AgentPrompt {
            text: text.to_owned(),
            ..AgentPrompt::default()
        },
    })
}

fn tick(lens: &mut Lens) -> Vec<AgentStreamPayload> {
    lens.next_locate = Instant::now();
    lens.tick()
}

fn finished(payloads: &[AgentStreamPayload], turn: u64) -> Option<usize> {
    payloads.iter().position(|payload| {
        matches!(
            payload,
            AgentStreamPayload::PromptFinished { turn_id, outcome: AgentPromptOutcome::Finished { .. } }
                if *turn_id == turn
        )
    })
}

fn said(payloads: &[AgentStreamPayload], text: &str) -> Option<usize> {
    payloads.iter().position(|payload| {
        matches!(
            payload,
            AgentStreamPayload::Update { update }
                if update["sessionUpdate"] == "agent_message_chunk" && update["content"]["text"] == text
        )
    })
}

fn view(rounds: &[&[AgentStreamPayload]]) -> ChatView {
    let mut shadow = Shadow::recording().attached();
    for payloads in rounds {
        shadow_lens(&mut shadow, payloads);
    }
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let mut view = ChatView::default();
    for event in &shadow.events {
        view.apply(event)
            .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
    }
    view
}

fn texts(view: &ChatView, kind: ItemKind) -> Vec<String> {
    let mut items = view
        .items
        .values()
        .filter(|item| item.kind == kind)
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.started_at);
    items.iter().map(|item| item.body.text.clone()).collect()
}

#[test]
fn a_prompt_that_starts_the_next_clis_session_keeps_its_turn_until_the_reply() {
    let rig = Rig::new();
    rig.line(OLD, &prompt("o1", None, "earlier question"));
    rig.line(OLD, &answer("o2", "o1", "earlier answer"));
    rig.record(OLD, "idle");
    let mut lens = rig.lens();
    let opened = lens.open();

    rig.record(NEW, "idle");
    let mut payloads = tick(&mut lens);
    assert!(send(&mut lens, 4, "ping").is_empty());
    assert_eq!(*rig.pane.sent.lock(), ["ping"]);
    rig.line(NEW, &prompt("n1", None, "ping"));
    rig.record(NEW, "busy");
    payloads.extend(tick(&mut lens));
    rig.line(NEW, &answer("n2", "n1", "pong"));
    payloads.extend(tick(&mut lens));
    rig.record(NEW, "idle");
    rig.line(NEW, &turn_duration("n3", "n2"));
    payloads.extend(tick(&mut lens));

    let reply = said(&payloads, "pong").expect("the reply reaches the lens");
    let done = finished(&payloads, 4).expect("the turn finishes");
    assert!(reply < done, "{payloads:?}");
    let sent = payloads
        .iter()
        .position(|payload| matches!(payload, AgentStreamPayload::Activity { busy: true }))
        .expect("the turn runs");
    assert!(
        !payloads[sent..done].iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::SessionReset { .. } | AgentStreamPayload::SessionSwitched { .. }
        )),
        "nothing resets the chat under a running turn: {payloads:?}"
    );
    let chat = view(&[&opened, &payloads]);
    assert_eq!(texts(&chat, ItemKind::User), ["ping"]);
    assert_eq!(texts(&chat, ItemKind::Agent), ["pong"]);
}

#[test]
fn a_turn_left_with_the_old_session_still_settles() {
    let rig = Rig::new();
    rig.line(OLD, &prompt("o1", None, "earlier question"));
    rig.record(OLD, "idle");
    let mut lens = rig.lens();
    lens.open();
    assert!(send(&mut lens, 5, "long job").is_empty());
    rig.record(OLD, "busy");
    let busy = tick(&mut lens);
    assert!(busy.contains(&AgentStreamPayload::Activity { busy: true }));

    rig.record(NEW, "idle");
    let moved = tick(&mut lens);
    let done = finished(&moved, 5).expect("the abandoned turn settles");
    let reset = moved
        .iter()
        .position(|payload| {
            matches!(
                payload,
                AgentStreamPayload::SessionReset { restoring: false }
            )
        })
        .expect("the chat empties for the session that has no transcript yet");
    assert!(done < reset, "{moved:?}");
    assert!(moved.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::SessionReady { session_id, .. } if session_id == "lens-%7"
    )));

    rig.line(NEW, &prompt("n1", None, "typed in the terminal"));
    rig.line(NEW, &answer("n2", "n1", "fine"));
    let later = tick(&mut lens);
    assert_eq!(said(&later, "fine"), Some(1), "{later:?}");
    assert!(
        !later
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. }))
    );
}

#[test]
fn a_cli_that_exits_takes_its_session_out_of_the_lens() {
    let mut cli = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let rig = Rig::in_pane(cli.id());
    rig.line(OLD, &prompt("o1", None, "earlier question"));
    rig.record(OLD, "idle");
    let mut lens = rig.lens();
    let opened = lens.open();
    assert!(opened.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::SessionReady { session_id, .. } if session_id == OLD
    )));
    cli.kill().unwrap();
    cli.wait().unwrap();
    std::fs::remove_file(
        rig.root
            .path()
            .join(format!("registry/{}.json", rig.pane.pid)),
    )
    .unwrap();
    let gone = tick(&mut lens);
    assert!(
        gone.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::SessionReset { restoring: false }
        )),
        "{gone:?}"
    );
    assert!(lens.session.is_none());
    assert!(tick(&mut lens).is_empty());
}
