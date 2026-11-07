use std::{error::Error, sync::Arc};

use serde_json::{Value, json};
use zz_protocol::{GitMark, PathEntry, PathKind};
use zz_ui::files::{GitMarks, file_rows};

fn case(
    chunks: &[Arc<[PathEntry]>],
    marks: &[(String, GitMark)],
    base: &str,
    filter: &str,
) -> Value {
    let mut git = GitMarks::default();
    git.extend(marks.to_vec());
    let rows = file_rows(chunks, &git, base, filter);
    json!({"base":base,"filter":filter,"chunks":chunks.iter().map(|chunk|chunk.iter().map(|entry|json!([entry.rel,entry.kind as u8,entry.symlink])).collect::<Vec<_>>()).collect::<Vec<_>>(),"marks":marks.iter().map(|(rel,mark)|json!([rel,*mark as u8])).collect::<Vec<_>>(),"rows":rows.iter().map(|row|json!([row.entry.rel,row.entry.kind as u8,row.entry.symlink,row.name,row.mark.map(|mark|mark as u8),row.matches.iter().map(|range|[range.start,range.end]).collect::<Vec<_>>()])).collect::<Vec<_>>()})
}

fn entries(names: &[&str], offset: usize) -> Arc<[PathEntry]> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| PathEntry {
            rel: (*name).to_owned(),
            kind: if name.ends_with('/') {
                PathKind::Dir
            } else {
                PathKind::File
            },
            symlink: (index + offset).is_multiple_of(3),
        })
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let chunks = vec![
        entries(
            &[
                "src/",
                "src/main.rs",
                "src/lib.rs",
                "src/sub/",
                "src/sub/file",
                "main",
                "foo",
                "foo/",
                "foo//",
                "foo/bar",
            ],
            0,
        ),
        Arc::from([]),
        entries(
            &[
                "世界/",
                "世界/é.txt",
                "é.txt",
                "src/main.rs",
                "src/hidden//",
                "src/hidden/file",
                "src2/main",
                "foo/a",
                "foo/a",
                "",
            ],
            10,
        ),
    ];
    let mut cases = Vec::new();
    let marks = [
        ("src/".to_owned(), GitMark::Modified),
        ("src/sub/".to_owned(), GitMark::Conflicted),
        ("src/main.rs".to_owned(), GitMark::Added),
        ("src/main.rs".to_owned(), GitMark::Untracked),
        ("foo/".to_owned(), GitMark::Added),
        ("世界/".to_owned(), GitMark::Conflicted),
    ];
    for base in ["", "src/", "src", "src/sub/", "世界/", "missing/"] {
        for filter in ["", "m", "main", "foo", "é", "世", "sub/fi", "src", "M"] {
            for count in 0..=marks.len() {
                cases.push(case(&chunks, &marks[..count], base, filter));
            }
        }
    }
    for length in [21, 32, 64, 100, 199, 200, 201, 600] {
        let repeated: Arc<[PathEntry]> = (0..length)
            .map(|index| PathEntry {
                rel: ["foo/a", "foo/b", "foo/a", "foo/c"][index % 4].to_owned(),
                kind: if index % 5 == 0 {
                    PathKind::Dir
                } else {
                    PathKind::File
                },
                symlink: index.is_multiple_of(3),
            })
            .collect();
        for filter in ["", "foo"] {
            cases.push(case(&[Arc::clone(&repeated)], &[], "", filter));
        }
    }
    let many: Arc<[PathEntry]> = (0..50_000)
        .rev()
        .map(|index| PathEntry {
            rel: format!("dir/{index:05}/main.rs"),
            kind: PathKind::File,
            symlink: false,
        })
        .collect();
    cases.push(case(
        &[many],
        &[("dir/".to_owned(), GitMark::Modified)],
        "dir/",
        "main",
    ));
    serde_json::to_writer(std::io::stdout().lock(), &json!({"cases":cases}))?;
    Ok(())
}
