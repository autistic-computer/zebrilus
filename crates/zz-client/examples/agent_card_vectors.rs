use std::{env, error::Error, fs};

use zz_client::{
    agent_completion::{
        AgentCommand, CommandCompletion, active_command_hint, command_label, completion_query,
        completion_score, composer_completions, meaningful_command_description, pane_commands,
        rendered_error, setting_completions,
    },
    agent_question::{QuestionCard, QuestionCardStep},
    agent_settings::{
        ComposerSetter, LensSettings, composer_setter, fork_chat_command, real_session, setter,
        short_folder, side_chat_command, source_message,
    },
};
use zz_protocol::{
    AgentProvider, AgentQuestionAnswer, CommandInvocation, PaneId,
    agent_stream::{AgentQuestion, AgentQuestionOption},
};

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';
const ONE: char = '\u{1d}';
const TWO: char = '\u{1c}';
const THREE: char = '\u{1b}';

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

fn fields(parts: &[String]) -> String {
    parts.join(&FIELD.to_string())
}

fn asked(id: &str, labels: &[&str], multi_select: bool, allow_other: bool) -> AgentQuestion {
    AgentQuestion {
        id: id.to_owned(),
        header: None,
        question: format!("Question {id}?"),
        options: labels
            .iter()
            .map(|label| AgentQuestionOption {
                label: (*label).to_owned(),
                description: None,
            })
            .collect(),
        multi_select,
        allow_other,
        secret: false,
    }
}

fn questions_text(questions: &[AgentQuestion]) -> String {
    list(
        &questions
            .iter()
            .map(|question| {
                fields(&[
                    question.id.clone(),
                    bit(question.multi_select),
                    bit(question.allow_other),
                    list(
                        &question
                            .options
                            .iter()
                            .map(|option| option.label.clone())
                            .collect::<Vec<_>>(),
                        ONE,
                    ),
                ])
            })
            .collect::<Vec<_>>(),
        RECORD,
    )
}

fn answers_text(answers: &[AgentQuestionAnswer]) -> String {
    list(
        &answers
            .iter()
            .map(|answer| format!("{}{TWO}{}", answer.id, list(&answer.answers, THREE)))
            .collect::<Vec<_>>(),
        ONE,
    )
}

fn step_text(step: &QuestionCardStep) -> String {
    match step {
        QuestionCardStep::Stay => "stay".to_owned(),
        QuestionCardStep::Handled => "handled".to_owned(),
        QuestionCardStep::Other(question) => format!("other{question}"),
        QuestionCardStep::Submit(answers) => format!("submit{}", answers_text(answers)),
        QuestionCardStep::Dismiss => "dismiss".to_owned(),
    }
}

