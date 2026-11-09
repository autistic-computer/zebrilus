use std::{env, error::Error, fs, path::Path, time::Duration};

use zz_client::{
    agent_flip::{
        AgentFlips, BUSY_TITLE, CANCEL_LABEL, Flip, FlipMode, FlipTo, NOW_LABEL, WHEN_IDLE_LABEL,
        armed_label, busy_detail, busy_reason, flip_command, flip_tooltip,
    },
    agent_output::output_pane_args,
    agent_timeline::{
        AgentToolKind, AgentToolStatus, ComposerAction, PlanItemState, StepNesting,
        TimelineGroupKind, TimelineItem, TimelineRow, TimelineShape, TraceHeader,
        append_timeline_row, composer_action, context_usage_fraction, context_usage_tooltip,
        duration_label, ending_failure, final_assistant_entry_id, fold_timeline_rows,
        git_file_count_label, git_summary_tooltip, is_turn_row, plan_chip_label, plan_items,
        plan_progress, single_line, split_turn, task_tray_label, thought_tail, tool_tag,
        trace_header,
    },
};
use zz_protocol::PaneId;

#[derive(Clone)]
struct Entry {
    id: u64,
    parent: Option<u64>,
    shape: TimelineShape,
    label: String,
}

impl TimelineItem for Entry {
    fn id(&self) -> u64 {
        self.id
    }

    fn parent(&self) -> Option<u64> {
        self.parent
    }

    fn shape(&self) -> TimelineShape {
        self.shape
    }

    fn label(&self) -> &str {
        &self.label
    }
}

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
}

const KINDS: [AgentToolKind; 7] = [
    AgentToolKind::Read,
    AgentToolKind::Search,
    AgentToolKind::Edit,
    AgentToolKind::Execute,
    AgentToolKind::Fetch,
    AgentToolKind::Think,
    AgentToolKind::Other,
];

const STATUSES: [AgentToolStatus; 6] = [
    AgentToolStatus::Pending,
    AgentToolStatus::Running,
    AgentToolStatus::NeedsApproval,
    AgentToolStatus::Completed,
    AgentToolStatus::Failed,
    AgentToolStatus::Canceled,
];

fn shape(code: &str) -> TimelineShape {
    let bytes = code.as_bytes();
    match bytes[0] {
        b'u' => TimelineShape::User,
        b'a' => TimelineShape::Assistant,
        b'n' => TimelineShape::Notice,
        b'r' => TimelineShape::Reasoning,
        b'p' => TimelineShape::Plan,
        _ => TimelineShape::Tool(
            KINDS[usize::from(bytes[1] - b'0')],
            STATUSES[usize::from(bytes[2] - b'0')],
        ),
    }
}

fn entries(spec: &str) -> Vec<Entry> {
    spec.split(';')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let fields = entry.splitn(4, '|').collect::<Vec<_>>();
            Entry {
                id: fields[0].parse().unwrap_or(0),
                parent: fields[1].parse().ok(),
                shape: shape(fields[2]),
                label: fields[3].to_owned(),
            }
        })
        .collect()
}

