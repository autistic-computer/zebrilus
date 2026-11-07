use std::{collections::HashMap, sync::Arc};

use serde_json::{Value, json};
use zz_protocol::{GitMark, InsertStyle, PathEntry, PathKind, PathListRoot, ProtocolMessage};

#[derive(Clone, Debug, serde::Serialize)]
struct SharedString(String);

impl From<String> for SharedString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../mojo/scripts/path_listing_reference.rs"
));

fn begin(error: bool) -> ProtocolMessage {
    ProtocolMessage::PathListBegin {
        request_id: 7,
        result: if error {
            Err("not found 世界".to_owned())
        } else {
            Ok(PathListRoot {
                root: "/home/me/src".to_owned(),
                display_root: "~/src/".to_owned(),
                cwd: Some("/home/me".to_owned()),
                insert: InsertStyle::Claude,
            })
        },
    }
}

fn chunk(entries: bool, done: bool, truncated: bool) -> ProtocolMessage {
    ProtocolMessage::PathListChunk {
        request_id: 7,
        entries: if entries {
            vec![
                PathEntry {
                    rel: "src/".to_owned(),
                    kind: PathKind::Dir,
                    symlink: false,
                },
                PathEntry {
                    rel: "src/main.rs".to_owned(),
                    kind: PathKind::File,
                    symlink: true,
                },
            ]
        } else {
            vec![]
        },
        done,
        truncated,
    }
}

fn git(marks: &[(&str, GitMark)]) -> ProtocolMessage {
    ProtocolMessage::PathListGit {
        request_id: 7,
        marks: marks
            .iter()
            .map(|(rel, mark)| ((*rel).to_owned(), *mark))
            .collect(),
    }
}

fn trace(name: &str, updates: &[ProtocolMessage]) -> Value {
    let lookups = [
        "src",
        "src/",
        "src/main.rs",
        "src/lib.rs",
        "src/nested/x",
        "src2",
        "a",
        "a/b",
        "other",
        "",
    ];
    let dirs = ["~/src/", "~/src/sub/", "~/src/../", "~/src2/", "", "~/"];
    let mut listing = Listing::new("here", "~/src/".to_owned());
    listing.request = Some(7);
    let mut states = Vec::new();
    for update in updates {
        let changed = match update {
            ProtocolMessage::PathListBegin { result, .. } => {
                listing.begin(result.clone());
                true
            }
            ProtocolMessage::PathListChunk {
                entries,
                done,
                truncated,
                ..
            } => listing.chunk(&Arc::from(entries.clone()), *done, *truncated),
            ProtocolMessage::PathListGit { marks, .. } => {
                listing.git(marks.clone());
                true
            }
            _ => unreachable!(),
        };
        let mut covers = Vec::new();
        for host in ["here", "there"] {
            covers.extend(dirs.iter().map(|dir| listing.covers(host, dir)));
        }
        states.push(json!({
            "changed": changed,
            "root": listing.root,
            "chunks": listing.chunks.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
            "marks": lookups.iter().map(|rel| listing.marks.get(rel)).collect::<Vec<_>>(),
            "letters": lookups.iter().map(|rel| listing.marks.get(rel).map(mark_letter)).collect::<Vec<_>>(),
            "error": listing.error,
            "done": listing.done,
            "truncated": listing.truncated,
            "empty": listing.is_empty(),
            "covers": covers,
        }));
    }
    json!({"name": name, "host": listing.host, "dir": listing.dir, "request": listing.request, "lookups": lookups, "dirs": dirs, "updates": updates, "states": states})
}

fn main() {
    let mut traces = Vec::new();
    let mut operations = vec![
        begin(false),
        begin(true),
        git(&[]),
        git(&[("src/", GitMark::Modified), ("src/main.rs", GitMark::Added)]),
    ];
    for entries in [false, true] {
        for done in [false, true] {
            for truncated in [false, true] {
                operations.push(chunk(entries, done, truncated));
            }
        }
    }
    for left in &operations {
        for right in &operations {
            traces.push(trace(
                "source-transition",
                &[
                    left.clone(),
                    right.clone(),
                    chunk(false, true, false),
                    git(&[("src/main.rs", GitMark::Conflicted)]),
                    begin(false),
                ],
            ));
        }
    }
    for first in [
        GitMark::Modified,
        GitMark::Added,
        GitMark::Untracked,
        GitMark::Conflicted,
    ] {
        for second in [
            GitMark::Modified,
            GitMark::Added,
            GitMark::Untracked,
            GitMark::Conflicted,
        ] {
            traces.push(trace(
                "mark-precedence",
                &[
                    begin(false),
                    git(&[("src/", first), ("src/nested/", second), ("src/", second)]),
                    git(&[
                        ("src/main.rs", first),
                        ("src/main.rs", second),
                        ("a/", first),
                    ]),
                    git(&[("src/main.rs", first)]),
                    chunk(true, true, false),
                ],
            ));
        }
    }
    println!("{}", json!({"traces": traces}));
}
