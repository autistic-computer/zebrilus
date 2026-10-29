use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use zz_machine::Machine;
use zz_protocol::chat::{
    Answer, Ask, AskOption, ChatEvent, ChatPhase, ChatView, ItemBody, ItemId, ItemKind, ItemStatus,
    OptionKind, QueuedPrompt, RequestClose, RequestId, SessionEnd, TurnBy, TurnId, TurnOutcome,
};

use super::chat::{ChatMachine, ChatState, Ctx, Effect, Fact, Reject};
use super::turn::{self, End, Outcome};

struct Chat {
    state: ChatState,
    view: ChatView,
    events: Vec<ChatEvent>,
    start: Instant,
    seconds: u64,
}

impl Chat {
    fn new() -> Self {
        Self {
            state: ChatState::default(),
            view: ChatView::default(),
            events: Vec::new(),
            start: Instant::now(),
            seconds: 0,
        }
    }

    fn ready() -> Self {
        let mut chat = Self::new();
        chat.ok(&Fact::SessionReady {
            session: "s1".to_owned(),
        });
        chat
    }

    fn step(&mut self, fact: &Fact) -> Result<Vec<Effect>, Reject> {
        let ctx = Ctx {
            now: self.start + Duration::from_secs(self.seconds),
            at: self.seconds * 1000,
        };
        let step = ChatMachine::step(&mut self.state, fact, &ctx);
        if let Some(reject) = step.rejected {
            assert!(step.effects.is_empty(), "a reject carries no effects");
            return Err(reject);
        }
        let mut effects = Vec::new();
        for effect in step.effects {
            match effect {
                Effect::Emit(event) => {
                    if let Err(skip) = self.view.apply(&event) {
                        panic!("the view skipped {event:?}: {skip:?}");
                    }
                    self.events.push(event);
                }
                other => effects.push(other),
            }
        }
        self.check();
        Ok(effects)
    }

    fn ok(&mut self, fact: &Fact) -> Vec<Effect> {
        match self.step(fact) {
            Ok(effects) => effects,
            Err(reject) => panic!("{fact:?} was rejected: {reject:?}"),
        }
    }

    fn later(&mut self, seconds: u64) {
        self.seconds += seconds;
    }

    fn check(&self) {
        let view_items = self
            .view
            .open_items()
            .map(|item| item.id.clone())
            .collect::<BTreeSet<_>>();
        let machine_items = self.state.open_items().cloned().collect::<BTreeSet<_>>();
        assert_eq!(view_items, machine_items, "open items");
        let view_requests = self
            .view
            .open_requests()
            .map(|request| request.id)
            .collect::<BTreeSet<_>>();
        let machine_requests = self.state.open_requests().collect::<BTreeSet<_>>();
        assert_eq!(view_requests, machine_requests, "open requests");
        assert_eq!(
            self.view.running().map(|turn| turn.id),
            self.state.current(),
            "running turn"
        );
        assert_eq!(self.view.waiting(), self.state.waiting(), "waiting");
        assert_eq!(self.view.queue, self.state.queue(), "queue");
        for item in self.view.open_items() {
            if let Some(turn) = item.turn
                && !item.background
            {
                assert_eq!(
                    self.view.running().map(|running| running.id),
                    Some(turn),
                    "{:?} is open in a turn that ended",
                    item.id
                );
            }
            if let Some(parent) = item
                .parent
                .as_ref()
                .and_then(|parent| self.view.item(parent))
            {
                assert!(
                    parent.status.is_open() || (item.background && !parent.background),
                    "{:?} is open under a closed parent",
                    item.id
                );
            }
        }
        for request in self.view.open_requests() {
            if let Some(turn) = request.turn {
                assert_eq!(self.view.running().map(|running| running.id), Some(turn));
            }
        }
        if self.view.phase() == ChatPhase::Ended {
            assert!(self.view.running().is_none());
            assert!(self.view.open_items().next().is_none());
            assert!(self.view.open_requests().next().is_none());
        }
    }

    fn item(&self, id: &str) -> &zz_protocol::chat::ItemView {
        self.view.item(&ItemId::new(id)).expect("item")
    }

