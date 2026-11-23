use std::{path::Path, process::Command};

use zz_daemon_client::unmasked::SpawnUnmasked as _;

use super::{UpdateError, move_entry, remove_path};

pub(super) fn stage(download: &Path, staged: &Path) -> Result<(), UpdateError> {
    remove_path(staged)?;
    move_entry(download, staged)
}

pub(super) fn launch(staged: &Path) -> Result<(), UpdateError> {
    Command::new(staged)
        .spawn_unmasked()
        .map(drop)
        .map_err(|source| UpdateError::io("run", staged, source))
}
