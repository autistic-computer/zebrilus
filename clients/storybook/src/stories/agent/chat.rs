use std::rc::Rc;

use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use zz_protocol::agent_stream::{AgentQuestion, AgentQuestionOption};
use zz_protocol::chat::RequestView;
use zz_protocol::chat::{
    Answer, Ask, AskOption, ChatEvent, ChatView, ItemBody, ItemId, ItemKind, ItemStatus,
    OptionKind, PlanStep, QuestionAnswer, RequestClose, RequestId, StepStatus, TurnBy, TurnId,
    TurnOutcome,
};
use zz_ui::agent::chat::{ChatClicks, ChatHooks, ChatOpen, ChatPane, chat_thread};
use zz_ui::agent::presentation::{permission_card, permission_option};
use zz_ui::button::Button;
use zz_ui::v_flex;

use crate::story::{stateless, states};

struct Script {
    view: ChatView,
    at: u64,
    turn: u64,
}

impl Script {
    fn new() -> Self {
        let mut script = Self {
            view: ChatView::default(),
            at: 0,
            turn: 0,
        };
        script.event(&ChatEvent::SessionReady {
            session: "story".to_owned(),
        });
        script
    }

    fn event(&mut self, event: &ChatEvent) -> &mut Self {
        self.at += 1500;
        let _ = self.view.apply(event);
        self
    }

    fn turn(&mut self, prompt: &str) -> &mut Self {
        self.turn += 1;
        let at = self.at;
        self.event(&ChatEvent::TurnStarted {
            turn: TurnId(self.turn),
            by: TurnBy::User { prompt: self.turn },
            at,
        });
        let id = format!("u{}", self.turn);
        self.start(&id, None, ItemKind::User, ItemBody::text(prompt));
        self.done(&id)
    }

    fn start(
        &mut self,
        id: &str,
        parent: Option<&str>,
        kind: ItemKind,
        body: ItemBody,
    ) -> &mut Self {
        let at = self.at;
        let turn = self.view.running().is_some().then_some(TurnId(self.turn));
        self.event(&ChatEvent::ItemStarted {
            item: ItemId::new(id),
            turn,
            parent: parent.map(ItemId::new),
            kind,
            body,
            at,
        })
    }

    fn text(&mut self, id: &str, text: &str) -> &mut Self {
        self.start(id, None, ItemKind::Agent, ItemBody::text(text))
    }

    fn think(&mut self, id: &str, text: &str) -> &mut Self {
        self.start(id, None, ItemKind::Reasoning, ItemBody::text(text))
    }

    fn tool(&mut self, id: &str, parent: Option<&str>, kind: ItemKind, target: &str) -> &mut Self {
        self.start(
            id,
            parent,
            kind,
            ItemBody {
                target: Some(target.to_owned()),
                ..ItemBody::default()
            },
        )
    }

    fn done(&mut self, id: &str) -> &mut Self {
        self.end(id, ItemStatus::Completed)
    }

    fn end(&mut self, id: &str, status: ItemStatus) -> &mut Self {
        let at = self.at;
        self.event(&ChatEvent::ItemEnded {
            item: ItemId::new(id),
            status,
            at,
        })
    }

    fn ask(&mut self, request: u64, item: Option<&str>, ask: Ask) -> &mut Self {
        let at = self.at;
        let turn = item
            .and_then(|item| self.view.item(&ItemId::new(item)))
            .and_then(|item| item.turn.filter(|_| !item.background))
            .or_else(|| self.view.running().map(|turn| turn.id));
        self.event(&ChatEvent::RequestOpened {
            request: RequestId(request),
            turn,
            item: item.map(ItemId::new),
            ask,
            blocking: true,
            at,
        })
    }

    fn answer(&mut self, request: u64, answers: &[&str]) -> &mut Self {
        let at = self.at;
        self.event(&ChatEvent::RequestClosed {
            request: RequestId(request),
            close: RequestClose::Answered {
                answer: Answer::Questions {
                    answers: answers
                        .iter()
                        .enumerate()
                        .map(|(index, answer)| QuestionAnswer {
                            question: format!("q{index}"),
                            answers: vec![(*answer).to_owned()],
                        })
                        .collect(),
                },
            },
            at,
        })
    }

    fn finish(&mut self, outcome: TurnOutcome) -> &mut Self {
        let at = self.at;
        self.event(&ChatEvent::TurnEnded {
            turn: TurnId(self.turn),
            outcome,
            at,
        })
    }

    fn background(&mut self, id: &str) -> &mut Self {
        self.event(&ChatEvent::ItemBackgrounded {
            item: ItemId::new(id),
        })
    }
}

fn completed() -> TurnOutcome {
    TurnOutcome::Completed {
        stop_reason: "end_turn".to_owned(),
    }
}

