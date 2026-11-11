use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::Command,
};

use zz_daemon_client::unmasked::SpawnUnmasked as _;

use super::{BUNDLE_NAME, LOG_TARGET, UpdateError, remove_path, run, writable};

pub(super) fn stage(
    running: &Path,
    image: &Path,
    staged: &Path,
    scratch: &Path,
) -> Result<(), UpdateError> {
    remove_path(staged)?;
    let result = copy_out(image, staged, scratch).and_then(|()| check_signature(running, staged));
    if result.is_err() {
        let _ = remove_path(staged);
    }
    result
}

fn copy_out(image: &Path, staged: &Path, scratch: &Path) -> Result<(), UpdateError> {
    let mount = Mount::attach(image, &scratch.join("mount"))?;
    let bundle = mount.0.join(BUNDLE_NAME);
    if !bundle.join("Contents").is_dir() {
        return Err(UpdateError::io(
            "find zz.app in",
            image,
            io::Error::from(ErrorKind::NotFound),
        ));
    }
    run(Command::new("ditto").arg(&bundle).arg(staged))?;
    Ok(())
}

struct Mount(PathBuf);

impl Mount {
    fn attach(image: &Path, point: &Path) -> Result<Self, UpdateError> {
        let _ = fs::remove_dir(point);
        fs::create_dir_all(point).map_err(|source| UpdateError::io("create", point, source))?;
        let attached = run(Command::new("hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-readonly",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(point)
            .arg(image));
        match attached {
            Ok(_) => Ok(Self(point.to_path_buf())),
            Err(error) => {
                let _ = fs::remove_dir(point);
                Err(error)
            }
        }
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        let detached = run(Command::new("hdiutil")
            .args(["detach", "-quiet"])
            .arg(&self.0))
        .or_else(|_| {
            run(Command::new("hdiutil")
                .args(["detach", "-quiet", "-force"])
                .arg(&self.0))
        });
        if let Err(error) = detached {
            log::warn!(target: LOG_TARGET, "could not detach the update image: {error}");
        }
        let _ = fs::remove_dir(&self.0);
    }
}

fn check_signature(running: &Path, staged: &Path) -> Result<(), UpdateError> {
    run(Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(staged))
    .map_err(rejected)?;
    let staged_team = team(staged);
    match team(running) {
        Some(expected) if staged_team.as_deref() == Some(expected.as_str()) => Ok(()),
        Some(expected) => Err(UpdateError::Signature(format!(
            "the update is signed by team {}, the running app by team {expected}",
            staged_team.as_deref().unwrap_or("none")
        ))),
        None => run(Command::new("spctl")
            .args(["--assess", "--type", "execute"])
            .arg(staged))
        .map(drop)
        .map_err(rejected),
    }
}

fn rejected(error: UpdateError) -> UpdateError {
    match error {
        UpdateError::Tool { stderr, .. } if !stderr.is_empty() => UpdateError::Signature(stderr),
        error => UpdateError::Signature(error.to_string()),
    }
}

fn team(bundle: &Path) -> Option<String> {
    let output = Command::new("codesign")
        .arg("-dv")
        .arg(bundle)
        .output_unmasked()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    team_identifier(&String::from_utf8_lossy(&output.stderr)).map(str::to_owned)
}

fn team_identifier(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|line| line.strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|team| !team.is_empty() && *team != "not set")
}

pub(super) fn link_cli(cli: &Path) {
    let local_bin = env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin"));
    let directories = [
        Some(PathBuf::from("/opt/homebrew/bin")),
        Some(PathBuf::from("/usr/local/bin")),
        local_bin.clone(),
    ];
    for directory in directories.into_iter().flatten() {
        if local_bin.as_ref() == Some(&directory) {
            let _ = fs::create_dir_all(&directory);
        }
        if !directory.is_dir() || !writable(&directory.join(".zz-link-probe")) {
            continue;
        }
        let link = directory.join("zz");
        match fs::read_link(&link) {
            Ok(existing) if replaces_link(&existing, link.exists(), cli) => {}
            Ok(_) => return,
            Err(_) if link.symlink_metadata().is_ok() => return,
            Err(_) => {}
        }
        match replace_symlink(cli, &link) {
            Ok(()) => {
                log::info!(target: LOG_TARGET, "linked {} -> {}", link.display(), cli.display());
            }
            Err(error) => {
                log::warn!(target: LOG_TARGET, "could not link {}: {error}", link.display());
            }
        }
        return;
    }
    log::warn!(target: LOG_TARGET, "found no writable directory for the zz CLI");
}

fn replaces_link(existing: &Path, existing_exists: bool, wanted: &Path) -> bool {
    existing != wanted && !existing_exists && is_bundle_launcher(existing)
}

fn is_bundle_launcher(path: &Path) -> bool {
    path.to_string_lossy()
        .contains(&format!("/{BUNDLE_NAME}/Contents/MacOS/"))
}

#[cfg(unix)]
fn replace_symlink(target: &Path, link: &Path) -> io::Result<()> {
    let temporary = link.with_file_name(format!(".zz-link-{}", std::process::id()));
    let _ = fs::remove_file(&temporary);
    std::os::unix::fs::symlink(target, &temporary)?;
    fs::rename(&temporary, link).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

#[cfg(not(unix))]
fn replace_symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::from(ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_team_comes_from_codesign_display_output() {
        let signed = "Executable=/Applications/zz.app/Contents/MacOS/zz\nIdentifier=dev.zz.app\nTeamIdentifier=Y4JYPG9TS8\nSealed Resources version=2 rules=13 files=11\n";
        let ad_hoc =
            "Executable=/tmp/zz.app/Contents/MacOS/zz\nSignature=adhoc\nTeamIdentifier=not set\n";
        assert_eq!(team_identifier(signed), Some("Y4JYPG9TS8"));
        assert_eq!(team_identifier(ad_hoc), None);
    }

    #[test]
    fn only_a_dangling_bundle_link_is_taken_over() {
        let wanted = Path::new("/Users/me/Downloads/zz.app/Contents/MacOS/cli");
        let other = Path::new("/Applications/zz.app/Contents/MacOS/cli");
        let foreign = Path::new("/opt/homebrew/Cellar/zz/1.0/bin/zz");
        assert!(!replaces_link(wanted, true, wanted));
        assert!(replaces_link(other, false, wanted));
        assert!(!replaces_link(other, true, wanted));
        assert!(!replaces_link(foreign, false, wanted));
    }
}
