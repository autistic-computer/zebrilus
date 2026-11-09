use std::{env, error::Error, fs};

use zz_client::chat_rows::{
    AgentState, RunLabel, Unit, agent_ms, agent_prompt, agent_rows, agent_state, answered,
    background_live, background_rows, duration, live_line, reasoning_body, run_shows, run_summary,
    turn_rows, unit_at,
};
use zz_protocol::{
    agent_stream::{AgentImage, AgentQuestion, AgentQuestionOption},
    chat::{
        Answer, Ask, AskOption, ChatEvent, ChatPhase, ChatView, ItemBody, ItemId, ItemKind,
        ItemStatus, OptionKind, PlanStep, QuestionAnswer, QueuedPrompt, RequestClose, RequestId,
        SessionEnd, SessionView, Skip, StepStatus, TurnBy, TurnId, TurnOutcome, Usage,
    },
};

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';
const ONE: char = '\u{1d}';
const TWO: char = '\u{1c}';
const THREE: char = '\u{1b}';
const FOUR: char = '\u{1a}';

#[derive(Default)]
struct Cases {
    out: Vec<u8>,
    count: u32,
}

impl Cases {
    fn text(&mut self, value: &str) {
        self.out
            .extend_from_slice(&u32::try_from(value.len()).unwrap_or(u32::MAX).to_le_bytes());
        self.out.extend_from_slice(value.as_bytes());
    }

    fn case(&mut self, op: &str, fields: &[&str], expected: &str) {
        self.text(op);
        self.out.push(u8::try_from(fields.len()).unwrap_or(u8::MAX));
        for field in fields {
            self.text(field);
        }
        self.text(expected);
        self.count += 1;
    }
}

struct Random(u64);

impl Random {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % bound
    }

    fn pick<'a, T>(&mut self, pool: &'a [T]) -> &'a T {
        &pool[usize::try_from(self.next(pool.len() as u64)).unwrap_or(0)]
    }

    fn chance(&mut self, of: u64) -> bool {
        self.next(of) == 0
    }
}

const TEXTS: &[&str] = &[
    "",
    "hello",
    "**Planning the work**\n\nFirst I look around.",
    "# Heading\nbody text",
    "## Sub heading.\n\n### Deeper...\nrest of it  ",
    "plain\n**Later heading**\nmore",
    "  \n\t**Bold** trailing",
    "**a** and **b**",
    "####### seven",
    "#nospace",
    "#\ttabbed.",
    "****",
    "**  **",
    "line one\r\nline two\r\n",
    "ünïcödé ✓ text",
    "\u{a0}\u{2003}**Wide space**\u{2003}",
    "**Unclosed heading",
    "# ...",
    "**Only a heading**",
    "# One\r\n## Two\r\nthree\r\n",
    "***",
    "**x**y**",
    "#",
    "###### six\n\n\nafter gaps\n\n",
];

const MORE: &[&str] = &[
    " more",
    "\n\n**Next step**\n",
    "\n# Done.",
    "…",
    "",
    "\nplain tail",
];

const TITLES: &[&str] = &[
    "",
    "Read file",
    "  ",
    "grep -n foo",
    "Explore agent",
    "Web lookup",
];

const TARGETS: &[Option<&str>] = &[
    None,
    None,
    Some(""),
    Some("src/main.rs"),
    Some("a b c"),
    Some("dir/sub/"),
    Some(".."),
    Some("x/.."),
    Some("./y"),
    Some("/"),
    Some("."),
    Some("a//b/./c"),
    Some("näme.rs"),
    Some("https://example.test/a/b?c=d"),
    Some("a/b/."),
    Some("trailing//"),
];

const ACTIVITIES: &[Option<&str>] = &[
    None,
    None,
    None,
    Some(""),
    Some("  "),
    Some(" Compiling crate "),
    Some("Running tests"),
];

const KINDS: &[ItemKind] = &[
    ItemKind::User,
    ItemKind::Agent,
    ItemKind::Agent,
    ItemKind::Reasoning,
    ItemKind::Reasoning,
    ItemKind::Plan,
    ItemKind::Notice,
    ItemKind::Error,
    ItemKind::Subagent,
    ItemKind::Subagent,
    ItemKind::Read,
    ItemKind::Read,
    ItemKind::Edit,
    ItemKind::Edit,
    ItemKind::Search,
    ItemKind::Execute,
    ItemKind::Execute,
    ItemKind::Fetch,
    ItemKind::Web,
    ItemKind::Mcp,
    ItemKind::Tool,
];

const SESSIONS: &[&str] = &["s1", "s1", "s1", "s2", ""];

fn body(random: &mut Random, kind: ItemKind) -> ItemBody {
    let mut body = ItemBody {
        title: (*random.pick(TITLES)).to_owned(),
        target: random.pick(TARGETS).map(str::to_owned),
        text: (*random.pick(TEXTS)).to_owned(),
        activity: random.pick(ACTIVITIES).map(str::to_owned),
        ..ItemBody::default()
    };
    if kind == ItemKind::Plan || random.chance(12) {
        for index in 0..random.next(4) {
            body.steps.push(PlanStep {
                text: format!("step {index}"),
                status: *random.pick(&[
                    StepStatus::Pending,
                    StepStatus::Running,
                    StepStatus::Done,
                    StepStatus::Cancelled,
                ]),
            });
        }
    }
    if random.chance(4) {
        body.exit_code = Some(*random.pick(&[0, 1, -1, 127, i32::MAX, i32::MIN]));
    }
    if random.chance(5) {
        for _ in 0..=random.next(2) {
            let size = random.next(7);
            body.images.push(AgentImage {
                format: (*random.pick(&["png", "jpeg", ""])).to_owned(),
                data: (0..size)
                    .map(|_| u8::try_from(random.next(256)).unwrap_or(0))
                    .collect(),
            });
        }
    }
    body
}

fn options(random: &mut Random) -> Vec<AskOption> {
    (0..random.next(4))
        .map(|index| AskOption {
            id: format!("o{index}"),
            label: (*random.pick(&["Allow", "Always allow", "Deny", ""])).to_owned(),
            kind: *random.pick(&[
                OptionKind::Allow,
                OptionKind::AllowAlways,
                OptionKind::Deny,
                OptionKind::Other,
            ]),
        })
        .collect()
}