fn approval(title: &str, allow_similar: &str) -> Ask {
    Ask::Approval {
        title: title.to_owned(),
        detail: None,
        options: vec![
            AskOption {
                id: "allow".to_owned(),
                label: "Allow once".to_owned(),
                kind: OptionKind::Allow,
            },
            AskOption {
                id: "always".to_owned(),
                label: allow_similar.to_owned(),
                kind: OptionKind::AllowAlways,
            },
            AskOption {
                id: "deny".to_owned(),
                label: "Deny".to_owned(),
                kind: OptionKind::Deny,
            },
        ],
    }
}

fn question() -> Ask {
    let option = |label: &str, description: &str| AgentQuestionOption {
        label: label.to_owned(),
        description: Some(description.to_owned()),
    };
    Ask::Questions {
        questions: vec![
            AgentQuestion {
                id: "q0".to_owned(),
                header: Some("Placement".to_owned()),
                question: "Where should the setting live?".to_owned(),
                options: vec![
                    option("Settings › Interface", "One value for every host"),
                    option("Per host", "Each daemon keeps its own value"),
                    option("Config file only", "A key in zz/config, no control"),
                ],
                ..AgentQuestion::default()
            },
            AgentQuestion {
                id: "q1".to_owned(),
                question: "How long by default?".to_owned(),
                ..AgentQuestion::default()
            },
        ],
    }
}

fn frame(element: impl IntoElement) -> AnyElement {
    div()
        .w_full()
        .max_w(px(640.0))
        .child(element)
        .into_any_element()
}

fn spinner_free(view: &ChatView, open: &ChatOpen, now: u64, cx: &App) -> AnyElement {
    frame(chat_thread(
        view,
        now,
        open,
        0.25,
        &ChatHooks::default(),
        cx,
    ))
}

fn working(script: &mut Script) {
    script
        .turn("The chat keeps spinning after Codex finishes a turn. Find out why and fix it.")
        .tool("r1", None, ItemKind::Read, "crates/zz-daemon/src/agent/fanout.rs")
        .done("r1")
        .tool("r2", None, ItemKind::Read, "crates/zz-daemon/src/agent/turn.rs")
        .done("r2")
        .tool("s1", None, ItemKind::Search, "turn_id")
        .done("s1")
        .think(
            "t1",
            "**Checking the turn machine.** Codex reports the thread idle a moment before turn/completed.",
        )
        .text(
            "a1",
            "The spinner comes from the fanout lane: it keeps `turn_id` after a lag replay.",
        )
        .tool("e1", None, ItemKind::Edit, "crates/zz-daemon/src/agent/fanout.rs")
        .done("e1");
}

pub fn live(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let mut running = Script::new();
            working(&mut running);
            running.tool(
                "x1",
                None,
                ItemKind::Execute,
                "cargo test -p zz-daemon agent::fanout",
            );
            let mut thinking = Script::new();
            working(&mut thinking);
            thinking.think(
                "t2",
                "**Reading the replay path.** The lane keeps its cursor.",
            );
            let now = running.at;
            states()
                .state(
                    "running a tool",
                    spinner_free(&running.view, &ChatOpen::default(), now, cx),
                )
                .state(
                    "between tools, the reasoning heading",
                    spinner_free(&thinking.view, &ChatOpen::default(), thinking.at, cx),
                )
                .into_any_element()
        },
        cx,
    )
}

pub fn finished(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let mut script = Script::new();
            working(&mut script);
            script
                .tool("x1", None, ItemKind::Execute, "cargo test -p zz-daemon agent::fanout")
                .done("x1")
                .done("a1")
                .done("t1")
                .text("a2", "Fixed. The fanout lane clears `turn_id` when its turn settles, and a new test covers the replay.\n\n`cargo test -p zz-daemon` passes, 281 tests.")
                .done("a2")
                .finish(completed());
            let opened = ChatOpen {
                turns: vec![TurnId(1)],
                runs: vec![ItemId::new("r1")],
                ..ChatOpen::default()
            };
            states()
                .state(
                    "folded",
                    spinner_free(&script.view, &ChatOpen::default(), script.at, cx),
                )
                .state(
                    "opened, first run expanded",
                    spinner_free(&script.view, &opened, script.at, cx),
                )
                .into_any_element()
        },
        cx,
    )
}

