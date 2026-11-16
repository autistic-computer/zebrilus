use std::path::Path;

use zz_protocol::EditorDescriptor;

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