    fn status(&self, id: &str) -> ItemStatus {
        self.item(id).status.clone()
    }

    fn close(&self, request: u64) -> Option<RequestClose> {
        self.view
            .request(RequestId(request))
            .and_then(|view| view.close.clone())
    }

    fn outcome(&self, turn: u64) -> Option<TurnOutcome> {
        self.view
            .turns
            .iter()
            .find(|view| view.id == TurnId(turn))
            .and_then(|view| view.outcome.clone())
    }
}

fn prompt(owner: u64, command: &str) -> Fact {
    Fact::Turn(turn::Fact::Prompt {
        turn: owner,
        command: Some(command.to_owned()),
    })
}

fn started(command: &str) -> Fact {
    Fact::Turn(turn::Fact::Started {
        command: command.to_owned(),
    })
}

fn ended(command: &str, end: End) -> Fact {
    Fact::Turn(turn::Fact::Ended {
        command: command.to_owned(),
        end,
    })
}

fn start(id: &str, parent: Option<&str>, kind: ItemKind) -> Fact {
    Fact::ItemStarted {
        item: ItemId::new(id),
        parent: parent.map(ItemId::new),
        kind,
        body: ItemBody::titled(id),
        background: false,
    }
}

fn start_background(id: &str, kind: ItemKind) -> Fact {
    Fact::ItemStarted {
        item: ItemId::new(id),
        parent: None,
        kind,
        body: ItemBody::titled(id),
        background: true,
    }
}

fn end(id: &str, status: ItemStatus) -> Fact {
    Fact::ItemEnded {
        item: ItemId::new(id),
        status,
    }
}

fn append(id: &str, text: &str) -> Fact {
    Fact::ItemAppended {
        item: ItemId::new(id),
        text: text.to_owned(),
    }
}

fn approval() -> Ask {
    Ask::Approval {
        title: "Run cargo test".to_owned(),
        detail: None,
        options: vec![
            AskOption {
                id: "allow".to_owned(),
                label: "Allow".to_owned(),
                kind: OptionKind::Allow,
            },
            AskOption {
                id: "deny".to_owned(),
                label: "Deny".to_owned(),
                kind: OptionKind::Deny,
            },
        ],
    }
}

fn ask(request: u64, item: Option<&str>) -> Fact {
    Fact::RequestOpened {
        request: RequestId(request),
        item: item.map(ItemId::new),
        ask: approval(),
        blocking: true,
    }
}

fn answer(request: u64) -> Fact {
    Fact::RequestAnswered {
        request: RequestId(request),
        answer: Answer::Option {
            id: "allow".to_owned(),
        },
    }
}

fn queued(id: u64) -> QueuedPrompt {
    QueuedPrompt {
        id,
        text: format!("prompt {id}"),
    }
}

fn running_turn(chat: &mut Chat, owner: u64, command: &str) {
    chat.ok(&prompt(owner, command));
    chat.ok(&started(command));
}

#[test]
fn a_prompt_opens_a_turn_and_its_items_close_with_it() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 7, "c1");
    assert_eq!(
        chat.view.running().map(|turn| turn.by),
        Some(TurnBy::User { prompt: 7 })
    );
    chat.ok(&start("u1", None, ItemKind::User));
    chat.ok(&end("u1", ItemStatus::Completed));
    chat.ok(&start("a1", None, ItemKind::Agent));
    chat.ok(&append("a1", "hello "));
    chat.ok(&append("a1", "there"));
    chat.ok(&start("t1", None, ItemKind::Execute));
    let effects = chat.ok(&ended("c1", End::Completed));
    assert!(effects.contains(&Effect::Settled {
        owner: Some(7),
        outcome: Outcome::Completed {
            stop_reason: "end_turn".to_owned()
        }
    }));
    assert_eq!(chat.item("a1").body.text, "hello there");
    assert_eq!(chat.status("a1"), ItemStatus::Completed);
    assert_eq!(chat.status("t1"), ItemStatus::Interrupted);
    assert_eq!(chat.view.phase(), ChatPhase::Idle);
    assert_eq!(
        chat.outcome(1),
        Some(TurnOutcome::Completed {
            stop_reason: "end_turn".to_owned()
        })
    );
}