pub fn asks(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let mut approve = Script::new();
            approve
                .turn("Clear the build cache and rerun the terminal benchmarks.")
                .tool("r1", None, ItemKind::Read, "Justfile")
                .done("r1")
                .text(
                    "a1",
                    "The benchmarks read `target/release`. I'll clear it first.",
                )
                .tool("x1", None, ItemKind::Execute, "rm -rf target/release")
                .ask(
                    1,
                    Some("x1"),
                    approval(
                        "Delete target/release and rebuild?",
                        "Allow rm in this chat",
                    ),
                );
            let mut ask = Script::new();
            ask.turn("Add a setting for how long finished chats stay in the sidebar.")
                .tool("r1", None, ItemKind::Read, "crates/zz-ui/src/settings.rs")
                .done("r1")
                .text("a1", "Two choices decide where this lives.")
                .ask(2, None, question());
            let mut answered = Script::new();
            answered
                .turn("Add a setting for how long finished chats stay in the sidebar.")
                .text("a1", "Two choices decide where this lives.")
                .ask(2, None, question())
                .answer(2, &["Settings › Interface", "7 days"])
                .tool("e1", None, ItemKind::Edit, "crates/zz-ui/src/settings.rs");
            states()
                .state(
                    "an approval on the tool that needs it",
                    spinner_free(&approve.view, &ChatOpen::default(), approve.at, cx),
                )
                .state(
                    "a question in the thread",
                    spinner_free(&ask.view, &ChatOpen::default(), ask.at, cx),
                )
                .state(
                    "answered: a record, the turn goes on",
                    spinner_free(&answered.view, &ChatOpen::default(), answered.at, cx),
                )
                .into_any_element()
        },
        cx,
    )
}

fn agent(title: &str, prompt: &str) -> ItemBody {
    ItemBody {
        target: Some(prompt.to_owned()),
        ..ItemBody::titled(title)
    }
}

fn agents_script() -> Script {
    let mut script = Script::new();
    script
        .turn("Find every place that works out agent state on its own. Use subagents.")
        .text("a1", "Splitting it three ways.")
        .start("ag1", None, ItemKind::Subagent, agent("Explore", "Find every place outside the chat machine that decides whether an agent is busy, idle or waiting."))
        .tool("c1", Some("ag1"), ItemKind::Search, "AgentConnectionState")
        .done("c1")
        .tool("c2", Some("ag1"), ItemKind::Read, "crates/zz/src/agent/controller.rs")
        .start("ag2", None, ItemKind::Subagent, agent("Protocol", "List the wire fields clients read to guess agent state."))
        .start(
            "c3",
            Some("ag2"),
            ItemKind::Agent,
            ItemBody::text("Found 6 places: the host phase, the fanout turn id, the desktop submitted flag, the thin reducer, the lens runner and the sidebar status."),
        )
        .done("c3")
        .done("ag2")
        .start("ag3", None, ItemKind::Subagent, agent("Replay", "Run the replay tests and report any rejected facts."))
        .tool("c4", Some("ag3"), ItemKind::Execute, "cargo test -p zz-daemon replay_tests")
        .ask(3, Some("c4"), approval("Replay wants to run cargo test -p zz-daemon replay_tests", "Allow cargo test"));
    script
}

pub fn subagents(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let script = agents_script();
            states()
                .state(
                    "working, done, needs you",
                    spinner_free(&script.view, &ChatOpen::default(), script.at, cx),
                )
                .state(
                    "Explore and Protocol opened in place",
                    spinner_free(
                        &script.view,
                        &ChatOpen {
                            agents: vec![ItemId::new("ag1"), ItemId::new("ag2")],
                            ..ChatOpen::default()
                        },
                        script.at,
                        cx,
                    ),
                )
                .into_any_element()
        },
        cx,
    )
}

pub fn background(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let mut script = Script::new();
            script
                .turn(
                    "Watch tonight's benchmark run in the background and tell me if p99 regresses.",
                )
                .start("w", None, ItemKind::Subagent, ItemBody::titled("Watcher"))
                .background("w")
                .text(
                    "a1",
                    "Started a background agent, Watcher, on the bench window.",
                )
                .done("a1")
                .finish(completed())
                .tool(
                    "w1",
                    Some("w"),
                    ItemKind::Execute,
                    "zz send-keys -t bench:1 'just bench terminal' Enter",
                )
                .ask(
                    4,
                    Some("w1"),
                    approval(
                        "Watcher wants to restart the run that stalled",
                        "Allow zz send-keys",
                    ),
                );
            spinner_free(&script.view, &ChatOpen::default(), script.at, cx)
        },
        cx,
    )
}

