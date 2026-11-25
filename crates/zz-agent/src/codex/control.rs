use std::path::{Path, PathBuf};

const SOCKET: &str = "app-server-control/app-server-control.sock";

pub fn socket(home: &Path) -> Option<PathBuf> {
    if crate::sandboxed() {
        return None;
    }
    std::fs::canonicalize(home.join(SOCKET)).ok()
}