fn ask(random: &mut Random) -> Ask {
    match random.next(5) {
        0 | 1 => Ask::Approval {
            title: (*random.pick(TITLES)).to_owned(),
            detail: random.pick(ACTIVITIES).map(str::to_owned),
            options: options(random),
        },
        2 => Ask::Plan {
            markdown: (*random.pick(TEXTS)).to_owned(),
            options: options(random),
        },
        _ => Ask::Questions {
            questions: (0..=random.next(3))
                .map(|index| AgentQuestion {
                    id: format!("q{index}"),
                    header: random
                        .pick(&[None, Some("Scope"), Some("")])
                        .map(str::to_owned),
                    question: (*random.pick(&["Which one?", "", "How far?", "q0"])).to_owned(),
                    options: (0..random.next(3))
                        .map(|choice| AgentQuestionOption {
                            label: format!("choice {choice}"),
                            description: random
                                .pick(&[None, Some("the safe one"), Some("")])
                                .map(str::to_owned),
                        })
                        .collect(),
                    multi_select: random.chance(2),
                    allow_other: random.chance(2),
                    secret: random.chance(5),
                })
                .collect(),
        },
    }
}

fn close(random: &mut Random, ask: &Ask) -> RequestClose {
    match random.next(8) {
        0 => RequestClose::Withdrawn,
        1 => RequestClose::Dismissed,
        2 => RequestClose::Answered {
            answer: Answer::Cancel,
        },
        3 => RequestClose::Answered {
            answer: Answer::Option {
                id: (*random.pick(&["o0", "o1", ""])).to_owned(),
            },
        },
        _ => {
            let mut answers = Vec::new();
            if let Ask::Questions { questions } = ask {
                for question in questions {
                    let key = match random.next(4) {
                        0 => continue,
                        1 => question.question.clone(),
                        _ => question.id.clone(),
                    };
                    answers.push(QuestionAnswer {
                        question: key,
                        answers: (0..random.next(3))
                            .map(|index| (*random.pick(&["yes", "no", "", "maybe"])).to_owned() + &"!".repeat(usize::try_from(index).unwrap_or(0)))
                            .collect(),
                    });
                }
            }
            RequestClose::Answered {
                answer: Answer::Questions { answers },
            }
        }
    }
}

struct Script {
    view: ChatView,
    lines: Vec<String>,
    results: String,
    clock: u64,
    item: u32,
    request: u64,
    turn: u64,
}

impl Script {
    fn new() -> Self {
        Self {
            view: ChatView::default(),
            lines: Vec::new(),
            results: String::new(),
            clock: 1_000,
            item: 0,
            request: 0,
            turn: 0,
        }
    }

    fn feed(&mut self, line: String) {
        self.results
            .push(match serde_json::from_str::<ChatEvent>(&line) {
                Ok(event) => match self.view.apply(&event) {
                    Ok(()) => '.',
                    Err(skip) => skipped(skip),
                },
                Err(_) => '!',
            });
        self.lines.push(line);
    }

    fn push(&mut self, event: &ChatEvent) {
        self.feed(serde_json::to_string(event).unwrap_or_default());
    }

    fn tick(&mut self, random: &mut Random) -> u64 {
        match random.next(10) {
            0 => self.clock = self.clock.saturating_sub(random.next(900)),
            1 | 2 => {}
            _ => self.clock += random.next(70_000),
        }
        self.clock
    }

    fn items(&self, wanted: impl Fn(&zz_protocol::chat::ItemView) -> bool) -> Vec<ItemId> {
        let mut ids = self
            .view
            .items
            .values()
            .filter(|item| wanted(item))
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids
    }

    fn some_item(
        &self,
        random: &mut Random,
        wanted: impl Fn(&zz_protocol::chat::ItemView) -> bool,
    ) -> ItemId {
        let ids = self.items(wanted);
        if ids.is_empty() || random.chance(12) {
            return ItemId::new(*random.pick(&["zz", "i0", "i1"]));
        }
        random.pick(&ids).clone()
    }

    fn turn_id(&self, random: &mut Random) -> TurnId {
        match self.view.running() {
            Some(turn) if !random.chance(10) => turn.id,
            _ => TurnId(random.next(self.turn + 2)),
        }
    }

