use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use zz_client::editor_pane::{
    EditorEffect, EditorModel, Retry, SyncAction, descriptor_sync_action, editor_title,
    language_for_path, read_editor_file, save_error, validated_descriptor_path,
};
use zz_daemon_client::atomic_write;
use zz_protocol::EditorDescriptor;

const PATHS: [&str; 6] = [
    "/w/a.rs",
    "/w/b.md",
    "/w/sub/c.json",
    "/w/d",
    "rel.rs",
    "/w/bad\u{1}.rs",
];
const CWDS: [&str; 3] = ["/w", "/w2", "rel"];
const CONTENTS: [&str; 4] = ["", "one", "two\n", "three"];
const MESSAGES: [&str; 2] = ["boom", "Could not open /w/a.rs: denied"];

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

fn shown(path: &Path) -> &str {
    path.to_str().expect("vector paths are UTF-8")
}

fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn effects(out: &mut Vec<u8>, effects: &[EditorEffect]) {
    uint(out, effects.len() as u64, 4);
    for effect in effects {
        match effect {
            EditorEffect::SetBuffer { contents, language } => {
                out.push(0);
                text(out, contents);
                text(out, language);
            }
            EditorEffect::SetLanguage(language) => {
                out.push(1);
                text(out, language);
            }
            EditorEffect::Load(path) => {
                out.push(2);
                text(out, shown(path));
            }
            EditorEffect::Write { path, contents } => {
                out.push(3);
                text(out, shown(path));
                text(out, contents);
            }
            EditorEffect::PromptNewPath {
                directory,
                suggested,
            } => {
                out.push(4);
                text(out, shown(directory));
                text(out, suggested);
            }
            EditorEffect::PublishPath(path) => {
                out.push(5);
                text(out, path);
            }
            EditorEffect::PublishTitle(title) => {
                out.push(6);
                text(out, title);
            }
        }
    }
}

fn state(out: &mut Vec<u8>, model: &EditorModel) {
    text(out, shown(model.cwd()));
    optional(out, model.path().map(shown));
    optional(out, model.snapshot_path());
    optional(out, model.pending_path());
    text(out, model.saved_contents());
    out.push(u8::from(model.dirty()));
    text(out, model.title());
    out.push(u8::from(model.error().is_some()));
    if let Some(error) = model.error() {
        text(out, &error.message);
        let (kind, path) = match &error.retry {
            Retry::Open(path) => (0, path),
            Retry::Save(path) => (1, path),
        };
        out.push(kind);
        text(out, shown(path));
        text(out, error.retry.title());
    }
    out.push(u8::from(model.picker_open()));
}

fn dump(out: &mut Vec<u8>, result: &[u8], produced: &[EditorEffect], model: &EditorModel) {
    let mut body = result.to_vec();
    effects(&mut body, produced);
    state(&mut body, model);
    uint(out, body.len() as u64, 4);
    out.extend_from_slice(&body);
}

fn descriptor(path: Option<&str>, cwd: &str) -> EditorDescriptor {
    EditorDescriptor {
        path: path.map(str::to_owned),
        cwd: cwd.to_owned(),
    }
}

