use std::{fs, io, path::PathBuf};

use zz_client::recent_pages::RecentPages;

use crate::{config::atomic_write, user_data::restrict_to_current_user};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub struct StoredPages {
    pub history: RecentPages,
    path: Option<PathBuf>,
}

impl StoredPages {
    pub fn load(path: Option<PathBuf>) -> Self {
        let history = path
            .as_deref()
            .map(|path| {
                match fs::metadata(path) {
                    Ok(metadata) if metadata.len() > MAX_FILE_BYTES => {
                        log::warn!(
                            target: "zz::recent_pages",
                            "ignoring oversized recent pages file path={} bytes={}",
                            path.display(),
                            metadata.len(),
                        );
                        return RecentPages::default();
                    }
                    _ => {}
                }
                match fs::read_to_string(path) {
                    Ok(source) => RecentPages::deserialize(&source),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => RecentPages::default(),
                    Err(error) => {
                        log::warn!(
                            target: "zz::recent_pages",
                            "could not load recent pages path={} error={error}",
                            path.display(),
                        );
                        RecentPages::default()
                    }
                }
            })
            .unwrap_or_default();
        Self { history, path }
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        atomic_write(path, self.history.serialize().as_bytes())?;
        restrict_to_current_user(path)
    }

    pub fn is_persistent(&self) -> bool {
        self.path.is_some()
    }
}
