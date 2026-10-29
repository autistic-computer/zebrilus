use zz_protocol::agent_stream::AgentQuestion;
use zz_protocol::chat::{
    Answer, Ask, AskOption, ChatEvent, ChatView, ItemBody, ItemId, ItemKind, ItemStatus,
    OptionKind, QuestionAnswer, RequestClose, RequestId, TurnBy, TurnId, TurnOutcome,
};

use crate::chat_rows::{
    AgentState, Fold, RunLabel, Unit, agent_ms, agent_prompt, agent_rows, agent_state, answered,
    background_live, background_rows, duration, run_summary, turn_rows, unit_at,
};

struct Thread {
    view: ChatView,
    at: u64,
}

impl Thread {
    fn new() -> Self {
        let mut thread = Self {
            view: ChatView::default(),
            at: 0,
        };
        thread.event(&ChatEvent::SessionReady {
            session: "s".to_owned(),
        });
        thread.event(&ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::User { prompt: 1 },
            at: 0,
        });
        thread
    }

    fn event(&mut self, event: &ChatEvent) {
        self.at += 1000;
        self.view.apply(event).expect("valid event");
    }

    fn start(&mut self, id: &str, parent: Option<&str>, kind: ItemKind, body: ItemBody) {
        let at = self.at;
        self.event(&ChatEvent::ItemStarted {
            item: ItemId::new(id),
            turn: Some(TurnId(1)),
            parent: parent.map(ItemId::new),
            kind,
            body,
            at,
        });
    }

    fn tool(&mut self, id: &str, kind: ItemKind, target: &str) {
        self.start(
            id,
            None,
            kind,
            ItemBody {
                target: Some(target.to_owned()),
                ..ItemBody::default()
            },
        );
    }

    fn end(&mut self, id: &str, status: ItemStatus) {
        let at = self.at;
        self.event(&ChatEvent::ItemEnded {
            item: ItemId::new(id),
            status,
            at,
        });
    }

    fn done(&mut self, id: &str) {
        self.end(id, ItemStatus::Completed);
    }

    fn finish(&mut self, outcome: TurnOutcome) {
        let at = self.at;
        self.event(&ChatEvent::TurnEnded {
            turn: TurnId(1),
            outcome,
            at,
        });
    }

    fn rows(&self) -> crate::chat_rows::TurnRows {
        turn_rows(&self.view, &self.view.turns[0], self.at)
    }

    fn background(&mut self, id: &str, kind: ItemKind) {
        let at = self.at;
        self.event(&ChatEvent::ItemStarted {
            item: ItemId::new(id),
            turn: None,
            parent: None,
            kind,
            body: ItemBody::default(),
            at,
        });
    }

    fn approval(&mut self, request: u64, item: &str) {
        let at = self.at;
        self.event(&ChatEvent::RequestOpened {
            request: RequestId(request),
            turn: Some(TurnId(1)),
            item: Some(ItemId::new(item)),
            ask: Ask::Approval {
                title: "Run?".to_owned(),
                detail: None,
                options: Vec::new(),
            },
            blocking: true,
            at,
        });
    }
}

fn ids(names: &[&str]) -> Vec<ItemId> {
    names.iter().map(|name| ItemId::new(*name)).collect()
}

fn completed() -> TurnOutcome {
    TurnOutcome::Completed {
        stop_reason: "end_turn".to_owned(),
    }
}

#[test]
fn a_run_of_tools_reads_as_one_sentence() {
    let mut thread = Thread::new();
    for (index, file) in ["a.rs", "b.rs", "c.rs"].iter().enumerate() {
        let id = format!("r{index}");
        thread.tool(&id, ItemKind::Read, file);
        thread.done(&id);
    }
    thread.tool("s1", ItemKind::Search, "turn_id");
    thread.done("s1");
    thread.tool("s2", ItemKind::Search, "finish_replay");
    thread.done("s2");
    thread.tool("e1", ItemKind::Edit, "crates/zz-daemon/src/agent/fanout.rs");
    thread.done("e1");
    assert_eq!(
        run_summary(&thread.view, &ids(&["r0", "r1", "r2", "s1", "s2", "e1"])),
        "Edited fanout.rs, read 3 files, searched 2 times"
    );
    assert_eq!(
        run_summary(&thread.view, &ids(&["s1"])),
        "Searched for turn_id"
    );
}

