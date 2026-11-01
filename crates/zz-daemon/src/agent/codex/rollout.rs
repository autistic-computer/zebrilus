use std::{
    fs::File,
    io::{BufRead as _, BufReader, Read as _},
    path::{Path, PathBuf},
};

use serde_json::Value;

const META_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RolloutMeta {
    pub(crate) thread: String,
    pub(crate) parent: Option<String>,
    pub(crate) cwd: PathBuf,
}

impl RolloutMeta {
    pub(crate) fn read(path: &Path) -> Option<Self> {
        let mut line = Vec::new();
        BufReader::new(File::open(path).ok()?)
            .take(META_LIMIT)
            .read_until(b'\n', &mut line)
            .ok()?;
        let entry: Value = serde_json::from_slice(&line).ok()?;
        if entry["type"] != "session_meta" {
            return None;
        }
        let meta = &entry["payload"];
        let parent = meta["parent_thread_id"]
            .as_str()
            .or_else(|| meta["source"]["subagent"]["thread_spawn"]["parent_thread_id"].as_str())
            .map(str::to_owned);
        Some(Self {
            thread: meta["id"].as_str()?.to_owned(),
            parent,
            cwd: PathBuf::from(meta["cwd"].as_str().unwrap_or("/")),
        })
    }
}