enum Op<'a> {
    Synchronize(Option<&'a str>, &'a str),
    BufferChanged(&'a str),
    OpenPicker,
    PickerClosed(Option<&'a str>),
    FinishLoad(&'a str, Result<&'a str, &'a str>),
    Save(&'a str),
    BeginSave(&'a str, &'a str),
    FinishSave(&'a str, &'a str, Result<(), &'a str>, &'a str),
    Retry(u8, &'a str, &'a str),
    ScratchHint(bool),
}

fn apply(out: &mut Vec<u8>, model: &mut EditorModel, op: &Op<'_>) {
    let mut result = Vec::new();
    let produced = match *op {
        Op::Synchronize(path, cwd) => {
            out.push(0);
            optional(out, path);
            text(out, cwd);
            model.synchronize(&descriptor(path, cwd))
        }
        Op::BufferChanged(contents) => {
            out.push(1);
            text(out, contents);
            model.buffer_changed(contents)
        }
        Op::OpenPicker => {
            out.push(2);
            result.push(u8::from(model.open_picker()));
            Vec::new()
        }
        Op::PickerClosed(path) => {
            out.push(3);
            optional(out, path);
            model.picker_closed(path.map(PathBuf::from))
        }
        Op::FinishLoad(path, loaded) => {
            out.push(4);
            text(out, path);
            out.push(u8::from(loaded.is_ok()));
            text(out, loaded.unwrap_or_else(|message| message));
            model.finish_load(
                PathBuf::from(path),
                loaded.map(str::to_owned).map_err(str::to_owned),
            )
        }
        Op::Save(contents) => {
            out.push(5);
            text(out, contents);
            model.save(contents)
        }
        Op::BeginSave(path, contents) => {
            out.push(6);
            text(out, path);
            text(out, contents);
            model.begin_save(PathBuf::from(path), contents)
        }
        Op::FinishSave(path, saved, written, contents) => {
            out.push(7);
            text(out, path);
            text(out, saved);
            out.push(u8::from(written.is_ok()));
            text(out, written.err().unwrap_or_default());
            text(out, contents);
            model.finish_save(
                PathBuf::from(path),
                saved.to_owned(),
                written.map_err(str::to_owned),
                contents,
            )
        }
        Op::Retry(kind, path, contents) => {
            out.push(8);
            out.push(kind);
            text(out, path);
            text(out, contents);
            let retry = if kind == 0 {
                Retry::Open(PathBuf::from(path))
            } else {
                Retry::Save(PathBuf::from(path))
            };
            model.retry(retry, contents)
        }
        Op::ScratchHint(empty) => {
            out.push(9);
            out.push(u8::from(empty));
            result.push(u8::from(model.scratch_hint(empty)));
            Vec::new()
        }
    };
    dump(out, &result, &produced, model);
}

fn scenario(out: &mut Vec<u8>, path: Option<&str>, cwd: &str, ops: &[Op<'_>]) {
    optional(out, path);
    text(out, cwd);
    let (mut model, produced) = EditorModel::new(&descriptor(path, cwd));
    dump(out, &[], &produced, &model);
    uint(out, ops.len() as u64, 4);
    for op in ops {
        apply(out, &mut model, op);
    }
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

    fn pick<'a>(&mut self, pool: &[&'a str]) -> &'a str {
        pool[self.below(pool.len())]
    }

    fn maybe<'a>(&mut self, pool: &[&'a str]) -> Option<&'a str> {
        (self.below(4) != 0).then(|| self.pick(pool))
    }
}

fn walk(seed: u64, steps: usize) -> Vec<Op<'static>> {
    let mut random = Random(seed);
    (0..steps)
        .map(|_| match random.below(14) {
            0 | 1 => Op::Synchronize(random.maybe(&PATHS), random.pick(&CWDS)),
            2 | 3 => Op::BufferChanged(random.pick(&CONTENTS)),
            4 => Op::OpenPicker,
            5 => Op::PickerClosed(random.maybe(&PATHS)),
            6 | 7 => {
                let path = random.pick(&PATHS);
                let loaded = if random.below(4) == 0 {
                    Err(random.pick(&MESSAGES))
                } else {
                    Ok(random.pick(&CONTENTS))
                };
                Op::FinishLoad(path, loaded)
            }
            8 => Op::Save(random.pick(&CONTENTS)),
            9 => Op::BeginSave(random.pick(&PATHS), random.pick(&CONTENTS)),
            10 | 11 => {
                let path = random.pick(&PATHS);
                let saved = random.pick(&CONTENTS);
                let written = if random.below(4) == 0 {
                    Err(random.pick(&MESSAGES))
                } else {
                    Ok(())
                };
                Op::FinishSave(path, saved, written, random.pick(&CONTENTS))
            }
            12 => Op::Retry(
                u8::from(random.below(2) == 1),
                random.pick(&PATHS),
                random.pick(&CONTENTS),
            ),
            _ => Op::ScratchHint(random.below(2) == 0),
        })
        .collect()
}