#[test]
fn children_end_before_their_parent() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("agent", None, ItemKind::Subagent));
    chat.ok(&start("read", Some("agent"), ItemKind::Read));
    chat.ok(&start("grep", Some("agent"), ItemKind::Search));
    chat.ok(&end("read", ItemStatus::Completed));
    chat.ok(&end("agent", ItemStatus::Completed));
    assert_eq!(chat.status("grep"), ItemStatus::Interrupted);
    assert_eq!(chat.status("read"), ItemStatus::Completed);
    assert_eq!(chat.item("agent").children.len(), 2);
}

#[test]
fn a_child_that_arrives_before_its_subagent_waits_for_it() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("read", Some("agent"), ItemKind::Read));
    chat.ok(&append("read", "src/main.rs"));
    chat.ok(&end("read", ItemStatus::Completed));
    chat.ok(&ask(4, Some("read")));
    assert!(chat.view.item(&ItemId::new("read")).is_none());
    chat.ok(&start("agent", None, ItemKind::Subagent));
    assert_eq!(chat.item("read").parent, Some(ItemId::new("agent")));
    assert_eq!(chat.item("read").body.text, "src/main.rs");
    assert_eq!(chat.status("read"), ItemStatus::Completed);
    assert_eq!(
        chat.view.request(RequestId(4)).map(|r| r.item.clone()),
        Some(Some(ItemId::new("read")))
    );
}

#[test]
fn late_events_for_a_finished_item_are_dropped() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("t1", None, ItemKind::Execute));
    chat.ok(&end("t1", ItemStatus::Completed));
    assert_eq!(
        chat.step(&append("t1", "late")),
        Err(Reject::StaleItem(ItemId::new("t1")))
    );
    assert_eq!(
        chat.step(&end("t1", ItemStatus::Failed { message: None })),
        Err(Reject::StaleItem(ItemId::new("t1")))
    );
    assert_eq!(
        chat.step(&start("t1", None, ItemKind::Execute)),
        Err(Reject::StaleItem(ItemId::new("t1")))
    );
    assert_eq!(chat.status("t1"), ItemStatus::Completed);
}

#[test]
fn a_repeated_start_refreshes_the_item_in_place() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("t1", None, ItemKind::Execute));
    chat.ok(&Fact::ItemStarted {
        item: ItemId::new("t1"),
        parent: None,
        kind: ItemKind::Execute,
        body: ItemBody::titled("cargo test --workspace"),
        background: false,
    });
    assert_eq!(chat.item("t1").body.title, "cargo test --workspace");
    assert_eq!(chat.view.running().map(|turn| turn.items.len()), Some(1));
}

#[test]
fn a_background_subagent_outlives_the_turn_and_keeps_its_approval() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("agent", None, ItemKind::Subagent));
    chat.ok(&Fact::ItemBackgrounded {
        item: ItemId::new("agent"),
    });
    chat.ok(&start("bash", Some("agent"), ItemKind::Execute));
    chat.ok(&ask(9, Some("bash")));
    let effects = chat.ok(&ended("c1", End::Completed));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Withdraw { .. })),
        "the background approval must not be withdrawn"
    );
    assert_eq!(chat.view.phase(), ChatPhase::Waiting);
    assert!(chat.item("agent").status.is_open());
    assert!(chat.item("bash").status.is_open());
    let effects = chat.ok(&answer(9));
    assert!(effects.contains(&Effect::Respond {
        request: RequestId(9),
        answer: Answer::Option {
            id: "allow".to_owned()
        }
    }));
    chat.ok(&end("bash", ItemStatus::Completed));
    chat.ok(&end("agent", ItemStatus::Completed));
    assert_eq!(chat.view.phase(), ChatPhase::Idle);
}

#[test]
fn a_turn_end_dismisses_its_approvals_and_tells_the_driver() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("t1", None, ItemKind::Execute));
    chat.ok(&ask(3, Some("t1")));
    assert_eq!(chat.view.phase(), ChatPhase::Waiting);
    let effects = chat.ok(&ended("c1", End::Failed("overloaded".to_owned())));
    assert!(effects.contains(&Effect::Withdraw {
        requests: vec![RequestId(3)]
    }));
    assert_eq!(chat.close(3), Some(RequestClose::Dismissed));
    assert_eq!(
        chat.outcome(1),
        Some(TurnOutcome::Failed {
            message: "overloaded".to_owned()
        })
    );
}

