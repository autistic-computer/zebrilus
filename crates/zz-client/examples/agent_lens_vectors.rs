use std::{env, error::Error, fs};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;
use zz_client::{
    agent_completion::AgentCommand,
    agent_lens::{
        AGENT_WELCOME_DETAIL, AgentCursor, AgentEmpty, LensTranscript, PendingPermission,
        accepts_prompt, agent_empty, agent_placeholder, agent_welcome_title, conversation_changed,
        pending_permissions, restore_reclaim_id, restored_prompts, track_usage, usage_from_update,
    },
};
use zz_protocol::{
    AgentConnectionPhase, AgentImage, AgentPaneWire, AgentPermissionWire, AgentProvider,
    ClientInstanceId,
    agent_stream::{AgentQuestion, AgentQuestionOption},
    chat::{
        Answer, Ask, AskOption, ChatEvent, ChatPhase, ChatView, ItemBody, ItemId, ItemKind,
        ItemStatus, OptionKind, QueuedPrompt, RequestClose, RequestId, TurnBy, TurnId, TurnOutcome,
        Usage,
    },
};

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';
const ONE: char = '\u{1d}';
const TWO: char = '\u{1c}';
const THREE: char = '\u{1b}';
const FOUR: char = '\u{1a}';
const FIVE: char = '\u{19}';
const SIX: char = '\u{18}';

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

    fn index(&mut self, bound: usize) -> usize {
        usize::try_from(self.next(bound as u64)).unwrap_or(0)
    }

    fn pick<'a, T>(&mut self, pool: &'a [T]) -> &'a T {
        &pool[self.index(pool.len())]
    }

    fn chance(&mut self, of: u64) -> bool {
        self.next(of) == 0
    }
}

fn some(value: Option<&str>) -> String {
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

const SETTINGS: &[&str] = &[
    r#"[{"id":"model","currentValue":"opus","options":[{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"}],"_meta":{"zz":{"label":"Opus 5.5"}}},{"id":"effort","currentValue":"xhigh","options":[{"value":"high","name":"High"},{"value":"xhigh","name":"Extra high"}]}]"#,
    r#"[{"id":"model","currentValue":"solo"}]"#,
    r#"[{"id":"effort","currentValue":"high"}]"#,
    r#"{"id":"model","currentValue":"object"}"#,
    "null",
    "[]",
];

const COMMANDS: &[&str] = &[
    r#"{"name":"review","description":"Review the diff","input":{"hint":"path"},"_meta":{"zz":{"skill":true}}}"#,
    r#"{"name":"plan"}"#,
    r#"{"name":"deep","description":7,"input":{"hint":3},"_meta":{"zz":{"skill":"yes"}}}"#,
    r#"{"description":"no name"}"#,
    r#"{"name":7}"#,
    r#""text""#,
    r#"{"name":"twice","name":"last","description":"a","description":"b"}"#,
    r#"{"name":"meta","_meta":{"zz":{"skill":false}},"input":null}"#,
];

const UPDATES: &[&str] = &[
    r#"{"sessionUpdate":"usage_update","used":1200,"size":200000}"#,
    r#"{"sessionUpdate":"usage_update","used":7,"size":9}"#,
    r#"{"sessionUpdate":"usage_update","used":"x","size":1}"#,
    r#"{"sessionUpdate":"usage_update","used":3}"#,
    r#"{"sessionUpdate":"usage_update","used":-3,"size":4}"#,
    r#"{"sessionUpdate":"usage_update","used":1.5,"size":4}"#,
    r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}"#,
    r#"{"sessionUpdate":"available_commands_update","availableCommands":"nope"}"#,
    r#"{"sessionUpdate":"available_commands_update"}"#,
    r#"{"sessionUpdate":7,"availableCommands":[{"name":"ignored"}]}"#,
    r#"{"availableCommands":[{"name":"ignored"}]}"#,
    r#"{"sessionUpdate":null,"availableCommands":null}"#,
];

fn commands_update(random: &mut Random) -> String {
    let count = if random.chance(12) { 300 } else { random.index(5) };
    let commands = (0..count)
        .map(|index| {
            if count > 100 {
                format!(r#"{{"name":"c{index}"}}"#)
            } else {
                (*random.pick(COMMANDS)).to_owned()
            }
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"sessionUpdate":"available_commands_update","availableCommands":[{}]}}"#,
        commands.join(",")
    )
}

fn update(random: &mut Random) -> String {
    if random.chance(3) {
        commands_update(random)
    } else {
        (*random.pick(UPDATES)).to_owned()
    }
}

