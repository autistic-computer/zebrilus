use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value, json};
use zz_protocol::chat::{
    Answer, Ask, ChatEvent, ChatView, ItemId, ItemKind, ItemStatus, QuestionAnswer, RequestClose,
    RequestView, Usage,
};
use zz_protocol::{AgentProvider, AgentQuestionAnswer, PaneId, agent_stream::AgentImage};

use super::{
    AgentPrompt, AgentStreamPayload, Codex, Found, Lens, LensPane, LensSpec, READ_LIMIT,
    RuntimeCommand, RuntimeControl, Shadow, Source, shadow_lens,
};

const ROOT: &str = "rollout-2026-10-31T15-26-23-01a19336-7aab-7cb2-b50a-7c1a49be3375.jsonl";
const CHILD: &str = "rollout-2026-10-31T15-33-30-01a1933c-fd86-7003-9bc8-2c9522c0c694.jsonl";
const QUESTION: &str = "call_331ba9da50bd4c2fb650d772e5d6d45e";
const COUNTER: &str = "call_1db63d77e8d243cd9f12c091a00172b6";
const BACKGROUND: &str = "call_2553eb67216f4bb68bbd47dca33a9f3a";
const TICKS: &str = "exec-b4d43416-19dd-4752-bc10-6790ffe5f6ae";
const ASKED: &str = "Which part of this chat UI test are you most interested in checking?";
const REPLY: &str = "send_user_message_question_reply";

#[derive(Default)]
struct Pane {
    sent: Mutex<Vec<String>>,
    keys: Mutex<Vec<String>>,
}

impl LensPane for Pane {
    fn pid(&self) -> Option<u32> {
        None
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

    fn attach(&self, _images: &[AgentImage]) -> Result<(), String> {
        Ok(())
    }

    fn screen(&self) -> Option<String> {
        None
    }
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/agent/codex/fixtures")
            .join(name),
    )
    .expect("fixture")
}

fn fixture_line(marker: &str) -> String {
    fixture("rollout-lens-stress.jsonl")
        .lines()
        .find(|line| line.contains(marker))
        .unwrap_or_else(|| panic!("no fixture line with {marker}"))
        .to_owned()
}

struct Rollout {
    dir: tempfile::TempDir,
}

impl Rollout {
    fn new() -> Self {
        Self::until(None)
    }

    fn asking() -> Self {
        Self::until(Some(REPLY))
    }

    fn until(marker: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = fixture("rollout-lens-stress.jsonl")
            .lines()
            .take_while(|line| marker.is_none_or(|marker| !line.contains(marker)))
            .fold(String::new(), |all, line| all + line + "\n");
        std::fs::write(dir.path().join(ROOT), root).unwrap();
        std::fs::write(
            dir.path().join(CHILD),
            fixture("rollout-lens-stress-child.jsonl"),
        )
        .unwrap();
        Self { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.path().join(ROOT)
    }

    fn append(&self, line: &str) {
        append(&self.path(), line);
    }

    fn lens(&self, pane: Arc<Pane>) -> (Lens, Vec<AgentStreamPayload>) {
        self.lens_with(pane, Codex::default())
    }

    fn lens_with(&self, pane: Arc<Pane>, codex: Codex) -> (Lens, Vec<AgentStreamPayload>) {
        open(self.path(), pane, codex)
    }
}

fn append(path: &Path, line: &str) {
    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(file, "{line}").unwrap();
}

fn open(path: PathBuf, pane: Arc<Pane>, codex: Codex) -> (Lens, Vec<AgentStreamPayload>) {
    let mut lens = Lens::with_source(
        LensSpec {
            pane: PaneId(7),
            provider: AgentProvider::Codex,
            io: pane,
        },
        Source::Codex(Box::new(codex)),
        Arc::new(AtomicU64::new(1)),
    );
    let mut payloads = lens.reload(
        Found {
            session: "01a19336-7aab-7cb2-b50a-7c1a49be3375".to_owned(),
            path,
            cwd: PathBuf::from("/work"),
        },
        true,
    );
    payloads.extend(lens.observe(false, Vec::new()));
    lens.next_locate = Instant::now() + Duration::from_mins(1);
    (lens, payloads)
}

fn chats(rounds: &[&[AgentStreamPayload]]) -> (Shadow, ChatView) {
    let mut shadow = Shadow::recording().codex().attached();
    for payloads in rounds {
        shadow_lens(&mut shadow, payloads);
    }
    let mut view = ChatView::default();
    for event in &shadow.events {
        view.apply(event)
            .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
    }
    (shadow, view)
}

fn record<'a>(view: &'a ChatView, item: &str) -> &'a RequestView {
    view.requests
        .iter()
        .find(|request| {
            request.item.as_ref().is_some_and(|id| id.as_str() == item)
                && matches!(request.ask, Ask::Questions { .. })
        })
        .unwrap_or_else(|| panic!("no answer record for {item}"))
}