fn sync_vectors(out: &mut Vec<u8>) {
    let values = [None, Some("/w/a.rs"), Some("/w/b.rs")];
    let mut cases = Vec::new();
    for pending in values {
        for snapshot in values {
            for local in values {
                cases.push((pending, snapshot, local));
            }
        }
    }
    cases.extend([
        (None, Some("/w//a.rs"), Some("/w/a.rs")),
        (None, Some("/w/a.rs/"), Some("/w/a.rs")),
        (None, Some("/w/./a.rs"), Some("/w/a.rs")),
        (None, Some("/w/a.rs/."), Some("/w/a.rs")),
        (None, Some("//w/a"), Some("/w/a")),
        (None, Some("/."), Some("/")),
        (None, Some("./a.rs"), Some("a.rs")),
        (None, Some("a/./b"), Some("a/b")),
        (None, Some("./."), Some(".")),
        (None, Some("."), Some("./")),
        (None, Some("."), Some("")),
        (None, Some(""), Some("")),
        (None, Some(""), None),
        (None, Some("/w/../a.rs"), Some("/a.rs")),
        (None, Some("a/.."), Some("a")),
        (None, Some("/w/a"), Some("w/a")),
        (None, Some("/W/a.rs"), Some("/w/a.rs")),
        (Some("/w//a.rs"), Some("/w/a.rs"), Some("/w/a.rs")),
        (Some("/w/a.rs"), Some("/w/a.rs"), None),
    ]);
    uint(out, cases.len() as u64, 4);
    for (pending, snapshot, local) in cases {
        optional(out, pending);
        optional(out, snapshot);
        optional(out, local);
        out.push(
            match descriptor_sync_action(pending, snapshot, local.map(Path::new)) {
                SyncAction::Ignore => 0,
                SyncAction::ClearPending => 1,
                SyncAction::Restore => 2,
            },
        );
    }
}

fn name_vectors(out: &mut Vec<u8>) {
    let paths = [
        "/tmp/main.rs",
        "/tmp/MAIN.RS",
        "/tmp/a.Md",
        "/tmp/a.markdown",
        "/tmp/a.mdx",
        "/tmp/a.json",
        "/tmp/a.JSONC",
        "/tmp/Cargo.toml",
        "/tmp/notes.txt",
        "/tmp/noext",
        "/tmp/.rs",
        "/tmp/.hidden.json",
        "/tmp/a.b.rs",
        "/tmp/dir.rs/file",
        "/tmp/a.rs/",
        "/tmp/a.rs//",
        "/tmp/a.rs/.",
        "/tmp/a.rs/./",
        "/tmp/a.rs/..",
        "/tmp/a.",
        "/tmp/..",
        "/tmp/...",
        "/tmp/ü.toml",
        "/tmp/a.rs ",
        "/",
        "//",
        "",
        ".",
        "./",
        "..",
        "main.rs",
        "./main.md",
    ];
    uint(out, paths.len() as u64, 4);
    for path in paths {
        text(out, path);
        text(out, language_for_path(Path::new(path)));
        text(out, &editor_title(Some(Path::new(path)), false));
        text(out, &editor_title(Some(Path::new(path)), true));
    }
    text(out, &editor_title(None, false));
    text(out, &editor_title(None, true));
}

fn validation_vectors(out: &mut Vec<u8>) {
    let long = format!("/{}", "a".repeat(16_383));
    let longer = format!("/{}", "a".repeat(16_384));
    let relative_long = "a".repeat(16_385);
    let long_control = format!("{}\u{1}", "a".repeat(16_384));
    let cases: Vec<(&str, &str)> = vec![
        ("/w/a.rs", "/w"),
        ("a.rs", "/w"),
        ("/w/a.rs", "w"),
        ("a.rs", "w"),
        (&long, "/w"),
        (&longer, "/w"),
        ("/w/a.rs", &long),
        ("/w/a.rs", &longer),
        (&relative_long, "/w"),
        (&long_control, "/w"),
        ("/w/a\u{1}", "/w"),
        ("/w/a\u{0}b", "/w"),
        ("/w/a\n", "/w"),
        ("/w/a\tb", "/w"),
        ("/w/a\u{1f}", "/w"),
        ("/w/a b", "/w"),
        ("/w/a\u{7f}", "/w"),
        ("/w/a\u{80}", "/w"),
        ("/w/a\u{85}", "/w"),
        ("/w/a\u{9f}", "/w"),
        ("/w/a\u{a0}", "/w"),
        ("/w/a\u{7e}", "/w"),
        ("a\u{1}", "/w"),
        ("/w/a.rs", "/w\u{85}"),
        ("/w/a\u{1}", "w\u{1}"),
        ("", "/w"),
        ("/w/a.rs", ""),
        ("/w/é.rs", "/w"),
        ("/w/\u{2028}.rs", "/w"),
        ("/", "/"),
    ];
    uint(out, cases.len() as u64, 4);
    for (path, cwd) in cases {
        text(out, path);
        text(out, cwd);
        let result = validated_descriptor_path(Path::new(path), Path::new(cwd));
        out.push(u8::from(result.is_ok()));
        text(out, &result.unwrap_or_else(|message| message));
    }
}

