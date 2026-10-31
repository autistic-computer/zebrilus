use std::sync::Arc;

use ala::{Image, ImageFormat};
use penna::IconName;
use zz_protocol::chat::{
    Answer, Ask, AskOption, ChatEvent, ChatView, ItemBody, ItemId, ItemKind, ItemStatus,
    OptionKind, PlanStep, QuestionAnswer, RequestClose, RequestId, StepStatus, TurnBy, TurnId,
    TurnOutcome,
};
use zz_protocol::{
    AgentTaskWire, PaneId, SessionId, WindowId,
    agent_stream::{AgentImage, AgentQuestion, AgentQuestionOption},
};
use zz_ui::command::{PaletteHostId, PaletteStatus};
use zz_ui::target::{TargetChat, TargetHost, TargetPane, TargetSession, TargetWindow};

const START: u64 = 1_793_365_200_000;
const STEP: u64 = 1500;

pub struct Script {
    pub view: ChatView,
    pub at: u64,
    turn: u64,
}

impl Script {
    pub fn new() -> Self {
        let mut script = Self {
            view: ChatView::default(),
            at: START,
            turn: 0,
        };
        script.event(&ChatEvent::SessionReady {
            session: "story".to_owned(),
        });
        script
    }

    pub fn event(&mut self, event: &ChatEvent) -> &mut Self {
        self.at += STEP;
        let _ = self.view.apply(event);
        self
    }

    pub fn pause(&mut self, ms: u64) -> &mut Self {
        self.at += ms;
        self
    }

    pub fn turn(&mut self, prompt: &str) -> &mut Self {
        self.turn_with_images(prompt, &[])
    }

    pub fn turn_with_images(&mut self, prompt: &str, images: &[Arc<Image>]) -> &mut Self {
        self.turn += 1;
        let at = self.at;
        self.event(&ChatEvent::TurnStarted {
            turn: TurnId(self.turn),
            by: TurnBy::User { prompt: self.turn },
            at,
        });
        let id = format!("u:story-{}", self.turn);
        let body = ItemBody {
            images: images
                .iter()
                .map(|image| AgentImage {
                    format: image.format.mime_type().to_owned(),
                    data: image.bytes.clone(),
                })
                .collect(),
            ..ItemBody::text(prompt)
        };
        self.start(&id, None, ItemKind::User, body);
        self.done(&id)
    }