fn answers(pairs: &[(&str, &str)]) -> RequestClose {
    RequestClose::Answered {
        answer: Answer::Questions {
            answers: pairs
                .iter()
                .map(|(question, answer)| QuestionAnswer {
                    question: (*question).to_owned(),
                    answers: vec![(*answer).to_owned()],
                })
                .collect(),
        },
    }
}

fn requested(payloads: &[AgentStreamPayload]) -> Vec<(u64, &Value, &Value, usize)> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call,
                options,
                questions,
            } => Some((*request_id, tool_call, options, questions.len())),
            _ => None,
        })
        .collect()
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

fn waiting(lens: &mut Lens) -> Option<String> {
    lens.source.status().expect("status").waiting
}

fn chat(payloads: &[AgentStreamPayload]) -> (Shadow, ChatView) {
    chats(&[payloads])
}

fn updates(payloads: &[AgentStreamPayload]) -> Vec<&Value> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::Update { update } => Some(update),
            _ => None,
        })
        .collect()
}

fn titled<'a>(view: &'a ChatView, title: &str) -> &'a zz_protocol::chat::ItemView {
    view.items
        .values()
        .find(|item| item.body.title == title)
        .unwrap_or_else(|| panic!("no item titled {title}"))
}

#[test]
fn a_recorded_codex_session_reads_as_a_clean_chat() {
    let rollout = Rollout::new();
    let (_, payloads) = rollout.lens(Arc::default());
    let (shadow, view) = chat(&payloads);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);

    let ticks = view.item(&ItemId::new(TICKS)).expect("tick loop");
    assert_eq!(
        ticks.body.target.as_deref(),
        Some("for i in $(seq 1 20); do echo tick $i; sleep 1; done")
    );
    assert_eq!(
        ticks.ended_at.map(|end| end - ticks.started_at),
        Some(19_995)
    );

    let mut agents = view
        .items
        .values()
        .filter(|item| item.kind == ItemKind::Subagent)
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    agents.sort_unstable();
    assert_eq!(agents, [COUNTER, BACKGROUND]);
    let wait = titled(&view, "Waited for agents");
    assert_eq!(wait.kind, ItemKind::Tool);
    assert_eq!(titled(&view, "Waited 1m").kind, ItemKind::Tool);

    let counter = view.item(&ItemId::new(COUNTER)).expect("counter agent");
    assert_eq!(counter.status, ItemStatus::Completed);
    assert_eq!(
        counter.body.text,
        "alpha.txt: 3 lines\nbeta.txt: 4 lines\ngamma.json: 4 lines\nTotal: 11 lines"
    );
    let count = titled(&view, "wc -l alpha.txt beta.txt gamma.json");
    assert_eq!(count.parent.as_ref(), Some(&counter.id));
    assert_eq!(counter.children, std::slice::from_ref(&count.id));
    let background = view
        .item(&ItemId::new(BACKGROUND))
        .expect("background agent");
    assert_eq!(background.status, ItemStatus::Completed);
    assert_eq!(
        background.body.text,
        "background complete: word=terminal, ready=true\n\nExit status: 0."
    );

    assert_eq!(
        record(&view, QUESTION).close,
        Some(answers(&[("q0", "Tool output and files")]))
    );
    assert!(!view.items.values().any(|item| {
        item.body
            .text
            .contains("<send_user_message_question_reply>")
    }));

    assert_eq!(titled(&view, "Edit 3 files").kind, ItemKind::Edit);
    let edit = titled(&view, "Edit beta.txt");
    let raw = updates(&payloads);
    let diff = raw
        .iter()
        .find(|update| update["toolCallId"] == edit.id.as_str() && update["content"].is_array())
        .expect("the edit's diff");
    assert_eq!(
        diff["content"][0]["oldText"],
        "terminal waiting\nwindow open\npane active\n"
    );
    assert_eq!(
        diff["content"][0]["newText"],
        "terminal ready\nwindow open\npane active\nedit complete\n"
    );
    assert_eq!(
        titled(&view, "Open https://example.com")
            .body
            .target
            .as_deref(),
        Some("https://example.com")
    );
    assert_eq!(
        titled(&view, "mkdir -p /work").body.target.as_deref(),
        Some("mkdir -p /work")
    );
    let failed = titled(&view, "bash -c 'exit 3'");
    assert_eq!(failed.status, ItemStatus::Failed { message: None });
    assert_eq!(failed.body.exit_code, Some(3));

    assert!(shadow.events.iter().any(|event| matches!(
        event,
        ChatEvent::UsageChanged {
            usage: Usage {
                used: 36_810,
                size: 475_000
            }
        }
    )));
    assert!(view.open_items().next().is_none());
    assert!(view.open_requests().next().is_none());
    assert!(
        !payloads
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PermissionRequested { .. }))
    );
}

