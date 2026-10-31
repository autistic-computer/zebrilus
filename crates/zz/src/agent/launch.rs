use std::{fs, io, path::PathBuf};

use zz_client::agent_launch::AgentLaunches;

use crate::{config::atomic_write, user_data::platform_data_dir};

const FILE_NAME: &str = "agent-launch.json";
const MAX_FILE_BYTES: usize = 4 * 1024;

fn path() -> Option<PathBuf> {
    platform_data_dir().map(|data| {
        data.join(zz_protocol::app_identity::DIRECTORY)
            .join(FILE_NAME)
    })
}

pub(crate) fn load() -> AgentLaunches {
    path()
        .and_then(|path| fs::read(path).ok())
        .filter(|bytes| bytes.len() <= MAX_FILE_BYTES)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub(crate) fn update(change: impl FnOnce(&mut AgentLaunches) -> bool) {
    let mut launches = load();
    if !change(&mut launches) {
        return;
    }
    let Some(path) = path() else {
        return;
    };
    let written = serde_json::to_vec_pretty(&launches)
        .map_err(io::Error::from)
        .and_then(|json| atomic_write(&path, &json));
    if let Err(error) = written {
        log::warn!(
            target: "zz::agent",
            "could not remember the agent model and effort path={} error={error}",
            path.display(),
        );
    }
}