#[test]
fn answering_a_closed_request_never_reaches_the_agent() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&ask(3, None));
    chat.ok(&answer(3));
    assert_eq!(
        chat.step(&answer(3)),
        Err(Reject::StaleRequest(RequestId(3)))
    );
    assert_eq!(
        chat.step(&answer(99)),
        Err(Reject::UnknownRequest(RequestId(99)))
    );
}

#[test]
fn a_request_resolved_elsewhere_closes_once() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&ask(3, None));
    chat.ok(&Fact::RequestWithdrawn {
        request: RequestId(3),
    });
    chat.ok(&Fact::RequestWithdrawn {
        request: RequestId(3),
    });
    assert_eq!(chat.close(3), Some(RequestClose::Withdrawn));
    assert_eq!(
        chat.step(&answer(3)),
        Err(Reject::StaleRequest(RequestId(3)))
    );
    assert_eq!(
        chat.step(&ask(3, None)),
        Err(Reject::StaleRequest(RequestId(3)))
    );
}

#[test]
fn ending_a_tool_withdraws_the_approval_still_open_on_it() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("agent", None, ItemKind::Subagent));
    chat.ok(&start("edit", Some("agent"), ItemKind::Edit));
    chat.ok(&ask(5, Some("edit")));
    let effects = chat.ok(&end("agent", ItemStatus::Failed { message: None }));
    assert!(effects.contains(&Effect::Withdraw {
        requests: vec![RequestId(5)]
    }));
    assert_eq!(chat.close(5), Some(RequestClose::Withdrawn));
    assert_eq!(chat.status("edit"), ItemStatus::Interrupted);
}

#[test]
fn stop_dismisses_the_turn_questions_and_waits_for_the_agent() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("t1", None, ItemKind::Execute));
    chat.ok(&ask(2, Some("t1")));
    let effects = chat.ok(&Fact::Turn(turn::Fact::Cancel));
    assert!(effects.contains(&Effect::Interrupt));
    assert!(effects.contains(&Effect::Withdraw {
        requests: vec![RequestId(2)]
    }));
    assert_eq!(chat.view.phase(), ChatPhase::Stopping);
    assert!(chat.item("t1").status.is_open());
    chat.ok(&ended("c1", End::Cancelled));
    assert_eq!(chat.outcome(1), Some(TurnOutcome::Cancelled));
    assert_eq!(chat.status("t1"), ItemStatus::Interrupted);
}

#[test]
fn stop_settles_after_the_grace_when_the_agent_never_answers() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&Fact::Turn(turn::Fact::Cancel));
    chat.later(5);
    chat.ok(&Fact::Turn(turn::Fact::Tick));
    assert_eq!(chat.view.phase(), ChatPhase::Stopping);
    chat.later(6);
    chat.ok(&Fact::Turn(turn::Fact::Tick));
    assert_eq!(chat.outcome(1), Some(TurnOutcome::Cancelled));
    assert_eq!(
        chat.step(&ended("c1", End::Completed)),
        Err(Reject::Turn(turn::Reject::Stale("c1".to_owned())))
    );
}

#[test]
fn exit_settles_everything_and_hands_back_the_queue() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&Fact::Submit { prompt: queued(2) });
    chat.ok(&start_background("bg", ItemKind::Execute));
    chat.ok(&start("t1", None, ItemKind::Execute));
    chat.ok(&ask(1, Some("bg")));
    let effects = chat.ok(&Fact::Exited {
        message: "killed".to_owned(),
    });
    assert!(effects.contains(&Effect::Reclaim {
        prompts: vec![queued(2)]
    }));
    assert_eq!(chat.view.phase(), ChatPhase::Ended);
    assert_eq!(chat.status("bg"), ItemStatus::Interrupted);
    assert_eq!(chat.close(1), Some(RequestClose::Dismissed));
    assert!(matches!(chat.outcome(1), Some(TurnOutcome::Failed { .. })));
    assert_eq!(
        chat.step(&start("late", None, ItemKind::Agent)),
        Err(Reject::Offline)
    );
}

