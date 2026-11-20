use std::path::{Path, PathBuf};

use serde_json::Value;

const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;

pub(crate) fn subdirectories(path: &Path) -> Vec<PathBuf> {
    let mut directories = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    directories
}

pub(crate) fn uuid(id: &str) -> bool {
    let dashes = [8, 13, 18, 23];
    id.len() == 36
        && id.bytes().enumerate().all(|(at, byte)| {
            if dashes.contains(&at) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

pub(crate) fn grok_summary(home: &Path, session: &str) -> Option<Value> {
    if !uuid(session) {
        return None;
    }
    subdirectories(&home.join("sessions"))
        .iter()
        .find_map(|folder| read_small(&folder.join(session).join("summary.json")))
        .and_then(|text| serde_json::from_str(&text).ok())
}

pub(crate) fn read_small(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    std::io::read_to_string(file).ok()
}