fn located(root: &Path, name: &str) -> PathBuf {
    if name.starts_with('/') {
        PathBuf::from(name)
    } else {
        root.join(name)
    }
}

fn read_vectors(out: &mut Vec<u8>, root: &Path) {
    let names = [
        "plain.txt",
        "empty",
        "missing",
        "dir",
        "nul.bin",
        "invalid.txt",
        "truncated.txt",
        "overlong.txt",
        "surrogate.txt",
        "highest.txt",
        "beyond.txt",
        "utf8.md",
        "bom.txt",
        "exact.txt",
        "over.bin",
        "noperm.txt",
        "link",
        "dangling",
        "plain.txt/child",
        "/dev/null",
        "/dev/zero",
    ];
    uint(out, names.len() as u64, 4);
    for name in names {
        text(out, name);
        match read_editor_file(&located(root, name)) {
            Ok(contents) => {
                out.push(1);
                uint(out, contents.len() as u64, 8);
                uint(out, hash(contents.as_bytes()), 8);
            }
            Err(message) => {
                out.push(0);
                text(out, &message);
            }
        }
    }
}

fn save_vectors(out: &mut Vec<u8>, root: &Path) {
    let saves = [
        ("new.txt", "hello\n"),
        ("keep.txt", "new contents"),
        ("link.txt", "through link"),
        ("deep/er/file.md", "# hi\n"),
        ("ro/file.txt", "x"),
        ("rolink.txt", "replaced link"),
        ("dangling.txt", "was dangling"),
        ("plain/child.txt", "x"),
        ("exec.sh", "#!/bin/sh\n"),
        ("empty.txt", ""),
        ("uni/ü.txt", "héllo"),
        ("new.txt", "second pass\n"),
        ("deep/./dotted.txt", "dotted"),
        ("deep/er/..", "x"),
        ("/", "x"),
    ];
    let prefix = shown(root).to_owned();
    uint(out, saves.len() as u64, 4);
    for (name, contents) in saves {
        text(out, name);
        text(out, contents);
        let path = located(root, name);
        let result = atomic_write(&path, contents.as_bytes())
            .map_err(|error| save_error(&path, &error).replace(&prefix, "<root>"));
        out.push(u8::from(result.is_ok()));
        text(out, &result.err().unwrap_or_default());
    }
}