#[test]
fn failures_and_denials_follow_the_sentence() {
    let mut thread = Thread::new();
    thread.tool("x1", ItemKind::Execute, "cargo test");
    thread.end("x1", ItemStatus::Failed { message: None });
    thread.tool("x2", ItemKind::Execute, "rm -rf target");
    thread.end("x2", ItemStatus::Declined);
    assert_eq!(
        run_summary(&thread.view, &ids(&["x1", "x2"])),
        "Ran 2 commands · 1 failed · 1 denied"
    );
}

#[test]
fn the_running_tool_names_the_live_line() {
    let mut thread = Thread::new();
    thread.tool("r1", ItemKind::Read, "a.rs");
    thread.done("r1");
    thread.tool("x1", ItemKind::Execute, "cargo test -p zz-daemon");
    let rows = thread.rows();
    assert_eq!(rows.fold, Some(Fold::Working { ms: 5000 }));
    let Some(Unit::Run(run)) = rows.shown.last() else {
        panic!("a run");
    };
    assert_eq!(
        run.label,
        RunLabel::Live {
            action: "Running",
            detail: "cargo test -p zz-daemon".to_owned()
        }
    );
}

#[test]
fn between_tools_the_live_line_shows_the_reasoning_heading() {
    let mut thread = Thread::new();
    thread.tool("r1", ItemKind::Read, "a.rs");
    thread.done("r1");
    thread.start(
        "t1",
        None,
        ItemKind::Reasoning,
        ItemBody::text("**Checking the turn machine.** Codex reports idle first."),
    );
    let rows = thread.rows();
    let Some(Unit::Run(run)) = rows.shown.last() else {
        panic!("a run");
    };
    assert_eq!(
        run.label,
        RunLabel::Thinking {
            heading: Some("Checking the turn machine".to_owned())
        }
    );
}

#[test]
fn text_breaks_runs_and_a_finished_turn_folds_its_work() {
    let mut thread = Thread::new();
    thread.start("u1", None, ItemKind::User, ItemBody::text("fix it"));
    thread.done("u1");
    thread.tool("r1", ItemKind::Read, "a.rs");
    thread.done("r1");
    thread.start("a1", None, ItemKind::Agent, ItemBody::text("found it"));
    thread.tool("e1", ItemKind::Edit, "a.rs");
    thread.done("e1");
    thread.start("a2", None, ItemKind::Agent, ItemBody::text("fixed"));
    thread.done("a1");
    thread.done("a2");
    thread.finish(completed());
    let rows = thread.rows();
    assert_eq!(rows.fold, Some(Fold::Worked { ms: 12000 }));
    assert_eq!(
        rows.fold.map(Fold::label).as_deref(),
        Some("Worked for 12s")
    );
    assert_eq!(
        rows.shown,
        vec![Unit::User(ItemId::new("u1")), Unit::Text(ItemId::new("a2"))]
    );
    assert_eq!(rows.folded.len(), 3);
    assert!(
        matches!(&rows.folded[0], Unit::Run(run) if run.label == RunLabel::Done { summary: "Read a.rs".to_owned() })
    );
    assert_eq!(rows.folded[1], Unit::Text(ItemId::new("a1")));
}

#[test]
fn a_stopped_turn_says_so() {
    let mut thread = Thread::new();
    thread.tool("x1", ItemKind::Execute, "sleep 100");
    thread.end("x1", ItemStatus::Interrupted);
    thread.start("a1", None, ItemKind::Agent, ItemBody::text("stopped"));
    thread.done("a1");
    thread.finish(TurnOutcome::Cancelled);
    assert_eq!(
        thread.rows().fold.map(Fold::label).as_deref(),
        Some("You stopped after 6s")
    );
}