pub fn rows(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let mut script = Script::new();
            script.turn("Run the release checklist for v0.24.").start(
                "p1",
                None,
                ItemKind::Plan,
                ItemBody {
                    steps: vec![
                        PlanStep {
                            text: "Read the changelog since v0.23".to_owned(),
                            status: StepStatus::Done,
                        },
                        PlanStep {
                            text: "Bump versions in the workspace".to_owned(),
                            status: StepStatus::Done,
                        },
                        PlanStep {
                            text: "Run the full test suite".to_owned(),
                            status: StepStatus::Running,
                        },
                        PlanStep {
                            text: "Build the signed macOS bundle".to_owned(),
                            status: StepStatus::Pending,
                        },
                    ],
                    ..ItemBody::default()
                },
            );
            script
                .start(
                    "n1",
                    None,
                    ItemKind::Notice,
                    ItemBody::text("Context compacted · 182k → 41k tokens"),
                )
                .done("n1")
                .start(
                    "n2",
                    None,
                    ItemKind::Error,
                    ItemBody::text("The model is overloaded · retry 2 of 5 in 4s"),
                )
                .done("n2")
                .tool("x1", None, ItemKind::Execute, "cargo ci-test");
            spinner_free(&script.view, &ChatOpen::default(), script.at, cx)
        },
        cx,
    )
}

struct Playground {
    script: Script,
    pane: ChatPane,
}

impl Playground {
    fn new() -> Self {
        let mut script = agents_script();
        script.ask(5, None, question());
        Self {
            script,
            pane: ChatPane::default(),
        }
    }

    fn answer(&mut self, request: RequestId, answer: Answer, cx: &mut Context<Self>) {
        let at = self.script.at;
        self.script.event(&ChatEvent::RequestClosed {
            request,
            close: RequestClose::Answered { answer },
            at,
        });
        cx.notify();
    }
}

fn answer_tile(
    me: &Entity<Playground>,
    request: RequestId,
    index: usize,
    label: String,
    danger: bool,
    answer: Answer,
    cx: &App,
) -> AnyElement {
    let me = me.clone();
    permission_option(
        (SharedString::from(format!("try-ask-{}", request.0)), index),
        index,
        label,
        danger,
        false,
        true,
        cx,
    )
    .on_click(move |_, _, cx| {
        me.update(cx, |playground, cx| {
            playground.answer(request, answer.clone(), cx);
        });
    })
    .into_any_element()
}

fn try_ask(me: &Entity<Playground>, request: &RequestView, cx: &App) -> Option<AnyElement> {
    let id = request.id;
    let (title, tiles) = match &request.ask {
        Ask::Approval { title, options, .. }
        | Ask::Plan {
            markdown: title,
            options,
        } => (
            title.clone(),
            options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    answer_tile(
                        me,
                        id,
                        index,
                        option.label.clone(),
                        option.kind == OptionKind::Deny,
                        Answer::Option {
                            id: option.id.clone(),
                        },
                        cx,
                    )
                })
                .collect(),
        ),
        Ask::Questions { questions } => {
            let question = questions.first()?;
            (
                question.question.clone(),
                question
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        let answer = Answer::Questions {
                            answers: questions
                                .iter()
                                .map(|asked| QuestionAnswer {
                                    question: asked.question.clone(),
                                    answers: vec![
                                        asked
                                            .options
                                            .get(index)
                                            .or(asked.options.first())
                                            .map(|option| option.label.clone())
                                            .unwrap_or_default(),
                                    ],
                                })
                                .collect(),
                        };
                        answer_tile(me, id, index, option.label.clone(), false, answer, cx)
                    })
                    .collect(),
            )
        }
    };
    let me = me.clone();
    Some(
        permission_card(
            title,
            None,
            tiles,
            Button::new(SharedString::from(format!("try-dismiss-{}", id.0)))
                .tooltip("Dismiss")
                .on_click(move |_, _, cx| {
                    me.update(cx, |playground, cx| {
                        playground.answer(id, Answer::Cancel, cx);
                    });
                }),
            cx,
        )
        .into_any_element(),
    )
}

impl Render for Playground {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity();
        let click: ChatClicks = {
            let me = me.clone();
            Rc::new(move |click, _, cx| {
                me.update(cx, |playground, cx| {
                    playground.pane.click(click);
                    cx.notify();
                });
            })
        };
        let reset = {
            let me = me.clone();
            move |_: &zz_gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                me.update(cx, |playground, cx| {
                    *playground = Playground::new();
                    cx.notify();
                });
            }
        };
        let ask = |request: &RequestView, cx: &App| try_ask(&me, request, cx);
        let hooks = ChatHooks {
            click: Some(click),
            ask: Some(&ask),
        };
        let body = chat_thread(
            &self.script.view,
            self.script.at,
            &self.pane.open,
            0.25,
            &hooks,
            cx,
        );
        v_flex()
            .id("try-chat")
            .w_full()
            .gap_3()
            .child(Button::new("try-reset").label("Reset").on_click(reset))
            .child(frame(body))
    }
}

pub fn try_it(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Playground::new()).into()
}