    fn settle(&mut self, random: &mut Random) {
        let Some(turn) = self.view.running().map(|turn| turn.id) else {
            return;
        };
        let mut open = self
            .view
            .items
            .values()
            .filter(|item| item.status.is_open())
            .map(|item| (item.started_at, item.id.clone()))
            .collect::<Vec<_>>();
        open.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.as_str().len().cmp(&left.1.as_str().len()))
                .then_with(|| right.1.as_str().cmp(left.1.as_str()))
        });
        for (_, item) in open {
            let at = self.tick(random);
            let status = status(random);
            self.push(&ChatEvent::ItemEnded { item, status, at });
        }
        let requests = self
            .view
            .open_requests()
            .map(|request| (request.id, request.ask.clone()))
            .collect::<Vec<_>>();
        for (request, ask) in requests {
            let at = self.tick(random);
            let close = close(random, &ask);
            self.push(&ChatEvent::RequestClosed { request, close, at });
        }
        let at = self.tick(random);
        let outcome = outcome(random);
        self.push(&ChatEvent::TurnEnded { turn, outcome, at });
    }

    fn step(&mut self, random: &mut Random) {
        let roll = random.next(104);
        let at = self.tick(random);
        match roll {
            0..=3 => self.push(&ChatEvent::SessionReady {
                session: (*random.pick(SESSIONS)).to_owned(),
            }),
            4 => self.push(&ChatEvent::SessionEnded {
                end: if random.chance(2) {
                    SessionEnd::Closed
                } else {
                    SessionEnd::Exited {
                        message: "agent exited".to_owned(),
                    }
                },
            }),
            5..=12 => {
                let turn = if random.chance(9) {
                    TurnId(random.next(self.turn + 1))
                } else {
                    self.turn += 1;
                    TurnId(self.turn)
                };
                let by = if random.chance(2) {
                    TurnBy::Agent
                } else {
                    TurnBy::User {
                        prompt: random.next(9),
                    }
                };
                self.push(&ChatEvent::TurnStarted { turn, by, at });
            }
            13 | 14 => {
                let turn = self.turn_id(random);
                self.push(&ChatEvent::TurnClaimed {
                    turn,
                    prompt: random.next(9),
                });
            }
            15 | 16 => {
                let turn = self.turn_id(random);
                self.push(&ChatEvent::TurnStopping { turn });
            }
            17..=19 => {
                let turn = self.turn_id(random);
                let outcome = outcome(random);
                self.push(&ChatEvent::TurnEnded { turn, outcome, at });
            }
            20..=25 => self.settle(random),
            26..=29 => {
                let turn = self.turn_id(random);
                let text = random.pick(ACTIVITIES).map(str::to_owned);
                self.push(&ChatEvent::TurnActivity { turn, text, at });
            }
            30..=55 => {
                let kind = *random.pick(KINDS);
                let item = if random.chance(14) {
                    self.some_item(random, |_| true)
                } else {
                    self.item += 1;
                    ItemId::new(format!("i{}", self.item))
                };
                let parent = (random.next(4) == 0).then(|| {
                    self.some_item(random, |item| {
                        item.status.is_open()
                            && (item.kind == ItemKind::Subagent || item.kind.is_tool())
                    })
                });
                let turn = match self.view.running() {
                    Some(turn) if !random.chance(7) => Some(turn.id),
                    _ if random.chance(3) => Some(TurnId(random.next(self.turn + 2))),
                    _ => None,
                };
                let body = body(random, kind);
                self.push(&ChatEvent::ItemStarted {
                    item,
                    turn,
                    parent,
                    kind,
                    body,
                    at,
                });
            }
            56..=62 => {
                let item = self.some_item(random, |item| item.status.is_open());
                self.push(&ChatEvent::ItemAppended {
                    item,
                    text: (*random.pick(MORE)).to_owned(),
                });
            }
            63..=66 => {
                let item = self.some_item(random, |item| item.status.is_open());
                let kind = self.view.item(&item).map_or(ItemKind::Tool, |item| item.kind);
                let body = body(random, kind);
                self.push(&ChatEvent::ItemChanged { item, body });
            }
            67 | 68 => {
                let item = self.some_item(random, |item| item.status.is_open());
                self.push(&ChatEvent::ItemBackgrounded { item });
            }
            69..=84 => {
                let item = self.some_item(random, |item| item.status.is_open());
                let status = if random.chance(25) {
                    ItemStatus::Running
                } else {
                    status(random)
                };
                self.push(&ChatEvent::ItemEnded { item, status, at });
            }
            85..=88 => {
                let tools = self.items(|item| item.kind.is_tool() || random_kind(item));
                let mut items = tools
                    .into_iter()
                    .filter(|_| random.chance(2))
                    .collect::<Vec<_>>();
                if random.chance(6) {
                    items.push(ItemId::new("nowhere"));
                }
                self.push(&ChatEvent::ItemsSummarized {
                    items,
                    summary: (*random.pick(&["Looked around", "Fixed the build", ""])).to_owned(),
                });
            }
            89..=94 => {
                self.request += 1;
                let request = if random.chance(12) {
                    RequestId(random.next(self.request + 1))
                } else {
                    RequestId(self.request)
                };
                let turn = match self.view.running() {
                    Some(turn) if !random.chance(5) => Some(turn.id),
                    _ if random.chance(4) => Some(TurnId(random.next(self.turn + 2))),
                    _ => None,
                };
                let item = match random.next(5) {
                    0 | 1 => None,
                    2 => Some(ItemId::new(format!("i{}", self.item + 1))),
                    _ => Some(self.some_item(random, |_| true)),
                };
                let ask = ask(random);
                self.push(&ChatEvent::RequestOpened {
                    request,
                    turn,
                    item,
                    ask,
                    blocking: random.chance(2),
                    at,
                });
            }
            95..=98 => {
                let open = self
                    .view
                    .open_requests()
                    .map(|request| (request.id, request.ask.clone()))
                    .collect::<Vec<_>>();
                let (request, ask) = if open.is_empty() || random.chance(8) {
                    (
                        RequestId(random.next(self.request + 2)),
                        Ask::Questions {
                            questions: Vec::new(),
                        },
                    )
                } else {
                    random.pick(&open).clone()
                };
                let close = close(random, &ask);
                self.push(&ChatEvent::RequestClosed { request, close, at });
            }
            99 => self.push(&ChatEvent::QueueChanged {
                prompts: (0..random.next(4))
                    .map(|index| QueuedPrompt {
                        id: index + random.next(5),
                        text: (*random.pick(TEXTS)).to_owned(),
                    })
                    .collect(),
            }),
            100 | 101 => self.push(&ChatEvent::UsageChanged {
                usage: Usage {
                    used: random.next(200_000),
                    size: *random.pick(&[0, 200_000, u64::MAX]),
                },
            }),
            102 => {
                let line = (*random.pick(MALFORMED)).to_owned();
                self.feed(line);
            }
            _ => {
                if random.chance(6) {
                    self.push(&ChatEvent::Restart);
                } else {
                    self.push(&ChatEvent::SessionReady {
                        session: "s1".to_owned(),
                    });
                }
            }
        }
    }
}

fn random_kind(item: &zz_protocol::chat::ItemView) -> bool {
    item.kind == ItemKind::Reasoning && item.started_at % 3 == 0
}

fn status(random: &mut Random) -> ItemStatus {
    match random.next(9) {
        0 => ItemStatus::Failed { message: None },
        1 => ItemStatus::Failed {
            message: Some("exit status 1".to_owned()),
        },
        2 => ItemStatus::Declined,
        3 => ItemStatus::Interrupted,
        _ => ItemStatus::Completed,
    }
}

fn outcome(random: &mut Random) -> TurnOutcome {
    match random.next(8) {
        0 => TurnOutcome::Cancelled,
        1 => TurnOutcome::Failed {
            message: "model overloaded".to_owned(),
        },
        2 => TurnOutcome::Dropped,
        _ => TurnOutcome::Completed {
            stop_reason: "end_turn".to_owned(),
        },
    }
}

