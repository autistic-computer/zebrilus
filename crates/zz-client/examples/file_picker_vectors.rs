use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use zz_client::file_picker::{
    AcceptTarget, FilePickerMode, PickerFlow, directory_query, entry_prior, preserved_selection,
    rank_directories, validate_selection,
};

const QUERIES: [&str; 9] = [
    "",
    " ",
    "src",
    "src ",
    "/abs/path",
    "  /abs/trimmed  ",
    "rel/path",
    "\u{3000}/wide",
    "~/home",
];
const ERRORS: [&str; 2] = ["Cannot read /w: denied", "/w/a is not a file."];

fn uint(out: &mut Vec<u8>, value: u64, count: usize) {
    out.extend_from_slice(&value.to_le_bytes()[..count]);
}

fn text(out: &mut Vec<u8>, value: &str) {
    uint(out, value.len() as u64, 4);
    out.extend_from_slice(value.as_bytes());
}

fn optional(out: &mut Vec<u8>, value: Option<&str>) {
    out.push(u8::from(value.is_some()));
    text(out, value.unwrap_or_default());
}

fn optional_index(out: &mut Vec<u8>, value: Option<usize>) {
    out.push(u8::from(value.is_some()));
    uint(out, value.unwrap_or_default() as u64, 4);
}

fn prior_vectors(out: &mut Vec<u8>) {
    let mut depths: Vec<u64> = (0..=70).collect();
    depths.extend([
        4_294_967_295,
        4_294_967_296,
        1 << 40,
        i64::MAX.unsigned_abs(),
    ]);
    uint(out, (depths.len() * 2) as u64, 4);
    for depth in depths {
        for workspace in [false, true] {
            uint(out, depth, 8);
            out.push(u8::from(workspace));
            let depth = usize::try_from(depth).expect("vectors run on 64-bit targets");
            uint(out, u64::from(entry_prior(depth, workspace)), 4);
        }
    }
}

fn selection_vectors(out: &mut Vec<u8>) {
    let cases: [(&[&str], Option<&str>); 9] = [
        (&[], None),
        (&[], Some("/w/a")),
        (&["/w/a"], None),
        (&["/w/a"], Some("/w/a")),
        (&["/w/a", "/w/b", "/w/c"], Some("/w/c")),
        (&["/w/a", "/w/b", "/w/c"], Some("/w/d")),
        (&["/w/a", "/w/b", "/w/b"], Some("/w/b")),
        (&["/w/a/", "/w/b"], Some("/w/a")),
        (&["/w/a", "/w/B"], Some("/w/b")),
    ];
    uint(out, cases.len() as u64, 4);
    for (entries, preferred) in cases {
        uint(out, entries.len() as u64, 4);
        for entry in entries {
            text(out, entry);
        }
        optional(out, preferred);
        optional_index(
            out,
            preserved_selection(
                entries.iter().map(|entry| Path::new(*entry)),
                preferred.map(Path::new),
            ),
        );
    }
}

fn query_vectors(out: &mut Vec<u8>) {
    let queries = [
        "",
        "  ",
        "src",
        "src/",
        "src//",
        "src\\",
        "src/\\/",
        " src/ ",
        "/",
        "//",
        "a/b/",
        "\tx/\n",
        "é/",
        "src/ x",
        "src / ",
        "\u{a0}src/\u{a0}",
        "\u{3000}src\u{3000}",
        "\u{200b}src/\u{200b}",
        "\u{85}a\u{2028}",
        "\u{1680}a/\u{2029}\u{202f}\u{205f}",
        "\u{2000}a\u{200a}",
        "\u{b}a\u{c}\r",
        "\u{1c}a\u{1f}",
        "a\u{feff}",
    ];
    uint(out, queries.len() as u64, 4);
    for query in queries {
        text(out, query);
        text(out, directory_query(query));
    }
}

