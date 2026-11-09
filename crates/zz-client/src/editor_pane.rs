use std::{
    fmt::Display,
    fs::{self, File},
    io::Read as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use zz_protocol::EditorDescriptor;

pub const MAX_EDITOR_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[cfg(target_os = "macos")]
pub const OPEN_HINT: &str = "⌘O to open a file";
#[cfg(not(target_os = "macos"))]
pub const OPEN_HINT: &str = "Ctrl+O to open a file";

pub const PICKER_PLACEHOLDER: &str = "Open a file in the editor";
pub const NEW_FILE_NAME: &str = "untitled";
pub const RETRY_LABEL: &str = "Try again";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Retry {
    Open(PathBuf),
    Save(PathBuf),
}

impl Retry {
    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self {
            Self::Open(_) => "Couldn’t open this file",
            Self::Save(_) => "Couldn’t save this file",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SyncAction {
    Ignore,
    ClearPending,
    Restore,
}

#[must_use]
pub fn descriptor_sync_action(
    pending: Option<&str>,
    snapshot: Option<&str>,
    local: Option<&Path>,
) -> SyncAction {
    if let Some(pending) = pending {
        return if snapshot == Some(pending) {
            SyncAction::ClearPending
        } else {
            SyncAction::Ignore
        };
    }
    if snapshot.map(Path::new) == local {
        SyncAction::Ignore
    } else {
        SyncAction::Restore
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorError {
    pub message: Arc<str>,
    pub retry: Retry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorEffect {
    SetBuffer {
        contents: String,
        language: &'static str,
    },
    SetLanguage(&'static str),
    Load(PathBuf),
    Write {
        path: PathBuf,
        contents: String,
    },
    PromptNewPath {
        directory: PathBuf,
        suggested: &'static str,
    },
    PublishPath(String),
    PublishTitle(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorModel {
    cwd: PathBuf,
    path: Option<PathBuf>,
    snapshot_path: Option<String>,
    pending_path: Option<String>,
    saved_contents: String,
    dirty: bool,
    last_title: String,
    error: Option<EditorError>,
    picker_open: bool,
}

impl EditorModel {
    #[must_use]
    pub fn new(descriptor: &EditorDescriptor) -> (Self, Vec<EditorEffect>) {
        let mut model = Self {
            cwd: PathBuf::from(&descriptor.cwd),
            path: None,
            snapshot_path: descriptor.path.clone(),
            pending_path: None,
            saved_contents: String::new(),
            dirty: false,
            last_title: String::new(),
            error: None,
            picker_open: false,
        };
        let mut effects = Vec::new();
        model.restore(descriptor, &mut effects);
        (model, effects)
    }

    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    #[must_use]
    pub fn snapshot_path(&self) -> Option<&str> {
        self.snapshot_path.as_deref()
    }

    #[must_use]
    pub fn pending_path(&self) -> Option<&str> {
        self.pending_path.as_deref()
    }

    #[must_use]
    pub fn saved_contents(&self) -> &str {
        &self.saved_contents
    }

    #[must_use]
    pub const fn dirty(&self) -> bool {
        self.dirty
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.last_title
    }

    #[must_use]
    pub const fn error(&self) -> Option<&EditorError> {
        self.error.as_ref()
    }

    #[must_use]
    pub const fn picker_open(&self) -> bool {
        self.picker_open
    }

    #[must_use]
    pub const fn scratch_hint(&self, buffer_empty: bool) -> bool {
        self.path.is_none() && buffer_empty && self.error.is_none() && !self.picker_open
    }

    pub fn synchronize(&mut self, descriptor: &EditorDescriptor) -> Vec<EditorEffect> {
        let mut effects = Vec::new();
        self.cwd = PathBuf::from(&descriptor.cwd);
        self.snapshot_path.clone_from(&descriptor.path);
        match descriptor_sync_action(
            self.pending_path.as_deref(),
            descriptor.path.as_deref(),
            self.path.as_deref(),
        ) {
            SyncAction::Ignore => {}
            SyncAction::ClearPending => self.pending_path = None,
            SyncAction::Restore => self.restore(descriptor, &mut effects),
        }
        effects
    }

    fn restore(&mut self, descriptor: &EditorDescriptor, effects: &mut Vec<EditorEffect>) {
        self.cwd = PathBuf::from(&descriptor.cwd);
        let Some(path) = descriptor.path.as_deref().map(PathBuf::from) else {
            self.path = None;
            self.saved_contents.clear();
            self.dirty = false;
            self.error = None;
            effects.push(EditorEffect::SetBuffer {
                contents: String::new(),
                language: "text",
            });
            self.publish_title(effects);
            return;
        };
        self.path = Some(path.clone());
        effects.push(EditorEffect::SetLanguage(language_for_path(&path)));
        self.publish_title(effects);
        effects.push(EditorEffect::Load(path));
    }

    pub fn buffer_changed(&mut self, contents: &str) -> Vec<EditorEffect> {
        let mut effects = Vec::new();
        let dirty = contents != self.saved_contents.as_str();
        if self.dirty != dirty {
            self.dirty = dirty;
            self.publish_title(&mut effects);
        }
        effects
    }

    pub fn open_picker(&mut self) -> bool {
        if self.picker_open {
            return false;
        }
        self.error = None;
        self.picker_open = true;
        true
    }

    pub fn picker_closed(&mut self, selected: Option<PathBuf>) -> Vec<EditorEffect> {
        self.picker_open = false;
        selected.map(EditorEffect::Load).into_iter().collect()
    }

    pub fn finish_load(
        &mut self,
        path: PathBuf,
        result: Result<String, String>,
    ) -> Vec<EditorEffect> {
        let mut effects = Vec::new();
        let contents = match result {
            Ok(contents) => contents,
            Err(message) => {
                self.error = Some(EditorError {
                    message: message.into(),
                    retry: Retry::Open(path),
                });
                return effects;
            }
        };
        let Ok(path_string) = validated_descriptor_path(&path, &self.cwd) else {
            self.error = Some(EditorError {
                message: format!("{} cannot be stored as an editor path", path.display()).into(),
                retry: Retry::Open(path),
            });
            return effects;
        };
        effects.push(EditorEffect::SetBuffer {
            contents: contents.clone(),
            language: language_for_path(&path),
        });
        self.path = Some(path);
        self.saved_contents = contents;
        self.dirty = false;
        self.error = None;
        self.publish_path(path_string, &mut effects);
        self.publish_title(&mut effects);
        effects
    }

    pub fn save(&mut self, contents: &str) -> Vec<EditorEffect> {
        if let Some(path) = self.path.clone() {
            return self.begin_save(path, contents);
        }
        vec![EditorEffect::PromptNewPath {
            directory: self.cwd.clone(),
            suggested: NEW_FILE_NAME,
        }]
    }

    pub fn begin_save(&mut self, path: PathBuf, contents: &str) -> Vec<EditorEffect> {
        if let Err(message) = validated_descriptor_path(&path, &self.cwd) {
            self.error = Some(EditorError {
                message: message.into(),
                retry: Retry::Save(path),
            });
            return Vec::new();
        }
        self.error = None;
        vec![EditorEffect::Write {
            path,
            contents: contents.to_owned(),
        }]
    }

    pub fn finish_save(
        &mut self,
        path: PathBuf,
        saved_contents: String,
        result: Result<(), String>,
        contents: &str,
    ) -> Vec<EditorEffect> {
        let mut effects = Vec::new();
        if let Err(message) = result {
            self.error = Some(EditorError {
                message: message.into(),
                retry: Retry::Save(path),
            });
            return effects;
        }
        let path_string = match validated_descriptor_path(&path, &self.cwd) {
            Ok(path) => path,
            Err(message) => {
                self.error = Some(EditorError {
                    message: message.into(),
                    retry: Retry::Save(path),
                });
                return effects;
            }
        };
        self.path = Some(path);
        self.saved_contents = saved_contents;
        self.dirty = contents != self.saved_contents.as_str();
        self.error = None;
        self.publish_path(path_string, &mut effects);
        self.publish_title(&mut effects);
        effects
    }

    pub fn retry(&mut self, retry: Retry, contents: &str) -> Vec<EditorEffect> {
        match retry {
            Retry::Open(path) => vec![EditorEffect::Load(path)],
            Retry::Save(path) => self.begin_save(path, contents),
        }
    }

    fn publish_path(&mut self, path: String, effects: &mut Vec<EditorEffect>) {
        if self.snapshot_path.as_deref() == Some(path.as_str()) {
            self.pending_path = None;
            return;
        }
        self.pending_path = Some(path.clone());
        effects.push(EditorEffect::PublishPath(path));
    }

    fn publish_title(&mut self, effects: &mut Vec<EditorEffect>) {
        let title = editor_title(self.path.as_deref(), self.dirty);
        if title == self.last_title {
            return;
        }
        self.last_title.clone_from(&title);
        effects.push(EditorEffect::PublishTitle(title));
    }
}

pub fn validated_descriptor_path(path: &Path, cwd: &Path) -> Result<String, String> {
    let path = path
        .to_str()
        .ok_or_else(|| "Editor paths must be valid UTF-8".to_owned())?
        .to_owned();
    let cwd = cwd
        .to_str()
        .ok_or_else(|| "The editor working directory must be valid UTF-8".to_owned())?
        .to_owned();
    EditorDescriptor {
        path: Some(path.clone()),
        cwd,
    }
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(path)
}

pub fn read_editor_file(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    if metadata.len() > MAX_EDITOR_FILE_BYTES {
        return Err(format!(
            "{} is larger than the 8 MiB editor limit",
            path.display()
        ));
    }
    let mut file =
        File::open(path).map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or_default());
    file.by_ref()
        .take(MAX_EDITOR_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_EDITOR_FILE_BYTES {
        return Err(format!(
            "{} is larger than the 8 MiB editor limit",
            path.display()
        ));
    }
    if bytes.contains(&0) {
        return Err(format!(
            "{} contains NUL bytes and appears to be binary",
            path.display()
        ));
    }
    String::from_utf8(bytes).map_err(|_| format!("{} is not valid UTF-8", path.display()))
}

#[must_use]
pub fn save_error(path: &Path, error: &dyn Display) -> String {
    format!("Could not save {}: {error}", path.display())
}

#[must_use]
pub fn language_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("rs") => "rust",
        Some("md" | "markdown" | "mdx") => "markdown",
        Some("json" | "jsonc") => "json",
        Some("toml") => "toml",
        _ => "text",
    }
}

#[must_use]
pub fn editor_title(path: Option<&Path>, dirty: bool) -> String {
    let mut title = path
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("editor")
        .to_owned();
    if dirty {
        title.push_str(" •");
    }
    title
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    #[test]
    fn language_and_dirty_titles_follow_the_file_name() {
        assert_eq!(language_for_path(Path::new("/tmp/main.rs")), "rust");
        assert_eq!(language_for_path(Path::new("/tmp/README.md")), "markdown");
        assert_eq!(language_for_path(Path::new("/tmp/data.json")), "json");
        assert_eq!(language_for_path(Path::new("/tmp/config.toml")), "toml");
        assert_eq!(language_for_path(Path::new("/tmp/notes.txt")), "text");
        assert_eq!(editor_title(None, false), "editor");
        assert_eq!(
            editor_title(Some(Path::new("/tmp/main.rs")), true),
            "main.rs •"
        );
    }

    #[test]
    fn stale_snapshots_never_restore_while_a_publish_is_in_flight() {
        let pending = Some("/workspace/new.rs");
        let local = Some(Path::new("/workspace/new.rs"));

        assert_eq!(
            descriptor_sync_action(pending, None, local),
            SyncAction::Ignore
        );
        assert_eq!(
            descriptor_sync_action(pending, Some("/workspace/old.rs"), local),
            SyncAction::Ignore
        );
        assert_eq!(
            descriptor_sync_action(pending, Some("/workspace/new.rs"), local),
            SyncAction::ClearPending
        );
        assert_eq!(
            descriptor_sync_action(None, Some("/workspace/new.rs"), local),
            SyncAction::Ignore
        );
        assert_eq!(
            descriptor_sync_action(None, Some("/workspace/other.rs"), local),
            SyncAction::Restore
        );
        assert_eq!(
            descriptor_sync_action(None, None, local),
            SyncAction::Restore
        );
    }

    #[test]
    fn bounded_reader_rejects_binary_invalid_utf8_and_oversized_files() {
        let directory = tempfile::tempdir().expect("temporary directory");

        let binary = directory.path().join("binary");
        fs::write(&binary, b"a\0b").expect("binary fixture");
        assert!(read_editor_file(&binary).unwrap_err().contains("NUL"));

        let invalid = directory.path().join("invalid");
        fs::write(&invalid, [0xff]).expect("invalid UTF-8 fixture");
        assert!(
            read_editor_file(&invalid)
                .unwrap_err()
                .contains("valid UTF-8")
        );

        let oversized = directory.path().join("oversized");
        let mut file = File::create(&oversized).expect("oversized fixture");
        file.write_all(b"x").expect("fixture byte");
        file.set_len(MAX_EDITOR_FILE_BYTES + 1)
            .expect("extend fixture");
        assert!(read_editor_file(&oversized).unwrap_err().contains("8 MiB"));
    }
}