const MALFORMED: &[&str] = &[
    "",
    "{",
    "[]",
    "null",
    "{\"event\":\"bogus\"}",
    "{\"event\":7}",
    "{\"session\":\"s1\"}",
    "{\"event\":\"sessionReady\"}",
    "{\"event\":\"sessionReady\",\"session\":null}",
    "{\"event\":\"sessionReady\",\"session\":\"s1\",\"session\":\"s2\"}",
    "{\"event\":\"sessionReady\",\"event\":\"sessionReady\",\"session\":\"s1\"}",
    "{\"session\":\"s1\",\"extra\":[1,{\"a\":null}],\"event\":\"sessionReady\",\"extra\":2}",
    "{\"event\":\"restart\",\"session\":3,\"session\":4}",
    "{\"event\":\"turnStarted\",\"turn\":1.0,\"by\":{\"by\":\"agent\"},\"at\":1}",
    "{\"event\":\"turnStarted\",\"turn\":-1,\"by\":{\"by\":\"agent\"},\"at\":1}",
    "{\"event\":\"turnStarted\",\"turn\":900,\"by\":{\"by\":\"agent\",\"prompt\":\"x\"},\"at\":18446744073709551615}",
    "{\"event\":\"turnStarted\",\"turn\":901,\"by\":{\"by\":\"user\"},\"at\":1}",
    "{\"event\":\"turnStarted\",\"turn\":902,\"by\":{\"by\":\"agent\"},\"at\":18446744073709551616}",
    "{\"event\":\"turnStarted\",\"turn\":903,\"by\":\"agent\",\"at\":1}",
    "{\"event\":\"turnStarted\",\"turn\":\"904\",\"by\":{\"by\":\"agent\"},\"at\":1}",
    "{\"event\":\"turnEnded\",\"turn\":1,\"outcome\":{\"outcome\":\"completed\",\"stopReason\":\"x\"},\"at\":1}",
    "{\"event\":\"turnEnded\",\"turn\":1,\"outcome\":{\"outcome\":\"cancelled\",\"stop_reason\":7},\"at\":1}",
    "{\"event\":\"turnActivity\",\"turn\":1,\"text\":null,\"at\":2}",
    "{\"event\":\"turnActivity\",\"turn\":1,\"text\":5,\"at\":2}",
    "{\"event\":\"itemStarted\",\"item\":\"m1\",\"kind\":\"read\",\"body\":{},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m2\",\"kind\":\"Read\",\"body\":{},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m3\",\"kind\":\"read\",\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m4\",\"turn\":null,\"parent\":null,\"kind\":\"tool\",\"body\":{\"title\":null},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m5\",\"turn\":null,\"parent\":null,\"kind\":\"tool\",\"body\":{\"target\":null,\"exitCode\":null,\"activity\":null,\"exit_code\":\"x\"},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m6\",\"kind\":\"execute\",\"body\":{\"exitCode\":2147483648},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m7\",\"kind\":\"execute\",\"body\":{\"exitCode\":-2147483648},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m8\",\"kind\":\"execute\",\"body\":{\"exitCode\":-2147483649},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m9\",\"kind\":\"execute\",\"body\":{\"exitCode\":1.5},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m10\",\"kind\":\"notice\",\"body\":{\"images\":[{\"format\":\"png\",\"data\":\"AAEC\"}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m11\",\"kind\":\"notice\",\"body\":{\"images\":[{\"format\":\"png\",\"data\":\"AAE\"}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m12\",\"kind\":\"notice\",\"body\":{\"images\":[{\"format\":\"png\",\"data\":\"AAF=\"}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m13\",\"kind\":\"notice\",\"body\":{\"images\":[{\"format\":\"png\",\"data\":\"AAE=\"}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m14\",\"kind\":\"notice\",\"body\":{\"images\":[{\"format\":\"png\",\"data\":\"A-E=\"}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m15\",\"kind\":\"notice\",\"body\":{\"images\":[{\"data\":\"\"}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m16\",\"kind\":\"notice\",\"body\":{\"images\":[{\"format\":\"gif\",\"data\":\"\"}],\"steps\":[{\"text\":\"a\",\"status\":\"done\"}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m17\",\"kind\":\"plan\",\"body\":{\"steps\":[{\"text\":\"a\",\"status\":\"finished\"}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m18\",\"kind\":\"plan\",\"body\":{\"steps\":null},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m19\",\"kind\":\"agent\",\"body\":{\"text\":\"a\\u00e9\\ud83d\\ude00\\n\\t\\\"\\\\\\/\"},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m20\",\"kind\":\"agent\",\"body\":{\"text\":\"\\ud83d\"},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m21\",\"kind\":\"agent\",\"body\":{\"text\":\"\\ude00\"},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m22\",\"kind\":\"agent\",\"body\":{\"text\":\"tab\there\"},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m23\",\"kind\":\"agent\",\"body\":{\"text\":\"\\x41\"},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m24\",\"kind\":\"agent\",\"body\":{\"text\":\"ok\"},\"at\":3,}",
    "{\"event\":\"itemStarted\",\"item\":\"m25\",\"kind\":\"agent\",\"body\":{\"text\":\"ok\"},\"at\":3} x",
    " \t\r{ \"event\" : \"itemStarted\" , \"item\" : \"m26\" , \"kind\" : \"agent\" , \"body\" : { } , \"at\" : 3 } \r",
    "{\"event\":\"itemStarted\",\"item\":\"m27\",\"kind\":\"agent\",\"body\":{},\"at\":03}",
    "{\"event\":\"itemStarted\",\"item\":\"m28\",\"kind\":\"agent\",\"body\":{},\"at\":3e0}",
    "{\"event\":\"itemStarted\",\"item\":\"m29\",\"kind\":\"agent\",\"body\":{},\"at\":-0}",
    "{\"event\":\"itemStarted\",\"item\":\"m30\",\"kind\":\"agent\",\"body\":{\"junk\":1e999},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m31\",\"kind\":\"agent\",\"body\":{\"junk\":[1e308,-1.5E-9,0e999,1e-999,0.000e5]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m32\",\"kind\":\"agent\",\"body\":{\"junk\":[1.,2]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m33\",\"kind\":\"agent\",\"body\":{\"junk\":[.5]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m34\",\"kind\":\"agent\",\"body\":{\"junk\":[1e]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m35\",\"kind\":\"agent\",\"body\":{\"junk\":[+1]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m36\",\"kind\":\"agent\",\"body\":{\"junk\":[tru]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m37\",\"kind\":\"agent\",\"body\":{\"junk\":[true,false,null,{}]},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m38\",\"kind\":\"agent\",\"body\":{\"junk\":123456789012345678901234567890},\"at\":3}",
    "{\"event\":\"itemStarted\",\"item\":\"m39\",\"kind\":\"agent\",\"body\":{\"junk\":2e308},\"at\":3}",
    "{\"event\":\"itemAppended\",\"item\":\"m19\"}",
    "{\"event\":\"itemAppended\",\"item\":\"m19\",\"text\":null}",
    "{\"event\":\"itemEnded\",\"item\":\"m19\",\"status\":{\"status\":\"failed\"},\"at\":4}",
    "{\"event\":\"itemEnded\",\"item\":\"m26\",\"status\":{\"status\":\"failed\",\"message\":null},\"at\":4}",
    "{\"event\":\"itemEnded\",\"item\":\"m1\",\"status\":{\"status\":\"failed\",\"message\":4},\"at\":4}",
    "{\"event\":\"itemEnded\",\"item\":\"m1\",\"status\":{\"status\":\"completed\",\"message\":4},\"at\":4}",
    "{\"event\":\"itemEnded\",\"item\":\"m1\",\"status\":\"completed\",\"at\":4}",
    "{\"event\":\"itemsSummarized\",\"items\":[\"m1\",7],\"summary\":\"x\"}",
    "{\"event\":\"itemsSummarized\",\"items\":[],\"summary\":\"x\"}",
    "{\"event\":\"requestOpened\",\"request\":700,\"ask\":{\"ask\":\"approval\",\"title\":\"t\",\"options\":[]},\"blocking\":false,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":701,\"ask\":{\"ask\":\"approval\",\"title\":\"t\"},\"blocking\":false,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":702,\"ask\":{\"ask\":\"approval\",\"title\":\"t\",\"detail\":null,\"options\":[{\"id\":\"a\",\"label\":\"b\",\"kind\":\"allowAlways\"}]},\"blocking\":1,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":703,\"ask\":{\"ask\":\"approval\",\"title\":\"t\",\"options\":[{\"id\":\"a\",\"label\":\"b\",\"kind\":\"allow_always\"}]},\"blocking\":true,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":704,\"ask\":{\"ask\":\"questions\",\"questions\":[{\"id\":\"a\",\"question\":\"b\"}]},\"blocking\":false,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":705,\"ask\":{\"ask\":\"questions\",\"questions\":[{\"id\":\"a\",\"question\":\"b\",\"header\":null,\"options\":[{\"label\":\"l\",\"description\":null}],\"multiSelect\":true,\"allowOther\":false,\"secret\":true}]},\"blocking\":false,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":706,\"ask\":{\"ask\":\"questions\",\"questions\":[{\"id\":\"a\",\"question\":\"b\",\"multiSelect\":null}]},\"blocking\":false,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":707,\"ask\":{\"ask\":\"questions\",\"questions\":[{\"id\":\"a\",\"question\":\"b\",\"options\":null}]},\"blocking\":false,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":708,\"ask\":{\"ask\":\"questions\",\"questions\":[{\"id\":\"a\",\"question\":\"b\",\"multi_select\":7}]},\"blocking\":false,\"at\":5}",
    "{\"event\":\"requestOpened\",\"request\":709,\"ask\":{\"ask\":\"plan\",\"markdown\":\"# p\",\"options\":[]},\"blocking\":true,\"at\":5,\"turn\":null,\"item\":null}",
    "{\"event\":\"requestOpened\",\"request\":710,\"ask\":{\"ask\":\"plan\",\"options\":[]},\"blocking\":true,\"at\":5}",
    "{\"event\":\"requestClosed\",\"request\":700,\"close\":{\"close\":\"answered\",\"answer\":{\"answer\":\"option\",\"id\":\"a\"}},\"at\":6}",
    "{\"event\":\"requestClosed\",\"request\":704,\"close\":{\"close\":\"answered\",\"answer\":{\"answer\":\"questions\",\"answers\":[{\"question\":\"a\",\"answers\":[\"x\",\"y\"]}]}},\"at\":6}",
    "{\"event\":\"requestClosed\",\"request\":705,\"close\":{\"close\":\"answered\",\"answer\":{\"answer\":\"questions\",\"answers\":[{\"question\":\"a\",\"answers\":[\"x\",7]}]}},\"at\":6}",
    "{\"event\":\"requestClosed\",\"request\":705,\"close\":{\"close\":\"answered\"},\"at\":6}",
    "{\"event\":\"requestClosed\",\"request\":705,\"close\":{\"close\":\"withdrawn\",\"answer\":{\"answer\":\"nope\"}},\"at\":6}",
    "{\"event\":\"requestClosed\",\"request\":709,\"close\":{\"close\":\"answered\",\"answer\":{\"answer\":\"cancel\",\"id\":9}},\"at\":6}",
    "{\"event\":\"queueChanged\",\"prompts\":[{\"id\":1,\"text\":\"a\"},{\"id\":1,\"text\":\"b\",\"more\":{}}]}",
    "{\"event\":\"queueChanged\",\"prompts\":[{\"id\":1}]}",
    "{\"event\":\"queueChanged\",\"prompts\":{}}",
    "{\"event\":\"usageChanged\",\"usage\":{\"used\":1,\"size\":2}}",
    "{\"event\":\"usageChanged\",\"usage\":{\"used\":1}}",
    "{\"event\":\"usageChanged\",\"usage\":{\"used\":1,\"size\":2.5}}",
    "{\"event\":\"sessionEnded\",\"end\":{\"end\":\"exited\"}}",
    "{\"event\":\"sessionEnded\",\"end\":{\"end\":\"closed\",\"message\":5}}",
    "{\"event\":\"sessionEnded\"}",
    "{\"event\":\"sessionReady\",\"session\":\"\u{e9}\u{1f600}\"}",
    "{\"event\":\"sessionReady\",\"session\":\"a\u{7f}b\"}",
    "{\"event\":\"sessionReady\",\"session\":\"a\\u0000b\"}",
    "{\"event\":\"sessionReady\",\"session\":\"a\\uD83D\\uDE00b\\u00E9\"}",
    "{\"event\":\"sessionReady\",\"session\":\"a\\ud83d\\u0041\"}",
    "{\"event\":\"sessionReady\",\"session\":\"a\\u12\"}",
    "{\"event\":\"sessionReady\",\"session\":\"a\\u12G4\"}",
    "{\"event\":\"sessionReady\",\"session\":\"unterminated}",
];

