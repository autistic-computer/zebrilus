use crate::{Endpoint, HostEntry, WslEndpoint};

#[cfg(windows)]
use std::{
    io::{self, BufReader, Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use crate::{
    EndpointError, HostProbe, HostStatus, RemoteInstall,
    endpoint::{
        FORWARD_COUNTER, INSTALL_SCRIPT, ProxyStart, REMOTE_DAEMON_TIMEOUT_STATUS,
        REMOTE_ZZ_MISSING_STATUS, SshStderr, await_proxy_ready_for, end_with_this_process,
        parse_remote_install_output, parse_remote_probe_output, refuse_development_install_target,
        remote_connect_script, remote_install_script, remote_socket_is_absolute, spawn_bridge,
        ssh_pipe,
    },
    host_probe::{host_probe_from, host_probe_script},
    unmasked::SpawnUnmasked as _,
};

#[cfg(windows)]
const WSL_LIST_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(windows)]
const WSL_PIPE_ROLE: &str = "zz-wsl";

#[cfg(any(windows, test))]
const IGNORED_DISTROS: [&str; 2] = ["docker-desktop", "docker-desktop-data"];

#[cfg(any(windows, test))]
fn is_ignored_distro(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    IGNORED_DISTROS.contains(&lower.as_str()) || lower.starts_with("podman-machine-")
}

#[cfg(any(windows, test))]
pub(crate) fn decode_wsl_text(bytes: &[u8]) -> String {
    let utf16 = bytes.starts_with(&[0xFF, 0xFE])
        || (bytes.len() >= 2
            && bytes.len().is_multiple_of(2)
            && bytes.iter().skip(1).step_by(2).any(|byte| *byte == 0));
    if utf16 {
        let slice = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
        let units = slice
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&units)
            .trim_start_matches('\u{feff}')
            .replace('\0', "")
    } else {
        String::from_utf8_lossy(bytes)
            .trim_start_matches('\u{feff}')
            .replace('\0', "")
    }
}

#[cfg(any(windows, test))]
pub(crate) fn parse_wsl_list_output(bytes: &[u8]) -> Vec<String> {
    let mut distros = Vec::new();
    for line in decode_wsl_text(bytes).lines() {
        let name = line.trim();
        if name.is_empty()
            || name.starts_with('-')
            || is_ignored_distro(name)
            || distros
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(name))
        {
            continue;
        }
        distros.push(name.to_owned());
    }
    distros
}

