use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use zz_protocol::EditorDescriptor;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Retry {
    Open(PathBuf),
    Save(PathBuf),
}

#[derive(Clone, Debug)]
pub struct EditorError {
    pub message: Arc<str>,
    pub retry: Retry,
}

impl EditorError {
    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self.retry {
            Retry::Open(_) => "Couldn’t open this file",
            Retry::Save(_) => "Couldn’t save this file",
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Publish {
    pub path: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Restore {
    pub language: &'static str,
    pub load: Option<PathBuf>,
    pub title: Option<String>,
}

#[derive(Debug)]
pub struct EditorDocument {
    cwd: PathBuf,
    path: Option<PathBuf>,
    loaded: Option<PathBuf>,
    snapshot_path: Option<String>,
    pending_path: Option<String>,
    saved_contents: String,
    dirty: bool,
    last_title: String,
    reported_title: String,
    error: Option<EditorError>,
}

impl EditorDocument {
    #[must_use]
    pub fn new(descriptor: &EditorDescriptor, reported_title: &str) -> Self {
        Self {
            cwd: PathBuf::from(&descriptor.cwd),
            path: None,
            loaded: None,
            snapshot_path: descriptor.path.clone(),
            pending_path: None,
            saved_contents: String::new(),
            dirty: false,
            last_title: String::new(),
            reported_title: reported_title.to_owned(),
            error: None,
        }
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
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    #[must_use]
    pub fn holds_file(&self) -> bool {
        self.loaded == self.path
    }

    #[must_use]
    pub const fn error(&self) -> Option<&EditorError> {
        self.error.as_ref()
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn fail(&mut self, retry: Retry, message: impl Into<Arc<str>>) {
        self.error = Some(EditorError {
            message: message.into(),
            retry,
        });
    }

    pub fn synchronize(
        &mut self,
        descriptor: &EditorDescriptor,
        reported_title: &str,
    ) -> Option<Restore> {
        self.cwd = PathBuf::from(&descriptor.cwd);
        self.snapshot_path.clone_from(&descriptor.path);
        reported_title.clone_into(&mut self.reported_title);
        match descriptor_sync_action(
            self.pending_path.as_deref(),
            descriptor.path.as_deref(),
            self.path.as_deref(),
        ) {
            SyncAction::Ignore => None,
            SyncAction::ClearPending => {
                self.pending_path = None;
                None
            }
            SyncAction::Restore => Some(self.restore(descriptor)),
        }
    }

    pub fn restore(&mut self, descriptor: &EditorDescriptor) -> Restore {
        self.cwd = PathBuf::from(&descriptor.cwd);
        self.path = descriptor.path.as_deref().map(PathBuf::from);
        self.loaded = None;
        self.saved_contents.clear();
        self.dirty = false;
        self.error = None;
        Restore {
            language: self.path.as_deref().map_or("text", language_for_path),
            load: self.path.clone(),
            title: self.title_change(),
        }
    }

    pub fn buffer_changed(&mut self, contents: &str) -> Option<String> {
        let dirty = contents != self.saved_contents;
        if self.dirty == dirty {
            return None;
        }
        self.dirty = dirty;
        self.title_change()
    }

    pub fn loaded(&mut self, path: PathBuf, contents: &str) -> Option<Publish> {
        let path_string = match validated_descriptor_path(&path, &self.cwd) {
            Ok(path_string) => path_string,
            Err(message) => {
                self.fail(Retry::Open(path), message);
                return None;
            }
        };
        self.path = Some(path.clone());
        self.loaded = Some(path);
        contents.clone_into(&mut self.saved_contents);
        self.dirty = false;
        self.error = None;
        Some(self.publish(path_string))
    }

    pub fn prepare_save(&mut self, path: &Path) -> Option<String> {
        if !self.holds_file() {
            return None;
        }
        match validated_descriptor_path(path, &self.cwd) {
            Ok(path_string) => {
                self.error = None;
                Some(path_string)
            }
            Err(message) => {
                self.fail(Retry::Save(path.to_owned()), message);
                None
            }
        }
    }

    pub fn saved(&mut self, path: PathBuf, contents: String, current: &str) -> Option<Publish> {
        let path_string = match validated_descriptor_path(&path, &self.cwd) {
            Ok(path_string) => path_string,
            Err(message) => {
                self.fail(Retry::Save(path), message);
                return None;
            }
        };
        self.path = Some(path.clone());
        self.loaded = Some(path);
        self.dirty = current != contents;
        self.saved_contents = contents;
        self.error = None;
        Some(self.publish(path_string))
    }

    fn publish(&mut self, path: String) -> Publish {
        let path = if self.snapshot_path.as_deref() == Some(path.as_str()) {
            self.pending_path = None;
            None
        } else {
            self.pending_path = Some(path.clone());
            Some(path)
        };
        Publish {
            path,
            title: self.title_change(),
        }
    }

    fn title_change(&mut self) -> Option<String> {
        let title = editor_title(self.path.as_deref(), self.dirty);
        if title == self.last_title {
            return None;
        }
        let marked = editor_title(self.path.as_deref(), true);
        let marked_here = self.last_title == marked;
        self.last_title.clone_from(&title);
        if !self.dirty && !marked_here && self.reported_title == marked {
            return None;
        }
        Some(title)
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

    fn descriptor(path: &str) -> EditorDescriptor {
        EditorDescriptor {
            path: Some(path.to_owned()),
            cwd: "/work".to_owned(),
        }
    }

    #[test]
    fn a_buffer_saves_only_over_the_file_it_holds() {
        let mut document = EditorDocument::new(&descriptor("/work/a.txt"), "a.txt");
        assert_eq!(
            document.restore(&descriptor("/work/a.txt")),
            Restore {
                language: "text",
                load: Some(PathBuf::from("/work/a.txt")),
                title: Some("a.txt".to_owned()),
            }
        );
        assert_eq!(document.prepare_save(Path::new("/work/a.txt")), None);
        assert_eq!(
            document.loaded(PathBuf::from("/work/a.txt"), "alpha"),
            Some(Publish::default())
        );
        assert_eq!(
            document.prepare_save(Path::new("/work/a.txt")),
            Some("/work/a.txt".to_owned())
        );

        let restore = document
            .synchronize(&descriptor("/work/b.dat"), "a.txt")
            .expect("another client picked b.dat");
        assert_eq!(restore.load, Some(PathBuf::from("/work/b.dat")));
        document.fail(Retry::Open(PathBuf::from("/work/b.dat")), "binary");
        assert_eq!(document.prepare_save(Path::new("/work/b.dat")), None);
        assert_eq!(document.prepare_save(Path::new("/work/a.txt")), None);
    }

    #[test]
    fn opening_a_pane_keeps_the_unsaved_marker_another_client_set() {
        let mut document = EditorDocument::new(&descriptor("/work/notes.md"), "notes.md •");
        assert_eq!(document.restore(&descriptor("/work/notes.md")).title, None);
        assert_eq!(
            document.loaded(PathBuf::from("/work/notes.md"), "text"),
            Some(Publish::default())
        );
        assert_eq!(
            document.buffer_changed("text!"),
            Some("notes.md •".to_owned())
        );
        assert_eq!(document.buffer_changed("text"), Some("notes.md".to_owned()));
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
}