fn skipped(skip: Skip) -> char {
    match skip {
        Skip::TurnStillOpen => 'a',
        Skip::NotRunning => 'b',
        Skip::UnknownTurn => 'c',
        Skip::UnknownItem => 'd',
        Skip::DuplicateItem => 'e',
        Skip::ItemClosed => 'f',
        Skip::ParentClosed => 'g',
        Skip::ChildrenOpen => 'h',
        Skip::StillRunning => 'i',
        Skip::UnknownRequest => 'j',
        Skip::DuplicateRequest => 'k',
        Skip::RequestClosed => 'l',
        Skip::RequestsOpen => 'm',
    }
}

fn some(value: Option<&str>) -> String {
    value.map_or_else(|| "N".to_owned(), |value| format!("S{value}"))
}

fn count(value: Option<u64>) -> String {
    value.map_or_else(|| "N".to_owned(), |value| format!("S{value}"))
}

fn bit(flag: bool) -> String {
    if flag { "1" } else { "0" }.to_owned()
}

fn list(parts: &[String], separator: char) -> String {
    let mut out = parts.len().to_string();
    for part in parts {
        out.push(separator);
        out.push_str(part);
    }
    out
}

fn joined(parts: &[String], separator: char) -> String {
    parts.join(&separator.to_string())
}

fn record(out: &mut String, fields: &[String]) {
    out.push_str(&joined(fields, FIELD));
    out.push(RECORD);
}

fn ids(items: &[ItemId]) -> Vec<String> {
    items.iter().map(|id| id.as_str().to_owned()).collect()
}

fn label(label: &RunLabel) -> String {
    match label {
        RunLabel::Live { action, detail } => format!("l{action}{THREE}{detail}"),
        RunLabel::Thinking { heading } => format!("t{}", some(heading.as_deref())),
        RunLabel::Done { summary } => format!("d{summary}"),
    }
}