fn card_state(card: &QuestionCard, result: &str) -> String {
    let state = card
        .questions()
        .iter()
        .enumerate()
        .map(|(index, question)| {
            let picks = (0..question.options.len())
                .map(|option| bit(card.is_picked(index, option)))
                .collect::<String>();
            format!(
                "{picks}/{}{}",
                bit(card.other_selected(index)),
                bit(card.is_answered(index))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    fields(&[
        result.to_owned(),
        card.focused().to_string(),
        state,
        bit(card.is_complete()),
        some(card.problem().as_deref()),
    ])
}

#[derive(Clone)]
enum CardOp {
    Pick(usize, usize),
    Other(usize),
    Text(usize, String),
    Key(String, bool),
    Focus(usize),
    Submit,
}

fn card_case(cases: &mut Cases, questions: &[AgentQuestion], ops: &[CardOp]) {
    let mut card = QuestionCard::new(7, questions.to_vec());
    let mut expected = String::new();
    let mut script = Vec::new();
    for op in ops {
        let result = match op {
            CardOp::Pick(question, option) => {
                script.push(fields(&[
                    "pick".to_owned(),
                    question.to_string(),
                    option.to_string(),
                ]));
                step_text(&card.pick(*question, *option))
            }
            CardOp::Other(question) => {
                script.push(fields(&["other".to_owned(), question.to_string(), String::new()]));
                step_text(&card.select_other(*question))
            }
            CardOp::Text(question, text) => {
                script.push(fields(&["text".to_owned(), question.to_string(), text.clone()]));
                bit(card.set_other_text(*question, text))
            }
            CardOp::Key(key, shift) => {
                script.push(fields(&["key".to_owned(), key.clone(), bit(*shift)]));
                step_text(&card.key(key, *shift))
            }
            CardOp::Focus(question) => {
                script.push(fields(&["focus".to_owned(), question.to_string(), String::new()]));
                bit(card.focus(*question))
            }
            CardOp::Submit => {
                script.push(fields(&["submit".to_owned(), String::new(), String::new()]));
                step_text(&card.submit())
            }
        };
        expected.push_str(&card_state(&card, &result));
        expected.push(RECORD);
    }
    expected.push_str(&some(
        card.answers().as_deref().map(answers_text).as_deref(),
    ));
    cases.case(
        "card",
        &[&questions_text(questions), &list(&script, RECORD)],
        &expected,
    );
}

const LABELS: &[&str] = &["Yes", "No", "Maybe", "Later", "", "Ünïcode ✓", "Yes"];
const OTHER_TEXTS: &[&str] = &["", " ", "custom", "  padded  ", "\u{a0}", "two words", "é"];
const KEYS: &[&str] = &[
    "escape", "tab", "down", "up", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "10", "+2",
    "01", "", "x", "-1", "+", "１", "enter", "Tab",
];

fn card_cases(cases: &mut Cases) {
    let mut random = Random(0x5eed_ca4d_0bad_f00d);
    for round in 0..260 {
        let count = if round % 37 == 0 { 0 } else { 1 + random.index(4) };
        let questions = (0..count)
            .map(|index| {
                let labels = (0..random.index(6))
                    .map(|_| *random.pick(LABELS))
                    .collect::<Vec<_>>();
                asked(
                    &format!("q{index}"),
                    &labels,
                    random.chance(2),
                    !random.chance(3),
                )
            })
            .collect::<Vec<_>>();
        let ops = (0..28)
            .map(|_| {
                let question = random.index(count + 2);
                match random.next(10) {
                    0..=2 => CardOp::Pick(question, random.index(7)),
                    3 => CardOp::Other(question),
                    4 | 5 => CardOp::Text(question, (*random.pick(OTHER_TEXTS)).to_owned()),
                    6..=8 => CardOp::Key((*random.pick(KEYS)).to_owned(), random.chance(2)),
                    _ if random.chance(4) => CardOp::Submit,
                    _ => CardOp::Focus(question),
                }
            })
            .collect::<Vec<_>>();
        card_case(cases, &questions, &ops);
    }
    let many = (0..33)
        .map(|index| asked(&format!("q{index}"), &["a"], false, false))
        .collect::<Vec<_>>();
    card_case(cases, &many, &[CardOp::Pick(0, 0), CardOp::Submit]);
    let long = "i".repeat(4097);
    card_case(
        cases,
        &[asked(&long, &["a"], true, false)],
        &[CardOp::Pick(0, 0), CardOp::Submit],
    );
    let edge = "i".repeat(4096);
    card_case(
        cases,
        &[asked(&edge, &["a"], true, false)],
        &[CardOp::Pick(0, 0), CardOp::Submit],
    );
    let labels = (0..33).map(|_| "x").collect::<Vec<_>>();
    let mut picks = (0..33).map(|option| CardOp::Pick(0, option)).collect::<Vec<_>>();
    picks.push(CardOp::Submit);
    picks.push(CardOp::Pick(0, 4));
    picks.push(CardOp::Submit);
    card_case(cases, &[asked("wide", &labels, true, true)], &picks);
    card_case(
        cases,
        &[asked("big", &["a"], false, true), asked("next", &["b"], false, false)],
        &[
            CardOp::Pick(1, 0),
            CardOp::Text(0, "y".repeat(16_385)),
            CardOp::Submit,
            CardOp::Text(0, format!("  {}  ", "y".repeat(16_384))),
            CardOp::Submit,
        ],
    );
}

const SETTINGS: &[&str] = &[
    r#"[{"id":"model","currentValue":"opus","options":[{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"}],"_meta":{"zz":{"label":"Opus 5.5"}}},{"id":"effort","currentValue":"xhigh","options":[{"value":"high","name":"High"},{"value":"xhigh","name":"Extra high"}]}]"#,
    r#"[{"id":"model","currentValue":"gpt-6-luna","options":[{"value":"gpt-6-luna","name":"GPT-6-Luna","_meta":{"zz":{"efforts":["low","high"],"defaultEffort":"low"}}},{"value":"gpt-6.1-sol","name":"GPT-6.1-Sol"}],"_meta":{"zz":{"label":"GPT-6-Luna"}}},{"id":"effort","currentValue":"high","options":[{"value":"low","name":"Low"},{"value":"high","name":"High"}]}]"#,
    r#"[{"id":"model","currentValue":"solo"}]"#,
    r#"[{"id":"model"}]"#,
    r#"[{"id":"effort","currentValue":"high"}]"#,
    r#"[{"id":"model","currentValue":"","options":[],"_meta":{"zz":{"label":""}}},{"id":"effort","currentValue":"deep"}]"#,
    r#"[{"id":"model","currentValue":7,"options":{"value":"x"},"_meta":"zz"},{"id":"effort","currentValue":null,"options":[1,"two",null,{"value":3,"name":"Three"},{"name":"No value"},{"value":"kept"},{"value":"named","name":9},[{"value":"nested"}]]}]"#,
    r#"[{"id":"model","currentValue":"a","currentValue":"b","id":"other","id":"model","options":[{"value":"v1","value":"v2","name":"n1","name":"n2"}],"_meta":{"zz":{"label":"first"},"zz":{"label":"second"}}}]"#,
    r#"[{"id":"mode","currentValue":"x"},{"id":"model","currentValue":"first"},{"id":"model","currentValue":"second"},{"id":"effort","currentValue":"e1","options":[{"value":"e1","name":""}]},{"id":"effort","currentValue":"e2"}]"#,
    r#"[1,"model",null,["id","model"],{"id":["model"]},{"id":"model","currentValue":"deep","_meta":{"zz":["label"]}}]"#,
    r#"{"id":"model","currentValue":"object"}"#,
    r#""model""#,
    "null",
    "[]",
    "",
    "[",
    r#"[{"id":"model","currentValue":"x"}] trailing"#,
    r#"  [ { "id" : "model" , "currentValue" : "spaced é ✓" , "_meta" : { "zz" : { "label" : 3 } } } ]  "#,
    r#"[{"id":"model","currentValue":"m","options":[{"value":"m","name":"Model M"},{"value":"M2","name":"second"},{"value":"zeta","name":"Mighty"},{"value":"alpha","name":"alpha"}]},{"id":"effort","currentValue":"low","options":[{"value":"low","name":"Low"},{"value":"medium","name":"Medium"},{"value":"max","name":"Low and slow"}]}]"#,
    r#"[{"id":"model","currentValue":"1e400","options":[{"value":"big","name":1e400}]}]"#,
    r#"[{"id":"model","currentValue":"😀 é","options":[{"value":"A","name":"\n"}]}]"#,
];

fn choices_text(choices: &[zz_client::agent_settings::Choice]) -> String {
    list(
        &choices
            .iter()
            .map(|choice| format!("{}{TWO}{}", choice.value, choice.name))
            .collect::<Vec<_>>(),
        ONE,
    )
}

fn settings_text(settings: Option<&LensSettings>) -> String {
    settings.map_or_else(
        || "N".to_owned(),
        |settings| {
            format!(
                "S{}",
                fields(&[
                    settings.model.clone(),
                    settings.model_label.clone(),
                    choices_text(&settings.models),
                    settings.effort.clone(),
                    choices_text(&settings.efforts),
                    settings.effort_name().to_owned(),
                    settings.summary(),
                ])
            )
        },
    )
}

const SETTERS: &[&str] = &[
    " /model gpt-6-luna ",
    "/effort high",
    "/model",
    "/effort ",
    "/model gpt-6-luna --reasoning=low",
    "/models",
    "switch /model opus",
    "/cd /tmp",
    "",
    "/",
    "//model",
    "/model\topus",
    "/model\u{a0}opus",
    "/model\u{3000}opus\u{2003}",
    "/model  opus",
    "/model opus\n",
    "\n/effort\nxhigh",
    "/Model opus",
    "/effort é",
    "/model m",
    "/effort low",
    "/effort e1",
    "/model solo",
    "/effort xhigh",
    "/model opus",
    "/model haiku",
    "/modelopus",
    "/ model opus",
];

const FOLDERS: &[(&str, Option<&str>)] = &[
    ("/Users/me/src/zz", Some("/Users/me")),
    ("/Users/me/src/zz", Some("/Users/m")),
    ("/", None),
    ("/", Some("/")),
    ("", None),
    ("", Some("/home/me")),
    ("///", Some("/home/me")),
    ("/home/me", Some("/home/me")),
    ("/home/me/", Some("/home/me///")),
    ("/home/me//src//", Some("/home/me")),
    ("/home/mele", Some("/home/me")),
    ("/home/me", Some("")),
    ("/home/me", Some("/")),
    ("relative/dir/", Some("relative")),
    ("/tmp/é/ü/", Some("/tmp/é")),
    ("/srv", None),
];

const SOURCES: &[&str] = &[
    "u:abc",
    "u:abc~2",
    "u:~2",
    "u:",
    "u:message",
    "u:user",
    "u:message~1",
    "u:users",
    "a:abc",
    "",
    "u",
    "u:a~b~c",
    "xu:abc",
    "u:é~x",
];

const SESSIONS: &[&str] = &["t", "s", "", "lens-1", "lens-", "lens", "a b", "it's", "x$y", "Lens-1"];
const FOLDS: &[&str] = &["/src/my app", "/a/it's $HOME", "/plain", "", "/q\"uote\\back's", "~"];

fn invocation_text(invocation: &CommandInvocation) -> String {
    let mut parts = vec![invocation.name.clone()];
    parts.extend(invocation.args.iter().map(|arg| arg.as_str().to_owned()));
    fields(&parts)
}

fn settings_cases(cases: &mut Cases) {
    for text in SETTINGS {
        cases.case(
            "settings",
            &[text],
            &settings_text(LensSettings::from_json(text).as_ref()),
        );
    }
    for (folder, home) in FOLDERS {
        cases.case("folder", &[folder, &some(*home)], &short_folder(folder, *home));
    }
    for text in SETTERS {
        let expected = setter(text).map_or_else(
            || "N".to_owned(),
            |(option, value)| format!("S{option}{FIELD}{}", some(value)),
        );
        cases.case("setter", &[text], &expected);
        for settings in [None, Some(SETTINGS[0]), Some(SETTINGS[2]), Some(SETTINGS[8]), Some(SETTINGS[18])] {
            let parsed = settings.and_then(LensSettings::from_json);
            let expected = match composer_setter(text, parsed.as_ref()) {
                None => "N".to_owned(),
                Some(ComposerSetter::List(option)) => format!("L{option}"),
                Some(ComposerSetter::Keep) => "K".to_owned(),
                Some(ComposerSetter::Set(option, value)) => format!("V{option}{FIELD}{value}"),
            };
            cases.case("composer", &[text, &some(settings)], &expected);
        }
    }
    for item in SOURCES {
        cases.case("source", &[item], &some(source_message(item)));
    }
    for session in SESSIONS {
        cases.case("real", &[session], &bit(real_session(session)));
    }
    let mut random = Random(0x0f0c_5eed_1234_abcd);
    for round in 0..80_u64 {
        let provider = if round % 2 == 0 {
            AgentProvider::Codex
        } else {
            AgentProvider::ClaudeCode
        };
        let session = *random.pick(SESSIONS);
        let cwd = *random.pick(FOLDS);
        let message = if random.chance(2) {
            Some(*random.pick(&["i", "m 1", "", "it's"]))
        } else {
            None
        };
        let settings = if random.chance(3) {
            None
        } else {
            Some(*random.pick(&[SETTINGS[0], SETTINGS[2], SETTINGS[4], SETTINGS[5], SETTINGS[8]]))
        };
        let parsed = settings.and_then(LensSettings::from_json);
        let program = bit(provider == AgentProvider::ClaudeCode);
        cases.case(
            "fork",
            &[&program, session, cwd, &some(message), &some(settings)],
            &invocation_text(&fork_chat_command(
                provider,
                session,
                cwd,
                message,
                parsed.as_ref(),
            )),
        );
        let pane = random.next(5000);
        cases.case(
            "side",
            &[&program, session, cwd, &pane.to_string(), &some(settings)],
            &invocation_text(&side_chat_command(
                provider,
                session,
                cwd,
                PaneId(pane),
                parsed.as_ref(),
            )),
        );
    }
}

fn command(name: &str, description: &str, hint: Option<&str>, skill: bool) -> AgentCommand {
    AgentCommand {
        name: name.to_owned(),
        description: description.to_owned(),
        input_hint: hint.map(str::to_owned),
        skill,
    }
}

fn vendor() -> Vec<AgentCommand> {
    vec![
        command("/review", "Review the diff", Some("branch"), false),
        command("compact", "Compact the conversation", None, false),
        command("$brainstorm", "Think out loud", Some("topic"), true),
        command("$hatch-pet", "...", None, true),
        command("gh-address-comments", "Address review comments", None, true),
        command("//double", "Two slashes", None, false),
        command("Compact", "Upper case twin", Some("why"), false),
        command("btw", "Vendor side question", None, false),
        command("/steer", "Vendor steer", None, false),
        command("fork", "Vendor fork", None, false),
        command("rewind", "Vendor rewind", None, false),
        command("/side", "Vendor side", None, false),
        command("model", "Pick a model", Some("name"), false),
        command("résumé", "Unicode name", Some("é"), false),
        command("", "", None, false),
        command("review-all", "…", None, false),
        command("rev", "Short", None, false),
        command("REVIEW", "Loud", None, false),
    ]
}

fn commands_text(commands: &[AgentCommand]) -> String {
    list(
        &commands
            .iter()
            .map(|command| {
                fields(&[
                    command.name.clone(),
                    command.description.clone(),
                    some(command.input_hint.as_deref()),
                    bit(command.skill),
                ])
            })
            .collect::<Vec<_>>(),
        RECORD,
    )
}

fn completions_text(completions: &[CommandCompletion]) -> String {
    list(
        &completions
            .iter()
            .map(|completion| {
                fields(&[
                    completion.command.name.clone(),
                    completion.command.description.clone(),
                    some(completion.command.input_hint.as_deref()),
                    bit(completion.command.skill),
                    completion.replacement.start.to_string(),
                    completion.replacement.end.to_string(),
                    completion.insertion(),
                    bit(completion.submits()),
                ])
            })
            .collect::<Vec<_>>(),
        RECORD,
    )
}

const ERRORS: &[&str] = &[
    "",
    "  plain error  ",
    "line one\nline two\ttabbed",
    "bell\u{7}here\u{0}nul\u{1b}[31m",
    "del\u{7f}c1\u{85}\u{9f}edge\u{a0}",
    "\r\ncarriage\r\n",
    "é✓ unicode stays",
];

const VALUES: &[&str] = &[
    "",
    "/",
    "/re",
    "/rev",
    "héllo /rev!",
    "line one\n/co",
    "line/one\n co",
    "a/b",
    "x /",
    "x\u{a0}/re",
    "x\u{3000}/re",
    "é/re",
    "$rev",
    "https://zed.dev",
    "/review branch",
    "/REV",
    "/gac",
    "//",
    "/model ",
    "/model g",
    "/model\tgp",
    "  /effort h",
    "/effort  x",
    "/model gpt x",
    "/MODEL g",
    "/model\u{a0}g",
    "/model gpt-6.1",
    "/effort Extra",
    "/effort high  ",
    "/model m",
    "/effort lo",
    "/model\topus hai",
    "\n/model o",
    "/models o",
    "/model é",
];

const HINTS: &[&str] = &[
    "/review",
    "/review ",
    "/review branch",
    "  /REVIEW\t",
    "/compact",
    "/Compact \u{a0}",
    "$brainstorm",
    "/$brainstorm",
    "/résumé",
    "/RÉSUMÉ",
    "/",
    "",
    "review",
    "/model\n",
    "/double",
];

fn completion_cases(cases: &mut Cases) {
    for text in ERRORS {
        cases.case("error", &[text], &rendered_error(text));
    }
    for width in [1018_usize, 1019, 1020, 1021, 1022, 1023, 1024, 4000] {
        let ascii = "e".repeat(width);
        cases.case("error", &[&ascii], &rendered_error(&ascii));
        let wide = format!("{}é✓\u{1}😀{}", "e".repeat(width - 8), "z".repeat(40));
        cases.case("error", &[&wide], &rendered_error(&wide));
        let control = format!("{}\u{2}\u{3}\n\u{4}tail", "c".repeat(width - 4));
        cases.case("error", &[&control], &rendered_error(&control));
    }
    let vendor = vendor();
    let spec = commands_text(&vendor);
    for verbs in [false, true] {
        cases.case(
            "commands",
            &[&spec, &bit(verbs)],
            &commands_text(&pane_commands(&vendor, verbs)),
        );
    }
    let merged = pane_commands(&vendor, true);
    let merged_spec = commands_text(&merged);
    for command in &merged {
        cases.case("label", &[&command.name], &command_label(&command.name));
        cases.case(
            "meaningful",
            &[&command.description],
            &some(meaningful_command_description(&command.description)),
        );
    }
    for description in ["", "  ", ".", "…", " ..… ", ". a .", "\u{a0}.\u{a0}", "x"] {
        cases.case(
            "meaningful",
            &[description],
            &some(meaningful_command_description(description)),
        );
    }
    let candidates = [
        "", "review", "brainstorm", "gh-address-comments", "rev", "résumé", "ab", "ba", "éa",
    ];
    let needles = ["", "rev", "brain", "gac", "xyz", "review", "vie", "wer", "é", "ré", "ésm", "aé"];
    for candidate in candidates {
        for needle in needles {
            cases.case(
                "score",
                &[candidate, needle],
                &completion_score(candidate, needle)
                    .map_or_else(|| "N".to_owned(), |score| score.to_string()),
            );
        }
    }
    for value in VALUES {
        for cursor in 0..=value.len() + 1 {
            let at = cursor.to_string();
            let query = completion_query(value, cursor).map_or_else(
                || "N".to_owned(),
                |query| {
                    format!(
                        "S{}",
                        fields(&[
                            query.needle.clone(),
                            query.replacement.start.to_string(),
                            query.replacement.end.to_string(),
                        ])
                    )
                },
            );
            cases.case("query", &[value, &at], &query);
            for settings in [None, Some(SETTINGS[0]), Some(SETTINGS[1]), Some(SETTINGS[18])] {
                let parsed = settings.and_then(LensSettings::from_json);
                cases.case(
                    "complete",
                    &[value, &at, &merged_spec, &some(settings)],
                    &completions_text(&composer_completions(
                        value,
                        cursor,
                        &merged,
                        parsed.as_ref(),
                    )),
                );
                if let Some(parsed) = parsed.as_ref() {
                    let expected = setting_completions(value, cursor, parsed).map_or_else(
                        || "N".to_owned(),
                        |found| format!("S{}", completions_text(&found)),
                    );
                    cases.case(
                        "setting",
                        &[value, &at, settings.unwrap_or_default()],
                        &expected,
                    );
                }
            }
        }
    }
    let crowd = (0..150)
        .map(|index| {
            command(
                &format!("/cmd-{:03}", (index * 37) % 150),
                &format!("Command {index}"),
                None,
                index % 2 == 0,
            )
        })
        .collect::<Vec<_>>();
    let crowd_spec = commands_text(&crowd);
    for value in ["/", "/cmd", "/c1", "/cmd-07", "/d0"] {
        cases.case(
            "complete",
            &[value, &value.len().to_string(), &crowd_spec, "N"],
            &completions_text(&composer_completions(value, value.len(), &crowd, None)),
        );
    }
    let wide = format!(
        "[{{\"id\":\"model\",\"currentValue\":\"m-090\",\"options\":[{}]}}]",
        (0..150)
            .map(|index| format!("{{\"value\":\"m-{index:03}\",\"name\":\"Model {}\"}}", (index * 7) % 150))
            .collect::<Vec<_>>()
            .join(",")
    );
    let parsed = LensSettings::from_json(&wide);
    for value in ["/model ", "/model m-0", "/model 9", "/model model 1"] {
        cases.case(
            "complete",
            &[value, &value.len().to_string(), &crowd_spec, &some(Some(&wide))],
            &completions_text(&composer_completions(
                value,
                value.len(),
                &crowd,
                parsed.as_ref(),
            )),
        );
    }
    for value in HINTS {
        cases.case(
            "hint",
            &[value, &merged_spec],
            &some(active_command_hint(value, &merged).as_deref()),
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).ok_or("usage: agent_card_vectors <output>")?;
    let mut cases = Cases::default();
    card_cases(&mut cases);
    settings_cases(&mut cases);
    completion_cases(&mut cases);
    let mut file = cases.count.to_le_bytes().to_vec();
    file.extend_from_slice(&cases.out);
    fs::write(path, file)?;
    Ok(())
}