#[test]
fn an_unanswered_codex_question_points_at_the_terminal_until_its_reply() {
    let rollout = Rollout::asking();
    let pane = Arc::new(Pane::default());
    let (mut lens, opened) = rollout.lens(pane.clone());
    let (shadow, view) = chat(&opened);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let open = view
        .open_items()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(open, [QUESTION]);
    let asked = requested(&opened);
    let [(request_id, tool_call, options, 0)] = asked.as_slice() else {
        panic!("not one terminal card: {asked:?}");
    };
    assert_eq!(tool_call["toolCallId"], QUESTION);
    assert_eq!(tool_call["title"], ASKED);
    assert_eq!(**options, json!([]));
    let request = view.open_requests().next().expect("a terminal card");
    assert_eq!(request.item.as_ref().map(ItemId::as_str), Some(QUESTION));
    assert_eq!(
        request.ask,
        Ask::Approval {
            title: ASKED.to_owned(),
            detail: None,
            options: Vec::new(),
        }
    );

    let picked = lens.control(RuntimeControl::AnswerQuestion {
        request_id: *request_id,
        answers: vec![AgentQuestionAnswer {
            id: "q0".to_owned(),
            answers: vec!["Tool output and files".to_owned()],
        }],
    });
    assert!(picked.is_empty(), "{picked:?}");
    assert!(pane.keys.lock().is_empty());
    assert_eq!(waiting(&mut lens).as_deref(), Some("question"));

    rollout.append(&fixture_line(REPLY));
    let replied = lens.tick();
    assert_eq!(resolved(&replied), [*request_id]);
    assert_eq!(waiting(&mut lens), None);
    let (shadow, view) = chats(&[&opened, &replied]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert_eq!(
        view.item(&ItemId::new(QUESTION))
            .expect("the question")
            .status,
        ItemStatus::Completed
    );
    assert_eq!(
        record(&view, QUESTION).close,
        Some(answers(&[("q0", "Tool output and files")]))
    );
    assert!(view.open_requests().next().is_none());
}

#[test]
fn prompts_and_steers_reach_codex_while_its_question_waits() {
    let rollout = Rollout::asking();
    let pane = Arc::new(Pane::default());
    let (mut lens, _) = rollout.lens(pane.clone());
    assert_eq!(waiting(&mut lens).as_deref(), Some("question"));
    let sent = lens.command(RuntimeCommand::Prompt {
        turn_id: 9,
        prompt: AgentPrompt {
            text: "also look at the diagrams".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(sent.is_empty(), "{sent:?}");
    let steered = lens.command(RuntimeCommand::Verb {
        prompt: AgentPrompt {
            text: "/steer and the tables".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(steered.is_empty(), "{steered:?}");
    assert_eq!(
        *pane.sent.lock(),
        ["also look at the diagrams", "and the tables"]
    );
}

#[test]
fn a_message_sent_while_codex_asks_leaves_the_question_open() {
    let rollout = Rollout::asking();
    let (mut lens, opened) = rollout.lens(Arc::default());
    let steer = json!({
        "timestamp": "2026-10-31T18:35:10.000Z", "type": "event_msg",
        "payload": { "type": "item_completed", "item": {
            "type": "UserMessage", "id": "steer-1",
            "content": [{ "type": "text", "text": "also look at the diagrams" }],
        } },
    });
    rollout.append(&steer.to_string());
    let steered = lens.tick();
    assert!(resolved(&steered).is_empty(), "{steered:?}");
    assert_eq!(waiting(&mut lens).as_deref(), Some("question"));
    rollout.append(&fixture_line(REPLY));
    let replied = lens.tick();
    let (shadow, view) = chats(&[&opened, &steered, &replied]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert!(
        view.items
            .values()
            .any(|item| item.body.text == "also look at the diagrams")
    );
    assert_eq!(
        record(&view, QUESTION).close,
        Some(answers(&[("q0", "Tool output and files")]))
    );
}

#[test]
fn a_codex_turn_that_ends_drops_the_question_it_left_open() {
    let rollout = Rollout::asking();
    let pane = Arc::new(Pane::default());
    let (mut lens, opened) = rollout.lens(pane.clone());
    let asked = requested(&opened);
    let [(request_id, ..)] = asked.as_slice() else {
        panic!("no card: {opened:?}");
    };
    let request_id = *request_id;
    rollout.append(&fixture_line("\"type\":\"task_complete\""));
    let ended = lens.tick();
    assert!(
        updates(&ended)
            .iter()
            .any(|update| update["toolCallId"] == QUESTION && update["status"] == "completed"),
        "{ended:?}"
    );
    assert_eq!(resolved(&ended), [request_id]);
    assert_eq!(waiting(&mut lens), None);
    let (shadow, view) = chats(&[&opened, &ended]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert_eq!(record(&view, QUESTION).close, Some(answers(&[("q0", "")])));
    assert!(view.open_items().next().is_none());
    assert!(view.open_requests().next().is_none());
    assert!(
        lens.command(RuntimeCommand::SetConfigOption {
            option_id: "cwd".to_owned(),
            value: "/elsewhere".to_owned(),
        })
        .iter()
        .all(|payload| !matches!(payload, AgentStreamPayload::SettingFailed { .. }))
    );
    assert_eq!(*pane.sent.lock(), ["/cd /elsewhere"]);
}

#[test]
fn a_codex_waiting_on_a_question_says_so_until_it_is_dismissed() {
    let rollout = Rollout::asking();
    let pane = Arc::new(Pane::default());
    let (mut lens, _) = rollout.lens(pane.clone());
    assert_eq!(waiting(&mut lens).as_deref(), Some("question"));
    let stopped = lens.control(RuntimeControl::Cancel {
        turn_id: 0,
        session_id: None,
    });
    assert_eq!(*pane.keys.lock(), ["Escape"]);
    assert_eq!(waiting(&mut lens), None);
    assert!(stopped.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::Update { update }
            if update["toolCallId"] == QUESTION && update["status"] == "completed"
    )));
    let resolved = lens.observe(false, Vec::new());
    assert!(
        resolved
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PermissionResolved { .. }))
    );
}

#[test]
fn a_codex_question_in_several_parts_waits_for_every_answer() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("rollout-2026-10-31T10-00-00-root.jsonl");
    let event = |at: &str, payload: Value| {
        json!({ "timestamp": at, "type": "event_msg", "payload": payload }).to_string()
    };
    let reply = |at: &str, index: u64, question: &str, answer: &str| {
        let id = json!(["request_user_input_async", "call_two", index]).to_string();
        let entries = json!([{ "questionItemId": id, "question": question, "answer": answer }]);
        event(
            at,
            json!({ "type": "item_completed", "item": {
            "type": "UserMessage", "id": format!("reply-{index}"),
            "content": [{ "type": "text", "text": format!(
                "<send_user_message_question_reply>\n{entries}\n</send_user_message_question_reply>\n"
            ) }],
        } }),
        )
    };
    let lines = [
        event(
            "2026-10-31T10:00:00.000Z",
            json!({ "type": "task_started" }),
        ),
        event(
            "2026-10-31T10:00:01.000Z",
            json!({ "type": "item_completed", "item": {
            "type": "AgentMessage", "id": "call_two", "delivery": "async",
            "content": [{ "type": "Text", "text": "Two questions" }],
            "questions": [
                { "title": "Which color?", "options": ["Red", "Blue"] },
                { "title": "Which size?", "options": ["Small", "Large"] },
            ],
        } }),
        ),
    ];
    std::fs::write(&root, lines.join("\n") + "\n").unwrap();
    let (mut lens, opened) = open(root.clone(), Arc::default(), Codex::default());
    let first = requested(&opened);
    let [(first, tool_call, _, 0)] = first.as_slice() else {
        panic!("no card: {opened:?}");
    };
    assert_eq!(tool_call["toolCallId"], "call_two");
    assert_eq!(tool_call["title"], "Which color?");
    let first = *first;

    append(
        &root,
        &reply("2026-10-31T10:00:02.000Z", 0, "Which color?", "Blue"),
    );
    let half = lens.tick();
    assert!(
        !updates(&half)
            .iter()
            .any(|update| update["toolCallId"] == "call_two"),
        "{half:?}"
    );
    assert_eq!(resolved(&half), [first]);
    let second = requested(&half);
    let [(second, tool_call, _, 0)] = second.as_slice() else {
        panic!("no card for the second question: {half:?}");
    };
    assert_eq!(tool_call["toolCallId"], "call_two");
    assert_eq!(tool_call["title"], "Which size?");
    let second = *second;
    assert_eq!(waiting(&mut lens).as_deref(), Some("question"));

    append(
        &root,
        &reply("2026-10-31T10:00:03.000Z", 1, "Which size?", "Large"),
    );
    let done = lens.tick();
    assert_eq!(resolved(&done), [second]);
    assert_eq!(waiting(&mut lens), None);
    let (shadow, view) = chats(&[&opened, &half, &done]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert_eq!(
        view.item(&ItemId::new("call_two"))
            .expect("the question")
            .status,
        ItemStatus::Completed
    );
    assert_eq!(
        record(&view, "call_two").close,
        Some(answers(&[("q0", "Blue"), ("q1", "Large")]))
    );
    assert!(view.open_requests().next().is_none());
}

#[test]
fn a_child_transcript_found_later_still_nests_under_its_agent() {
    let rollout = Rollout::new();
    let child = rollout.dir.path().join(CHILD);
    let hidden = rollout.dir.path().join("held.jsonl");
    std::fs::rename(&child, &hidden).unwrap();
    let mut codex = Codex::default();
    let text = std::fs::read_to_string(rollout.path()).unwrap();
    let cut = text
        .lines()
        .position(|line| line.contains("\"kind\":\"started\""))
        .expect("an agent start");
    let head = text
        .lines()
        .take(cut + 1)
        .fold(String::new(), |all, line| all + line + "\n");
    std::fs::write(rollout.path(), &head).unwrap();
    codex.load(rollout.path(), Path::new("/work"));
    std::fs::rename(&hidden, &child).unwrap();
    std::fs::write(rollout.path(), text).unwrap();
    let read = codex.read();
    let nested = read
        .updates
        .iter()
        .find(|update| update["title"] == "wc -l alpha.txt beta.txt gamma.json")
        .expect("the child's command");
    assert_eq!(nested["_meta"]["zz"]["parent"], COUNTER);
}

#[test]
fn a_child_transcript_found_only_when_its_agent_ends_still_comes_first() {
    let rollout = Rollout::new();
    let child = rollout.dir.path().join(CHILD);
    let hidden = rollout.dir.path().join("held.jsonl");
    std::fs::rename(&child, &hidden).unwrap();
    let mut codex = Codex::default();
    let text = std::fs::read_to_string(rollout.path()).unwrap();
    let cut = text
        .lines()
        .position(|line| line.contains("\"kind\":\"started\""))
        .expect("an agent start");
    let head = text
        .lines()
        .take(cut + 1)
        .fold(String::new(), |all, line| all + line + "\n");
    std::fs::write(rollout.path(), &head).unwrap();
    codex.load(rollout.path(), Path::new("/work"));
    codex.children[0].lost = true;
    std::fs::rename(&hidden, &child).unwrap();
    std::fs::write(rollout.path(), text).unwrap();
    let read = codex.read();
    let nested = read
        .updates
        .iter()
        .position(|update| update["title"] == "wc -l alpha.txt beta.txt gamma.json")
        .expect("the child's command");
    let ended = read
        .updates
        .iter()
        .position(|update| update["toolCallId"] == COUNTER && update["status"] == "completed")
        .expect("the agent's end");
    assert!(
        nested < ended,
        "the child's step came after its agent ended"
    );
}

#[test]
fn a_pick_waits_for_the_turn_not_for_a_question() {
    let pick = |lens: &mut Lens| {
        let picked = lens.command(RuntimeCommand::SetConfigOption {
            option_id: "cwd".to_owned(),
            value: "/elsewhere".to_owned(),
        });
        assert!(
            picked
                .iter()
                .all(|payload| !matches!(payload, AgentStreamPayload::SettingFailed { .. })),
            "{picked:?}"
        );
    };
    let running = Rollout::asking();
    let pane = Arc::new(Pane::default());
    let (mut lens, _) = running.lens(pane.clone());
    assert_eq!(waiting(&mut lens).as_deref(), Some("question"));
    pick(&mut lens);
    assert!(
        pane.sent.lock().is_empty(),
        "a pick waits while the turn runs"
    );

    let idle = Rollout::asking();
    let text = std::fs::read_to_string(idle.path()).unwrap();
    let text = text
        .lines()
        .filter(|line| !line.contains("\"type\":\"task_started\""))
        .fold(String::new(), |all, line| all + line + "\n");
    std::fs::write(idle.path(), text).unwrap();
    let pane = Arc::new(Pane::default());
    let (mut lens, _) = idle.lens(pane.clone());
    assert_eq!(waiting(&mut lens).as_deref(), Some("question"));
    assert!(!lens.source.status().expect("status").busy);
    pick(&mut lens);
    assert_eq!(*pane.sent.lock(), ["/cd /elsewhere"]);
}

#[test]
fn a_question_from_before_this_codex_started_is_not_waiting() {
    let rollout = Rollout::asking();
    let pane = Arc::new(Pane::default());
    let codex = Codex {
        born: Some(4_000_000_000),
        ..Codex::default()
    };
    let (mut lens, payloads) = rollout.lens_with(pane.clone(), codex);
    assert_eq!(waiting(&mut lens), None);
    assert!(requested(&payloads).is_empty(), "{payloads:?}");
}

#[test]
fn a_question_from_before_this_codex_started_leaves_no_running_row() {
    let rollout = Rollout::asking();
    let codex = Codex {
        born: Some(4_000_000_000),
        ..Codex::default()
    };
    let (_, payloads) = rollout.lens_with(Arc::default(), codex);
    let (shadow, view) = chat(&payloads);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let question = view.item(&ItemId::new(QUESTION)).expect("the question row");
    assert_eq!(question.status, ItemStatus::Completed);
    assert!(view.open_items().next().is_none());
    assert!(view.open_requests().next().is_none());
}

#[test]
fn a_codex_resumed_in_the_same_pane_drops_the_question_the_old_one_left_open() {
    let rollout = Rollout::asking();
    let (mut lens, opened) = rollout.lens(Arc::default());
    let relaunch = |lens: &mut Lens, born: u64| {
        let Source::Codex(codex) = &mut lens.source else {
            unreachable!()
        };
        codex.launched(std::process::id(), Some(born));
    };
    relaunch(&mut lens, 1);
    assert_eq!(
        waiting(&mut lens).as_deref(),
        Some("question"),
        "a question asked after it started stays"
    );
    relaunch(&mut lens, 4_000_000_000);
    assert_eq!(waiting(&mut lens), None);
    let resumed = lens.tick();
    assert!(
        updates(&resumed)
            .iter()
            .any(|update| update["toolCallId"] == QUESTION && update["status"] == "completed"),
        "{resumed:?}"
    );
    assert_eq!(resolved(&resumed).len(), 1, "{resumed:?}");
    let (shadow, view) = chats(&[&opened, &resumed]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert!(view.open_items().next().is_none());
    assert!(view.open_requests().next().is_none());
}

#[test]
fn loading_a_rollout_reads_each_child_where_order_matters_not_before_every_line() {
    let dir = tempfile::tempdir().unwrap();
    let event = |payload: Value| {
        json!({ "timestamp": "2026-10-31T10:00:01.000Z", "type": "event_msg", "payload": payload })
            .to_string()
    };
    let mut lines = vec![event(json!({ "type": "task_started" }))];
    for child in 0..3 {
        let thread = format!("child-{child}");
        lines.push(event(json!({ "type": "item_completed", "item": {
            "type": "SubAgentActivity", "id": format!("call_{child}"), "kind": "started",
            "agent_thread_id": thread, "agent_path": format!("/root/a{child}"),
        } })));
        std::fs::write(
            dir.path()
                .join(format!("rollout-2026-10-31T10-00-01-{thread}.jsonl")),
            format!(
                "{}\n",
                json!({ "type": "session_meta", "payload": {
                    "id": thread, "parent_thread_id": "root", "cwd": "/work",
                } })
            ),
        )
        .unwrap();
    }
    for index in 0..200 {
        lines.push(event(json!({ "type": "item_completed", "item": {
            "type": "Reasoning", "id": format!("r{index}"), "summary_text": ["**x**"], "raw_content": [],
        } })));
    }
    lines.push(event(json!({ "type": "task_complete" })));
    let root = dir.path().join("rollout-2026-10-31T10-00-00-root.jsonl");
    std::fs::write(&root, lines.join("\n") + "\n").unwrap();
    let mut codex = Codex::default();
    codex.load(root, Path::new("/work"));
    assert_eq!(codex.children.len(), 3);
    for child in &codex.children {
        let reads = child.tail.as_ref().expect("found the child").reads;
        assert!(reads <= 2, "{} read {reads} times", child.thread);
    }
}

#[test]
fn a_child_rollout_past_the_read_limit_ends_before_its_agent_does() {
    let dir = tempfile::tempdir().unwrap();
    let event = |at: &str, payload: Value| {
        json!({ "timestamp": at, "type": "event_msg", "payload": payload }).to_string()
    };
    let thread = "child-big";
    let result = "counted every line";
    let mut child = vec![
        json!({ "type": "session_meta", "payload": {
            "id": thread, "parent_thread_id": "root", "cwd": "/work",
        } })
        .to_string(),
        event(
            "2026-10-31T10:00:02.000Z",
            json!({ "type": "task_started" }),
        ),
    ];
    let filler = "x".repeat(64 * 1024);
    let mut size = 0;
    let mut index = 0;
    while size <= READ_LIMIT + 64 * 1024 {
        let line = event(
            "2026-10-31T10:00:03.000Z",
            json!({ "type": "item_completed", "item": {
                "type": "Reasoning", "id": format!("r{index}"), "summary_text": [filler], "raw_content": [],
            } }),
        );
        size += line.len() as u64 + 1;
        index += 1;
        child.push(line);
    }
    child.push(event(
        "2026-10-31T10:00:04.000Z",
        json!({ "type": "item_completed", "item": {
            "type": "CommandExecution", "id": "exec-late", "command": ["wc", "-l", "big.txt"],
            "cwd": "/work", "parsed_cmd": [{ "type": "unknown", "cmd": "wc -l big.txt" }],
            "status": "completed", "aggregated_output": "9 big.txt\n", "exit_code": 0,
        } }),
    ));
    child.push(event(
        "2026-10-31T10:00:05.000Z",
        json!({ "type": "item_completed", "item": {
            "type": "AgentMessage", "id": "msg-late",
            "content": [{ "type": "Text", "text": result }], "phase": "final_answer",
        } }),
    ));
    child.push(event(
        "2026-10-31T10:00:05.100Z",
        json!({ "type": "task_complete" }),
    ));
    let child = child.join("\n") + "\n";
    assert!(child.find(result).unwrap() as u64 > READ_LIMIT);
    std::fs::write(
        dir.path()
            .join(format!("rollout-2026-10-31T10-00-01-{thread}.jsonl")),
        child,
    )
    .unwrap();
    let agent = |at: &str, id: &str, kind: &str| {
        event(
            at,
            json!({ "type": "item_completed", "item": {
            "type": "SubAgentActivity", "id": id, "kind": kind,
            "agent_thread_id": thread, "agent_path": "/root/big_counter",
        } }),
        )
    };
    let root = dir.path().join("rollout-2026-10-31T10-00-00-root.jsonl");
    let lines = [
        event(
            "2026-10-31T10:00:00.000Z",
            json!({ "type": "task_started" }),
        ),
        agent("2026-10-31T10:00:01.000Z", "call_big", "started"),
        agent(
            "2026-10-31T10:00:06.000Z",
            "subagent-completed-big",
            "completed",
        ),
        event(
            "2026-10-31T10:00:07.000Z",
            json!({ "type": "task_complete" }),
        ),
    ];
    std::fs::write(&root, lines.join("\n") + "\n").unwrap();
    let (_, payloads) = open(root, Arc::default(), Codex::default());
    let (shadow, view) = chat(&payloads);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let agent = view.item(&ItemId::new("call_big")).expect("the agent row");
    assert_eq!(agent.status, ItemStatus::Completed);
    assert_eq!(agent.body.text, result);
    let command = titled(&view, "wc -l big.txt");
    assert_eq!(command.parent.as_ref(), Some(&agent.id));
}

#[test]
fn a_duration_too_long_to_count_is_left_off() {
    let entry = json!({ "type": "event_msg", "payload": { "type": "item_completed", "item": {
        "duration": { "secs": u64::MAX, "nanos": 999_999_999 },
    } } });
    let updates = super::timed(vec![json!({ "sessionUpdate": "tool_call" })], &entry);
    assert!(updates[0]["_meta"].is_null(), "{updates:?}");
}