fn unit(view: &ChatView, unit: &Unit) -> String {
    let at = count(unit_at(view, unit));
    let fields = match unit {
        Unit::User(id) => vec!["0".to_owned(), id.as_str().to_owned(), at],
        Unit::Text(id) => vec!["1".to_owned(), id.as_str().to_owned(), at],
        Unit::Run(run) => vec![
            "2".to_owned(),
            joined(&ids(&run.items), THREE),
            at,
            label(&run.label),
            run_summary(view, &run.items),
            bit(run_shows(view, &run.items)),
        ],
        Unit::Subagent(id) => vec!["3".to_owned(), id.as_str().to_owned(), at],
        Unit::Agents(items) => vec!["4".to_owned(), joined(&ids(items), THREE), at],
        Unit::Plan(id) => vec!["5".to_owned(), id.as_str().to_owned(), at],
        Unit::Notice(id) => vec!["6".to_owned(), id.as_str().to_owned(), at],
        Unit::Ask(request) => vec!["7".to_owned(), request.0.to_string(), at],
        Unit::Record(request) => vec!["8".to_owned(), request.0.to_string(), at],
    };
    joined(&fields, TWO)
}

fn units(view: &ChatView, units: &[Unit]) -> String {
    list(
        &units.iter().map(|each| unit(view, each)).collect::<Vec<_>>(),
        ONE,
    )
}

fn choices(options: &[AskOption]) -> String {
    list(
        &options
            .iter()
            .map(|option| {
                format!(
                    "{}{THREE}{}{THREE}{}",
                    option.id, option.label, option.kind as u8
                )
            })
            .collect::<Vec<_>>(),
        TWO,
    )
}

fn asked(ask: &Ask) -> String {
    match ask {
        Ask::Approval {
            title,
            detail,
            options,
        } => format!(
            "a{ONE}{title}{ONE}{}{ONE}{}",
            some(detail.as_deref()),
            choices(options)
        ),
        Ask::Plan { markdown, options } => format!("p{ONE}{markdown}{ONE}{}", choices(options)),
        Ask::Questions { questions } => {
            let mut out = "q".to_owned();
            for question in questions {
                out.push(ONE);
                out.push_str(&joined(
                    &[
                        question.id.clone(),
                        some(question.header.as_deref()),
                        question.question.clone(),
                        format!(
                            "{}{}{}",
                            bit(question.multi_select),
                            bit(question.allow_other),
                            bit(question.secret)
                        ),
                        list(
                            &question
                                .options
                                .iter()
                                .map(|option| {
                                    format!(
                                        "{}{FOUR}{}",
                                        option.label,
                                        some(option.description.as_deref())
                                    )
                                })
                                .collect::<Vec<_>>(),
                            THREE,
                        ),
                    ],
                    TWO,
                ));
            }
            out
        }
    }
}

fn closed(close: Option<&RequestClose>) -> String {
    match close {
        None => "N".to_owned(),
        Some(RequestClose::Withdrawn) => "w".to_owned(),
        Some(RequestClose::Dismissed) => "d".to_owned(),
        Some(RequestClose::Answered { answer }) => match answer {
            Answer::Cancel => "c".to_owned(),
            Answer::Option { id } => format!("o{id}"),
            Answer::Questions { answers } => {
                let mut out = "q".to_owned();
                for answer in answers {
                    out.push(ONE);
                    out.push_str(&answer.question);
                    for text in &answer.answers {
                        out.push(TWO);
                        out.push_str(text);
                    }
                }
                out
            }
        },
    }
}

fn state(state: Option<&AgentState>) -> String {
    match state {
        None => "N".to_owned(),
        Some(AgentState::Working { label: run }) => format!("w{}", label(run)),
        Some(AgentState::Waiting) => "W".to_owned(),
        Some(AgentState::Done { result }) => format!("d{}", some(result.as_deref())),
        Some(AgentState::Failed { message }) => format!("f{}", some(message.as_deref())),
        Some(AgentState::Stopped) => "s".to_owned(),
    }
}

fn dump(view: &ChatView, results: &str, now: u64) -> String {
    let mut out = String::new();
    record(&mut out, &["results".to_owned(), results.to_owned()]);
    let (kind, text) = match &view.session {
        SessionView::Starting => (0, ""),
        SessionView::Ready { session } => (1, session.as_str()),
        SessionView::Ended(SessionEnd::Closed) => (2, ""),
        SessionView::Ended(SessionEnd::Exited { message }) => (3, message.as_str()),
    };
    let phase = match view.phase() {
        ChatPhase::Starting => 0,
        ChatPhase::Idle => 1,
        ChatPhase::Running => 2,
        ChatPhase::Waiting => 3,
        ChatPhase::Stopping => 4,
        ChatPhase::Ended => 5,
    };
    record(
        &mut out,
        &[
            "view".to_owned(),
            phase.to_string(),
            kind.to_string(),
            text.to_owned(),
            some(view.last_session.as_deref()),
            view.skipped.to_string(),
            view.usage.map_or_else(
                || "N".to_owned(),
                |usage| format!("S{}/{}", usage.used, usage.size),
            ),
        ],
    );
    record(
        &mut out,
        &[
            "queue".to_owned(),
            list(
                &view
                    .queue
                    .iter()
                    .map(|prompt| format!("{}{TWO}{}", prompt.id, prompt.text))
                    .collect::<Vec<_>>(),
                ONE,
            ),
        ],
    );
    for turn in &view.turns {
        record(
            &mut out,
            &[
                "turn".to_owned(),
                turn.id.0.to_string(),
                match turn.by {
                    TurnBy::Agent => "a".to_owned(),
                    TurnBy::User { prompt } => format!("u{prompt}"),
                },
                turn.started_at.to_string(),
                count(turn.ended_at),
                match &turn.outcome {
                    None => "N".to_owned(),
                    Some(TurnOutcome::Completed { stop_reason }) => format!("c{stop_reason}"),
                    Some(TurnOutcome::Cancelled) => "x".to_owned(),
                    Some(TurnOutcome::Failed { message }) => format!("f{message}"),
                    Some(TurnOutcome::Dropped) => "d".to_owned(),
                },
                bit(turn.stopping),
                list(&ids(&turn.items), ONE),
                turn.activity.as_ref().map_or_else(
                    || "N".to_owned(),
                    |activity| format!("S{}:{}", activity.at, activity.text),
                ),
            ],
        );
    }
    let mut known = view.items.values().collect::<Vec<_>>();
    known.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    for item in &known {
        let (status, message) = match &item.status {
            ItemStatus::Running => (0, None),
            ItemStatus::Completed => (1, None),
            ItemStatus::Failed { message } => (2, message.as_deref()),
            ItemStatus::Declined => (3, None),
            ItemStatus::Interrupted => (4, None),
        };
        record(
            &mut out,
            &[
                "item".to_owned(),
                item.id.as_str().to_owned(),
                (item.kind as u8).to_string(),
                status.to_string(),
                some(message),
                count(item.turn.map(|turn| turn.0)),
                some(item.parent.as_ref().map(ItemId::as_str)),
                list(&ids(&item.children), ONE),
                bit(item.background),
                item.started_at.to_string(),
                count(item.ended_at),
                item.summary.as_ref().map_or_else(
                    || "N".to_owned(),
                    |summary| format!("S{}:{}", summary.batch, summary.text),
                ),
                item.body.title.clone(),
                some(item.body.target.as_deref()),
                item.body.text.clone(),
                list(
                    &item
                        .body
                        .steps
                        .iter()
                        .map(|step| format!("{}{}", step.status as u8, step.text))
                        .collect::<Vec<_>>(),
                    ONE,
                ),
                item.body
                    .exit_code
                    .map_or_else(|| "N".to_owned(), |code| format!("S{code}")),
                list(
                    &item
                        .body
                        .images
                        .iter()
                        .map(|image| {
                            format!(
                                "{}:{}:{}",
                                image.format,
                                image.data.len(),
                                image.data.iter().map(|byte| u64::from(*byte)).sum::<u64>()
                            )
                        })
                        .collect::<Vec<_>>(),
                    ONE,
                ),
                some(item.body.activity.as_deref()),
            ],
        );
    }
    record(
        &mut out,
        &["background".to_owned(), list(&ids(&view.background), ONE)],
    );
    for request in &view.requests {
        record(
            &mut out,
            &[
                "request".to_owned(),
                request.id.0.to_string(),
                count(request.turn.map(|turn| turn.0)),
                some(request.item.as_ref().map(ItemId::as_str)),
                bit(request.blocking),
                request.opened_at.to_string(),
                count(request.closed_at),
                asked(&request.ask),
                closed(request.close.as_ref()),
                list(
                    &answered(request)
                        .iter()
                        .map(|(label, value)| format!("{label}{TWO}{value}"))
                        .collect::<Vec<_>>(),
                    ONE,
                ),
            ],
        );
    }
    for turn in &view.turns {
        let rows = turn_rows(view, turn, now);
        record(
            &mut out,
            &[
                "rows".to_owned(),
                turn.id.0.to_string(),
                units(view, &rows.folded),
                units(view, &rows.shown),
                rows.fold
                    .map_or_else(|| "N".to_owned(), |fold| format!("S{}", fold.label())),
                some(rows.activity.as_deref()),
                some(live_line(view, turn).as_deref()),
            ],
        );
    }
    record(
        &mut out,
        &[
            "bg".to_owned(),
            units(view, &background_rows(view)),
            bit(background_live(view)),
        ],
    );
    for item in &known {
        if item.kind == ItemKind::Subagent {
            record(
                &mut out,
                &[
                    "agent".to_owned(),
                    item.id.as_str().to_owned(),
                    units(view, &agent_rows(view, &item.id)),
                    state(agent_state(view, &item.id).as_ref()),
                    count(agent_ms(view, &item.id, now)),
                    some(agent_prompt(view, &item.id)),
                ],
            );
        }
        if item.kind == ItemKind::Reasoning {
            record(
                &mut out,
                &[
                    "think".to_owned(),
                    item.id.as_str().to_owned(),
                    some(reasoning_body(&item.body.text).as_deref()),
                ],
            );
        }
    }
    out
}