fn model_vectors(out: &mut Vec<u8>) {
    let long = format!("/w/{}", "a".repeat(16_400));
    let handwritten: Vec<(Option<&str>, &str, Vec<Op<'_>>)> = vec![
        (
            None,
            "/w",
            vec![
                Op::ScratchHint(true),
                Op::ScratchHint(false),
                Op::BufferChanged("draft"),
                Op::Save("draft"),
                Op::BeginSave("/w/new.rs", "draft"),
                Op::BufferChanged("draft more"),
                Op::FinishSave("/w/new.rs", "draft", Ok(()), "draft more"),
                Op::Synchronize(None, "/w"),
                Op::Synchronize(Some("/w/old.rs"), "/w"),
                Op::Synchronize(Some("/w/new.rs"), "/w"),
                Op::Synchronize(Some("/w/new.rs"), "/w"),
                Op::Save("draft more"),
                Op::FinishSave("/w/new.rs", "draft more", Ok(()), "draft more"),
                Op::Synchronize(None, "/w3"),
                Op::ScratchHint(true),
            ],
        ),
        (
            Some("/w/a.rs"),
            "/w",
            vec![
                Op::FinishLoad("/w/a.rs", Err("Could not open /w/a.rs: gone")),
                Op::ScratchHint(true),
                Op::Retry(0, "/w/a.rs", ""),
                Op::FinishLoad("/w/a.rs", Ok("fn main() {}\n")),
                Op::BufferChanged("fn main() {}\n"),
                Op::BufferChanged("fn main() { }\n"),
                Op::BufferChanged("fn main() {  }\n"),
                Op::BufferChanged("fn main() {}\n"),
                Op::OpenPicker,
                Op::OpenPicker,
                Op::ScratchHint(true),
                Op::PickerClosed(None),
                Op::OpenPicker,
                Op::PickerClosed(Some("/w/b.md")),
                Op::FinishLoad("/w/b.md", Ok("# b\n")),
                Op::Synchronize(Some("/w/a.rs"), "/w"),
                Op::Synchronize(Some("/w/b.md"), "/w"),
                Op::Synchronize(Some("/w/c.json"), "/w"),
                Op::FinishLoad("/w/c.json", Ok("{}")),
            ],
        ),
        (
            Some("/w/a.rs"),
            "/w",
            vec![
                Op::FinishLoad("/w/a.rs", Ok("one")),
                Op::BufferChanged("two"),
                Op::Save("two"),
                Op::FinishSave("/w/a.rs", "two", Err("Could not save /w/a.rs: full"), "two"),
                Op::Retry(1, "/w/a.rs", "two"),
                Op::FinishSave("/w/a.rs", "two", Ok(()), "two"),
                Op::BeginSave("relative.rs", "two"),
                Op::Retry(1, "relative.rs", "two"),
                Op::BeginSave(&long, "two"),
                Op::FinishLoad(&long, Ok("x")),
                Op::FinishSave(&long, "x", Ok(()), "x"),
                Op::FinishLoad("/w/ctl\u{7f}.rs", Ok("x")),
                Op::OpenPicker,
                Op::PickerClosed(None),
            ],
        ),
        (
            Some("relative.md"),
            "rel",
            vec![
                Op::FinishLoad("relative.md", Ok("x")),
                Op::Save("x"),
                Op::Synchronize(None, "rel"),
                Op::Save("x"),
                Op::FinishLoad("/w/a.rs", Ok("x")),
                Op::Synchronize(None, "/w"),
                Op::FinishLoad("/w/a.rs", Ok("x")),
                Op::Synchronize(None, "/w"),
                Op::Synchronize(Some("/w/a.rs"), "/w"),
                Op::Synchronize(None, "/w"),
                Op::Synchronize(Some("/w/a.rs"), "/w"),
                Op::FinishLoad("/w/a.rs", Ok("y")),
                Op::Synchronize(Some("/w//a.rs"), "/w"),
                Op::FinishLoad("/w/a.rs", Ok("z")),
                Op::Synchronize(Some("/w/./a.rs"), "/w"),
                Op::Synchronize(Some("/w/a.rs"), "/w"),
                Op::Synchronize(Some("/w/a.rs/"), "/w"),
                Op::Synchronize(Some("/w/a.rs/.."), "/w"),
            ],
        ),
    ];
    let seeds = [1_u64, 2, 3, 5, 8, 13, 21, 34];
    uint(out, (handwritten.len() + seeds.len()) as u64, 4);
    for (path, cwd, ops) in &handwritten {
        scenario(out, *path, cwd, ops);
    }
    let mut random = Random(99);
    for seed in seeds {
        let path = random.maybe(&PATHS);
        scenario(out, path, random.pick(&CWDS), &walk(seed, 400));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let output = arguments.next().ok_or("missing output path")?;
    let read_root = PathBuf::from(arguments.next().ok_or("missing read fixtures")?);
    let save_root = PathBuf::from(arguments.next().ok_or("missing save directory")?);
    let mut out = Vec::new();
    sync_vectors(&mut out);
    name_vectors(&mut out);
    validation_vectors(&mut out);
    read_vectors(&mut out, &read_root);
    save_vectors(&mut out, &save_root);
    model_vectors(&mut out);
    fs::write(&output, &out)?;
    println!("editor-pane-vectors bytes={}", out.len());
    Ok(())
}