#[must_use]
pub fn distro_host_name(distro: &str) -> String {
    let mut name = String::with_capacity(distro.len());
    for character in distro.chars() {
        if character.is_whitespace() || character.is_control() {
            if !name.ends_with('-') {
                name.push('-');
            }
        } else {
            name.push(character);
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() || matches!(name, "local" | "all") {
        format!("wsl-{name}")
    } else {
        name.to_owned()
    }
}

#[cfg(windows)]
#[must_use]
pub fn distros() -> Vec<String> {
    if std::env::var_os("ZZ_WSL_DISCOVERY").as_deref() == Some(std::ffi::OsStr::new("0")) {
        return Vec::new();
    }
    let mut command = Command::new("wsl.exe");
    command
        .args(["-l", "-q"])
        .env("WSL_UTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut child) = command.spawn_unmasked() else {
        return Vec::new();
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Vec::new();
    };
    let reader = thread::Builder::new()
        .name("zz-wsl-list".to_owned())
        .spawn(move || {
            let mut buffer = Vec::new();
            let _ = stdout.read_to_end(&mut buffer);
            buffer
        });
    let deadline = Instant::now() + WSL_LIST_TIMEOUT;
    let exited = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let Some(status) = exited else {
        return Vec::new();
    };
    if !status.success() {
        return Vec::new();
    }
    let Ok(handle) = reader else {
        return Vec::new();
    };
    let Ok(output) = handle.join() else {
        return Vec::new();
    };
    parse_wsl_list_output(&output)
}

#[cfg(not(windows))]
#[must_use]
pub fn distros() -> Vec<String> {
    Vec::new()
}

#[must_use]
pub fn hosts(hidden: &[String]) -> Vec<HostEntry> {
    distros()
        .into_iter()
        .filter_map(|distro| {
            let name = distro_host_name(&distro);
            if hidden.iter().any(|item| item.eq_ignore_ascii_case(&name)) {
                return None;
            }
            Some(HostEntry {
                name,
                endpoint: Endpoint::Wsl(WslEndpoint {
                    distro,
                    user: None,
                    remote_socket: None,
                }),
            })
        })
        .collect()
}

#[cfg(windows)]
fn wsl_shell_command(endpoint: &WslEndpoint, script: &str) -> Command {
    let mut command = Command::new("wsl.exe");
    command.arg("-d").arg(&endpoint.distro);
    if let Some(user) = &endpoint.user {
        command.arg("-u").arg(user);
    }
    command
        .args(["--cd", "~", "--exec", "/bin/sh", "-lc", script])
        .env("WSL_UTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[cfg(windows)]
pub(crate) struct WslForward {
    child: Child,
    local_socket: PathBuf,
    stopped: Arc<AtomicBool>,
}

#[cfg(windows)]
impl WslForward {
    pub(crate) fn start(endpoint: &WslEndpoint) -> Result<Self, EndpointError> {
        let target = endpoint.to_string();
        let script = remote_connect_script(endpoint.remote_socket.as_deref(), None);
        let (local_socket, listener) = ssh_pipe(
            WSL_PIPE_ROLE,
            FORWARD_COUNTER.fetch_add(1, Ordering::Relaxed),
        )
        .map_err(|source| EndpointError::ForwardIo {
            target: target.clone(),
            action: "creating",
            path: PathBuf::from(WSL_PIPE_ROLE),
            source,
        })?;
        let mut child = wsl_shell_command(endpoint, &script)
            .spawn_unmasked()
            .map_err(|source| EndpointError::WslSpawn {
                operation: "socket proxy",
                source,
            })?;
        end_with_this_process(&child);
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EndpointError::ForwardIo {
                target,
                action: "opening a pipe to wsl.exe for",
                path: local_socket,
                source: io::Error::other("wsl.exe was spawned without redirected stdio"),
            });
        };
        let mut stderr = SshStderr::drain(stderr);
        let mut stdout = BufReader::new(stdout);
        let preamble = match await_proxy_ready_for(&target, &mut stdout, &local_socket) {
            Ok(ProxyStart::Ready(preamble)) => preamble,
            Ok(ProxyStart::Exited(preamble)) => {
                let status = child.wait().ok();
                let stderr = stderr.settled();
                return Err(connect_failure(&target, status, &preamble, &stderr));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let probe = parse_remote_probe_output(&target, &preamble);
        let probed = endpoint.remote_socket.clone().map_or(probe.socket, Ok);
        let stopped = Arc::new(AtomicBool::new(false));
        let started = probed
            .and_then(|socket| {
                if remote_socket_is_absolute(&socket) {
                    Ok(())
                } else {
                    Err(EndpointError::InvalidRemoteSocket(socket))
                }
            })
            .and_then(|()| {
                spawn_bridge(listener, stdin, stdout, Arc::clone(&stopped)).map_err(|source| {
                    EndpointError::ForwardIo {
                        target: target.clone(),
                        action: "starting the bridge for",
                        path: local_socket.clone(),
                        source,
                    }
                })
            });
        if let Err(error) = started {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self {
            child,
            local_socket,
            stopped,
        })
    }

    pub(crate) fn local_socket(&self) -> &Path {
        &self.local_socket
    }
}

#[cfg(windows)]
impl Drop for WslForward {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(windows)]
fn connect_failure(
    target: &str,
    status: Option<std::process::ExitStatus>,
    preamble: &[u8],
    stderr: &str,
) -> EndpointError {
    if preamble
        .windows(b"zz-probe-protocol=".len())
        .any(|window| window == b"zz-probe-protocol=")
        && let Err(error) = parse_remote_probe_output(target, preamble).socket
    {
        return error;
    }
    match status.and_then(|status| status.code()) {
        Some(REMOTE_ZZ_MISSING_STATUS) => EndpointError::RemoteBinaryMissing {
            target: target.to_owned(),
        },
        Some(REMOTE_DAEMON_TIMEOUT_STATUS) => EndpointError::RemoteDaemonUnavailable {
            target: target.to_owned(),
        },
        _ => {
            let detail = decode_wsl_text(stderr.as_bytes());
            let detail = detail.trim();
            let reason = if detail.is_empty() {
                status.map_or_else(|| "wsl exited unexpectedly".to_owned(), |s| s.to_string())
            } else {
                detail.to_owned()
            };
            EndpointError::WslFailed {
                target: target.to_owned(),
                reason,
            }
        }
    }
}

#[cfg(windows)]
pub fn probe_wsl(endpoint: &WslEndpoint) -> HostProbe {
    let output = wsl_shell_command(endpoint, &host_probe_script())
        .stdin(Stdio::null())
        .output_unmasked();
    match output {
        Ok(output) => host_probe_from(
            output.status.code(),
            &output.stdout,
            &output.stderr,
            &output.status.to_string(),
        ),
        Err(error) => HostProbe::failed(
            HostStatus::Unreachable,
            format!("could not run wsl.exe: {error}"),
        ),
    }
}

#[cfg(not(windows))]
pub fn probe_wsl(_endpoint: &WslEndpoint) -> crate::HostProbe {
    crate::HostProbe::failed(
        crate::HostStatus::Unreachable,
        "WSL hosts are only available on Windows",
    )
}

#[cfg(windows)]
pub fn install_wsl(
    endpoint: &WslEndpoint,
    restart_daemon: bool,
) -> Result<RemoteInstall, EndpointError> {
    let target = endpoint.to_string();
    refuse_development_install_target(&target)?;
    let script = remote_install_script(endpoint.remote_socket.as_deref(), restart_daemon);
    let spawn_failed = |source| EndpointError::WslSpawn {
        operation: "remote install",
        source,
    };
    let mut child = wsl_shell_command(endpoint, &script)
        .spawn_unmasked()
        .map_err(spawn_failed)?;
    if let Some(mut stdin) = child.stdin.take()
        && let Err(error) = stdin.write_all(INSTALL_SCRIPT.as_bytes())
    {
        log::debug!("the WSL install for {endpoint} stopped reading its script: {error}");
    }
    let output = child.wait_with_output().map_err(spawn_failed)?;
    parse_remote_install_output(
        &target,
        output.status.code(),
        &output.stdout,
        &output.stderr,
        &output.status.to_string(),
    )
}

#[cfg(not(windows))]
pub fn install_wsl(
    _endpoint: &WslEndpoint,
    _restart_daemon: bool,
) -> Result<crate::RemoteInstall, crate::EndpointError> {
    Err(crate::EndpointError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utf16_and_utf8_wsl_lists_and_filters_container_distros() {
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "Ubuntu-24.04\r\ndocker-desktop\r\ndocker-desktop-data\r\narchlinux\r\npodman-machine-default\r\n"
            .encode_utf16()
        {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(parse_wsl_list_output(&utf16), ["Ubuntu-24.04", "archlinux"]);
        assert_eq!(
            parse_wsl_list_output(b"archlinux\r\nUbuntu\r\narchlinux\r\n"),
            ["archlinux", "Ubuntu"]
        );
    }

    #[test]
    fn distro_host_names_avoid_whitespace_and_reserved_names() {
        assert_eq!(distro_host_name("Ubuntu-24.04"), "Ubuntu-24.04");
        assert_eq!(distro_host_name("My Distro"), "My-Distro");
        assert_eq!(distro_host_name("local"), "wsl-local");
    }
}