fn emit(cases: &mut Cases, script: &Script, now: u64) {
    cases.case(
        "chat",
        &[
            &script.lines.join("\n"),
            &now.to_string(),
            &script.lines.len().to_string(),
        ],
        &dump(&script.view, &script.results, now),
    );
}

fn random_scripts(cases: &mut Cases) {
    let mut random = Random(0x5eed_c4a7_0123_4567);
    for round in 0..360_u64 {
        let mut script = Script::new();
        if !random.chance(6) {
            script.push(&ChatEvent::SessionReady {
                session: "s1".to_owned(),
            });
        }
        for _ in 0..(4 + random.next(30 + round % 140)) {
            script.step(&mut random);
        }
        let now = script.clock + random.next(4_000_000);
        emit(cases, &script, now);
    }
}

fn started(item: &str, turn: Option<u64>, parent: Option<&str>, kind: ItemKind, body: ItemBody, at: u64) -> ChatEvent {
    ChatEvent::ItemStarted {
        item: ItemId::new(item),
        turn: turn.map(TurnId),
        parent: parent.map(ItemId::new),
        kind,
        body,
        at,
    }
}

fn ended(item: &str, status: ItemStatus, at: u64) -> ChatEvent {
    ChatEvent::ItemEnded {
        item: ItemId::new(item),
        status,
        at,
    }
}

fn targeted(title: &str, target: &str) -> ItemBody {
    ItemBody {
        title: title.to_owned(),
        target: Some(target.to_owned()),
        ..ItemBody::default()
    }
}