fn numbers(values: &[usize]) -> String {
    values
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn rows_dump(rows: &[TimelineRow<Entry>]) -> String {
    rows.iter()
        .map(|row| match row {
            TimelineRow::Single(entry) => format!("S{}", entry.id),
            TimelineRow::Group { kind, id, entries } => format!(
                "{}{id}[{}]",
                if *kind == TimelineGroupKind::Turn {
                    'T'
                } else {
                    'R'
                },
                entries
                    .iter()
                    .map(|entry| entry.id.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn header_dump(header: &TraceHeader) -> String {
    let body = match header {
        TraceHeader::Waiting { detail } => format!("W:{detail}"),
        TraceHeader::Live { label, tool } => format!("L:{label}:{}", u8::from(*tool)),
        TraceHeader::Done { label, counts } => format!(
            "D:{label}:{}",
            counts
                .iter()
                .map(|count| format!(
                    "{}={}/{}/{}",
                    count.slot,
                    count.count,
                    u8::from(count.failed),
                    count.describe()
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
    };
    format!("{body}|{}", header.label())
}

fn optional(value: Option<&str>) -> String {
    value.map_or_else(|| "N".to_owned(), |value| format!("S{value}"))
}

fn timeline(cases: &mut Cases) {
    let mut specs = vec![
        String::new(),
        "1|-|u|hi;2|-|a|yo;3|-|t03|read a;4|-|r|think;5|-|a|done;6|-|n|note;7|-|p|plan;8|6|t34|x"
            .to_owned(),
        "1|-|u|a;2|1|t31|run\nnow;3|2|t01|read;4|3|a|sub;5|-|a|answer;6|-|a|more".to_owned(),
        "1|2|t00|a;2|1|t00|b;3|-|t12|grep\r\n  two ;4|9|r|r;5|-|t64|other;6|-|t54|think".to_owned(),
        "1|-|t22|edit;1|-|a|dup;2|1|a|child;3|-|u|q;4|3|a|r;5|-|r|t".to_owned(),
    ];
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    for _ in 0..160 {
        let count = 1 + random.next(12);
        let mut spec = String::new();
        for index in 0..count {
            let id = if random.next(12) == 0 {
                1 + random.next(count)
            } else {
                index + 1
            };
            let parent = match random.next(4) {
                0 => (1 + random.next(count + 2)).to_string(),
                _ => "-".to_owned(),
            };
            let shape = match random.next(9) {
                0 => "u".to_owned(),
                1 | 2 => "a".to_owned(),
                3 => "n".to_owned(),
                4 => "r".to_owned(),
                5 => "p".to_owned(),
                _ => format!("t{}{}", random.next(7), random.next(6)),
            };
            spec.push_str(&format!("{id}|{parent}|{shape}|step {index}\n x;"));
        }
        specs.push(spec);
    }
    for spec in &specs {
        let members = entries(spec);
        let folded = fold_timeline_rows(&members);
        let flags = folded
            .rows
            .iter()
            .map(|row| u8::from(is_turn_row(row)).to_string())
            .collect::<String>();
        cases.case(
            "fold",
            &[spec],
            &format!(
                "{}#{}#{}#{flags}",
                rows_dump(&folded.rows),
                numbers(&folded.entry_to_row),
                final_assistant_entry_id(&folded.rows)
                    .map_or_else(|| "-".to_owned(), |id| id.to_string()),
            ),
        );
        let mut rows = Vec::new();
        let mut steps = String::new();
        for entry in &members {
            let (index, new) = append_timeline_row(&mut rows, entry.clone());
            steps.push_str(&format!("({index},{})", u8::from(new)));
        }
        let replaced = rows.first_mut().is_some_and(|row: &mut TimelineRow<Entry>| {
            let id = row.id();
            let mut entry = members[0].clone();
            entry.label = "replaced".to_owned();
            row.replace_entry(id, entry.clone()) & !row.replace_entry(id + 1000, entry)
        });
        cases.case(
            "append",
            &[spec],
            &format!("{steps}#{}#{}", rows_dump(&rows), u8::from(replaced)),
        );
        let nesting = StepNesting::new(&members);
        let (shown, answer) = split_turn(&members, &nesting);
        let all = (0..members.len()).collect::<Vec<_>>();
        let mut dump = format!("{}#", numbers(&nesting.top));
        for (index, entry) in members.iter().enumerate() {
            if members.iter().position(|other| other.id == entry.id) == Some(index) {
                dump.push_str(&format!("{}={};", entry.id, nesting.substeps(entry.id)));
            }
        }
        dump.push_str(&format!("#{}#{}", numbers(&shown), numbers(&answer)));
        for answering in [false, true] {
            for live in [false, true] {
                for elapsed in [None, Some(Duration::from_secs(3725))] {
                    for steps in [&shown, &all] {
                        dump.push('#');
                        dump.push_str(&header_dump(&trace_header(
                            &members, &nesting, steps, answering, live, elapsed,
                        )));
                    }
                }
            }
        }
        dump.push_str(&format!(
            "#{:?}#{:?}",
            ending_failure(&members, &shown),
            ending_failure(&members, &all)
        ));
        cases.case("nest", &[spec], &dump);
    }
    for (status, _) in STATUSES.iter().enumerate() {
        for exit in [None, Some(0), Some(2), Some(-9)] {
            let exit_text = exit.map_or_else(|| "-".to_owned(), |code: i64| code.to_string());
            let tag = tool_tag(STATUSES[status], exit)
                .map(|(label, failed)| format!("{label}:{}", u8::from(failed)));
            cases.case(
                "tool_tag",
                &[&status.to_string(), &exit_text],
                &optional(tag.as_deref()),
            );
        }
    }
    for seconds in [0_u64, 1, 59, 60, 61, 3599, 3600, 3661, 86_399, 360_000] {
        cases.case(
            "duration",
            &[&seconds.to_string()],
            &duration_label(Duration::from_secs(seconds)),
        );
    }
    let long = "word é ".repeat(60);
    let unbroken = "é".repeat(300);
    for text in [
        "",
        " \n\t ",
        "one\n two\u{a0}three ",
        long.as_str(),
        unbroken.as_str(),
        &format!("{unbroken} tail words here"),
    ] {
        cases.case("thought_tail", &[text], &optional(thought_tail(text).as_deref()));
    }
    for text in ["plain", "a\nb", " a \r\n\r\n b\u{2003}\rc ", "\n", "x\r"] {
        cases.case("single_line", &[text], &single_line(text));
    }
    for source in [
        "",
        "- [x] done\n- [~] doing it \n- [ ] later\n",
        "  - [X] caps\r\n- [é] odd\n- [] none\n- [ ]\n-[x] no\n- [ ] first\n- [ ] second",
        "- [~] a\n- [~] b",
        "text only\n- [",
        "- [x] one\n- [x] two\n",
    ] {
        let items = plan_items(source)
            .iter()
            .map(|(state, text)| {
                format!(
                    "{}:{text}",
                    match state {
                        PlanItemState::Pending => 0,
                        PlanItemState::InProgress => 1,
                        PlanItemState::Done => 2,
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let (done, total, current) = plan_progress(source);
        let chip = plan_chip_label(source).map(|(label, _)| label);
        cases.case(
            "plan",
            &[source],
            &format!(
                "{items}#{done}/{total}/{}#{}",
                optional(current),
                optional(chip.as_deref())
            ),
        );
    }
    for count in [0_usize, 1, 12] {
        cases.case("tray", &[&count.to_string()], &task_tray_label(count));
    }
    for (active, content) in [(false, false), (false, true), (true, false), (true, true)] {
        let action = match composer_action(active, content) {
            ComposerAction::Send => "0",
            ComposerAction::Queue => "1",
            ComposerAction::Stop => "2",
        };
        cases.case(
            "composer",
            &[&u8::from(active).to_string(), &u8::from(content).to_string()],
            action,
        );
    }
    for (branch, files, additions, deletions) in [
        (None, 0_u32, 0_u32, 0_u32),
        (Some("main"), 1, 12, 3),
        (Some("feature/é"), 27, 4_000_000_000, 1),
    ] {
        cases.case(
            "git",
            &[
                branch.unwrap_or("-"),
                &files.to_string(),
                &additions.to_string(),
                &deletions.to_string(),
            ],
            &format!(
                "{}|{}",
                git_file_count_label(files),
                git_summary_tooltip(branch, files, additions, deletions)
            ),
        );
    }
    let mut usages = vec![
        (0_u64, 0_u64),
        (5, 0),
        (0, 10),
        (1, 8),
        (3, 8),
        (5, 8),
        (7, 8),
        (1, 200),
        (3, 200),
        (5, 1000),
        (15, 1000),
        (25, 1000),
        (300, 200),
        (u64::MAX, u64::MAX),
        (u64::MAX / 3, u64::MAX),
    ];
    for used in 0..=200 {
        usages.push((used, 200));
        usages.push((used * 5 + 1, 2000));
    }
    for (used, size) in usages {
        cases.case(
            "usage",
            &[&used.to_string(), &size.to_string()],
            &format!(
                "{}|{}",
                context_usage_fraction(used, size).to_bits(),
                context_usage_tooltip(used, size)
            ),
        );
    }
}

fn flip_state(flip: &Flip) -> String {
    let body = match flip {
        Flip::Idle => "i".to_owned(),
        Flip::Sending(mode) => format!("s{}", mode_number(*mode)),
        Flip::Busy(reason) => format!("b({reason})"),
        Flip::Armed => "a".to_owned(),
    };
    format!(
        "{body}{}{}{}",
        u8::from(flip.pending()),
        u8::from(flip.armed()),
        u8::from(flip.busy().is_some())
    )
}

const fn mode_number(mode: FlipMode) -> u8 {
    match mode {
        FlipMode::Ask => 0,
        FlipMode::Now => 1,
        FlipMode::WhenIdle => 2,
        FlipMode::Cancel => 3,
    }
}

const MODES: [FlipMode; 4] = [
    FlipMode::Ask,
    FlipMode::Now,
    FlipMode::WhenIdle,
    FlipMode::Cancel,
];

const fn side(value: u64) -> FlipTo {
    if value == 0 { FlipTo::Chat } else { FlipTo::Cli }
}

fn flip_script(script: &str) -> String {
    let mut flips = AgentFlips::default();
    let mut dump = String::new();
    for line in script.split('\n') {
        let fields = line.split('\t').collect::<Vec<_>>();
        let number = |index: usize| fields[index].parse::<u64>().unwrap_or(0);
        let request = |index: usize| fields[index].parse::<u64>().ok();
        let result = match fields[0] {
            "p" => u8::from(flips.press(PaneId(number(1)), side(number(2)), |_| request(3)))
                .to_string(),
            "c" => u8::from(flips.choose(
                PaneId(number(1)),
                MODES[usize::try_from(number(2)).unwrap_or(0)],
                |_| request(3),
            ))
            .to_string(),
            "d" => {
                flips.dismiss(PaneId(number(1)));
                String::new()
            }
            "s" => {
                let result = if fields[2] == "ok" {
                    Ok(())
                } else {
                    Err(fields[2])
                };
                optional(flips.settle(number(1), result).as_deref())
            }
            _ => {
                flips.observe_panes(fields[1..].iter().filter(|field| !field.is_empty()).map(
                    |field| {
                        let parts = field.split(',').collect::<Vec<_>>();
                        (
                            PaneId(parts[0].parse().unwrap_or(0)),
                            side(parts[1].parse().unwrap_or(0)),
                            parts[2] == "1",
                        )
                    },
                ));
                String::new()
            }
        };
        dump.push_str(&result);
        for pane in 1..=4 {
            dump.push(' ');
            dump.push_str(&flip_state(flips.state(PaneId(pane))));
        }
        dump.push(' ');
        for request in 1..=9 {
            dump.push_str(&u8::from(flips.owns(request)).to_string());
        }
        dump.push('\n');
    }
    dump
}

fn flip(cases: &mut Cases) {
    for error in [
        "",
        "busy:",
        "busy: a turn is running ",
        "flip-agent: busy: 2 tasks",
        "server said busy: no",
        "error: busy:   ",
        "not it",
        "x: busy: é",
    ] {
        cases.case("busy_reason", &[error], &optional(busy_reason(error)));
    }
    let mut labels = format!(
        "{}|{}|{BUSY_TITLE}|{WHEN_IDLE_LABEL}|{NOW_LABEL}|{CANCEL_LABEL}|{}",
        FlipTo::Chat.label(),
        FlipTo::Cli.label(),
        busy_detail("a turn")
    );
    for to in [FlipTo::Chat, FlipTo::Cli] {
        labels.push_str(&format!(
            "|{}|{}|{}",
            armed_label(to),
            flip_tooltip(to, false),
            flip_tooltip(to, true)
        ));
    }
    for mode in MODES {
        let command = flip_command(PaneId(42), mode);
        labels.push_str(&format!("|{}", command.name));
        for arg in &command.args {
            labels.push_str(&format!(" {}", arg.as_str()));
        }
    }
    cases.case("flip_labels", &[], &labels);
    let scripts = [
        "p\t1\t0\t1\ns\t1\tok\np\t1\t0\t-\np\t2\t1\t2\np\t2\t1\t3\ns\t2\tflip-agent: busy: a turn\np\t2\t1\t4",
        "p\t1\t0\t1\ns\t1\tbusy: tools\nc\t1\t3\t2\nc\t1\t2\t2\ns\t2\tok\nc\t1\t1\t3\nc\t1\t3\t3\ns\t3\tno such pane\np\t1\t0\t4\ns\t4\tok",
        "p\t1\t0\t1\ns\t1\tbusy:\nc\t1\t1\t2\ns\t2\tbroken\ns\t2\tok\ns\t7\toops\nd\t1",
        "o\t1,0,1\t2,1,0\np\t1\t0\t1\ns\t1\tdenied\np\t2\t1\t2\no\t1,0,0\t2,0,0\ns\t2\tok\nc\t3\t1\t5",
        "p\t1\t0\t1\ns\t1\tbusy: x\nd\t2\nd\t1\np\t1\t1\t2\no\t1,1,0\np\t3\t0\t3\no\t\ns\t2\tok\ns\t3\tlate",
        "p\t1\t0\t1\ns\t1\tbusy: x\nc\t1\t2\t2\np\t1\t0\t3\ns\t2\tok\no\t1,0,1\nc\t1\t3\t4\ns\t4\tok\no\t1,0,0",
        "p\t1\t0\t1\np\t1\t0\t2\ns\t1\tbusy: y\nc\t1\t2\t3\nc\t1\t1\t1\ns\t3\tx\ns\t1\tlate error\ns\t1\tok",
    ];
    for script in scripts {
        cases.case("flip_script", &[script], &flip_script(script));
    }
}

fn output(cases: &mut Cases) {
    let big = "line é of output\n".repeat(20_000);
    let offset = format!("x{big}");
    for (pane, cwd, command, exit, text) in [
        (7_u64, "/work", "true", None, "  \n"),
        (
            7,
            "/work",
            "cargo test",
            Some(101_i64),
            "error: it's `broken` \"here\"",
        ),
        (0, "", "", Some(0), "ok\n"),
        (
            123_456,
            "/tmp/a b",
            &"é".repeat(70),
            Some(-3),
            "a\u{0}b\u{7f}",
        ),
        (9, "/", "seq", None, big.as_str()),
        (9, "/", "seq", Some(1), offset.as_str()),
        (1, "/", "x", None, "ab"),
        (1, "/", "x", None, "abc"),
        (1, "/", "x", None, "abcd"),
    ] {
        let args = output_pane_args(PaneId(pane), Path::new(cwd), command, exit, text)
            .map(|args| args.join("\u{1f}"));
        cases.case(
            "output",
            &[
                &pane.to_string(),
                cwd,
                command,
                &exit.map_or_else(|| "-".to_owned(), |code| code.to_string()),
                text,
            ],
            &optional(args.as_deref()),
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).ok_or("usage: agent_logic_vectors <output>")?;
    let mut cases = Cases::default();
    timeline(&mut cases);
    flip(&mut cases);
    output(&mut cases);
    let mut file = cases.count.to_le_bytes().to_vec();
    file.extend_from_slice(&cases.out);
    fs::write(path, file)?;
    Ok(())
}