#[test]
fn an_open_ask_sits_after_its_tool_and_stays_out_of_the_fold() {
    let mut thread = Thread::new();
    thread.tool("x1", ItemKind::Execute, "rm -rf target");
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(1),
        turn: Some(TurnId(1)),
        item: Some(ItemId::new("x1")),
        ask: Ask::Approval {
            title: "Delete?".to_owned(),
            detail: None,
            options: vec![AskOption {
                id: "allow".to_owned(),
                label: "Allow".to_owned(),
                kind: OptionKind::Allow,
            }],
        },
        blocking: true,
        at,
    });
    let rows = thread.rows();
    assert!(matches!(&rows.shown[0], Unit::Run(_)));
    assert_eq!(rows.shown[1], Unit::Ask(RequestId(1)));
}

#[test]
fn an_answered_question_leaves_a_record() {
    let mut thread = Thread::new();
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(2),
        turn: Some(TurnId(1)),
        item: None,
        ask: Ask::Questions {
            questions: Vec::new(),
        },
        blocking: true,
        at,
    });
    thread.event(&ChatEvent::RequestClosed {
        request: RequestId(2),
        close: RequestClose::Answered {
            answer: Answer::Questions {
                answers: Vec::new(),
            },
        },
        at,
    });
    assert_eq!(thread.rows().shown, vec![Unit::Record(RequestId(2))]);
}

#[test]
fn a_question_tool_shows_as_its_record_not_as_a_tool_in_the_run() {
    let mut thread = Thread::new();
    thread.tool("r1", ItemKind::Read, "src/lib.rs");
    thread.tool("q1", ItemKind::Tool, "Which file?");
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(3),
        turn: Some(TurnId(1)),
        item: Some(ItemId::new("q1")),
        ask: Ask::Questions {
            questions: Vec::new(),
        },
        blocking: false,
        at,
    });
    thread.event(&ChatEvent::RequestClosed {
        request: RequestId(3),
        close: RequestClose::Answered {
            answer: Answer::Questions {
                answers: Vec::new(),
            },
        },
        at,
    });
    let rows = thread.rows();
    let Unit::Run(run) = &rows.shown[0] else {
        panic!("{:?}", rows.shown);
    };
    assert_eq!(run.items, ids(&["r1"]));
    assert_eq!(rows.shown[1], Unit::Record(RequestId(3)));
}

#[test]
fn a_subagent_never_joins_a_run_and_opens_its_own_rows() {
    let mut thread = Thread::new();
    thread.tool("r1", ItemKind::Read, "a.rs");
    thread.done("r1");
    thread.start("ag", None, ItemKind::Subagent, ItemBody::titled("Explore"));
    thread.start(
        "c1",
        Some("ag"),
        ItemKind::Search,
        ItemBody {
            target: Some("active_turn".to_owned()),
            ..ItemBody::default()
        },
    );
    let rows = thread.rows();
    assert_eq!(rows.shown.len(), 2);
    assert_eq!(rows.shown[1], Unit::Subagent(ItemId::new("ag")));
    assert_eq!(
        agent_state(&thread.view, &ItemId::new("ag")),
        Some(AgentState::Working {
            label: RunLabel::Live {
                action: "Searching for",
                detail: "active_turn".to_owned()
            }
        })
    );
    thread.done("c1");
    thread.start("c2", Some("ag"), ItemKind::Agent, ItemBody::text("found 6"));
    thread.done("c2");
    thread.done("ag");
    assert_eq!(agent_rows(&thread.view, &ItemId::new("ag")).len(), 2);
    assert_eq!(
        agent_state(&thread.view, &ItemId::new("ag")),
        Some(AgentState::Done {
            result: Some("found 6".to_owned())
        })
    );
}

