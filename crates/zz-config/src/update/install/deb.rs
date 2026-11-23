use std::{fs, path::Path, process::Command};

use zz_daemon_client::unmasked::SpawnUnmasked as _;

use super::{UpdateError, move_entry, remove_path};

pub(super) fn is_dpkg_owned(executable: &Path) -> bool {
    if executable != Path::new("/usr/lib/zz/zz") {
        return false;
    }
    let info = Path::new("/var/lib/dpkg/info");
    fs::read_dir(info).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name == "zz.list" || (name.starts_with("zz:") && name.ends_with(".list"))
        })
    })
}

pub(super) fn stage(download: &Path, staged: &Path) -> Result<(), UpdateError> {
    remove_path(staged)?;
    move_entry(download, staged)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(staged, fs::Permissions::from_mode(0o644))
            .map_err(|source| UpdateError::io("chmod", staged, source))?;
    }
    Ok(())
}

pub(super) fn install(staged: &Path) -> Result<(), UpdateError> {
    let elevated = !is_root();
    let mut command = if elevated {
        let mut command = Command::new("pkexec");
        command.args([
            "env",
            "DEBIAN_FRONTEND=noninteractive",
            "apt-get",
            "install",
            "-y",
        ]);
        command
    } else {
        let mut command = Command::new("apt-get");
        command
            .env("DEBIAN_FRONTEND", "noninteractive")
            .args(["install", "-y"]);
        command
    };
    command.arg(staged);
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output_unmasked()
        .map_err(|source| UpdateError::io("run", &program, source))?;
    if output.status.success() {
        let _ = remove_path(staged);
        return Ok(());
    }
    if elevated && output.status.code() == Some(126) {
        return Err(UpdateError::Cancelled);
    }
    let stderr = String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned();
    Err(UpdateError::Tool {
        program,
        status: output.status,
        stderr,
    })
}

pub(super) fn is_root() -> bool {
    #[cfg(unix)]
    {
        rustix::process::geteuid().is_root()
    }
    #[cfg(not(unix))]
    {
        false
    }
}