#[test]
fn a_prompt_sent_while_running_waits_in_the_queue() {
    let mut chat = Chat::ready();
    let effects = chat.ok(&Fact::Submit { prompt: queued(1) });
    assert_eq!(effects, vec![Effect::Send { prompt: queued(1) }]);
    chat.ok(&Fact::Submit { prompt: queued(2) });
    assert_eq!(chat.view.queue, vec![queued(2)]);
    running_turn(&mut chat, 1, "c1");
    chat.ok(&Fact::Submit { prompt: queued(3) });
    let effects = chat.ok(&ended("c1", End::Completed));
    assert!(effects.contains(&Effect::Send { prompt: queued(2) }));
    assert_eq!(chat.view.queue, vec![queued(3)]);
    running_turn(&mut chat, 2, "c2");
    chat.ok(&ended("c2", End::Cancelled));
    assert!(chat.view.queue.is_empty());
}

#[test]
fn a_cancelled_turn_hands_the_queue_back() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&Fact::Submit { prompt: queued(2) });
    chat.ok(&Fact::Submit { prompt: queued(3) });
    chat.ok(&Fact::Unqueue { prompt: 2 });
    chat.ok(&Fact::Turn(turn::Fact::Cancel));
    let effects = chat.ok(&ended("c1", End::Cancelled));
    assert!(effects.contains(&Effect::Reclaim {
        prompts: vec![queued(3)]
    }));
    assert!(chat.view.queue.is_empty());
}

#[test]
fn a_send_that_fails_goes_back_to_the_composer_and_the_next_one_goes() {
    let mut chat = Chat::ready();
    chat.ok(&Fact::Submit { prompt: queued(1) });
    chat.ok(&Fact::Submit { prompt: queued(2) });
    let effects = chat.ok(&Fact::SendFailed { prompt: 1 });
    assert_eq!(
        effects,
        vec![
            Effect::Reclaim {
                prompts: vec![queued(1)]
            },
            Effect::Send { prompt: queued(2) }
        ]
    );
    assert!(chat.view.queue.is_empty());
    assert_eq!(
        chat.step(&Fact::SendFailed { prompt: 1 }),
        Err(Reject::UnknownPrompt(1))
    );
}

#[test]
fn a_non_blocking_question_does_not_make_the_chat_wait() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&Fact::RequestOpened {
        request: RequestId(1),
        item: None,
        ask: approval(),
        blocking: false,
    });
    assert_eq!(chat.view.phase(), ChatPhase::Running);
    chat.ok(&ask(2, None));
    assert_eq!(chat.view.phase(), ChatPhase::Waiting);
}

#[test]
fn a_turn_the_agent_starts_gets_its_own_turn() {
    let mut chat = Chat::ready();
    chat.ok(&Fact::Turn(turn::Fact::Busy));
    assert_eq!(chat.view.running().map(|turn| turn.by), Some(TurnBy::Agent));
    chat.ok(&start("a1", None, ItemKind::Agent));
    let effects = chat.ok(&Fact::Turn(turn::Fact::Idle));
    assert!(effects.contains(&Effect::Settled {
        owner: None,
        outcome: Outcome::Completed {
            stop_reason: "end_turn".to_owned()
        }
    }));
    assert_eq!(chat.status("a1"), ItemStatus::Completed);
}

#[test]
fn output_while_idle_goes_to_the_background_not_a_new_turn() {
    let mut chat = Chat::ready();
    chat.ok(&start("out", None, ItemKind::Execute));
    assert!(chat.view.running().is_none());
    assert!(chat.item("out").background);
    assert_eq!(chat.view.background, vec![ItemId::new("out")]);
}