#[test]
fn a_subagent_waiting_on_an_approval_says_so() {
    let mut thread = Thread::new();
    thread.start("ag", None, ItemKind::Subagent, ItemBody::titled("Replay"));
    thread.start("x1", Some("ag"), ItemKind::Execute, ItemBody::default());
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(3),
        turn: Some(TurnId(1)),
        item: Some(ItemId::new("x1")),
        ask: Ask::Questions {
            questions: Vec::new(),
        },
        blocking: true,
        at,
    });
    assert_eq!(
        agent_state(&thread.view, &ItemId::new("ag")),
        Some(AgentState::Waiting)
    );
}

#[test]
fn durations_read_like_a_clock() {
    assert_eq!(duration(400), "0s");
    assert_eq!(duration(12_000), "12s");
    assert_eq!(duration(72_000), "1m 12s");
    assert_eq!(duration(3_603_000), "1h 0m 3s");
}

#[test]
fn every_block_of_the_final_answer_stays_out_of_the_fold() {
    let mut thread = Thread::new();
    thread.tool("r1", ItemKind::Read, "a.rs");
    thread.done("r1");
    thread.start("a1", None, ItemKind::Agent, ItemBody::text("first block"));
    thread.start("a2", None, ItemKind::Agent, ItemBody::text("second block"));
    thread.done("a1");
    thread.done("a2");
    thread.finish(completed());
    let rows = thread.rows();
    assert_eq!(
        rows.shown,
        vec![Unit::Text(ItemId::new("a1")), Unit::Text(ItemId::new("a2"))]
    );
    assert_eq!(rows.folded.len(), 1);
}

#[test]
fn a_background_subagent_that_outlives_its_turn_stays_visible() {
    let mut thread = Thread::new();
    thread.start("ag", None, ItemKind::Subagent, ItemBody::titled("Watcher"));
    thread.event(&ChatEvent::ItemBackgrounded {
        item: ItemId::new("ag"),
    });
    thread.start(
        "a1",
        None,
        ItemKind::Agent,
        ItemBody::text("started a watcher"),
    );
    thread.done("a1");
    thread.finish(completed());
    let rows = thread.rows();
    assert_eq!(
        rows.shown,
        vec![
            Unit::Subagent(ItemId::new("ag")),
            Unit::Text(ItemId::new("a1"))
        ]
    );
    assert!(rows.folded.is_empty());
}

#[test]
fn a_request_for_an_item_that_has_not_started_gets_its_own_row() {
    let mut thread = Thread::new();
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(4),
        turn: Some(TurnId(1)),
        item: Some(ItemId::new("later")),
        ask: Ask::Questions {
            questions: Vec::new(),
        },
        blocking: true,
        at,
    });
    assert_eq!(thread.rows().shown, vec![Unit::Ask(RequestId(4))]);
}

#[test]
fn a_finished_card_stays_out_of_the_fold_while_work_under_it_runs() {
    let mut thread = Thread::new();
    thread.start("p", None, ItemKind::Subagent, ItemBody::titled("Planner"));
    thread.start("c", Some("p"), ItemKind::Execute, ItemBody::default());
    thread.event(&ChatEvent::ItemBackgrounded {
        item: ItemId::new("c"),
    });
    thread.end("p", ItemStatus::Interrupted);
    thread.start("a1", None, ItemKind::Agent, ItemBody::text("done"));
    thread.done("a1");
    thread.finish(completed());
    let rows = thread.rows();
    assert_eq!(rows.shown[0], Unit::Subagent(ItemId::new("p")));
    assert!(rows.folded.is_empty());
}