    pub fn start(
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

    pub fn text(&mut self, id: &str, text: &str) -> &mut Self {
        self.start(id, None, ItemKind::Agent, ItemBody::text(text))
    }

    pub fn reply(&mut self, id: &str, text: &str) -> &mut Self {
        self.text(id, text).done(id)
    }

    pub fn think(&mut self, id: &str, text: &str) -> &mut Self {
        self.start(id, None, ItemKind::Reasoning, ItemBody::text(text))
    }

    pub fn tool(
        &mut self,
        id: &str,
        parent: Option<&str>,
        kind: ItemKind,
        target: &str,
    ) -> &mut Self {
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

    pub fn ran(&mut self, id: &str, kind: ItemKind, target: &str) -> &mut Self {
        self.tool(id, None, kind, target).done(id)
    }

    pub fn notice(&mut self, id: &str, kind: ItemKind, text: &str) -> &mut Self {
        self.start(id, None, kind, ItemBody::text(text)).done(id)
    }

    pub fn plan(&mut self, id: &str, steps: &[(&str, StepStatus)]) -> &mut Self {
        self.start(
            id,
            None,
            ItemKind::Plan,
            ItemBody {
                steps: steps
                    .iter()
                    .map(|(text, status)| PlanStep {
                        text: (*text).to_owned(),
                        status: *status,
                    })
                    .collect(),
                ..ItemBody::default()
            },
        )
    }

    pub fn done(&mut self, id: &str) -> &mut Self {
        self.end(id, ItemStatus::Completed)
    }

    pub fn end(&mut self, id: &str, status: ItemStatus) -> &mut Self {
        let at = self.at;
        self.event(&ChatEvent::ItemEnded {
            item: ItemId::new(id),
            status,
            at,
        })
    }

    pub fn ask(&mut self, request: u64, item: Option<&str>, ask: Ask) -> &mut Self {
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

    pub fn answer(&mut self, request: u64, answers: &[&str]) -> &mut Self {
        self.close(
            RequestId(request),
            RequestClose::Answered {
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
        )
    }

    pub fn close(&mut self, request: RequestId, close: RequestClose) -> &mut Self {
        let at = self.at;
        self.event(&ChatEvent::RequestClosed { request, close, at })
    }

    pub fn finish(&mut self, outcome: TurnOutcome) -> &mut Self {
        let at = self.at;
        self.event(&ChatEvent::TurnEnded {
            turn: TurnId(self.turn),
            outcome,
            at,
        })
    }

    pub fn background(&mut self, id: &str) -> &mut Self {
        self.event(&ChatEvent::ItemBackgrounded {
            item: ItemId::new(id),
        })
    }
}

pub fn completed() -> TurnOutcome {
    TurnOutcome::Completed {
        stop_reason: "end_turn".to_owned(),
    }
}

fn ask_option(id: &str, label: &str, kind: OptionKind) -> AskOption {
    AskOption {
        id: id.to_owned(),
        label: label.to_owned(),
        kind,
    }
}

pub fn approval(title: &str, allow_similar: &str) -> Ask {
    Ask::Approval {
        title: title.to_owned(),
        detail: None,
        options: vec![
            ask_option("allow", "Allow once", OptionKind::Allow),
            ask_option("always", allow_similar, OptionKind::AllowAlways),
            ask_option("deny", "Deny", OptionKind::Deny),
        ],
    }
}

pub fn lens_approval(title: &str) -> Ask {
    Ask::Approval {
        title: title.to_owned(),
        detail: None,
        options: vec![
            ask_option("allow", "Allow", OptionKind::Allow),
            ask_option("deny", "Deny", OptionKind::Deny),
        ],
    }
}

pub fn palettes() -> Ask {
    let names = [
        "Tokyo Night",
        "Catppuccin",
        "Nord",
        "Gruvbox",
        "Solarized",
        "Rosé Pine",
        "Dracula",
        "One Dark",
        "Kanagawa",
        "Everforest",
    ];
    Ask::Approval {
        title: "Which palette should the storybook use?".to_owned(),
        detail: None,
        options: names
            .iter()
            .enumerate()
            .map(|(index, name)| ask_option(&format!("palette-{index}"), name, OptionKind::Allow))
            .collect(),
    }
}

pub fn terminal_ask(title: &str) -> Ask {
    Ask::Approval {
        title: title.to_owned(),
        detail: None,
        options: Vec::new(),
    }
}

pub fn placement() -> Ask {
    Ask::Questions {
        questions: vec![
            AgentQuestion {
                header: Some("Placement".to_owned()),
                ..question(
                    "q0",
                    "Where should the setting live?",
                    vec![
                        option("Settings › Interface", Some("One value for every host")),
                        option("Per host", Some("Each daemon keeps its own value")),
                        option("Config file only", Some("A key in zz/config, no control")),
                    ],
                )
            },
            question("q1", "How long by default?", Vec::new()),
        ],
    }
}

pub fn agent(title: &str, prompt: &str) -> ItemBody {
    ItemBody {
        target: Some(prompt.to_owned()),
        ..ItemBody::titled(title)
    }
}

pub fn agents_script() -> Script {
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
        .pause(9000)
        .done("ag2")
        .start("ag3", None, ItemKind::Subagent, agent("Replay", "Run the replay tests and report any rejected facts."))
        .tool("c4", Some("ag3"), ItemKind::Execute, "cargo test -p zz-daemon replay_tests")
        .ask(3, Some("c4"), approval("Replay wants to run cargo test -p zz-daemon replay_tests", "Allow cargo test"));
    script
}

pub fn story_hosts() -> Vec<TargetHost> {
    let pane = |id: u64, program: &str| TargetPane {
        id: PaneId(id),
        title: String::new(),
        program: Some(program.to_owned().into()),
        icon: IconName::SquareTerminal,
        chat: None,
    };
    vec![TargetHost {
        id: PaletteHostId(0),
        name: "macbook".to_owned(),
        status: PaletteStatus::Online,
        here: true,
        sessions: vec![TargetSession {
            id: SessionId(0),
            name: "zebrilus".to_owned(),
            windows: vec![TargetWindow {
                id: WindowId(1),
                index: 1,
                name: "build".to_owned(),
                panes: vec![
                    pane(0, "bash"),
                    TargetPane {
                        chat: Some(TargetChat {
                            title: "Fix the fanout replay".into(),
                            icon: zz_ui::program::CLAUDE_ICON,
                        }),
                        ..pane(1, "claude")
                    },
                ],
            }],
        }],
    }]
}

const TAB_STRIP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="300" viewBox="0 0 480 300">
<rect width="480" height="300" rx="14" fill="#1b1d22"/>
<rect width="480" height="46" rx="14" fill="#262931"/>
<rect y="30" width="480" height="16" fill="#262931"/>
<rect x="14" y="10" width="26" height="26" rx="6" fill="#3a3f4b"/>
<rect x="44" y="10" width="10" height="26" rx="3" fill="#3a3f4b"/>
<rect x="58" y="10" width="6" height="26" rx="2" fill="#3a3f4b"/>
<rect x="70" y="8" width="132" height="30" rx="7" fill="#1b1d22"/>
<rect x="82" y="19" width="12" height="8" rx="2" fill="#7aa2f7"/>
<rect x="100" y="20" width="84" height="6" rx="3" fill="#a9b1c6"/>
<rect x="208" y="14" width="110" height="18" rx="5" fill="#30343e"/>
<rect x="324" y="14" width="110" height="18" rx="5" fill="#30343e"/>
<rect x="30" y="76" width="300" height="12" rx="6" fill="#3a3f4b"/>
<rect x="30" y="102" width="420" height="10" rx="5" fill="#2c3039"/>
<rect x="30" y="122" width="390" height="10" rx="5" fill="#2c3039"/>
<rect x="30" y="142" width="410" height="10" rx="5" fill="#2c3039"/>
<rect x="30" y="182" width="420" height="88" rx="10" fill="#22252c"/>
<circle cx="52" cy="23" r="18" fill="none" stroke="#f7768e" stroke-width="3"/>
</svg>"##;

const BAR_CHART: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="420" height="300" viewBox="0 0 420 300">
<rect width="420" height="300" rx="14" fill="#f6f4ef"/>
<rect x="40" y="250" width="340" height="2" fill="#c9c4b8"/>
<rect x="60" y="150" width="40" height="100" rx="4" fill="#9ab4d8"/>
<rect x="120" y="110" width="40" height="140" rx="4" fill="#9ab4d8"/>
<rect x="180" y="170" width="40" height="80" rx="4" fill="#9ab4d8"/>
<rect x="240" y="60" width="40" height="190" rx="4" fill="#e07a5f"/>
<rect x="300" y="130" width="40" height="120" rx="4" fill="#9ab4d8"/>
<rect x="40" y="30" width="160" height="12" rx="6" fill="#4a4740"/>
</svg>"##;

fn svg(source: &str) -> Arc<Image> {
    Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        source.as_bytes().to_vec(),
    ))
}

pub fn tab_strip_screenshot() -> Arc<Image> {
    svg(TAB_STRIP)
}

pub fn chart_screenshot() -> Arc<Image> {
    svg(BAR_CHART)
}

pub const PLAN: &str = "- [x] Find where the tab strip divides its width\n\
- [x] Give pinned tabs a floor at their icon width\n\
- [~] Fix drag reorder offsets across the pinned boundary\n\
- [ ] Run the browser test suite\n\
- [ ] Update the tab strip notes in knowledge/";

pub const PLAN_DONE: &str = "- [x] Find where the tab strip divides its width\n\
- [x] Give pinned tabs a floor at their icon width\n\
- [x] Fix drag reorder offsets across the pinned boundary";

pub const PLAN_STEPS: &[(&str, StepStatus)] = &[
    (
        "Find where the tab strip divides its width",
        StepStatus::Done,
    ),
    (
        "Give pinned tabs a floor at their icon width",
        StepStatus::Done,
    ),
    (
        "Fix drag reorder offsets across the pinned boundary",
        StepStatus::Running,
    ),
    ("Run the browser test suite", StepStatus::Pending),
    (
        "Update the tab strip notes in knowledge/",
        StepStatus::Pending,
    ),
];

pub fn tasks() -> Vec<AgentTaskWire> {
    vec![
        AgentTaskWire {
            id: "task-1".into(),
            kind: "shell".into(),
            description: "cargo watch -x 'test -p zz-ui browser'".into(),
            tool_call_id: Some("call-1".into()),
        },
        AgentTaskWire {
            id: "task-2".into(),
            kind: "agent".into(),
            description: "Survey every caller of TabStrip::layout".into(),
            tool_call_id: Some("call-2".into()),
        },
        AgentTaskWire {
            id: "task-3".into(),
            kind: "monitor".into(),
            description: "Watch target/storybook for a fresh wasm build".into(),
            tool_call_id: None,
        },
    ]
}

pub fn option(label: &str, description: Option<&str>) -> AgentQuestionOption {
    AgentQuestionOption {
        label: label.to_owned(),
        description: description.map(str::to_owned),
    }
}

pub fn question(id: &str, text: &str, options: Vec<AgentQuestionOption>) -> AgentQuestion {
    AgentQuestion {
        id: id.to_owned(),
        header: None,
        question: text.to_owned(),
        options,
        multi_select: false,
        allow_other: true,
        secret: false,
    }
}

pub fn branch_question() -> AgentQuestion {
    question(
        "q0",
        "Which branch should the pull request target?",
        vec![
            option("main", Some("Ships with the next nightly")),
            option("release/0.16", Some("Backport to the current release")),
        ],
    )
}

pub fn checks_question() -> AgentQuestion {
    AgentQuestion {
        multi_select: true,
        ..question(
            "q1",
            "Which checks should run before I push?",
            vec![
                option("cargo test --workspace", None),
                option("cargo clippy", None),
                option("just web build", None),
                option("The storybook screenshots", None),
            ],
        )
    }
}
