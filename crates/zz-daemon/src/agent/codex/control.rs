use std::path::{Path, PathBuf};

const SOCKET: &str = "app-server-control/app-server-control.sock";

pub(crate) fn socket(home: &Path) -> Option<PathBuf> {
    if crate::agent::sandboxed() {
        return None;
    }
    std::fs::canonicalize(home.join(SOCKET)).ok()
}