#[test]
fn an_approval_on_a_subagent_sits_right_after_its_card() {
    let mut thread = Thread::new();
    thread.start("ag", None, ItemKind::Subagent, ItemBody::titled("Explore"));
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(9),
        turn: Some(TurnId(1)),
        item: Some(ItemId::new("ag")),
        ask: Ask::Questions {
            questions: Vec::new(),
        },
        blocking: true,
        at,
    });
    assert_eq!(
        thread.rows().shown,
        vec![Unit::Subagent(ItemId::new("ag")), Unit::Ask(RequestId(9))]
    );
}

#[test]
fn a_live_request_under_finished_work_keeps_that_work_in_view() {
    let mut thread = Thread::new();
    thread.start("p", None, ItemKind::Subagent, ItemBody::titled("Planner"));
    thread.start("c", Some("p"), ItemKind::Execute, ItemBody::default());
    thread.done("c");
    thread.done("p");
    thread.start("a1", None, ItemKind::Agent, ItemBody::text("done"));
    thread.done("a1");
    thread.finish(completed());
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(10),
        turn: None,
        item: Some(ItemId::new("c")),
        ask: Ask::Questions {
            questions: Vec::new(),
        },
        blocking: true,
        at,
    });
    let rows = thread.rows();
    assert_eq!(rows.shown[0], Unit::Subagent(ItemId::new("p")));
}

#[test]
fn adjacent_subagents_merge_into_one_group() {
    let mut thread = Thread::new();
    thread.start("a", None, ItemKind::Subagent, ItemBody::titled("One"));
    thread.start("b", None, ItemKind::Subagent, ItemBody::titled("Two"));
    thread.start("c", None, ItemKind::Subagent, ItemBody::titled("Three"));
    thread.tool("r1", ItemKind::Read, "a.rs");
    thread.done("r1");
    thread.start("d", None, ItemKind::Subagent, ItemBody::titled("Four"));
    let rows = thread.rows();
    assert_eq!(rows.shown.len(), 3);
    assert_eq!(rows.shown[0], Unit::Agents(ids(&["a", "b", "c"])));
    assert!(matches!(&rows.shown[1], Unit::Run(_)));
    assert_eq!(rows.shown[2], Unit::Subagent(ItemId::new("d")));
}

#[test]
fn a_group_with_one_live_agent_stays_out_of_the_fold() {
    let mut thread = Thread::new();
    thread.start("a", None, ItemKind::Subagent, ItemBody::titled("One"));
    thread.start("b", None, ItemKind::Subagent, ItemBody::titled("Two"));
    thread.done("a");
    thread.event(&ChatEvent::ItemBackgrounded {
        item: ItemId::new("b"),
    });
    thread.start("t1", None, ItemKind::Agent, ItemBody::text("started two"));
    thread.done("t1");
    thread.finish(completed());
    let rows = thread.rows();
    assert_eq!(
        rows.shown,
        vec![
            Unit::Agents(ids(&["a", "b"])),
            Unit::Text(ItemId::new("t1"))
        ]
    );
    assert!(rows.folded.is_empty());
}

#[test]
fn a_finished_group_folds_like_any_other_work() {
    let mut thread = Thread::new();
    thread.start("a", None, ItemKind::Subagent, ItemBody::titled("One"));
    thread.start("b", None, ItemKind::Subagent, ItemBody::titled("Two"));
    thread.done("a");
    thread.done("b");
    thread.start("t1", None, ItemKind::Agent, ItemBody::text("both done"));
    thread.done("t1");
    thread.finish(completed());
    let rows = thread.rows();
    assert_eq!(rows.folded, vec![Unit::Agents(ids(&["a", "b"]))]);
    assert_eq!(rows.shown, vec![Unit::Text(ItemId::new("t1"))]);
}