#[test]
fn backgrounding_moves_the_whole_subtree_out_of_the_turn() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("agent", None, ItemKind::Subagent));
    chat.ok(&start("read", Some("agent"), ItemKind::Read));
    chat.ok(&start("t1", None, ItemKind::Execute));
    chat.ok(&Fact::ItemBackgrounded {
        item: ItemId::new("agent"),
    });
    chat.ok(&ended("c1", End::Completed));
    assert!(chat.item("agent").status.is_open());
    assert!(chat.item("read").status.is_open());
    assert_eq!(chat.status("t1"), ItemStatus::Interrupted);
    chat.ok(&start("grep", Some("agent"), ItemKind::Search));
    assert_eq!(chat.item("grep").turn, None);
}

#[test]
fn a_session_reset_closes_everything_and_turn_ids_keep_counting() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("t1", None, ItemKind::Execute));
    chat.ok(&Fact::Submit { prompt: queued(5) });
    chat.ok(&Fact::SessionClosed);
    assert_eq!(chat.outcome(1), Some(TurnOutcome::Dropped));
    assert_eq!(chat.view.phase(), ChatPhase::Ended);
    assert_eq!(chat.view.queue, vec![queued(5)]);
    let effects = chat.ok(&Fact::SessionReady {
        session: "s2".to_owned(),
    });
    assert_eq!(effects, vec![Effect::Send { prompt: queued(5) }]);
    running_turn(&mut chat, 5, "c5");
    assert_eq!(chat.view.running().map(|turn| turn.id), Some(TurnId(2)));
}

#[test]
fn a_steered_command_keeps_the_turn_running() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&Fact::Turn(turn::Fact::Steer {
        command: "c2".to_owned(),
    }));
    chat.ok(&started("c2"));
    chat.ok(&ended("c1", End::Cancelled));
    assert_eq!(chat.view.phase(), ChatPhase::Running);
    chat.ok(&ended("c2", End::Completed));
    assert!(matches!(
        chat.outcome(1),
        Some(TurnOutcome::Completed { .. })
    ));
}

#[test]
fn a_stale_end_from_a_settled_turn_does_not_touch_the_next() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&ended("c1", End::Completed));
    running_turn(&mut chat, 2, "c2");
    assert_eq!(
        chat.step(&ended("c1", End::Failed("late".to_owned()))),
        Err(Reject::Turn(turn::Reject::Stale("c1".to_owned())))
    );
    assert_eq!(chat.view.phase(), ChatPhase::Running);
}

#[test]
fn a_dead_session_reports_why_even_before_it_was_ready() {
    let mut chat = Chat::new();
    chat.ok(&Fact::Exited {
        message: "claude: command not found".to_owned(),
    });
    assert_eq!(
        chat.view.session,
        zz_protocol::chat::SessionView::Ended(SessionEnd::Exited {
            message: "claude: command not found".to_owned()
        })
    );
    chat.ok(&Fact::Exited {
        message: "again".to_owned(),
    });
}

#[test]
fn turn_facts_that_belong_to_the_session_are_refused() {
    let mut chat = Chat::ready();
    assert_eq!(
        chat.step(&Fact::Turn(turn::Fact::SessionReady)),
        Err(Reject::Misrouted)
    );
}

#[test]
fn the_events_rebuild_the_same_view_after_a_wire_round_trip() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("agent", None, ItemKind::Subagent));
    chat.ok(&start("read", Some("agent"), ItemKind::Read));
    chat.ok(&ask(1, Some("read")));
    chat.ok(&answer(1));
    chat.ok(&Fact::Submit { prompt: queued(2) });
    chat.ok(&ended("c1", End::Completed));
    let mut replayed = ChatView::default();
    for event in &chat.events {
        let json = serde_json::to_string(event).expect("encode");
        let decoded = serde_json::from_str::<ChatEvent>(&json).expect("decode");
        replayed.apply(&decoded).expect("apply");
    }
    assert_eq!(replayed, chat.view);
}

struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

const KINDS: [ItemKind; 6] = [
    ItemKind::Agent,
    ItemKind::Reasoning,
    ItemKind::Execute,
    ItemKind::Read,
    ItemKind::Subagent,
    ItemKind::Edit,
];

