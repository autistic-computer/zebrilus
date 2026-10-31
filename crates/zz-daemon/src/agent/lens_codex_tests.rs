use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use parking_lot::Mutex;
use serde_json::Value;
use zz_protocol::chat::{
    Answer, Ask, ChatEvent, ChatView, ItemId, ItemKind, ItemStatus, RequestClose, Usage,
};
use zz_protocol::{AgentProvider, AgentQuestionAnswer, PaneId, agent_stream::AgentImage};

use super::{
    AgentPrompt, AgentPromptOutcome, AgentStreamPayload, CODEX_ASKING, Codex, Found, Lens,
    LensPane, LensSpec, RuntimeCommand, RuntimeControl, Shadow, Source, shadow_lens,
};

const ROOT: &str = "rollout-2026-10-31T15-26-23-01a19336-7aab-7cb2-b50a-7c1a49be3375.jsonl";
const CHILD: &str = "rollout-2026-10-31T15-33-30-01a1933c-fd86-7003-9bc8-2c9522c0c694.jsonl";
const QUESTION: &str = "call_331ba9da50bd4c2fb650d772e5d6d45e";
const COUNTER: &str = "call_1db63d77e8d243cd9f12c091a00172b6";
const BACKGROUND: &str = "call_2553eb67216f4bb68bbd47dca33a9f3a";
const TICKS: &str = "exec-b4d43416-19dd-4752-bc10-6790ffe5f6ae";

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

struct Rollout {
    dir: tempfile::TempDir,
}

impl Rollout {
    fn new(answered: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = fixture("rollout-lens-stress.jsonl")
            .lines()
            .filter(|line| answered || !line.contains("send_user_message_question_reply"))
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

    fn lens(&self, pane: Arc<Pane>) -> (Lens, Vec<AgentStreamPayload>) {
        let mut lens = Lens::with_source(
            LensSpec {
                pane: PaneId(7),
                provider: AgentProvider::Codex,
                io: pane,
            },
            Source::Codex(Box::default()),
            Arc::new(AtomicU64::new(1)),
        );
        let mut payloads = lens.reload(
            Found {
                session: "01a19336-7aab-7cb2-b50a-7c1a49be3375".to_owned(),
                path: self.path(),
                cwd: PathBuf::from("/work"),
            },
            true,
        );
        payloads.extend(lens.observe(false, Vec::new()));
        (lens, payloads)
    }
}

fn chat(payloads: &[AgentStreamPayload]) -> (Shadow, ChatView) {
    let mut shadow = Shadow::recording().codex().attached();
    shadow_lens(&mut shadow, payloads);
    let mut view = ChatView::default();
    for event in &shadow.events {
        view.apply(event)
            .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
    }
    (shadow, view)
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
    let rollout = Rollout::new(true);
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

    let record = view
        .requests
        .iter()
        .find(|request| {
            request
                .item
                .as_ref()
                .is_some_and(|item| item.as_str() == QUESTION)
        })
        .expect("an answer record");
    assert_eq!(
        record.close,
        Some(RequestClose::Answered {
            answer: Answer::Questions {
                answers: vec![zz_protocol::chat::QuestionAnswer {
                    question: "q0".to_owned(),
                    answers: vec!["Tool output and files".to_owned()],
                }],
            },
        })
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
fn an_unanswered_codex_question_is_the_only_thing_left_open() {
    let rollout = Rollout::new(false);
    let pane = Arc::new(Pane::default());
    let (mut lens, payloads) = rollout.lens(pane.clone());
    let (shadow, view) = chat(&payloads);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let open = view
        .open_items()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(open, [QUESTION]);
    let request = view.open_requests().next().expect("a question card");
    assert_eq!(request.item.as_ref().map(ItemId::as_str), Some(QUESTION));
    let Ask::Questions { questions } = &request.ask else {
        panic!("not a question: {:?}", request.ask);
    };
    assert_eq!(questions.len(), 1);
    assert_eq!(
        questions[0]
            .options
            .iter()
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Markdown and diagrams",
            "Tool output and files",
            "Background tasks and agents"
        ]
    );

    let refused = lens.command(RuntimeCommand::Prompt {
        turn_id: 9,
        prompt: AgentPrompt {
            text: "2".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(refused.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PromptFinished { turn_id: 9, outcome: AgentPromptOutcome::Failed { message } }
            if message == CODEX_ASKING
    )));
    let steered = lens.command(RuntimeCommand::Verb {
        prompt: AgentPrompt {
            text: "/steer 3".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(steered.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PromptsReclaimed { prompts } if prompts[0].text == "3"
    )));
    assert!(pane.sent.lock().is_empty());

    let answered = lens.control(RuntimeControl::AnswerQuestion {
        request_id: request.id.0,
        answers: vec![AgentQuestionAnswer {
            id: questions[0].id.clone(),
            answers: vec!["Tool output and files".to_owned()],
        }],
    });
    assert_eq!(*pane.keys.lock(), ["2"]);
    assert_eq!(
        answered,
        [AgentStreamPayload::PermissionResolved {
            request_id: request.id.0,
            canceled: false,
        }]
    );
}

#[test]
fn a_codex_waiting_on_a_question_says_so_until_it_is_dismissed() {
    let rollout = Rollout::new(false);
    let pane = Arc::new(Pane::default());
    let (mut lens, _) = rollout.lens(pane.clone());
    let status = lens.source.status().expect("status");
    assert!(!status.busy);
    assert_eq!(status.waiting.as_deref(), Some("question"));
    let stopped = lens.control(RuntimeControl::Cancel {
        turn_id: 0,
        session_id: None,
    });
    assert_eq!(*pane.keys.lock(), ["Escape"]);
    assert!(!lens.source.asking());
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
fn a_child_transcript_found_later_still_nests_under_its_agent() {
    let rollout = Rollout::new(true);
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