#[test]
fn an_agent_reports_its_time_and_prompt() {
    let mut thread = Thread::new();
    thread.start(
        "ag",
        None,
        ItemKind::Subagent,
        ItemBody {
            title: "Survey".to_owned(),
            target: Some("Read the reducer".to_owned()),
            ..ItemBody::default()
        },
    );
    let started = thread.at - 1000;
    assert_eq!(
        agent_ms(&thread.view, &ItemId::new("ag"), started + 4500),
        Some(4500)
    );
    assert_eq!(
        agent_prompt(&thread.view, &ItemId::new("ag")),
        Some("Read the reducer")
    );
    thread.at += 2000;
    thread.done("ag");
    assert_eq!(
        agent_ms(&thread.view, &ItemId::new("ag"), u64::MAX),
        Some(3000)
    );
    thread.start("bare", None, ItemKind::Subagent, ItemBody::titled("Bare"));
    assert_eq!(agent_prompt(&thread.view, &ItemId::new("bare")), None);
    assert_eq!(agent_ms(&thread.view, &ItemId::new("bare"), 0), Some(0));
    assert_eq!(agent_ms(&thread.view, &ItemId::new("missing"), 0), None);
}

fn question(id: &str, header: Option<&str>, text: &str) -> AgentQuestion {
    AgentQuestion {
        id: id.to_owned(),
        header: header.map(str::to_owned),
        question: text.to_owned(),
        ..AgentQuestion::default()
    }
}

fn ask_questions(thread: &mut Thread, request: u64) {
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(request),
        turn: Some(TurnId(1)),
        item: None,
        ask: Ask::Questions {
            questions: vec![
                question("q1", Some("Scope"), "Which crates?"),
                question("q2", None, "Run the tests?"),
                question("q3", Some(""), "Anything else?"),
            ],
        },
        blocking: true,
        at,
    });
}

fn close(thread: &mut Thread, request: u64, close: RequestClose) {
    let at = thread.at;
    thread.event(&ChatEvent::RequestClosed {
        request: RequestId(request),
        close,
        at,
    });
}

fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(label, value)| ((*label).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn an_answered_question_card_lists_each_answer() {
    let mut thread = Thread::new();
    ask_questions(&mut thread, 1);
    let request = thread.view.request(RequestId(1)).expect("request");
    assert!(answered(request).is_empty());
    close(
        &mut thread,
        1,
        RequestClose::Answered {
            answer: Answer::Questions {
                answers: vec![
                    QuestionAnswer {
                        question: "q1".to_owned(),
                        answers: vec!["zz-client".to_owned(), "zz-daemon".to_owned()],
                    },
                    QuestionAnswer {
                        question: "Run the tests?".to_owned(),
                        answers: vec!["Yes".to_owned()],
                    },
                ],
            },
        },
    );
    let request = thread.view.request(RequestId(1)).expect("request");
    assert_eq!(
        answered(request),
        pairs(&[
            ("Scope", "zz-client, zz-daemon"),
            ("Run the tests?", "Yes"),
            ("Anything else?", "no answer"),
        ])
    );
}

#[test]
fn a_withdrawn_question_card_has_no_answers() {
    let mut thread = Thread::new();
    ask_questions(&mut thread, 2);
    close(&mut thread, 2, RequestClose::Withdrawn);
    ask_questions(&mut thread, 3);
    close(
        &mut thread,
        3,
        RequestClose::Answered {
            answer: Answer::Cancel,
        },
    );
    let expected = pairs(&[
        ("Scope", "no answer"),
        ("Run the tests?", "no answer"),
        ("Anything else?", "no answer"),
    ]);
    for request in [2, 3] {
        let request = thread.view.request(RequestId(request)).expect("request");
        assert_eq!(answered(request), expected);
    }
    let at = thread.at;
    thread.event(&ChatEvent::RequestOpened {
        request: RequestId(4),
        turn: Some(TurnId(1)),
        item: None,
        ask: Ask::Approval {
            title: "Run?".to_owned(),
            detail: None,
            options: Vec::new(),
        },
        blocking: true,
        at,
    });
    close(&mut thread, 4, RequestClose::Dismissed);
    let request = thread.view.request(RequestId(4)).expect("request");
    assert!(answered(request).is_empty());
}

#[test]
fn a_finished_background_run_rests_and_a_live_one_thinks() {
    let mut thread = Thread::new();
    for id in ["h1", "h2"] {
        thread.background(id, ItemKind::Read);
        thread.done(id);
    }
    assert!(!background_live(&thread.view));
    assert!(matches!(
        &background_rows(&thread.view)[..],
        [Unit::Run(run)] if run.label == RunLabel::Done { summary: "Read 2 files".to_owned() }
    ));
    thread.background("h3", ItemKind::Reasoning);
    assert!(background_live(&thread.view));
    assert!(matches!(
        &background_rows(&thread.view)[..],
        [Unit::Run(run)] if run.label == RunLabel::Thinking { heading: None }
    ));
}

#[test]
fn every_unit_knows_when_it_started() {
    let mut thread = Thread::new();
    let user = thread.at;
    thread.start("u1", None, ItemKind::User, ItemBody::text("go"));
    thread.done("u1");
    let run = thread.at;
    thread.tool("r1", ItemKind::Read, "a.rs");
    thread.done("r1");
    thread.tool("r2", ItemKind::Read, "b.rs");
    thread.done("r2");
    let agents = thread.at;
    thread.start("a", None, ItemKind::Subagent, ItemBody::titled("One"));
    thread.start("b", None, ItemKind::Subagent, ItemBody::titled("Two"));
    let asked = thread.at;
    ask_questions(&mut thread, 7);
    let shown = thread.rows().shown;
    assert_eq!(
        shown,
        vec![
            Unit::User(ItemId::new("u1")),
            Unit::Run(crate::chat_rows::Run {
                items: ids(&["r1", "r2"]),
                label: RunLabel::Done {
                    summary: "Read 2 files".to_owned()
                }
            }),
            Unit::Agents(ids(&["a", "b"])),
            Unit::Ask(RequestId(7)),
        ]
    );
    assert_eq!(
        shown
            .iter()
            .map(|unit| unit_at(&thread.view, unit))
            .collect::<Vec<_>>(),
        [user, run, agents, asked].map(Some)
    );
    for unit in [
        Unit::Text(ItemId::new("missing")),
        Unit::Agents(Vec::new()),
        Unit::Record(RequestId(99)),
    ] {
        assert_eq!(unit_at(&thread.view, &unit), None);
    }
}

#[test]
fn closed_approvals_do_not_split_a_run() {
    let mut thread = Thread::new();
    for (request, id) in [(1, "x1"), (2, "x2"), (3, "x3")] {
        thread.tool(id, ItemKind::Execute, "cargo test");
        thread.approval(request, id);
        close(
            &mut thread,
            request,
            RequestClose::Answered {
                answer: Answer::Option {
                    id: "allow".to_owned(),
                },
            },
        );
        thread.done(id);
    }
    assert!(matches!(
        &thread.rows().shown[..],
        [Unit::Run(run)] if run.items == ids(&["x1", "x2", "x3"])
    ));
}

#[test]
fn a_working_agent_shows_its_run_only_while_the_run_is_last() {
    let mut thread = Thread::new();
    thread.start("ag", None, ItemKind::Subagent, ItemBody::titled("Explore"));
    thread.start("r1", Some("ag"), ItemKind::Read, ItemBody::default());
    thread.done("r1");
    thread.start("t1", Some("ag"), ItemKind::Agent, ItemBody::text("found"));
    let thinking = Some(AgentState::Working {
        label: RunLabel::Thinking { heading: None },
    });
    assert_eq!(agent_state(&thread.view, &ItemId::new("ag")), thinking);
    thread.done("t1");
    thread.start("r2", Some("ag"), ItemKind::Read, ItemBody::default());
    thread.done("r2");
    thread.start(
        "in",
        Some("ag"),
        ItemKind::Subagent,
        ItemBody::titled("Inner"),
    );
    assert_eq!(agent_state(&thread.view, &ItemId::new("ag")), thinking);
}