fn random_fact(dice: &mut Dice, owner: &mut u64) -> Fact {
    let item = |dice: &mut Dice| ItemId::new(format!("i{}", dice.below(24)));
    let command = |dice: &mut Dice| format!("c{}", dice.below(6));
    match dice.below(26) {
        0 => {
            *owner += 1;
            prompt(*owner, &command(dice))
        }
        1 => Fact::Turn(turn::Fact::Prompt {
            turn: {
                *owner += 1;
                *owner
            },
            command: None,
        }),
        2 => Fact::Turn(turn::Fact::Accepted {
            turn: *owner,
            command: command(dice),
        }),
        3 => started(&command(dice)),
        4 => ended(
            &command(dice),
            match dice.below(3) {
                0 => End::Completed,
                1 => End::Cancelled,
                _ => End::Failed("boom".to_owned()),
            },
        ),
        5 => Fact::Turn(turn::Fact::Steer {
            command: command(dice),
        }),
        6 => Fact::Turn(turn::Fact::Cancel),
        7 => Fact::Turn(turn::Fact::Busy),
        8 => Fact::Turn(turn::Fact::Idle),
        9 => Fact::Turn(turn::Fact::Tick),
        10 => Fact::Turn(turn::Fact::Reported {
            command: Some(command(dice)),
            stop_reason: Some("end_turn".to_owned()),
            error: None,
        }),
        11..=14 => Fact::ItemStarted {
            item: item(dice),
            parent: dice.chance(45).then(|| item(dice)),
            kind: KINDS[usize::try_from(dice.below(6)).unwrap_or(0)],
            body: ItemBody::titled("x"),
            background: dice.chance(15),
        },
        15 => Fact::ItemAppended {
            item: item(dice),
            text: "t".to_owned(),
        },
        16 | 17 => Fact::ItemEnded {
            item: item(dice),
            status: match dice.below(4) {
                0 => ItemStatus::Failed { message: None },
                1 => ItemStatus::Declined,
                _ => ItemStatus::Completed,
            },
        },
        18 => Fact::ItemBackgrounded { item: item(dice) },
        19 => Fact::RequestOpened {
            request: RequestId(dice.below(10)),
            item: dice.chance(60).then(|| item(dice)),
            ask: approval(),
            blocking: dice.chance(85),
        },
        20 => answer(dice.below(10)),
        21 => Fact::RequestWithdrawn {
            request: RequestId(dice.below(10)),
        },
        22 => Fact::Submit {
            prompt: queued(1000 + dice.below(1000)),
        },
        23 => Fact::Unqueue {
            prompt: 1000 + dice.below(1000),
        },
        24 => match dice.below(10) {
            0 => Fact::Exited {
                message: "gone".to_owned(),
            },
            1 => Fact::SessionClosed,
            _ => Fact::SessionReady {
                session: "s".to_owned(),
            },
        },
        _ => Fact::Usage {
            usage: zz_protocol::chat::Usage { used: 1, size: 2 },
        },
    }
}

#[test]
fn random_facts_keep_the_view_and_the_machine_in_step() {
    for seed in 1..=40_u64 {
        let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut chat = Chat::ready();
        let mut owner = 0;
        for _ in 0..3000 {
            if dice.chance(10) {
                chat.later(dice.below(12));
            }
            let fact = random_fact(&mut dice, &mut owner);
            let Ok(mut effects) = chat.step(&fact) else {
                continue;
            };
            while let Some(effect) = effects.pop() {
                if let Effect::Send { prompt } = effect {
                    assert!(chat.view.running().is_none(), "sent into a running turn");
                    let sent = !dice.chance(20)
                        && chat
                            .step(&Fact::Turn(turn::Fact::Prompt {
                                turn: prompt.id,
                                command: Some(format!("p{}", prompt.id)),
                            }))
                            .is_ok();
                    if !sent {
                        effects.extend(chat.ok(&Fact::SendFailed { prompt: prompt.id }));
                    }
                }
            }
            assert!(chat.state.sending().is_none(), "a prompt stuck in flight");
        }
        let mut replayed = ChatView::default();
        for event in &chat.events {
            replayed.apply(event).expect("replay");
        }
        assert_eq!(replayed, chat.view, "seed {seed}");
        let mut log = zz_protocol::chat::ChatLog::default();
        for event in &chat.events {
            log.push(event);
        }
        let mut compacted = ChatView::default();
        compacted.apply(&ChatEvent::Restart).expect("restart");
        for event in log.events() {
            compacted.apply(event).expect("compacted replay");
        }
        assert_eq!(
            compacted, chat.view,
            "seed {seed}: the compacted log rebuilds the view"
        );
    }
}

