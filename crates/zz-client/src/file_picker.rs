use std::path::{Path, PathBuf};

pub const MAX_PICKER_ROWS: usize = 500;
pub const WORKSPACE_PRIOR: u32 = 1 << 10;
pub const SCANNING_NOTE: &str = "Scanning…";
pub const LIMITED_NOTE: &str = "Search limited. Enter a full path to open another folder.";
pub const NO_MATCH: &str = "Nothing matches that search.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilePickerMode {
    Files,
    Directories,
}

impl FilePickerMode {
    #[must_use]
    pub const fn empty_label(self) -> &'static str {
        match self {
            Self::Files => "No files under this folder.",
            Self::Directories => "No folders under this folder.",
        }
    }
}

#[must_use]
pub fn entry_prior(depth: usize, is_workspace: bool) -> u32 {
    let shallowness = u32::try_from(depth).map_or(0, |depth| 64_u32.saturating_sub(depth));
    if is_workspace {
        WORKSPACE_PRIOR + shallowness
    } else {
        shallowness
    }
}

pub fn preserved_selection<'a>(
    mut entries: impl ExactSizeIterator<Item = &'a Path>,
    preferred: Option<&Path>,
) -> Option<usize> {
    let count = entries.len();
    preferred
        .and_then(|path| entries.position(|entry| entry == path))
        .or_else(|| (count > 0).then_some(0))
}

pub fn validate_selection(path: &Path, mode: FilePickerMode) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    if match mode {
        FilePickerMode::Files => metadata.is_file(),
        FilePickerMode::Directories => metadata.is_dir(),
    } {
        Ok(())
    } else {
        Err(format!(
            "{} is not a {}.",
            path.display(),
            match mode {
                FilePickerMode::Files => "file",
                FilePickerMode::Directories => "folder",
            }
        ))
    }
}

#[must_use]
pub fn directory_query(query: &str) -> &str {
    query.trim().trim_end_matches(['/', '\\'])
}

pub fn rank_directories(query: &str, labels: &[&str], prior: impl Fn(usize) -> u32) -> Vec<usize> {
    crate::path_rank::rank(directory_query(query), labels, prior, MAX_PICKER_ROWS)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcceptTarget {
    Typed(PathBuf),
    Row(usize),
    Nothing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerFlow {
    pub query: String,
    pub selected: Option<usize>,
    pub scanning: bool,
    pub limited: bool,
    pub error: Option<String>,
    pub rank_generation: u64,
    pub accept_generation: u64,
}

impl Default for PickerFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl PickerFlow {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            query: String::new(),
            selected: None,
            scanning: true,
            limited: false,
            error: None,
            rank_generation: 0,
            accept_generation: 0,
        }
    }

    #[must_use]
    pub fn shows_first_batch(&self, listed: bool) -> bool {
        !listed && self.query.trim().is_empty()
    }

    pub fn scan_finished(&mut self, result: Result<bool, String>) {
        self.scanning = false;
        match result {
            Ok(limited) => self.limited = limited,
            Err(error) => self.error = Some(error),
        }
    }

    pub fn query_changed(&mut self, query: String, indexed: bool) -> bool {
        if query == self.query {
            return false;
        }
        self.query = query;
        if self.scanning || indexed {
            self.error = None;
        }
        self.accept_generation = self.accept_generation.saturating_add(1);
        true
    }

    pub const fn begin_rank(&mut self, clear: bool) -> u64 {
        if clear {
            self.selected = None;
        }
        self.rank_generation = self.rank_generation.saturating_add(1);
        self.rank_generation
    }

    pub const fn begin_accept(&mut self) -> u64 {
        self.accept_generation = self.accept_generation.saturating_add(1);
        self.accept_generation
    }

    #[must_use]
    pub fn accept_target(&self) -> AcceptTarget {
        let typed = PathBuf::from(self.query.trim());
        if typed.is_absolute() {
            AcceptTarget::Typed(typed)
        } else if let Some(index) = self.selected {
            AcceptTarget::Row(index)
        } else {
            AcceptTarget::Nothing
        }
    }

    pub fn navigate(&mut self, count: usize, direction: isize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let current = self.selected.unwrap_or_default();
        let selected = if direction < 0 {
            current.checked_sub(1).unwrap_or(count - 1)
        } else {
            (current + 1) % count
        };
        self.selected = Some(selected);
        Some(selected)
    }

    pub fn hover(&mut self, index: usize) -> bool {
        if self.selected == Some(index) {
            return false;
        }
        self.selected = Some(index);
        true
    }

    #[must_use]
    pub const fn scanning_note(&self, listed: bool) -> bool {
        listed && self.scanning
    }

    #[must_use]
    pub fn empty_message(&self, input_empty: bool, mode: FilePickerMode) -> &str {
        if self.scanning {
            return SCANNING_NOTE;
        }
        if let Some(error) = &self.error {
            return error;
        }
        if input_empty {
            mode.empty_label()
        } else {
            NO_MATCH
        }
    }
}
