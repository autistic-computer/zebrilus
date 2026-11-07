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

#[derive(Debug, Default)]
pub(crate) struct Listing {
    pub(crate) host: String,
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