#[test]
fn a_background_child_outlives_its_foreground_parent_and_keeps_its_approval() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("parent", None, ItemKind::Subagent));
    chat.ok(&start("child", Some("parent"), ItemKind::Execute));
    chat.ok(&Fact::ItemBackgrounded {
        item: ItemId::new("child"),
    });
    chat.ok(&ask(7, Some("child")));
    let effects = chat.ok(&ended("c1", End::Completed));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Withdraw { .. })),
        "{effects:?}"
    );
    assert_eq!(chat.status("parent"), ItemStatus::Interrupted);
    assert!(chat.item("child").status.is_open());
    assert_eq!(chat.close(7), None);
    assert_eq!(chat.view.phase(), ChatPhase::Waiting);
}

#[test]
fn a_request_for_an_item_still_on_its_way_shows_at_once_and_stays_resolved() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("child", Some("agent"), ItemKind::Execute));
    chat.ok(&ask(8, Some("child")));
    assert_eq!(chat.view.phase(), ChatPhase::Waiting);
    chat.ok(&Fact::RequestWithdrawn {
        request: RequestId(8),
    });
    chat.ok(&start("agent", None, ItemKind::Subagent));
    assert!(chat.item("child").status.is_open());
    assert_eq!(chat.close(8), Some(RequestClose::Withdrawn));
    assert_eq!(
        chat.step(&answer(8)),
        Err(Reject::StaleRequest(RequestId(8)))
    );
}

#[test]
fn a_request_follows_its_item_into_the_background_once_the_item_starts() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&ask(9, Some("late")));
    chat.ok(&start_background("late", ItemKind::Execute));
    let effects = chat.ok(&ended("c1", End::Completed));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Withdraw { .. }))
    );
    assert_eq!(chat.close(9), None);
    assert_eq!(chat.view.request(RequestId(9)).and_then(|r| r.turn), None);
}

#[test]
fn a_prompt_that_takes_over_an_agent_turn_owns_it_in_the_view() {
    let mut chat = Chat::ready();
    chat.ok(&Fact::Turn(turn::Fact::Busy));
    assert_eq!(chat.view.running().map(|turn| turn.by), Some(TurnBy::Agent));
    chat.ok(&prompt(5, "c5"));
    assert_eq!(
        chat.view.running().map(|turn| turn.by),
        Some(TurnBy::User { prompt: 5 })
    );
}

#[test]
fn a_finished_item_stays_stale_after_hundreds_of_later_items() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    for index in 0..700 {
        let id = format!("t{index}");
        chat.ok(&start(&id, None, ItemKind::Read));
        chat.ok(&end(&id, ItemStatus::Completed));
    }
    assert_eq!(
        chat.step(&start("t0", None, ItemKind::Read)),
        Err(Reject::StaleItem(ItemId::new("t0")))
    );
}

#[test]
fn a_request_on_a_backgrounded_child_still_waiting_for_its_parent_survives_the_turn() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&start("child", Some("agent"), ItemKind::Execute));
    chat.ok(&Fact::ItemBackgrounded {
        item: ItemId::new("child"),
    });
    chat.ok(&ask(6, Some("child")));
    let effects = chat.ok(&ended("c1", End::Completed));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Withdraw { .. })),
        "{effects:?}"
    );
    assert_eq!(chat.close(6), None);
}

#[test]
fn an_approval_that_names_an_item_nobody_has_seen_yet_does_not_die_with_the_turn() {
    let mut chat = Chat::ready();
    running_turn(&mut chat, 1, "c1");
    chat.ok(&ask(11, Some("child")));
    chat.ok(&start("child", Some("agent"), ItemKind::Execute));
    chat.ok(&Fact::ItemBackgrounded {
        item: ItemId::new("child"),
    });
    let effects = chat.ok(&ended("c1", End::Completed));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Withdraw { .. })),
        "{effects:?}"
    );
    assert_eq!(chat.close(11), None);
}
