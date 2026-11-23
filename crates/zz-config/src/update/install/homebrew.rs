use std::{
    collections::VecDeque,
    env,
    io::{BufRead as _, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use semver::Version;
use zz_daemon_client::unmasked::SpawnUnmasked as _;

use super::{Progress, UpdateError};

const TIMEOUT: Duration = Duration::from_mins(15);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const PUBLISH_GAP: Duration = Duration::from_millis(100);
const OUTPUT_GRACE: Duration = Duration::from_millis(500);
const FAILURE_LINES: usize = 3;
const MAX_LINE_BYTES: usize = 240;

pub(super) fn upgrade(
    brew: &Path,
    cask: &str,
    app: &Path,
    expected: &Version,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(), UpdateError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(UpdateError::Cancelled);
    }
    on_progress(Progress::Running {
        detail: String::new(),
    });
    let mut command = Command::new(brew);
    command.args(["upgrade", "--cask", cask]);
    prepare(&mut command, brew);
    run_streaming(command, cancel, on_progress)?;
    if let Some(installed) = installed_version(app)
        && installed < *expected
    {
        return Err(UpdateError::TapBehind {
            manager: "Homebrew",
            version: expected.clone(),
        });
    }
    Ok(())
}

fn prepare(command: &mut Command, brew: &Path) {
    let mut directories = Vec::new();
    if let Some(bin) = brew.parent() {
        directories.push(bin.to_path_buf());
        if let Some(prefix) = bin.parent() {
            let sbin = prefix.join("sbin");
            if sbin.is_dir() {
                directories.push(sbin);
            }
        }
    }
    if let Some(existing) = env::var_os("PATH") {
        for directory in env::split_paths(&existing) {
            if !directories.contains(&directory) {
                directories.push(directory);
            }
        }
    }
    if let Ok(path) = env::join_paths(directories) {
        command.env("PATH", path);
    }
    command
        .env_remove("HOMEBREW_NO_AUTO_UPDATE")
        .env("NONINTERACTIVE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
}

fn run_streaming(
    mut command: Command,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(), UpdateError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let (reader, writer) =
        std::io::pipe().map_err(|source| UpdateError::io("pipe", &program, source))?;
    let writer_err = writer
        .try_clone()
        .map_err(|source| UpdateError::io("pipe", &program, source))?;
    command.stdout(writer).stderr(writer_err);
    let mut child = command
        .spawn_unmasked()
        .map_err(|source| UpdateError::io("run", &program, source))?;
    drop(command);
    let (lines, received) = mpsc::channel();
    let pump = thread::Builder::new()
        .name("zz-update-brew-output".to_owned())
        .spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut buffer = Vec::new();
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        for segment in String::from_utf8_lossy(&buffer).split('\r') {
                            let line = plain(segment);
                            if !line.is_empty() && lines.send(line).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        })
        .map_err(|source| UpdateError::io("read output of", &program, source))?;

    let deadline = Instant::now() + TIMEOUT;
    let mut published = Instant::now()
        .checked_sub(PUBLISH_GAP)
        .unwrap_or_else(Instant::now);
    let mut tail = VecDeque::with_capacity(FAILURE_LINES);
    let mut exited: Option<(std::process::ExitStatus, Instant)> = None;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            kill_group(&mut child);
            let _ = pump.join();
            return Err(UpdateError::Cancelled);
        }
        match received.recv_timeout(POLL_INTERVAL) {
            Ok(line) => {
                let line = bounded(&line);
                if tail.len() == FAILURE_LINES {
                    tail.pop_front();
                }
                tail.push_back(line.clone());
                if published.elapsed() >= PUBLISH_GAP {
                    published = Instant::now();
                    on_progress(Progress::Running { detail: line });
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = match exited {
                    Some((status, _)) => status,
                    None => child
                        .wait()
                        .map_err(|source| UpdateError::io("wait for", &program, source))?,
                };
                let _ = pump.join();
                break status;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        match exited {
            Some((status, at)) if at.elapsed() >= OUTPUT_GRACE => break status,
            Some(_) => {}
            None => {
                if let Ok(Some(status)) = child.try_wait() {
                    exited = Some((status, Instant::now()));
                }
            }
        }
        if exited.is_none() && Instant::now() >= deadline {
            kill_group(&mut child);
            let _ = pump.join();
            return Err(UpdateError::Network(format!(
                "{program} timed out after {} minutes",
                TIMEOUT.as_secs().div_ceil(60)
            )));
        }
    };
    if status.success() {
        return Ok(());
    }
    let stderr = tail.into_iter().collect::<Vec<_>>().join("\n");
    Err(UpdateError::Tool {
        program,
        status,
        stderr,
    })
}

fn kill_group(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn installed_version(app: &Path) -> Option<Version> {
    let cli = app.join("Contents/MacOS/cli");
    if !cli.is_file() {
        return None;
    }
    let output = Command::new(cli)
        .arg("--version")
        .stdin(Stdio::null())
        .output_unmasked()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_cli_version(&String::from_utf8_lossy(&output.stdout))
}

fn parse_cli_version(stdout: &str) -> Option<Version> {
    stdout
        .split_whitespace()
        .find_map(|token| Version::parse(token.trim_start_matches('v')).ok())
}

fn plain(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            match characters.next() {
                Some('[') => {
                    for next in characters.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(next) = characters.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && characters.peek() == Some(&'\\') {
                            characters.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if !character.is_control() {
            output.push(character);
        }
    }
    output.trim().to_owned()
}

fn bounded(text: &str) -> String {
    if text.len() <= MAX_LINE_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_LINE_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

pub(super) fn executable(candidate: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        candidate.is_file()
    }
}

pub(super) fn brew_binary(prefix: &Path) -> Option<PathBuf> {
    let brew = prefix.join("bin/brew");
    executable(&brew).then_some(brew)
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt as _};

    use super::*;

    fn write_executable(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parents");
        }
        fs::write(path, body).expect("write script");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod script");
    }

    #[test]
    fn homebrew_upgrade_streams_output_and_checks_the_installed_version() {
        let root = tempfile::tempdir().expect("temp dir");
        let app = root.path().join("zz.app");
        let cli = app.join("Contents/MacOS/cli");
        write_executable(&cli, "#!/bin/sh\necho 'zz 0.23.0-beta.1'\n");
        let brew = root.path().join("bin/brew");
        write_executable(
            &brew,
            &format!(
                "#!/bin/sh\nprintf '\\033[32m==> Downloading zz\\033[0m\\n'\n\
                 printf '#!/bin/sh\\necho \"zz 0.23.0-beta.2\"\\n' > '{}'\n\
                 chmod 0755 '{}'\n",
                cli.display(),
                cli.display(),
            ),
        );
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        let expected = Version::parse("0.23.0-beta.2").expect("version");

        upgrade(&brew, "zz@beta", &app, &expected, &cancel, &mut |step| {
            seen.push(step);
        })
        .expect("upgrade succeeds");

        assert!(seen.contains(&Progress::Running {
            detail: "==> Downloading zz".to_owned(),
        }));
    }

    #[test]
    fn homebrew_upgrade_reports_when_the_tap_still_serves_the_old_version() {
        let root = tempfile::tempdir().expect("temp dir");
        let app = root.path().join("zz.app");
        let cli = app.join("Contents/MacOS/cli");
        write_executable(&cli, "#!/bin/sh\necho 'zz 0.23.0'\n");
        let brew = root.path().join("bin/brew");
        write_executable(
            &brew,
            "#!/bin/sh\necho 'Warning: Not upgrading zz, the latest version is already installed'\n",
        );
        let cancel = AtomicBool::new(false);
        let expected = Version::parse("0.24.0").expect("version");

        let error = upgrade(&brew, "zz", &app, &expected, &cancel, &mut |_| {})
            .expect_err("stale tap is rejected");

        assert!(
            matches!(
                &error,
                UpdateError::TapBehind {
                    manager: "Homebrew",
                    version,
                } if version == &expected
            ),
            "{error:?}"
        );
    }

    #[test]
    fn cancelling_homebrew_upgrade_kills_the_running_process() {
        let root = tempfile::tempdir().expect("temp dir");
        let app = root.path().join("zz.app");
        let brew = root.path().join("bin/brew");
        write_executable(&brew, "#!/bin/sh\nsleep 30\n");
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let trigger = std::sync::Arc::clone(&cancel);
        let stopper = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            trigger.store(true, Ordering::Relaxed);
        });
        let expected = Version::parse("0.24.0").expect("version");

        let error =
            upgrade(&brew, "zz", &app, &expected, &cancel, &mut |_| {}).expect_err("cancelled");
        stopper.join().expect("stopper");

        assert!(matches!(error, UpdateError::Cancelled), "{error:?}");
    }
}