fn rank_set(out: &mut Vec<u8>, labels: &[String], priors: &[u32], queries: &[&str]) {
    let borrowed: Vec<&str> = labels.iter().map(String::as_str).collect();
    uint(out, labels.len() as u64, 4);
    for (label, prior) in labels.iter().zip(priors) {
        text(out, label);
        uint(out, u64::from(*prior), 4);
    }
    uint(out, queries.len() as u64, 4);
    for query in queries {
        text(out, query);
        let ranked = rank_directories(query, &borrowed, |index| priors[index]);
        uint(out, ranked.len() as u64, 4);
        for index in ranked {
            uint(out, index as u64, 4);
        }
    }
}

fn rank_vectors(out: &mut Vec<u8>) {
    let tree = [
        ("src", false),
        ("src/editor", false),
        ("src/workspace", false),
        ("crates", false),
        ("crates/zz", true),
        ("crates/zz-client", false),
        ("crates/zz-client/src", false),
        ("docs", false),
        ("mojo", true),
        ("mojo/zzmojo", false),
        ("mojo/tests", false),
        ("Source", false),
        ("srv/Editor", true),
        ("a/b/c/d/e", false),
        ("é/ü", false),
        (".config", false),
        (".config/zz", true),
        ("scratch", false),
        ("src/editor/view", false),
        ("zz", false),
        ("zz", true),
        ("tests", false),
    ];
    let labels: Vec<String> = tree.iter().map(|(label, _)| (*label).to_owned()).collect();
    let priors: Vec<u32> = tree
        .iter()
        .map(|(label, workspace)| entry_prior(label.matches('/').count() + 1, *workspace))
        .collect();
    let many: Vec<String> = (0..700)
        .map(|index| {
            format!(
                "dir{:03}/sub{}/leaf{}",
                (index * 37) % 700,
                index % 7,
                index % 3
            )
        })
        .collect();
    let many_priors: Vec<u32> = (0..700_usize)
        .map(|index| entry_prior(index % 5 + 1, index % 11 == 0))
        .collect();
    let flat: Vec<String> = (0..40).map(|index| format!("p{:02}", 39 - index)).collect();
    let flat_priors = vec![7_u32; 40];
    uint(out, 4, 4);
    rank_set(
        out,
        &labels,
        &priors,
        &[
            "",
            " ",
            "src",
            "src/",
            "src\\",
            "SRC",
            "Src",
            "zz",
            "zzc",
            "ed",
            "editor",
            "crates/zz",
            "mjo",
            "  src  ",
            "x",
            "é",
            "config",
            "c",
            "s",
            "tests/",
            "srcc",
            "wrkspace",
            "/",
            "a/b",
            "edtor",
            "zz-client/src",
            "Editor",
            "view",
        ],
    );
    rank_set(
        out,
        &many,
        &many_priors,
        &["", "dir", "sub3", "d1", "leaf2/", "dir69", "sub", "0"],
    );
    rank_set(out, &flat, &flat_priors, &["", "p", "p1", "3"]);
    rank_set(out, &[], &[], &["", "a"]);
}

fn located(root: &Path, name: &str) -> PathBuf {
    if name.starts_with('/') {
        PathBuf::from(name)
    } else {
        root.join(name)
    }
}

fn validation_vectors(out: &mut Vec<u8>, root: &Path) {
    let names = [
        "plain.txt",
        "dir",
        "missing",
        "link",
        "dirlink",
        "dangling",
        "noperm.txt",
        "plain.txt/child",
        "dir/",
        "/dev/null",
        "/",
    ];
    uint(out, (names.len() * 2) as u64, 4);
    for name in names {
        for (code, mode) in [(0, FilePickerMode::Files), (1, FilePickerMode::Directories)] {
            text(out, name);
            out.push(code);
            let result = validate_selection(&located(root, name), mode);
            out.push(u8::from(result.is_ok()));
            text(out, &result.err().unwrap_or_default());
        }
    }
}

