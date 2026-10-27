//! A folder on a host as the daemon walks it, and the rows a filter picks
//! from it: the folder's own entries while the filter is empty, the best
//! matches anywhere under it once you type, with git's letter on each.

use std::{collections::HashMap, ops::Range, sync::Arc};

use zz_client::path_rank;
use zz_gpui::SharedString;
use zz_protocol::{GitMark, PathEntry, PathKind, PathListRoot};

const MAX_ROWS: usize = 200;

#[derive(Clone, Debug, Default)]
pub(crate) struct GitMarks {
    exact: HashMap<String, GitMark>,
    dirs: Vec<(String, GitMark)>,
}

impl GitMarks {
    pub(crate) fn extend(&mut self, marks: Vec<(String, GitMark)>) {
        for (rel, mark) in marks {
            if rel.ends_with('/') {
                self.dirs.push((rel, mark));
            } else {
                self.exact.insert(rel, mark);
            }
        }
    }

    pub(crate) fn get(&self, rel: &str) -> Option<GitMark> {
        self.exact.get(rel).copied().or_else(|| {
            self.dirs
                .iter()
                .find(|(prefix, _)| {
                    rel.starts_with(prefix.as_str()) || rel == prefix.trim_end_matches('/')
                })
                .map(|(_, mark)| *mark)
        })
    }
}

pub(crate) const fn mark_letter(mark: GitMark) -> &'static str {
    match mark {
        GitMark::Modified => "M",
        GitMark::Added => "A",
        GitMark::Untracked => "U",
        GitMark::Conflicted => "C",
    }
}

/// Everything under one folder on one host, as the daemon walks it.
#[derive(Debug, Default)]
pub(crate) struct Listing {
    pub(crate) host: String,
    /// The folder asked for, backslashes resolved.
    pub(crate) dir: String,
    pub(crate) request: Option<u64>,
    pub(crate) root: Option<PathListRoot>,
    pub(crate) chunks: Vec<Arc<[PathEntry]>>,
    pub(crate) marks: Arc<GitMarks>,
    pub(crate) error: Option<SharedString>,
    pub(crate) done: bool,
    pub(crate) truncated: bool,
}

impl Listing {
    pub(crate) fn new(host: &str, dir: String) -> Self {
        Self {
            host: host.to_owned(),
            dir,
            ..Self::default()
        }
    }

    /// Whether the listing holds `dir` on `host` whole: `dir` is its folder
    /// or one under it, and the walk finished without stopping short.
    pub(crate) fn covers(&self, host: &str, dir: &str) -> bool {
        self.host == host
            && dir.starts_with(&self.dir)
            && (dir == self.dir || (self.done && !self.truncated && self.error.is_none()))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.chunks.iter().all(|chunk| chunk.is_empty())
    }

    pub(crate) fn begin(&mut self, result: Result<PathListRoot, String>) {
        match result {
            Ok(root) => self.root = Some(root),
            Err(error) => {
                self.error = Some(error.into());
                self.done = true;
            }
        }
    }

    /// Take a chunk of entries. Whether the rows may have changed.
    pub(crate) fn chunk(
        &mut self,
        entries: &Arc<[PathEntry]>,
        done: bool,
        truncated: bool,
    ) -> bool {
        let added = !entries.is_empty();
        if added {
            self.chunks.push(Arc::clone(entries));
        }
        let finished = done && !self.done;
        self.done |= done;
        self.truncated |= truncated;
        added || finished
    }

    pub(crate) fn git(&mut self, marks: Vec<(String, GitMark)>) {
        Arc::make_mut(&mut self.marks).extend(marks);
    }
}

/// One file or folder a filter kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileRow {
    pub(crate) entry: PathEntry,
    /// Its path under the folder listed, without a trailing `/`.
    pub(crate) name: String,
    pub(crate) matches: Vec<Range<usize>>,
    pub(crate) mark: Option<GitMark>,
}

/// The rows under `base`, a folder relative to the listing's own: its
/// entries, folders first, while `filter` is empty, and the best matches
/// anywhere under it once you type.
pub(crate) fn file_rows(
    chunks: &[Arc<[PathEntry]>],
    marks: &GitMarks,
    base: &str,
    filter: &str,
) -> Vec<FileRow> {
    let entries = chunks
        .iter()
        .flat_map(|chunk| chunk.iter())
        .filter(|entry| entry.rel.len() > base.len() && entry.rel.starts_with(base))
        .collect::<Vec<_>>();
    let label = |entry: &PathEntry| entry.rel[base.len()..].trim_end_matches('/').to_owned();
    let row = |entry: &PathEntry, matches| FileRow {
        entry: entry.clone(),
        name: label(entry),
        matches,
        mark: marks.get(&entry.rel),
    };
    if filter.is_empty() {
        let mut shown = entries
            .into_iter()
            .filter(|entry| !label(entry).contains('/'))
            .collect::<Vec<_>>();
        shown.sort_by(|left, right| {
            path_rank::browse_order(
                (left.rel.as_str(), left.kind == PathKind::Dir),
                (right.rel.as_str(), right.kind == PathKind::Dir),
            )
        });
        shown.truncate(MAX_ROWS);
        return shown
            .into_iter()
            .map(|entry| row(entry, Vec::new()))
            .collect();
    }
    let labels = entries.iter().map(|entry| label(entry)).collect::<Vec<_>>();
    let labels = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let kept = path_rank::rank(
        filter,
        &labels,
        |index| {
            path_rank::path_prior(
                filter,
                labels[index],
                marks.get(&entries[index].rel).is_some(),
            )
        },
        MAX_ROWS,
    );
    let kept_labels = kept.iter().map(|index| labels[*index]).collect::<Vec<_>>();
    kept.into_iter()
        .zip(path_rank::highlights(filter, &kept_labels))
        .map(|(index, matches)| row(entries[index], matches))
        .collect()
}