fn fixed_scripts(cases: &mut Cases) {
    let mut script = Script::new();
    script.push(&ChatEvent::SessionReady {
        session: "s1".to_owned(),
    });
    script.push(&ChatEvent::TurnStarted {
        turn: TurnId(1),
        by: TurnBy::User { prompt: 4 },
        at: 100,
    });
    script.push(&started("u", Some(1), None, ItemKind::User, ItemBody::text("fix the build"), 100));
    script.push(&ended("u", ItemStatus::Completed, 101));
    script.push(&started("r1", Some(1), None, ItemKind::Reasoning, ItemBody::text("**Looking around**\n\nThe build fails."), 110));
    script.push(&started("t1", Some(1), None, ItemKind::Read, targeted("Read", "crates/zz/src/main.rs"), 120));
    script.push(&started("t2", Some(1), None, ItemKind::Execute, targeted("cargo check", "cargo check"), 130));
    emit(cases, &script, 5_000);
    script.push(&ended("r1", ItemStatus::Completed, 131));
    script.push(&ended("t1", ItemStatus::Completed, 132));
    script.push(&ended("t2", ItemStatus::Failed { message: Some("exit 101".to_owned()) }, 140));
    script.push(&started("t3", Some(1), None, ItemKind::Edit, targeted("Edit", "src/lib.rs"), 150));
    script.push(&ended("t3", ItemStatus::Declined, 160));
    script.push(&started("t4", Some(1), None, ItemKind::Edit, targeted("Edit", "src/other.rs"), 170));
    script.push(&ended("t4", ItemStatus::Completed, 180));
    script.push(&started("r2", Some(1), None, ItemKind::Reasoning, ItemBody::text("# Wrapping up."), 181));
    script.push(&ended("r2", ItemStatus::Completed, 182));
    emit(cases, &script, 5_000);
    script.push(&ChatEvent::ItemsSummarized {
        items: vec![ItemId::new("t1"), ItemId::new("t2")],
        summary: "Checked the build".to_owned(),
    });
    emit(cases, &script, 5_000);
    script.push(&started("a1", Some(1), None, ItemKind::Agent, ItemBody::text("Fixed."), 190));
    script.push(&ended("a1", ItemStatus::Completed, 200));
    script.push(&started("a2", Some(1), None, ItemKind::Agent, ItemBody::text("Anything else?"), 201));
    script.push(&ended("a2", ItemStatus::Completed, 202));
    emit(cases, &script, 5_000);
    script.push(&ChatEvent::TurnEnded {
        turn: TurnId(1),
        outcome: TurnOutcome::Completed {
            stop_reason: "end_turn".to_owned(),
        },
        at: 3_725_300,
    });
    emit(cases, &script, 9_000_000);

    let mut script = Script::new();
    script.push(&ChatEvent::SessionReady {
        session: "s1".to_owned(),
    });
    script.push(&ChatEvent::TurnStarted {
        turn: TurnId(3),
        by: TurnBy::Agent,
        at: 10,
    });
    for (index, name) in ["g1", "g2", "g3"].iter().enumerate() {
        script.push(&started(name, Some(3), None, ItemKind::Subagent, targeted("Explore", "look for the bug"), 20 + index as u64));
    }
    script.push(&started("g1r", None, Some("g1"), ItemKind::Reasoning, ItemBody::text("## Reading."), 30));
    script.push(&started("g1t", None, Some("g1"), ItemKind::Search, targeted("rg", "needle"), 31));
    script.push(&started("g2a", None, Some("g2"), ItemKind::Agent, ItemBody::text("found it"), 32));
    script.push(&ChatEvent::RequestOpened {
        request: RequestId(1),
        turn: Some(TurnId(3)),
        item: Some(ItemId::new("g1t")),
        ask: Ask::Approval {
            title: "Run rg?".to_owned(),
            detail: None,
            options: Vec::new(),
        },
        blocking: true,
        at: 33,
    });
    emit(cases, &script, 90_000);
    script.push(&ChatEvent::ItemBackgrounded {
        item: ItemId::new("g3"),
    });
    script.push(&ended("g2a", ItemStatus::Completed, 40));
    script.push(&ended("g2", ItemStatus::Completed, 41));
    script.push(&ChatEvent::RequestClosed {
        request: RequestId(1),
        close: RequestClose::Dismissed,
        at: 42,
    });
    script.push(&ended("g1t", ItemStatus::Completed, 43));
    script.push(&ended("g1r", ItemStatus::Completed, 44));
    script.push(&ended("g1", ItemStatus::Failed { message: Some("gave up".to_owned()) }, 45));
    script.push(&ChatEvent::RequestOpened {
        request: RequestId(2),
        turn: Some(TurnId(3)),
        item: None,
        ask: Ask::Questions {
            questions: vec![AgentQuestion {
                id: "scope".to_owned(),
                header: Some("Scope".to_owned()),
                question: String::new(),
                options: Vec::new(),
                multi_select: false,
                allow_other: true,
                secret: false,
            }],
        },
        blocking: false,
        at: 46,
    });
    emit(cases, &script, 90_000);
    script.push(&ChatEvent::RequestClosed {
        request: RequestId(2),
        close: RequestClose::Answered {
            answer: Answer::Questions {
                answers: vec![QuestionAnswer {
                    question: "scope".to_owned(),
                    answers: vec!["all".to_owned(), "of it".to_owned()],
                }],
            },
        },
        at: 47,
    });
    script.push(&ChatEvent::TurnEnded {
        turn: TurnId(3),
        outcome: TurnOutcome::Cancelled,
        at: 61_010,
    });
    emit(cases, &script, 90_000);

    let mut script = Script::new();
    script.push(&ChatEvent::SessionReady {
        session: "s1".to_owned(),
    });
    for index in 0..11_u64 {
        let body = ItemBody {
            images: vec![AgentImage {
                format: "png".to_owned(),
                data: vec![u8::try_from(index).unwrap_or(0); 3],
            }],
            ..ItemBody::default()
        };
        script.push(&started(&format!("p{index}"), None, None, ItemKind::Notice, body, index));
    }
    script.push(&ChatEvent::ItemChanged {
        item: ItemId::new("p4"),
        body: ItemBody {
            images: vec![AgentImage {
                format: "jpeg".to_owned(),
                data: vec![9, 9],
            }],
            ..ItemBody::default()
        },
    });
    script.push(&ChatEvent::ItemChanged {
        item: ItemId::new("p5"),
        body: ItemBody::text("no picture"),
    });
    for index in 11..15_u64 {
        let body = ItemBody {
            images: vec![AgentImage {
                format: "png".to_owned(),
                data: vec![1; 2],
            }],
            ..ItemBody::default()
        };
        script.push(&started(&format!("p{index}"), None, None, ItemKind::Notice, body, index));
    }
    emit(cases, &script, 20);
    script.push(&ChatEvent::SessionReady {
        session: "s2".to_owned(),
    });
    emit(cases, &script, 20);

    let mut script = Script::new();
    for line in MALFORMED {
        script.feed((*line).to_owned());
    }
    emit(cases, &script, 50);
    let mut script = Script::new();
    script.feed(format!(
        "{{\"event\":\"sessionReady\",\"session\":\"deep\",\"junk\":{}1{}}}",
        "[".repeat(126),
        "]".repeat(126)
    ));
    script.feed(format!(
        "{{\"event\":\"sessionReady\",\"session\":\"deeper\",\"junk\":{}1{}}}",
        "[".repeat(127),
        "]".repeat(127)
    ));
    script.feed(format!(
        "{{\"event\":\"sessionReady\",\"session\":\"long\",\"junk\":{}}}",
        "9".repeat(400)
    ));
    script.feed(format!(
        "{{\"event\":\"sessionReady\",\"session\":\"long\",\"junk\":0.{}1e400}}",
        "0".repeat(120)
    ));
    script.feed(format!(
        "{{\"event\":\"sessionReady\",\"session\":\"long\",\"junk\":[1e{},0e{},1e-{}]}}",
        "9".repeat(30),
        "9".repeat(30),
        "9".repeat(30)
    ));
    emit(cases, &script, 50);
}

fn pure(cases: &mut Cases) {
    for ms in [
        0, 999, 1_000, 59_999, 60_000, 61_000, 3_599_999, 3_600_000, 3_661_000, 90_061_000,
        u64::MAX,
    ] {
        cases.case("duration", &[&ms.to_string()], &duration(ms));
    }
    for text in TEXTS.iter().chain(MORE) {
        cases.case("reasoning", &[text], &some(reasoning_body(text).as_deref()));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).ok_or("usage: chat_rows_vectors <output>")?;
    let mut cases = Cases::default();
    fixed_scripts(&mut cases);
    random_scripts(&mut cases);
    pure(&mut cases);
    let mut file = cases.count.to_le_bytes().to_vec();
    file.extend_from_slice(&cases.out);
    fs::write(path, file)?;
    Ok(())
}