enum Op {
    ScanFinished(Result<bool, &'static str>),
    QueryChanged(&'static str, bool),
    BeginRank(bool),
    BeginAccept,
    AcceptTarget,
    Navigate(u32, i8),
    Hover(u32),
    ShowsFirstBatch(bool),
    ScanningNote(bool),
    EmptyMessage(bool, bool),
    Select(Option<u32>),
    Fail(&'static str),
    Generations(u64, u64),
}

fn mode(directories: bool) -> FilePickerMode {
    if directories {
        FilePickerMode::Directories
    } else {
        FilePickerMode::Files
    }
}

fn apply(out: &mut Vec<u8>, flow: &mut PickerFlow, op: &Op) {
    let mut body = Vec::new();
    match *op {
        Op::ScanFinished(result) => {
            out.push(0);
            out.push(u8::from(result.is_ok()));
            out.push(u8::from(result.unwrap_or(false)));
            text(out, result.err().unwrap_or_default());
            flow.scan_finished(result.map_err(str::to_owned));
        }
        Op::QueryChanged(query, indexed) => {
            out.push(1);
            text(out, query);
            out.push(u8::from(indexed));
            body.push(u8::from(flow.query_changed(query.to_owned(), indexed)));
        }
        Op::BeginRank(clear) => {
            out.push(2);
            out.push(u8::from(clear));
            uint(&mut body, flow.begin_rank(clear), 8);
        }
        Op::BeginAccept => {
            out.push(3);
            uint(&mut body, flow.begin_accept(), 8);
        }
        Op::AcceptTarget => {
            out.push(4);
            match flow.accept_target() {
                AcceptTarget::Typed(path) => {
                    body.push(0);
                    text(&mut body, path.to_str().expect("vector paths are UTF-8"));
                }
                AcceptTarget::Row(index) => {
                    body.push(1);
                    uint(&mut body, index as u64, 4);
                }
                AcceptTarget::Nothing => body.push(2),
            }
        }
        Op::Navigate(count, direction) => {
            out.push(5);
            uint(out, u64::from(count), 4);
            out.push(direction.to_le_bytes()[0]);
            optional_index(
                &mut body,
                flow.navigate(count as usize, isize::from(direction)),
            );
        }
        Op::Hover(index) => {
            out.push(6);
            uint(out, u64::from(index), 4);
            body.push(u8::from(flow.hover(index as usize)));
        }
        Op::ShowsFirstBatch(listed) => {
            out.push(7);
            out.push(u8::from(listed));
            body.push(u8::from(flow.shows_first_batch(listed)));
        }
        Op::ScanningNote(listed) => {
            out.push(8);
            out.push(u8::from(listed));
            body.push(u8::from(flow.scanning_note(listed)));
        }
        Op::EmptyMessage(input_empty, directories) => {
            out.push(9);
            out.push(u8::from(input_empty));
            out.push(u8::from(directories));
            text(
                &mut body,
                flow.empty_message(input_empty, mode(directories)),
            );
        }
        Op::Select(selected) => {
            out.push(10);
            optional_index(out, selected.map(|index| index as usize));
            flow.selected = selected.map(|index| index as usize);
        }
        Op::Fail(error) => {
            out.push(11);
            text(out, error);
            flow.error = Some(error.to_owned());
        }
        Op::Generations(rank, accept) => {
            out.push(12);
            uint(out, rank, 8);
            uint(out, accept, 8);
            flow.rank_generation = rank;
            flow.accept_generation = accept;
        }
    }
    text(&mut body, &flow.query);
    optional_index(&mut body, flow.selected);
    body.push(u8::from(flow.scanning));
    body.push(u8::from(flow.limited));
    optional(&mut body, flow.error.as_deref());
    uint(&mut body, flow.rank_generation, 8);
    uint(&mut body, flow.accept_generation, 8);
    uint(out, body.len() as u64, 4);
    out.extend_from_slice(&body);
}

struct Random(u64);

impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % bound as u64) as usize
    }

    fn flag(&mut self) -> bool {
        self.below(2) == 0
    }
}