fn config(random: &mut Random) -> String {
    match random.next(5) {
        0 => String::new(),
        1 | 2 => format!(r#","config_options":{}"#, random.pick(SETTINGS)),
        3 => format!(r#","configOptions":{}"#, random.pick(SETTINGS)),
        _ => format!(
            r#","config_options":{},"configOptions":{}"#,
            random.pick(SETTINGS),
            random.pick(SETTINGS)
        ),
    }
}

fn chat_events() -> Vec<String> {
    let text = |value: &str| ItemBody::text(value);
    let events = vec![
        ChatEvent::SessionReady {
            session: "s1".to_owned(),
        },
        ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::User { prompt: 4 },
            at: 100,
        },
        ChatEvent::ItemStarted {
            item: ItemId::new("u"),
            turn: Some(TurnId(1)),
            parent: None,
            kind: ItemKind::User,
            body: text("fix the build"),
            at: 100,
        },
        ChatEvent::ItemStarted {
            item: ItemId::new("a"),
            turn: Some(TurnId(1)),
            parent: None,
            kind: ItemKind::Agent,
            body: text("Looking"),
            at: 110,
        },
        ChatEvent::ItemAppended {
            item: ItemId::new("a"),
            text: " around".to_owned(),
        },
        ChatEvent::ItemEnded {
            item: ItemId::new("a"),
            status: ItemStatus::Completed,
            at: 120,
        },
        ChatEvent::RequestOpened {
            request: RequestId(3),
            turn: Some(TurnId(1)),
            item: None,
            ask: Ask::Approval {
                title: "Run cargo check".to_owned(),
                detail: Some("cargo check".to_owned()),
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
            },
            blocking: true,
            at: 130,
        },
        ChatEvent::RequestOpened {
            request: RequestId(5),
            turn: None,
            item: None,
            ask: Ask::Questions {
                questions: vec![asked("q0", &["one", "two"], false, true)],
            },
            blocking: false,
            at: 131,
        },
        ChatEvent::RequestClosed {
            request: RequestId(3),
            close: RequestClose::Answered {
                answer: Answer::Option {
                    id: "allow".to_owned(),
                },
            },
            at: 140,
        },
        ChatEvent::RequestClosed {
            request: RequestId(5),
            close: RequestClose::Dismissed,
            at: 141,
        },
        ChatEvent::QueueChanged {
            prompts: vec![QueuedPrompt {
                id: 9,
                text: "then run the tests".to_owned(),
            }],
        },
        ChatEvent::QueueChanged { prompts: vec![] },
        ChatEvent::UsageChanged {
            usage: Usage {
                used: 1200,
                size: 200_000,
            },
        },
        ChatEvent::TurnEnded {
            turn: TurnId(1),
            outcome: TurnOutcome::Completed {
                stop_reason: "end_turn".to_owned(),
            },
            at: 150,
        },
        ChatEvent::TurnStarted {
            turn: TurnId(2),
            by: TurnBy::Agent,
            at: 160,
        },
        ChatEvent::TurnEnded {
            turn: TurnId(2),
            outcome: TurnOutcome::Cancelled,
            at: 170,
        },
        ChatEvent::Restart,
    ];
    let mut texts = events
        .iter()
        .filter_map(|event| serde_json::to_string(event).ok())
        .collect::<Vec<_>>();
    texts.push(r#"{"event":"bogus"}"#.to_owned());
    texts.push("7".to_owned());
    texts
}

fn asked(id: &str, labels: &[&str], multi_select: bool, allow_other: bool) -> AgentQuestion {
    AgentQuestion {
        id: id.to_owned(),
        header: (!multi_select).then(|| "Scope".to_owned()),
        question: format!("Which {id}?"),
        options: labels
            .iter()
            .enumerate()
            .map(|(index, label)| AgentQuestionOption {
                label: (*label).to_owned(),
                description: (index == 0).then(|| "the safe one".to_owned()),
            })
            .collect(),
        multi_select,
        allow_other,
        secret: false,
    }
}

fn questions(random: &mut Random) -> String {
    match random.next(8) {
        0 => r#","questions":"x""#.to_owned(),
        1 => r#","questions":[{"id":1}]"#.to_owned(),
        2 => r#","questions":[]"#.to_owned(),
        3 | 4 => {
            let asked = vec![
                asked("q0", &["one", "two"], false, true),
                asked("q1", &[], true, false),
            ];
            format!(
                r#","questions":{}"#,
                serde_json::to_string(&asked).unwrap_or_default()
            )
        }
        _ => String::new(),
    }
}

const OPTIONS: &[&str] = &[
    r#"{"optionId":"allow","name":"Allow","kind":"allow_once"}"#,
    r#"{"optionId":"always","name":"Always allow","kind":"allow_always"}"#,
    r#"{"optionId":"deny","name":"Deny","kind":"reject_once"}"#,
    r#"{"optionId":"never","name":"","kind":"reject_always","_meta":null}"#,
];

const BAD_OPTIONS: &[&str] = &[
    r#"{"optionId":"x","name":"X","kind":"sometimes"}"#,
    r#"{"id":"x","name":"X","kind":"allow_once"}"#,
    r#"{"optionId":"x","kind":"allow_once"}"#,
    r#"{"optionId":7,"name":"X","kind":"allow_once"}"#,
    r#""allow""#,
];

fn permission(random: &mut Random) -> String {
    let request = random.next(4) + 1;
    let id = match random.next(12) {
        0 => String::new(),
        1 => format!(r#","request_id":"{request}""#),
        _ => format!(r#","request_id":{request}"#),
    };
    let tool = match random.next(12) {
        0 => String::new(),
        1 => r#","tool_call":{"title":"No id"}"#.to_owned(),
        2 => r#","tool_call":"t1""#.to_owned(),
        _ => format!(
            r#","tool_call":{{"toolCallId":"t{}","title":"{}"}}"#,
            random.next(3),
            random.pick(&["Run cargo check", "Edit src/lib.rs", ""])
        ),
    };
    let count = if random.chance(10) { 40 } else { random.index(4) };
    let mut options = (0..count)
        .map(|index| {
            if count > 32 {
                format!(r#"{{"optionId":"o{index}","name":"Option {index}","kind":"allow_once"}}"#)
            } else {
                (*random.pick(OPTIONS)).to_owned()
            }
        })
        .collect::<Vec<_>>();
    if random.chance(8) {
        options.push((*random.pick(BAD_OPTIONS)).to_owned());
    }
    let options = match random.next(14) {
        0 => String::new(),
        1 => r#","options":{"optionId":"allow"}"#.to_owned(),
        _ => format!(r#","options":[{}]"#, options.join(",")),
    };
    format!("{id}{tool}{options}{}", questions(random))
}

fn prompts(random: &mut Random) -> String {
    let count = random.index(4);
    let prompts = (0..count)
        .map(|_| {
            let owner = match random.next(6) {
                0 => String::new(),
                1 => r#""owner":0,"#.to_owned(),
                2 => r#""owner":7,"#.to_owned(),
                3 => r#""owner":8,"#.to_owned(),
                4 => r#""owner":null,"#.to_owned(),
                _ => r#""owner":"7","#.to_owned(),
            };
            let text = match random.next(8) {
                0 => r#""text":"not base64!""#.to_owned(),
                1 => format!(r#""text":"{}""#, BASE64.encode([0xff_u8, 0xfe])),
                2 => r#""text":7"#.to_owned(),
                3 => format!(r#""text":"{}""#, BASE64.encode("")),
                4 => format!(r#""text":"{}""#, BASE64.encode("  \n")),
                _ => format!(
                    r#""text":"{}""#,
                    BASE64.encode(*random.pick(&["fix the build", "run the tests\n", "é ok"]))
                ),
            };
            let images = match random.next(8) {
                0 => format!(
                    r#","images":[{{"format":"png","data":"{}"}}]"#,
                    BASE64.encode([1_u8, 2, 3])
                ),
                1 => format!(
                    r#","images":[{{"format":"jpeg","data":"{}"}},{{"format":"png","data":""}}]"#,
                    BASE64.encode([9_u8])
                ),
                2 => r#","images":[{"format":"png","data":"***"}]"#.to_owned(),
                3 => r#","images":[{"data":"AQ=="}]"#.to_owned(),
                4 => r#","images":"none""#.to_owned(),
                _ => String::new(),
            };
            format!("{{{owner}{text}{images}}}")
        })
        .collect::<Vec<_>>();
    let reclaim = match random.next(6) {
        0 => String::new(),
        1 => format!(r#","reclaimId":{}"#, random.next(6)),
        2 => r#","reclaim_id":"3""#.to_owned(),
        _ => format!(r#","reclaim_id":{}"#, random.next(6)),
    };
    let prompts = if random.chance(12) {
        r#","prompts":{"text":"x"}"#.to_owned()
    } else {
        format!(r#","prompts":[{}]"#, prompts.join(","))
    };
    format!("{reclaim}{prompts}")
}

const NAMES: &[&str] = &[
    "ready",
    "sessionReady",
    "sessionSwitched",
    "update",
    "update",
    "chat",
    "chat",
    "chat",
    "permissionRequested",
    "permissionRequested",
    "permissionRequested",
    "permissionResolved",
    "promptFinished",
    "paneFailed",
    "promptsRestored",
    "promptsRestored",
    "promptsReclaimed",
    "configOptionsChanged",
    "stateSynced",
];

fn tail(random: &mut Random, name: &str, events: &[String]) -> String {
    match name {
        "ready" => match random.next(5) {
            0 => String::new(),
            1 => r#","capabilities":{}"#.to_owned(),
            2 => r#","capabilities":{"list":true,"extra":1}"#.to_owned(),
            _ => format!(
                r#","capabilities":{{"list":{},"load":{},"delete":{},"verbs":{},"watch":{}}}"#,
                random.chance(2),
                random.chance(2),
                random.chance(2),
                random.chance(2),
                random.chance(2)
            ),
        },
        "sessionReset" => match random.next(3) {
            0 => r#","restoring":true"#.to_owned(),
            1 => r#","restoring":false"#.to_owned(),
            _ => String::new(),
        },
        "sessionReady" | "configOptionsChanged" => config(random),
        "sessionSwitched" => {
            let replay = (0..random.index(3))
                .map(|_| update(random))
                .collect::<Vec<_>>();
            format!(r#","replay":[{}]{}"#, replay.join(","), config(random))
        }
        "update" => format!(r#","update":{}"#, update(random)),
        "chat" => {
            if random.chance(15) {
                String::new()
            } else {
                format!(r#","event":{}"#, random.pick(events))
            }
        }
        "permissionRequested" => permission(random),
        "permissionResolved" => match random.next(6) {
            0 => String::new(),
            1 => r#","request_id":"1""#.to_owned(),
            _ => format!(r#","request_id":{},"canceled":{}"#, random.next(4) + 1, random.chance(2)),
        },
        "promptFinished" => (*random.pick(&[
            r#","outcome":{"outcome":"completed","stop_reason":"end_turn"}"#,
            r#","outcome":{"outcome":"completed","stop_reason":"cancelled"}"#,
            r#","outcome":{"outcome":"failed","message":"boom"}"#,
            "",
        ]))
        .to_owned(),
        "paneFailed" => r#","message":"boom""#.to_owned(),
        "promptsRestored" | "promptsReclaimed" => prompts(random),
        _ => String::new(),
    }
}

const BROKEN: &[&str] = &[
    "not json",
    "[1,2]",
    "",
    r#"{"item":"update"}"#,
    r#"{"seq":1.5,"item":"update"}"#,
    r#"{"seq":-1,"item":"update"}"#,
    r#"{"seq":SEQ,"item":"chat","event":{}}"#,
    r#"{"seq":SEQ,"seq":SEQ,"item":"update"}"#,
    r#"{"seq":SEQ,"item":"update","item":"chat"}"#,
    r#"{"seq":SEQ,"item":"update","restoring":null}"#,
    r#"{"seq":SEQ,"item":"update","restoring":1}"#,
    r#"{"seq":SEQ,"item":"update","replay":null}"#,
    r#"{"seq":SEQ,"item":"update","replay":[{"sessionUpdate":"x"},7]}"#,
    r#"{"seq":SEQ,"item":"update","update":5}"#,
    r#"{"seq":SEQ,"item":"update","update":{"sessionUpdate":"a","sessionUpdate":"b"}}"#,
    r#"{"seq":SEQ,"item":"update","capabilities":null}"#,
    r#"{"seq":SEQ,"item":"update","capabilities":{"list":1}}"#,
    r#"{"seq":SEQ,"item":"update","capabilities":{"list":true,"list":false}}"#,
    r#"{"seq":SEQ,"item":5}"#,
    r#"{"seq":SEQ,"item":"update"} trailing"#,
    r#"{"seq":SEQ,"item":"update""#,
];

const ODD: &[&str] = &[
    r#"{"seq":SEQ,"item":"update","update":null}"#,
    r#"{"seq":SEQ,"item":"update","extra":{"deep":[1,2,{"a":null}]},"seqs":1}"#,
    r#" { "seq" : SEQ , "item" : "ready" , "restoring" : false } "#,
    r#"{"item":"chat","seq":SEQ,"event":{"event":"restart"}}"#,
    r#"{"seq":SEQ,"item":"","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"anywhere"}]}}"#,
    r#"{"seq":SEQ,"item":"ready","replay":[{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"replayed"}]}],"capabilities":{"verbs":true}}"#,
];

fn item(random: &mut Random, seq: u64, reset: bool, events: &[String]) -> String {
    if random.chance(9) {
        return random.pick(BROKEN).replace("SEQ", &seq.to_string());
    }
    if random.chance(12) {
        return random.pick(ODD).replace("SEQ", &seq.to_string());
    }
    let name = if reset && random.chance(4) {
        "sessionReset"
    } else {
        random.pick(NAMES)
    };
    format!(
        r#"{{"seq":{seq},"item":"{name}"{}}}"#,
        tail(random, name, events)
    )
}

struct Batch {
    first: u64,
    items: Vec<String>,
}

fn batches(random: &mut Random, events: &[String], resets_anywhere: bool) -> Vec<Batch> {
    let mut next = if random.chance(6) { random.next(4) } else { 1 };
    (0..=random.index(7))
        .map(|_| {
            let first = match random.next(14) {
                0 => next + random.next(3) + 1,
                1 => next.saturating_sub(random.next(3) + 1),
                _ => next,
            };
            let items = (0..random.index(6))
                .map(|offset| {
                    let positional = first + offset as u64;
                    let seq = match random.next(24) {
                        0 => positional + random.next(3) + 1,
                        1 => positional.saturating_sub(random.next(3) + 1),
                        2 => 0,
                        _ => positional,
                    };
                    item(random, seq, resets_anywhere || offset == 0, events)
                })
                .collect::<Vec<_>>();
            next = first + items.len() as u64;
            Batch { first, items }
        })
        .collect()
}

fn encoded(batches: &[Batch]) -> String {
    let parts = batches
        .iter()
        .map(|batch| format!("{}{ONE}{}", batch.first, list(&batch.items, ONE)))
        .collect::<Vec<_>>();
    list(&parts, RECORD)
}

fn commands(commands: &[AgentCommand]) -> String {
    let parts = commands
        .iter()
        .map(|command| {
            joined(
                &[
                    command.name.clone(),
                    command.description.clone(),
                    some(command.input_hint.as_deref()),
                    bit(command.skill),
                ],
                THREE,
            )
        })
        .collect::<Vec<_>>();
    list(&parts, TWO)
}

fn cursor_state(cursor: &AgentCursor) -> Vec<String> {
    vec![
        cursor.last_seq.to_string(),
        bit(cursor.replay_pending),
        [
            cursor.history_supported,
            cursor.session_load_supported,
            cursor.session_delete_supported,
            cursor.verbs_supported,
            cursor.watch,
        ]
        .iter()
        .map(|flag| bit(*flag))
        .collect(),
        commands(&cursor.commands),
        cursor.bytes.to_string(),
    ]
}

fn requested(request: Option<u64>) -> String {
    some(request.map(|seq| seq.to_string()).as_deref())
}

fn bytes(batch: &Batch) -> Vec<Vec<u8>> {
    batch
        .items
        .iter()
        .map(|item| item.as_bytes().to_vec())
        .collect()
}

fn cursor_case(cases: &mut Cases, batches: &[Batch]) {
    let mut cursor = AgentCursor::default();
    let mut journal = Vec::new();
    let mut expected = Vec::new();
    for batch in batches {
        let (request, invalid) = cursor.apply(&mut journal, batch.first, &bytes(batch));
        let mut row = vec![requested(request), bit(invalid)];
        row.extend(cursor_state(&cursor));
        row.push(journal.len().to_string());
        expected.push(joined(&row, FIELD));
    }
    cases.case("cursor", &[&encoded(batches)], &joined(&expected, RECORD));
}

fn question(question: &AgentQuestion) -> String {
    let options = question
        .options
        .iter()
        .map(|option| format!("{}{SIX}{}", option.label, some(option.description.as_deref())))
        .collect::<Vec<_>>();
    joined(
        &[
            question.id.clone(),
            some(question.header.as_deref()),
            question.question.clone(),
            list(&options, FIVE),
            bit(question.multi_select),
            bit(question.allow_other),
            bit(question.secret),
        ],
        FOUR,
    )
}

fn pending(pending: &PendingPermission) -> String {
    let options = pending
        .options
        .iter()
        .map(|option| {
            let allow = match option.allow {
                Some(true) => "1",
                Some(false) => "0",
                None => "N",
            };
            format!("{}{FOUR}{}{FOUR}{allow}", option.id, option.name)
        })
        .collect::<Vec<_>>();
    let questions = pending.questions.iter().map(question).collect::<Vec<_>>();
    joined(
        &[
            pending.request_id.to_string(),
            pending.title.clone(),
            list(&options, THREE),
            list(&questions, THREE),
        ],
        TWO,
    )
}

fn restored(prompts: &[(String, Vec<AgentImage>)]) -> String {
    let parts = prompts
        .iter()
        .map(|(text, images)| {
            let images = images
                .iter()
                .map(|image| format!("{}{FOUR}{}", image.format, hex(&image.data)))
                .collect::<Vec<_>>();
            format!("{text}{TWO}{}", list(&images, THREE))
        })
        .collect::<Vec<_>>();
    list(&parts, ONE)
}

fn usage(usage: Option<(u64, u64)>) -> String {
    usage.map_or_else(|| "N".to_owned(), |(used, size)| format!("{used},{size}"))
}

fn phase(view: &ChatView) -> u8 {
    match view.phase() {
        ChatPhase::Starting => 0,
        ChatPhase::Idle => 1,
        ChatPhase::Running => 2,
        ChatPhase::Waiting => 3,
        ChatPhase::Stopping => 4,
        ChatPhase::Ended => 5,
    }
}

fn chat(view: &ChatView) -> String {
    let requests = view
        .requests
        .iter()
        .map(|request| format!("{}:{}", request.id.0, bit(request.is_open())))
        .collect::<Vec<_>>();
    joined(
        &[
            view.turns.len().to_string(),
            view.items.len().to_string(),
            list(&requests, TWO),
            view.queue.len().to_string(),
            view.skipped.to_string(),
            phase(view).to_string(),
        ],
        ONE,
    )
}

fn nothing(_: &str, _: Vec<u8>) -> Option<()> {
    None
}

fn feed_case(
    cases: &mut Cases,
    batches: &[Batch],
    client: Option<u64>,
    parked: Option<&AgentPermissionWire>,
) {
    let mut cursor = AgentCursor::default();
    let mut journal = Vec::new();
    let mut lens = LensTranscript::new(nothing);
    let mut used = None;
    let mut last_reclaim = 0_u64;
    let owner = client.map(ClientInstanceId);
    let mut expected = Vec::new();
    for batch in batches {
        journal.clear();
        let (request, invalid) = cursor.apply(&mut journal, batch.first, &bytes(batch));
        let mut restarted = false;
        let mut switched = false;
        let mut prompts = Vec::new();
        let mut acknowledged = Vec::new();
        for (_, bytes) in &journal {
            let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
                continue;
            };
            if let Some(reclaim) = restore_reclaim_id(&value)
                && !reclaim.is_some_and(|id| id <= last_reclaim)
            {
                let found = restored_prompts(&value, owner);
                if !found.is_empty() {
                    prompts.extend(found);
                    if let Some(id) = reclaim {
                        last_reclaim = id;
                        acknowledged.push(id.to_string());
                    }
                }
            }
            track_usage(&mut used, &value);
            switched |= conversation_changed(&value);
            restarted |= lens.apply(&value);
        }
        let mut row = vec![
            requested(request),
            bit(invalid),
            journal.len().to_string(),
            bit(restarted),
            bit(switched),
        ];
        row.extend(cursor_state(&cursor));
        row.push(usage(used));
        row.push(lens.settings.as_ref().map_or_else(
            || "N".to_owned(),
            |settings| {
                format!(
                    "S{}{ONE}{}{ONE}{}{ONE}{}",
                    settings.model,
                    settings.model_label,
                    settings.effort,
                    settings.summary()
                )
            },
        ));
        row.push(chat(&lens.chat));
        let waiting = pending_permissions(lens.model.permissions(), parked)
            .iter()
            .map(pending)
            .collect::<Vec<_>>();
        row.push(list(&waiting, ONE));
        row.push(restored(&prompts));
        row.push(list(&acknowledged, ','));
        expected.push(joined(&row, FIELD));
    }
    let (parked_id, payload) = parked.map_or_else(
        || ("N".to_owned(), String::new()),
        |parked| (format!("S{}", parked.request_id), parked.payload.clone()),
    );
    cases.case(
        "feed",
        &[
            &requested(client),
            &parked_id,
            &payload,
            &encoded(batches),
        ],
        &joined(&expected, RECORD),
    );
}

const PAYLOADS: &[&str] = &[
    r#"{"tool_call":{"title":"Run cargo check"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"deny","name":"Deny","kind":"reject_always"}]}"#,
    r#"{"toolCall":{"title":"Camel title"},"options":[{"id":"plain"},{"optionId":"named","name":"Named","kind":"allow_always"},{"optionId":7},{"id":"odd","kind":"sometimes","name":9},"text",{"optionId":"both","id":"ignored","kind":"reject_once"}]}"#,
    r#"{"tool_call":{"title":7},"toolCall":{"title":"Not reached"},"options":"none"}"#,
    r#"{"tool_call":{},"toolCall":{"title":"Reached"}}"#,
    r#"{"tool_call":["title"],"questions":[{"id":"q0","header":"Scope","question":"Which one?","options":[{"label":"one","description":"the safe one"},{"label":"two"}],"multiSelect":true,"allowOther":true,"secret":true}]}"#,
    r#"{"questions":[{"id":"q0","question":"ok"},{"id":"q1"}]}"#,
    r#"{"questions":{"id":"q0"}}"#,
    r#"{"options":[{"optionId":null,"id":"fallback"},{"optionId":"a","optionId":"b","name":"first","name":"last"}]}"#,
    "not json",
    "[1,2]",
    "null",
    "",
];

const RESTORES: &[&str] = &[
    r#"{"item":"promptsRestored","reclaim_id":4,"prompts":[{"text":"Zml4IHRoZSBidWlsZA=="}]}"#,
    r#"{"item":"promptsReclaimed","reclaimId":9,"prompts":[{"owner":7,"text":"cnVuIHRoZSB0ZXN0cwo=","images":[{"format":"png","data":"AQID"}]},{"owner":8,"text":"b3RoZXI="},{"owner":0,"text":""}]}"#,
    r#"{"item":"promptsRestored","reclaim_id":"4","reclaimId":5,"prompts":[{"text":"//4="},{"text":"bm90IGJhc2U2NCE"},{"text":"b2s=","images":[{"format":"png","data":"AQ"}]},{"text":"b2s=","images":[{"format":7,"data":"AQ=="}]},{"text":"a2VwdA==","images":[]}]}"#,
    r#"{"item":"promptsRestored","prompts":[{"owner":18446744073709551615,"text":"bWF4"},{"owner":-1,"text":"bmVn"},{"owner":1.0,"text":"ZmxvYXQ="},{"owner":"7","text":"dGV4dA=="}]}"#,
    r#"{"item":"update","reclaim_id":4,"prompts":[{"text":"b2s="}]}"#,
    r#"{"item":"promptsRestored","reclaim_id":null,"reclaimId":6}"#,
    r#"{"item":"promptsRestored","prompts":[{"text":"b2s=","text":"bGFzdA=="}],"prompts":[{"text":"d2lucw=="}]}"#,
    r#"{"item":7}"#,
    "[]",
];

fn helpers(cases: &mut Cases) {
    for (index, payload) in PAYLOADS.iter().enumerate() {
        let wire = AgentPermissionWire {
            request_id: index as u64 + 1,
            payload: (*payload).to_owned(),
        };
        let found = pending_permissions(&[], Some(&wire));
        let expected = found.iter().map(pending).collect::<Vec<_>>();
        cases.case(
            "parked",
            &[&wire.request_id.to_string(), payload],
            &list(&expected, ONE),
        );
    }
    for text in RESTORES {
        let value = serde_json::from_str::<Value>(text).unwrap_or_default();
        for client in [None, Some(7_u64), Some(8), Some(u64::MAX)] {
            cases.case(
                "restored",
                &[text, &requested(client)],
                &restored(&restored_prompts(&value, client.map(ClientInstanceId))),
            );
        }
        let reclaim = restore_reclaim_id(&value)
            .map_or_else(|| "N".to_owned(), |id| format!("S{}", requested(id)));
        cases.case("reclaim", &[text], &reclaim);
        cases.case("changed", &[text], &bit(conversation_changed(&value)));
    }
    for name in ["sessionReset", "sessionSwitched", "sessionReady", "chat"] {
        let text = format!(r#"{{"item":"{name}"}}"#);
        let value = serde_json::from_str::<Value>(&text).unwrap_or_default();
        cases.case("changed", &[&text], &bit(conversation_changed(&value)));
    }
    let wrapped = UPDATES
        .iter()
        .map(|update| format!(r#"{{"item":"update","update":{update}}}"#))
        .chain(UPDATES.iter().map(|update| (*update).to_owned()))
        .chain([
            r#"{"item":"sessionSwitched","replay":[{"sessionUpdate":"usage_update","used":1,"size":2},{"sessionUpdate":"usage_update","used":3,"size":4}]}"#.to_owned(),
            r#"{"item":"sessionSwitched","replay":[{"item":"sessionReset","sessionUpdate":"usage_update","used":5,"size":6}]}"#.to_owned(),
            r#"{"item":"sessionReset"}"#.to_owned(),
            r#"{"item":"sessionSwitched","replay":{"sessionUpdate":"usage_update","used":1,"size":2}}"#.to_owned(),
            r#"{"update":"text","sessionUpdate":"usage_update","used":1,"size":2}"#.to_owned(),
            r#"{"sessionUpdate":"usage_update","used":18446744073709551615,"size":0}"#.to_owned(),
        ])
        .collect::<Vec<_>>();
    for text in &wrapped {
        let value = serde_json::from_str::<Value>(text).unwrap_or_default();
        cases.case("usage", &[text], &usage(usage_from_update(&value)));
    }
    let mut random = Random(0x51ed_270b_9f3c_1a77);
    for _ in 0..60 {
        let script = (0..=random.index(6))
            .map(|_| (*random.pick(&wrapped)).clone())
            .collect::<Vec<_>>();
        let mut used = None;
        for text in &script {
            let value = serde_json::from_str::<Value>(text).unwrap_or_default();
            track_usage(&mut used, &value);
        }
        cases.case("track", &[&list(&script, RECORD)], &usage(used));
    }
    let phases = [
        AgentConnectionPhase::Starting,
        AgentConnectionPhase::Ready,
        AgentConnectionPhase::Running,
        AgentConnectionPhase::AwaitingPermission,
        AgentConnectionPhase::Failed {
            message: "boom".to_owned(),
        },
    ];
    let providers = [
        None,
        Some(AgentProvider::Codex),
        Some(AgentProvider::ClaudeCode),
    ];
    let provided = |provider: Option<AgentProvider>| match provider {
        None => "N",
        Some(AgentProvider::Codex) => "0",
        Some(AgentProvider::ClaudeCode) => "1",
    };
    for (index, phase) in phases.iter().enumerate() {
        for parked in [false, true] {
            let state = AgentPaneWire {
                phase: phase.clone(),
                pending_permission: parked.then(|| AgentPermissionWire {
                    request_id: 1,
                    payload: String::new(),
                }),
                ..AgentPaneWire::default()
            };
            cases.case(
                "accepts",
                &[&index.to_string(), &bit(parked)],
                &bit(accepts_prompt(&state)),
            );
        }
        for connected in [false, true] {
            for provider in providers {
                let expected = match agent_empty(connected, phase, provider) {
                    AgentEmpty::Welcome(provider) => {
                        format!("W{}", agent_welcome_title(provider))
                    }
                    AgentEmpty::Message { text, busy } => format!("M{text}{FIELD}{}", bit(busy)),
                };
                cases.case(
                    "empty",
                    &[&bit(connected), &index.to_string(), provided(provider)],
                    &expected,
                );
            }
        }
    }
    for provider in providers {
        cases.case("placeholder", &[provided(provider)], &agent_placeholder(provider));
    }
    cases.case("detail", &[], AGENT_WELCOME_DETAIL);
}

fn fixed(cases: &mut Cases) {
    let plain = |first: u64, items: &[&str]| Batch {
        first,
        items: items.iter().map(|item| (*item).to_owned()).collect(),
    };
    let scripts = [
        vec![
            plain(1, &[r#"{"seq":1,"item":"ready","capabilities":{"list":true,"verbs":true}}"#]),
            plain(5, &[r#"{"seq":5,"item":"update"}"#]),
            plain(5, &[r#"{"seq":5,"item":"update"}"#]),
            plain(2, &[r#"{"seq":2,"item":"update"}"#, r#"{"seq":3,"item":"update"}"#]),
            plain(9, &["broken"]),
            plain(4, &["broken", r#"{"seq":5,"item":"update"}"#]),
        ],
        vec![
            plain(1, &[r#"{"seq":1,"item":"update"}"#]),
            plain(40, &[r#"{"seq":40,"item":"sessionReset","restoring":true}"#, r#"{"seq":41,"item":"sessionReady"}"#]),
            plain(60, &[r#"{"seq":60,"item":"sessionReset"}"#]),
            plain(42, &[r#"{"seq":42,"item":"update"}"#, r#"{"seq":44,"item":"update"}"#, r#"{"seq":43,"item":"update"}"#]),
        ],
        vec![
            plain(u64::MAX - 1, &[r#"{"seq":18446744073709551614,"item":"sessionReset","restoring":true}"#, r#"{"seq":18446744073709551615,"item":"update"}"#, r#"{"seq":18446744073709551615,"item":"update"}"#, "broken"]),
            plain(u64::MAX, &["broken", "broken"]),
        ],
        vec![
            plain(1, &[r#"{"seq":1,"item":"update","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"kept"}]}}"#]),
            plain(2, &[r#"{"seq":2,"item":"sessionSwitched"}"#]),
            plain(3, &[r#"{"seq":3,"item":"update","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"again"}]}}"#, r#"{"seq":4,"item":"sessionReset"}"#, r#"{"seq":5,"item":"update","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"restored"}]}}"#, r#"{"seq":6,"item":"sessionSwitched"}"#, r#"{"seq":7,"item":"sessionSwitched"}"#]),
        ],
        vec![
            plain(0, &[r#"{"seq":0,"item":"update"}"#, "broken", "broken", r#"{"seq":3,"item":"update"}"#]),
            plain(3, &[r#"{"seq":3,"item":"update"}"#]),
        ],
    ];
    for script in &scripts {
        cursor_case(cases, script);
    }
    let parked = AgentPermissionWire {
        request_id: 2,
        payload: PAYLOADS[0].to_owned(),
    };
    let feeds = [
        vec![
            plain(1, &[
                r#"{"seq":1,"item":"permissionRequested","request_id":1,"tool_call":{"toolCallId":"t1","title":"Run cargo check"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"deny","name":"Deny","kind":"reject_once"}]}"#,
                r#"{"seq":2,"item":"permissionRequested","request_id":2,"tool_call":{"toolCallId":"t2","title":"Edit src/lib.rs"},"options":[]}"#,
            ]),
            plain(3, &[r#"{"seq":3,"item":"permissionRequested","request_id":1,"tool_call":{"toolCallId":"t1","title":"Run it again"},"options":[{"optionId":"always","name":"Always","kind":"allow_always"}]}"#]),
            plain(4, &[r#"{"seq":4,"item":"permissionResolved","request_id":2}"#]),
            plain(5, &[r#"{"seq":5,"item":"permissionResolved","request_id":9}"#]),
            plain(6, &[r#"{"seq":6,"item":"promptFinished","outcome":{"outcome":"completed","stop_reason":"end_turn"}}"#]),
        ],
        vec![
            plain(1, &[
                r#"{"seq":1,"item":"promptsRestored","reclaim_id":3,"prompts":[{"owner":7,"text":"Zml4IHRoZSBidWlsZA=="},{"owner":8,"text":"b3RoZXI="}]}"#,
                r#"{"seq":2,"item":"promptsRestored","reclaim_id":3,"prompts":[{"text":"cmVwZWF0"}]}"#,
                r#"{"seq":3,"item":"promptsRestored","reclaim_id":2,"prompts":[{"text":"b2xkZXI="}]}"#,
                r#"{"seq":4,"item":"promptsRestored","prompts":[{"text":"bm8gaWQ="}]}"#,
                r#"{"seq":5,"item":"promptsReclaimed","reclaim_id":4,"prompts":[{"owner":9,"text":"bm90IG1pbmU="}]}"#,
                r#"{"seq":6,"item":"promptsReclaimed","reclaim_id":4,"prompts":[{"text":"bWluZQ=="}]}"#,
            ]),
        ],
    ];
    for script in &feeds {
        feed_case(cases, script, Some(7), None);
        feed_case(cases, script, None, Some(&parked));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).ok_or("usage: agent_lens_vectors <out>")?;
    let mut cases = Cases::default();
    let events = chat_events();
    fixed(&mut cases);
    helpers(&mut cases);
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    for _ in 0..400 {
        let script = batches(&mut random, &events, true);
        cursor_case(&mut cases, &script);
    }
    for round in 0..400 {
        let script = batches(&mut random, &events, false);
        let client = match round % 3 {
            0 => None,
            1 => Some(7),
            _ => Some(8),
        };
        let parked = random.chance(3).then(|| AgentPermissionWire {
            request_id: random.next(4) + 1,
            payload: (*random.pick(PAYLOADS)).to_owned(),
        });
        feed_case(&mut cases, &script, client, parked.as_ref());
    }
    let mut out = cases.count.to_le_bytes().to_vec();
    out.extend_from_slice(&cases.out);
    fs::write(&path, &out)?;
    println!("{} cases, {} bytes", cases.count, out.len());
    Ok(())
}