fn walk(seed: u64, steps: usize) -> Vec<Op> {
    let mut random = Random(seed);
    (0..steps)
        .map(|_| match random.below(16) {
            0 => {
                if random.below(3) == 0 {
                    Op::ScanFinished(Err(ERRORS[random.below(ERRORS.len())]))
                } else {
                    Op::ScanFinished(Ok(random.flag()))
                }
            }
            1..=3 => Op::QueryChanged(QUERIES[random.below(QUERIES.len())], random.flag()),
            4 => Op::BeginRank(random.flag()),
            5 => Op::BeginAccept,
            6 | 7 => Op::AcceptTarget,
            8 | 9 => Op::Navigate(
                [0, 1, 2, 5][random.below(4)],
                [-1, 1, 0, -128, 127][random.below(5)],
            ),
            10 => Op::Hover(u32::try_from(random.below(6)).expect("small index")),
            11 => Op::ShowsFirstBatch(random.flag()),
            12 => Op::ScanningNote(random.flag()),
            13 => Op::EmptyMessage(random.flag(), random.flag()),
            14 => Op::Select(
                (random.below(3) != 0)
                    .then(|| u32::try_from(random.below(9)).expect("small index")),
            ),
            _ => Op::Fail(ERRORS[random.below(ERRORS.len())]),
        })
        .collect()
}

fn flow_vectors(out: &mut Vec<u8>) {
    let handwritten = vec![
        vec![
            Op::ShowsFirstBatch(false),
            Op::ShowsFirstBatch(true),
            Op::ScanningNote(true),
            Op::ScanningNote(false),
            Op::EmptyMessage(true, false),
            Op::AcceptTarget,
            Op::Navigate(0, 1),
            Op::Navigate(3, 1),
            Op::Navigate(3, 1),
            Op::Navigate(3, 1),
            Op::Navigate(3, -1),
            Op::Navigate(3, -1),
            Op::Hover(1),
            Op::Hover(1),
            Op::AcceptTarget,
            Op::QueryChanged("src", false),
            Op::QueryChanged("src", false),
            Op::BeginRank(true),
            Op::ShowsFirstBatch(false),
            Op::ScanFinished(Ok(true)),
            Op::ScanningNote(true),
            Op::EmptyMessage(true, true),
            Op::EmptyMessage(false, true),
            Op::EmptyMessage(true, false),
            Op::Fail("/w/a is not a file."),
            Op::EmptyMessage(false, false),
            Op::QueryChanged("srcs", false),
            Op::QueryChanged("src", true),
            Op::QueryChanged("  /abs/x  ", false),
            Op::Select(Some(4)),
            Op::AcceptTarget,
            Op::BeginAccept,
            Op::Select(Some(7)),
            Op::Navigate(3, 1),
            Op::Select(Some(7)),
            Op::Navigate(3, -1),
        ],
        vec![
            Op::ScanFinished(Err("Cannot read /w: denied")),
            Op::EmptyMessage(true, true),
            Op::QueryChanged("x", false),
            Op::EmptyMessage(false, true),
            Op::Generations(u64::MAX - 1, u64::MAX - 1),
            Op::BeginRank(false),
            Op::BeginRank(false),
            Op::BeginAccept,
            Op::BeginAccept,
            Op::QueryChanged("y", true),
        ],
    ];
    let seeds = [1_u64, 2, 3, 5, 8, 13];
    uint(out, (handwritten.len() + seeds.len()) as u64, 4);
    for ops in handwritten
        .into_iter()
        .chain(seeds.into_iter().map(|seed| walk(seed, 300)))
    {
        let mut flow = PickerFlow::new();
        uint(out, ops.len() as u64, 4);
        for op in &ops {
            apply(out, &mut flow, op);
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let output = arguments.next().ok_or("missing output path")?;
    let read_root = PathBuf::from(arguments.next().ok_or("missing read fixtures")?);
    let mut out = Vec::new();
    prior_vectors(&mut out);
    selection_vectors(&mut out);
    query_vectors(&mut out);
    rank_vectors(&mut out);
    validation_vectors(&mut out, &read_root);
    flow_vectors(&mut out);
    fs::write(&output, &out)?;
    println!("file-picker-vectors bytes={}", out.len());
    Ok(())
}
