use std::{
    fmt,
    io::{self, Write as _},
    path::{Path, PathBuf},
    str::FromStr,
};

#[cfg(any(unix, windows, test))]
use std::process::Command;
#[cfg(unix)]
use std::{
    ffi::{OsStr, OsString},
    fs,
    hash::{Hash as _, Hasher as _},
    net::{Ipv4Addr, TcpListener},
    os::unix::fs::{FileTypeExt, PermissionsExt},
    process::{Child, Stdio},
    sync::OnceLock,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};
#[cfg(windows)]
use std::{
    fmt::Write as _,
    io::{BufRead as _, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

#[cfg(windows)]
use parking_lot::Mutex;
use thiserror::Error;

use crate::DaemonError;
#[cfg(any(unix, windows))]
use crate::askpass::{AskpassListener, SshPrompts};
#[cfg(windows)]
use crate::transport::{LocalListener, LocalStream, TransportListener as _};
use crate::transport::{LocalTransport, Transport as _, TransportStream as _};
#[cfg(any(unix, windows, test))]
use crate::unmasked::SpawnUnmasked as _;

/// `sh -l` reads `/etc/profile` and `~/.profile`; a PATH exported from `~/.bash_profile` or
/// `~/.zshrc` never reaches it, which hides the CLI that install.sh linked into one of these.
/// Appending leaves whatever the profile did contribute ahead of the fallbacks. A Mac app dragged
/// out of the disk image has no CLI link at all, so `$zz_cli` falls back to the bundle's own `cli`.
#[cfg(any(unix, windows, test))]
fn remote_path_fallback() -> String {
    format!(
        "PATH=\"$PATH:$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin\"; export PATH; \
         zz_cli={name}; if ! command -v {name} >/dev/null 2>&1; then \
         for zz_apps in /Applications \"$HOME/Applications\"; do \
         if [ -x \"$zz_apps/{app}.app/Contents/MacOS/cli\" ]; \
         then zz_cli=\"$zz_apps/{app}.app/Contents/MacOS/cli\"; break; fi; done; fi; ",
        name = zz_protocol::app_identity::DIRECTORY,
        app = zz_protocol::app_identity::DISPLAY_NAME,
    )
}

#[cfg(any(unix, windows, test))]
// Runs under `sh -lc` so `zz` resolves through the login shell's PATH; the sentinel prefixes
// let the parser skip whatever a login profile prints around the probe's own output.
pub(crate) fn remote_socket_probe(key_line: Option<&str>) -> String {
    shell_quote(&remote_socket_probe_script(key_line))
}

#[cfg(any(unix, windows, test))]
fn remote_socket_probe_script(key_line: Option<&str>) -> String {
    let desktop_socket = format!(
        "elif [ \"$(uname -s)\" = Darwin ] && [ -S \"/tmp/{name}-$USER/default.sock\" ]; \
         then zz_dir=\"/tmp/{name}-$USER\"; ",
        name = zz_protocol::app_identity::DIRECTORY,
    );
    let enrollment = key_line.map_or_else(String::new, |line| {
        format!(
            "( {} ) || printf 'zz-probe-key=failed\\n'; {}",
            authorized_key_script(crate::ssh_key::key_body(line), line),
            remote_zz_key_script(),
        )
    });
    format!(
        "{fallback}{enrollment}if [ -n \"$XDG_RUNTIME_DIR\" ]; then zz_dir=\"$XDG_RUNTIME_DIR/{name}\"; \
         {desktop_socket}else zz_tmp=\"$TMPDIR\"; \
         if [ -z \"$zz_tmp\" ]; then zz_tmp=$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null); fi; \
         case \"$zz_tmp\" in /*) ;; *) zz_tmp=/tmp ;; esac; \
         zz_dir=\"${{zz_tmp%/}}/{name}-$USER\"; fi; \
         printf \"zz-probe-socket=%s\\n\" \"$zz_dir/default.sock\"; \
         if command -v \"$zz_cli\" >/dev/null 2>&1; then printf \"zz-probe-protocol=%s\\n\" \"$(\"$zz_cli\" protocol-version 2>/dev/null || echo unknown)\"; \
         else printf \"zz-probe-protocol=missing\\n\"; fi",
        fallback = remote_path_fallback(),
        name = zz_protocol::app_identity::DIRECTORY,
    )
}

#[cfg(any(unix, windows, test))]
pub(crate) fn remote_zz_key_script() -> String {
    format!(
        "if [ \"$(uname -s)\" = Darwin ]; then zz_ssh=\"$HOME/Library/Application Support/{name}/ssh\"; \
         else case \"$XDG_DATA_HOME\" in /*) zz_ssh=\"$XDG_DATA_HOME/{name}/ssh\" ;; \
         *) zz_ssh=\"$HOME/.local/share/{name}/ssh\" ;; esac; fi; \
         if [ ! -f \"$zz_ssh/id_ed25519\" ] && command -v ssh-keygen >/dev/null 2>&1; then \
         zz_host=$(uname -n | cut -d. -f1 | tr -cd 'A-Za-z0-9_-'); \
         ( umask 077; mkdir -p \"$zz_ssh\" && chmod 700 \"$zz_ssh\" && ssh-keygen -q -t ed25519 -N '' \
         -C \"zz@${{zz_host:-host}}\" -f \"$zz_ssh/id_ed25519\" </dev/null >/dev/null 2>&1 ); fi; \
         if [ -f \"$zz_ssh/id_ed25519.pub\" ]; \
         then printf 'zz-probe-pubkey=%s\\n' \"$(cat \"$zz_ssh/id_ed25519.pub\")\"; fi; ",
        name = zz_protocol::app_identity::DIRECTORY,
    )
}
/// Exit codes the auto-start script picks for itself; ssh reports its own failures as 255.
#[cfg(any(unix, windows, test))]
pub(crate) const REMOTE_ZZ_MISSING_STATUS: i32 = 127;
#[cfg(any(unix, windows, test))]
pub(crate) const REMOTE_DAEMON_TIMEOUT_STATUS: i32 = 3;
#[cfg(any(unix, windows, test))]
const REMOTE_INSTALL_ELSEWHERE_STATUS: i32 = 4;
#[cfg(any(unix, windows, test))]
const REMOTE_INSTALL_FAILED_STATUS: i32 = 5;
#[cfg(any(unix, windows, test))]
const REMOTE_INSTALL_BROKEN_STATUS: i32 = 6;
#[cfg(any(unix, windows, test))]
const INSTALL_SCRIPT: &str = include_str!("../../../install.sh");
#[cfg(any(unix, windows, test))]
const REMOTE_DAEMON_POLL_ATTEMPTS: u32 = 50;
#[cfg(any(unix, windows, test))]
const REMOTE_DAEMON_POLL_INTERVAL: &str = "0.1";
/// POSIX only requires whole-second `sleep`, so a remote that rejects the fraction polls seconds.
#[cfg(any(unix, windows, test))]
const REMOTE_DAEMON_POLL_FALLBACK_ATTEMPTS: u32 = 5;
#[cfg(unix)]
const FORWARD_POLL_ATTEMPTS: usize = 250;
#[cfg(unix)]
const FORWARD_POLL_INTERVAL: Duration = Duration::from_millis(20);
#[cfg(unix)]
const MAX_FORWARD_SOCKET_PATH_BYTES: usize = 100;
#[cfg(unix)]
const CONTROL_PERSIST_SECONDS: u32 = 60;
/// Leaves room inside `sun_path` for the private directory and the socket names under it.
#[cfg(unix)]
const MAX_SSH_RUNTIME_ROOT_BYTES: usize = 60;
#[cfg(unix)]
static FORWARD_COUNTER: AtomicU64 = AtomicU64::new(1);
#[cfg(windows)]
static FORWARD_COUNTER: AtomicU64 = AtomicU64::new(1);
/// What the remote proxy prints before its first protocol byte, ahead of any login chatter.
pub(crate) const PROXY_READY_MARKER: &[u8] = b"zz-proxy-1\n";
#[cfg(windows)]
const MAX_PROXY_PREAMBLE_BYTES: usize = 64 * 1024;
#[cfg(windows)]
const MAX_SSH_STDERR_BYTES: usize = 8 * 1024;
const PUMP_BUFFER_BYTES: usize = 16 * 1024;
#[cfg(windows)]
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    Local(PathBuf),
    Ssh(SshEndpoint),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshEndpoint {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub remote_socket: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum EndpointError {
    #[error("invalid endpoint URI `{input}`: {reason}")]
    UriParse { input: String, reason: String },
    #[error("failed to spawn ssh for {operation}: {source}")]
    SshSpawn {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("ssh socket probe for {target} failed: {reason}")]
    ProbeFailure { target: String, reason: String },
    #[error("ssh to {target} failed: {reason}")]
    SshFailed { target: String, reason: String },
    #[error("the SSH host key for {target} was not trusted")]
    HostKeyRejected { target: String },
    #[error("SSH authentication to {target} failed: {reason}")]
    AuthenticationFailed { target: String, reason: String },
    #[error("{target} closed the connection before sign-in finished")]
    SignInExpired { target: String },
    #[error("zz is not installed on {target}")]
    RemoteBinaryMissing { target: String },
    #[error("the zz daemon on {target} never started listening")]
    RemoteDaemonUnavailable { target: String },
    #[error("zz on {target} speaks protocol v{daemon}; this zz speaks v{client}")]
    RemoteProtocolMismatch {
        target: String,
        daemon: u16,
        client: u16,
    },
    #[error("zz on {target} predates protocol version reporting; update it, then reconnect")]
    RemoteProtocolUnknown { target: String },
    #[error("zz on {target} is {path}, which installing over ssh does not manage")]
    RemoteInstallElsewhere { target: String, path: String },
    #[error("installing zz on {target} failed: {reason}")]
    RemoteInstallFailed { target: String, reason: String },
    #[error("the zz installed on {target} speaks protocol v{remote}; this zz speaks v{client}")]
    RemoteInstallMismatch {
        target: String,
        remote: u16,
        client: u16,
    },
    #[error(
        "ssh forward to {target} exited with {status} before creating {local_socket}",
        local_socket = local_socket.display()
    )]
    ForwardExited {
        target: String,
        status: String,
        local_socket: PathBuf,
    },
    #[error(
        "ssh forward to {target} did not create {local_socket} within five seconds",
        local_socket = local_socket.display()
    )]
    ForwardTimeout {
        target: String,
        local_socket: PathBuf,
    },
    #[error("ssh forward to {target} failed while {action} {path}: {source}", path = path.display())]
    ForwardIo {
        target: String,
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "ssh remote socket path must be absolute: {path}",
        path = .0.display()
    )]
    InvalidRemoteSocket(PathBuf),
    #[error("ssh endpoints are unsupported on this platform")]
    UnsupportedPlatform,
}

impl EndpointError {
    /// The endpoint failure behind `error`, looking through the `io::Error` a connect wraps it in.
    #[must_use]
    pub fn find<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a Self> {
        let mut source = Some(error);
        while let Some(current) = source {
            if let Some(found) = current.downcast_ref::<Self>() {
                return Some(found);
            }
            source = match current
                .downcast_ref::<io::Error>()
                .and_then(io::Error::get_ref)
            {
                Some(inner) => Some(inner),
                None => current.source(),
            };
        }
        None
    }

    /// The same failure `Display` reports, said in terms of what the user can do about it.
    #[must_use]
    pub fn ssh_reason(&self) -> Option<String> {
        Some(match self {
            Self::SshSpawn { operation, source } => {
                format!("Could not run ssh for the {operation}: {source}")
            }
            Self::ProbeFailure { target, reason } | Self::SshFailed { target, reason } => {
                format!("Could not reach {target} over ssh: {reason}")
            }
            Self::HostKeyRejected { target } => {
                format!("The SSH host key for {target} was not trusted.")
            }
            Self::AuthenticationFailed { target, reason } => {
                format!("Could not sign in to {target}: {reason}")
            }
            Self::SignInExpired { target } => format!(
                "{target} stopped waiting for the password.\nssh allows about two minutes to sign \
                 in; connect again."
            ),
            Self::RemoteBinaryMissing { target } => format!(
                "{} is not installed on {target}.\nInstall it there with `curl -fsSL \
                 https://zzmux.sh/install.sh | sh`, then connect again.",
                zz_protocol::app_identity::DIRECTORY
            ),
            Self::RemoteDaemonUnavailable { target } => format!(
                "Started {name} on {target}, but its daemon socket never appeared.\nRun `{name} daemon` \
                 there to see why.",
                name = zz_protocol::app_identity::DIRECTORY,
            ),
            Self::RemoteProtocolMismatch {
                target,
                daemon,
                client,
            } => format!(
                "{name} on {target} speaks protocol v{daemon}; this {name} speaks v{client}.\nUpdate {name} on \
                 that host, then reconnect.",
                name = zz_protocol::app_identity::DIRECTORY,
            ),
            Self::RemoteProtocolUnknown { target } => {
                format!(
                    "{} on {target} predates protocol version reporting; update it, then reconnect",
                    zz_protocol::app_identity::DIRECTORY,
                )
            }
            Self::RemoteInstallElsewhere { target, path } => format!(
                "{name} on {target} is {path}, which installing over ssh does not manage.\nUpdate \
                 it the way it was installed, then reconnect.",
                name = zz_protocol::app_identity::DIRECTORY,
            ),
            Self::RemoteInstallFailed { target, reason } => format!(
                "Could not install {} on {target}: {reason}",
                zz_protocol::app_identity::DIRECTORY,
            ),
            Self::RemoteInstallMismatch {
                target,
                remote,
                client,
            } => format!(
                "Installed {name} {version} on {target}, but it speaks protocol v{remote} and this \
                 {name} speaks v{client}.\nThis build has no matching release.",
                name = zz_protocol::app_identity::DIRECTORY,
                version = env!("CARGO_PKG_VERSION"),
            ),
            Self::ForwardExited { target, status, .. } => format!(
                "The ssh forward to {target} exited ({status}).\nDoes its sshd allow local \
                 forwarding?"
            ),
            Self::ForwardTimeout { target, .. } => {
                format!("ssh connected to {target}, but never created the forwarded socket.")
            }
            Self::ForwardIo {
                target,
                action,
                source,
                ..
            } => format!("The ssh forward to {target} failed while {action} its socket: {source}"),
            Self::InvalidRemoteSocket(path) => format!(
                "The remote socket path must be absolute: {path}",
                path = path.display()
            ),
            Self::UnsupportedPlatform => "ssh hosts are not supported on this platform.".to_owned(),
            Self::UriParse { .. } => return None,
        })
    }
}

impl Endpoint {
    pub fn parse(input: &str) -> Result<Self, EndpointError> {
        if input.starts_with("quic://") {
            return Err(parse_error(
                input,
                "quic endpoints were removed; use ssh://",
            ));
        }
        if let Some(value) = input.strip_prefix("ssh://") {
            return parse_ssh_endpoint(input, value).map(Self::Ssh);
        }
        if let Some(path) = input.strip_prefix("unix://") {
            if !path.starts_with('/') {
                return Err(parse_error(input, "unix endpoint path must be absolute"));
            }
            return Ok(Self::Local(PathBuf::from(path)));
        }
        if input.is_empty() {
            return Err(parse_error(input, "endpoint is empty"));
        }
        if let Some((scheme, _)) = input.split_once("://") {
            return Err(parse_error(
                input,
                format!("unsupported endpoint scheme `{scheme}`"),
            ));
        }
        Ok(Self::Local(PathBuf::from(input)))
    }
}

impl FromStr for Endpoint {
    type Err = EndpointError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(path) => path.display().fmt(formatter),
            Self::Ssh(endpoint) => endpoint.fmt(formatter),
        }
    }
}

impl fmt::Display for SshEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ssh://")?;
        if let Some(user) = &self.user {
            write!(formatter, "{user}@")?;
        }
        if self.host.contains(':') {
            write!(formatter, "[{}]", self.host)?;
        } else {
            formatter.write_str(&self.host)?;
        }
        if let Some(port) = self.port {
            write!(formatter, ":{port}")?;
        }
        if let Some(remote_socket) = &self.remote_socket {
            remote_socket.display().fmt(formatter)?;
        }
        Ok(())
    }
}

impl From<EndpointError> for DaemonError {
    fn from(error: EndpointError) -> Self {
        Self::Io(io::Error::other(error))
    }
}

fn parse_ssh_endpoint(input: &str, value: &str) -> Result<SshEndpoint, EndpointError> {
    let (authority, remote_socket) = match value.split_once('/') {
        Some((authority, path)) => (authority, Some(PathBuf::from(format!("/{path}")))),
        None => (value, None),
    };
    if authority.is_empty() {
        return Err(parse_error(input, "ssh host is empty"));
    }

    let (user, host_and_port) = match authority.split_once('@') {
        Some((user, host)) => {
            if user.is_empty() {
                return Err(parse_error(input, "ssh user is empty"));
            }
            if host.contains('@') {
                return Err(parse_error(
                    input,
                    "ssh authority contains multiple `@` signs",
                ));
            }
            if user.chars().any(char::is_whitespace) {
                return Err(parse_error(input, "ssh user contains whitespace"));
            }
            (Some(user.to_owned()), host)
        }
        None => (None, authority),
    };

    let (host, port) = parse_host_and_port(input, host_and_port)?;
    if host.is_empty() {
        return Err(parse_error(input, "ssh host is empty"));
    }
    if host.chars().any(char::is_whitespace) {
        return Err(parse_error(input, "ssh host contains whitespace"));
    }
    // A leading `-` reaches ssh's option parser instead of its destination argument.
    if host.starts_with('-') {
        return Err(parse_error(input, "ssh host must not start with `-`"));
    }

    Ok(SshEndpoint {
        user,
        host,
        port,
        remote_socket,
    })
}

fn parse_host_and_port(
    input: &str,
    host_and_port: &str,
) -> Result<(String, Option<u16>), EndpointError> {
    if let Some(bracketed) = host_and_port.strip_prefix('[') {
        let Some(closing) = bracketed.find(']') else {
            return Err(parse_error(input, "unterminated bracketed ssh host"));
        };
        let host = &bracketed[..closing];
        let suffix = &bracketed[closing + 1..];
        let port = if suffix.is_empty() {
            None
        } else {
            let Some(port) = suffix.strip_prefix(':') else {
                return Err(parse_error(input, "invalid bracketed ssh authority"));
            };
            Some(parse_port(input, port)?)
        };
        return Ok((host.to_owned(), port));
    }
    if host_and_port.contains(['[', ']']) {
        return Err(parse_error(input, "invalid bracketed ssh authority"));
    }

    match host_and_port.rsplit_once(':') {
        Some((host, _port)) if host.contains(':') => Err(parse_error(
            input,
            "IPv6 ssh hosts must be enclosed in brackets",
        )),
        Some((host, port)) => Ok((host.to_owned(), Some(parse_port(input, port)?))),
        None => Ok((host_and_port.to_owned(), None)),
    }
}

fn parse_port(input: &str, port: &str) -> Result<u16, EndpointError> {
    match port.parse::<u16>() {
        Ok(port) if port != 0 => Ok(port),
        _ => Err(parse_error(input, "ssh port must be between 1 and 65535")),
    }
}

fn parse_error(input: &str, reason: impl Into<String>) -> EndpointError {
    EndpointError::UriParse {
        input: input.to_owned(),
        reason: reason.into(),
    }
}

#[cfg(any(unix, windows, test))]
#[derive(Clone, Copy, Debug, Default)]
struct SshSession<'a> {
    #[cfg(not(windows))]
    control_path: Option<&'a Path>,
    askpass: Option<SshAskpass<'a>>,
}

#[cfg(any(unix, windows, test))]
#[derive(Clone, Copy, Debug)]
struct SshAskpass<'a> {
    helper: &'a Path,
    socket: &'a Path,
}

#[cfg(any(unix, test))]
fn forwarding_spec(local_socket: &Path, remote_socket: &Path) -> std::ffi::OsString {
    let mut forwarding = local_socket.as_os_str().to_os_string();
    forwarding.push(":");
    forwarding.push(remote_socket.as_os_str());
    forwarding
}

/// Binds SOCKS to loopback explicitly: ssh's default bind address follows `GatewayPorts`.
#[cfg(any(unix, test))]
fn socks_spec(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

#[cfg(any(unix, test))]
fn ssh_forward_command(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    local_socket: &Path,
    remote_socket: &Path,
    socks_port: Option<u16>,
) -> Command {
    let mut command = Command::new("ssh");
    command
        .arg("-N")
        .arg("-o")
        .arg("ExitOnForwardFailure=yes")
        .arg("-o")
        .arg("StreamLocalBindMask=0177")
        .arg("-L")
        .arg(forwarding_spec(local_socket, remote_socket));
    if let Some(port) = socks_port {
        command.arg("-D").arg(socks_spec(port));
    }
    append_ssh_options(&mut command, endpoint, session);
    command.arg("--").arg(&endpoint.host);
    command
}

/// `ssh -D 0` is invalid, so ask the kernel for a candidate and retry startup if the handoff races.
#[cfg(unix)]
fn available_socks_port() -> Option<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    drop(listener);
    Some(port)
}

#[cfg(any(unix, windows, test))]
fn ssh_probe_command(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    identity: Option<&Path>,
    key_line: Option<&str>,
) -> Command {
    ssh_script_command(
        endpoint,
        session,
        identity,
        &[],
        &remote_socket_probe(key_line),
    )
}

#[cfg(any(unix, windows, test))]
fn ssh_script_command(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    identity: Option<&Path>,
    options: &[&str],
    script: &str,
) -> Command {
    let mut command = Command::new("ssh");
    if let Some(identity) = identity {
        zz_key_login_options(&mut command, identity);
    }
    for option in options {
        command.arg("-o").arg(option);
    }
    append_ssh_options(&mut command, endpoint, session);
    command
        .arg("--")
        .arg(&endpoint.host)
        .arg("sh")
        .arg("-lc")
        .arg(script);
    command
}

#[cfg(any(unix, windows, test))]
fn zz_key_login_options(command: &mut Command, identity: &Path) {
    command
        .arg("-o")
        .arg("IdentitiesOnly=yes")
        .arg("-o")
        .arg(format!(
            "IdentityFile=\"{}\"",
            identity.to_string_lossy().replace('%', "%%")
        ))
        .arg("-o")
        .arg("PreferredAuthentications=publickey")
        .arg("-o")
        .arg("BatchMode=yes");
}

#[cfg(any(unix, windows))]
fn zz_key_first(
    identity: Option<&Path>,
    command: impl Fn(Option<&Path>) -> Command,
) -> io::Result<std::process::Output> {
    if let Some(identity) = identity {
        let output = command(Some(identity)).output_unmasked()?;
        if !refused_login(output.status.code(), &output.stderr) {
            return Ok(output);
        }
        log::debug!(
            target: "zz_daemon::diagnostics",
            "the zz key did not sign in: {}",
            String::from_utf8_lossy(&output.stderr).trim(),
        );
    }
    command(None).output_unmasked()
}

#[cfg(any(unix, windows, test))]
fn refused_login(code: Option<i32>, stderr: &[u8]) -> bool {
    code == Some(SSH_FAILURE_STATUS)
        && SshFailure::classify(&String::from_utf8_lossy(stderr)).is_some_and(SshFailure::is_login)
}

#[cfg(any(unix, windows))]
fn zz_identity() -> Option<(PathBuf, String)> {
    let identity = crate::ssh_key::ssh_key_path()
        .and_then(|path| crate::ssh_key::ssh_public_key().map(|line| (path, line)));
    match identity {
        Ok(identity) => Some(identity),
        Err(error) => {
            log::debug!(target: "zz_daemon::diagnostics", "no zz ssh key: {error}");
            None
        }
    }
}

#[cfg(any(unix, windows, test))]
fn ssh_daemon_start_command(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    remote_socket: &Path,
) -> Command {
    let mut command = Command::new("ssh");
    append_ssh_options(&mut command, endpoint, session);
    command
        .arg("--")
        .arg(&endpoint.host)
        .arg("sh")
        .arg("-lc")
        .arg(shell_quote(&remote_daemon_start_script(remote_socket)));
    command
}

#[cfg(any(unix, windows, test))]
pub(crate) fn remote_daemon_start_script(remote_socket: &Path) -> String {
    let socket = shell_quote(&remote_socket.to_string_lossy());
    format!(
        "{fallback}command -v \"$zz_cli\" >/dev/null 2>&1 || exit {REMOTE_ZZ_MISSING_STATUS}; \
         if setsid true >/dev/null 2>&1; \
         then setsid \"$zz_cli\" daemon --socket {socket} >/dev/null 2>&1 </dev/null & \
         else nohup \"$zz_cli\" daemon --socket {socket} >/dev/null 2>&1 </dev/null & fi; \
         if sleep {REMOTE_DAEMON_POLL_INTERVAL} 2>/dev/null; \
         then delay={REMOTE_DAEMON_POLL_INTERVAL}; attempts={REMOTE_DAEMON_POLL_ATTEMPTS}; \
         else delay=1; attempts={REMOTE_DAEMON_POLL_FALLBACK_ATTEMPTS}; fi; \
         attempt=0; \
         while [ \"$attempt\" -lt \"$attempts\" ]; do \
         if [ -S {socket} ]; then exit 0; fi; \
         sleep \"$delay\"; attempt=$((attempt + 1)); done; \
         exit {REMOTE_DAEMON_TIMEOUT_STATUS}",
        fallback = remote_path_fallback(),
    )
}

#[cfg(any(unix, windows, test))]
pub(crate) fn remote_install_script(remote_socket: Option<&Path>, restart_daemon: bool) -> String {
    let installed = format!(
        "\"$HOME/.local/bin/{}\"",
        zz_protocol::app_identity::DIRECTORY
    );
    let restart = if restart_daemon {
        let socket = remote_socket.map_or_else(String::new, |socket| {
            format!("--socket {} ", shell_quote(&socket.to_string_lossy()))
        });
        format!(
            "if [ \"$zz_protocol\" = {protocol} ]; then {installed} {socket}kill-server \
             >/dev/null 2>&1; fi; ",
            protocol = zz_protocol::PROTOCOL_VERSION,
        )
    } else {
        String::new()
    };
    format!(
        "{fallback}zz_found=$(command -v \"$zz_cli\" 2>/dev/null); \
         if [ -n \"$zz_found\" ] && {{ [ \"$zz_found\" != {installed} ] || [ -L \"$zz_found\" ]; }}; \
         then printf 'zz-install-elsewhere=%s\\n' \"$zz_found\"; exit {REMOTE_INSTALL_ELSEWHERE_STATUS}; fi; \
         sh -s -- --headless --version {version} || exit {REMOTE_INSTALL_FAILED_STATUS}; \
         zz_protocol=$({installed} protocol-version) || exit {REMOTE_INSTALL_BROKEN_STATUS}; \
         {restart}printf 'zz-install-protocol=%s\\n' \"$zz_protocol\"",
        fallback = remote_path_fallback(),
        version = env!("CARGO_PKG_VERSION"),
    )
}

#[cfg(any(unix, windows, test))]
fn ssh_install_command(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    restart_daemon: bool,
) -> Command {
    let mut command = Command::new("ssh");
    append_ssh_options(&mut command, endpoint, session);
    command
        .arg("--")
        .arg(&endpoint.host)
        .arg("sh")
        .arg("-lc")
        .arg(shell_quote(&remote_install_script(
            endpoint.remote_socket.as_deref(),
            restart_daemon,
        )));
    command
}

#[cfg(any(unix, windows, test))]
pub(crate) fn parse_remote_install_output(
    target: &str,
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    status: &str,
) -> Result<(), EndpointError> {
    let stdout = String::from_utf8_lossy(stdout);
    let value = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .map(|value| value.trim().to_owned())
    };
    let stderr = String::from_utf8_lossy(stderr);
    let target = target.to_owned();
    match code {
        Some(0) => match value("zz-install-protocol=").map(|value| value.parse::<u16>()) {
            Some(Ok(zz_protocol::PROTOCOL_VERSION)) => Ok(()),
            Some(Ok(remote)) => Err(EndpointError::RemoteInstallMismatch {
                target,
                remote,
                client: zz_protocol::PROTOCOL_VERSION,
            }),
            _ => Err(EndpointError::RemoteInstallFailed {
                target,
                reason: "the installed binary did not report its protocol version".to_owned(),
            }),
        },
        Some(REMOTE_INSTALL_ELSEWHERE_STATUS)
            if let Some(path) = value("zz-install-elsewhere=") =>
        {
            Err(EndpointError::RemoteInstallElsewhere { target, path })
        }
        Some(REMOTE_INSTALL_FAILED_STATUS) => {
            let reason = ssh_failure_reason(&stderr, status);
            Err(EndpointError::RemoteInstallFailed {
                target,
                reason: reason.strip_prefix("error: ").unwrap_or(&reason).to_owned(),
            })
        }
        Some(REMOTE_INSTALL_BROKEN_STATUS) => Err(EndpointError::RemoteInstallFailed {
            target,
            reason: format!(
                "the installed binary does not run there: {}",
                ssh_failure_reason(&stderr, status)
            ),
        }),
        _ => Err(EndpointError::SshFailed {
            target,
            reason: ssh_failure_reason(&stderr, status),
        }),
    }
}

/// The remote half of an attach that cannot forward a Unix socket: `zz proxy` on its stdio.
#[cfg(any(windows, target_os = "ios", test))]
pub(crate) fn remote_proxy_script(remote_socket: &Path) -> String {
    format!(
        "{fallback}exec \"$zz_cli\" proxy --socket {socket}",
        fallback = remote_path_fallback(),
        socket = shell_quote(&remote_socket.to_string_lossy()),
    )
}

/// Appends `line` to the remote `~/.ssh/authorized_keys` unless `key` is already there, creating
/// the directory and file private as sshd's `StrictModes` wants, and never gluing onto a last line
/// that lacks its newline.
#[cfg(any(unix, windows, test))]
pub(crate) fn authorized_key_script(key: &str, line: &str) -> String {
    let file = "\"$HOME/.ssh/authorized_keys\"";
    format!(
        "umask 077; mkdir -p \"$HOME/.ssh\" && touch {file} || exit 1; \
         if grep -qF {key} {file}; then printf 'zz-probe-key=present\\n'; exit 0; fi; \
         if [ -s {file} ] && [ -n \"$(tail -c 1 {file})\" ]; then echo >> {file}; fi; \
         printf '%s\\n' {line} >> {file} || exit 1; printf 'zz-probe-key=added\\n'",
        key = shell_quote(key),
        line = shell_quote(line),
    )
}

/// Quote a value so the remote shell sees it verbatim; an ssh command line is parsed twice.
#[cfg(any(unix, windows, test))]
pub fn shell_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for character in value.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(any(unix, windows, test))]
const SSH_FAILURE_STATUS: i32 = 255;

#[cfg(any(unix, windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SshFailure {
    SecurityKey,
    Login,
    HostKey,
    Resolve,
    Refused,
    Timeout,
    Unreachable,
}

#[cfg(any(unix, windows, test))]
impl SshFailure {
    pub(crate) fn classify(stderr: &str) -> Option<Self> {
        let stderr = stderr.to_ascii_lowercase();
        let contains = |needle: &str| stderr.contains(needle);
        if (contains("signing failed for") && contains("-sk"))
            || contains("device not found")
            || contains("user presence")
        {
            Some(Self::SecurityKey)
        } else if contains("permission denied")
            || contains("too many authentication failures")
            || contains("no supported authentication methods")
        {
            Some(Self::Login)
        } else if contains("host key verification failed")
            || contains("remote host identification has changed")
        {
            Some(Self::HostKey)
        } else if contains("could not resolve hostname")
            || contains("name or service not known")
            || contains("nodename nor servname")
        {
            Some(Self::Resolve)
        } else if contains("connection refused") {
            Some(Self::Refused)
        } else if contains("timed out") {
            Some(Self::Timeout)
        } else if contains("no route to host") || contains("network is unreachable") {
            Some(Self::Unreachable)
        } else {
            None
        }
    }

    pub(crate) const fn is_login(self) -> bool {
        matches!(self, Self::SecurityKey | Self::Login | Self::HostKey)
    }

    const fn hint(self) -> &'static str {
        match self {
            Self::SecurityKey => "the security key was not touched",
            Self::Login => "ssh rejected the login: add your key to the host or start an ssh agent",
            Self::HostKey => {
                "the host key does not match known_hosts: verify the host before connecting"
            }
            Self::Resolve => "the host name did not resolve",
            Self::Refused => {
                "nothing accepted the ssh connection: start sshd there, or on a Mac turn on Remote \
                 Login in System Settings › General › Sharing"
            }
            Self::Timeout => "the ssh connection timed out",
            Self::Unreachable => "the host is not reachable from this network",
        }
    }
}

/// ssh collapses every failure of its own into exit 255, so stderr is all there is to go on.
#[cfg(any(unix, windows, test))]
pub(crate) fn ssh_failure_hint(stderr: &str) -> Option<&'static str> {
    SshFailure::classify(stderr).map(SshFailure::hint)
}

#[cfg(any(unix, windows, test))]
fn ssh_failure_reason(stderr: &str, status: &str) -> String {
    if let Some(hint) = ssh_failure_hint(stderr) {
        return hint.to_owned();
    }
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map_or_else(|| format!("ssh exited with {status}"), ToOwned::to_owned)
}

#[cfg(any(unix, windows, test))]
fn append_ssh_options(command: &mut Command, endpoint: &SshEndpoint, session: SshSession<'_>) {
    command
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-o")
        .arg("ServerAliveInterval=5")
        .arg("-o")
        .arg("ServerAliveCountMax=3")
        .arg("-o")
        .arg("Compression=yes");
    // Windows OpenSSH has no connection sharing, and any set `ControlPath` still routes children
    // into its broken mux stub, so both options are overridden rather than omitted.
    #[cfg(windows)]
    command
        .arg("-o")
        .arg("ControlMaster=no")
        .arg("-o")
        .arg("ControlPath=none");
    #[cfg(not(windows))]
    if let Some(control_path) = session.control_path {
        command
            .arg("-o")
            .arg("ControlMaster=auto")
            .arg("-o")
            .arg(control_path_option(control_path))
            .arg("-o")
            .arg(format!("ControlPersist={CONTROL_PERSIST_SECONDS}"));
    }
    if let Some(port) = endpoint.port {
        command.arg("-p").arg(port.to_string());
    }
    if let Some(user) = &endpoint.user {
        command.arg("-l").arg(user);
    }
    if let Some(askpass) = session.askpass {
        // `force`, not `prefer`: `prefer` skips the helper whenever ssh has a controlling TTY, and
        // on Windows it resolves to off outright. `BatchMode` stays unset; it kills prompts.
        command
            .env(crate::askpass::SSH_ASKPASS_ENV, askpass.helper)
            .env(crate::askpass::SSH_ASKPASS_REQUIRE_ENV, "force")
            .env(crate::askpass::ASKPASS_SOCKET_ENV, askpass.socket);
    }
}

/// ssh expands `%` tokens inside `ControlPath`, so a literal one has to be escaped.
#[cfg(any(unix, test))]
fn control_path_option(control_path: &Path) -> String {
    format!(
        "ControlPath={}",
        control_path.to_string_lossy().replace('%', "%%")
    )
}

/// A 0700 per-process directory for the ssh control and askpass sockets, both of which are
/// capabilities: one owns an ssh session, the other can phish a password out of the dialog.
#[cfg(unix)]
pub(crate) fn ssh_runtime_dir() -> io::Result<&'static Path> {
    static DIRECTORY: OnceLock<Result<tempfile::TempDir, String>> = OnceLock::new();
    match DIRECTORY.get_or_init(|| {
        let directory = tempfile::Builder::new()
            .prefix("zz-ssh-")
            .tempdir_in(ssh_runtime_root())
            .map_err(|error| error.to_string())?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        if directory.path().to_str().is_none() {
            return Err(format!(
                "temporary directory {} is not valid UTF-8",
                directory.path().display()
            ));
        }
        Ok(directory)
    }) {
        Ok(directory) => Ok(directory.path()),
        Err(error) => Err(io::Error::other(error.clone())),
    }
}

#[cfg(unix)]
fn ssh_runtime_root() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .filter(|path| {
            path.is_absolute()
                && path.as_os_str().as_encoded_bytes().len() <= MAX_SSH_RUNTIME_ROOT_BYTES
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

#[cfg(unix)]
fn control_master_path(endpoint: &SshEndpoint) -> io::Result<PathBuf> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    endpoint.user.hash(&mut hasher);
    endpoint.host.hash(&mut hasher);
    endpoint.port.hash(&mut hasher);
    Ok(ssh_runtime_dir()?.join(format!("c{:016x}", hasher.finish())))
}

#[cfg(unix)]
fn discard_dead_control_master(endpoint: &SshEndpoint, control_path: &Path) {
    if !control_path.exists() {
        return;
    }
    let alive = ssh_control_command(endpoint, control_path, "check", None)
        .status_unmasked()
        .is_ok_and(|status| status.success());
    if !alive {
        let _ = fs::remove_file(control_path);
    }
}

#[cfg(unix)]
fn ssh_control_command(
    endpoint: &SshEndpoint,
    control_path: &Path,
    request: &str,
    forwarding: Option<(&str, &OsStr)>,
) -> Command {
    let mut command = Command::new("ssh");
    command
        .arg("-O")
        .arg(request)
        .arg("-o")
        .arg(control_path_option(control_path));
    if let Some((flag, spec)) = forwarding {
        command.arg(flag).arg(spec);
    }
    if let Some(port) = endpoint.port {
        command.arg("-p").arg(port.to_string());
    }
    if let Some(user) = &endpoint.user {
        command.arg("-l").arg(user);
    }
    command
        .arg("--")
        .arg(&endpoint.host)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[cfg(any(unix, windows))]
fn probe_remote_socket(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
) -> Result<PathBuf, EndpointError> {
    let target = endpoint.to_string();
    let identity = zz_identity();
    let key_line = identity.as_ref().map(|(_, line)| line.as_str());
    let output = zz_key_first(identity.as_ref().map(|(path, _)| path.as_path()), |phase| {
        ssh_probe_command(endpoint, session, phase, key_line)
    })
    .map_err(|source| EndpointError::SshSpawn {
        operation: "remote socket probe",
        source,
    })?;
    if !output.status.success() {
        return Err(EndpointError::SshFailed {
            target,
            reason: ssh_failure_reason(
                &String::from_utf8_lossy(&output.stderr),
                &output.status.to_string(),
            ),
        });
    }

    let probe = parse_remote_probe_output(&target, &output.stdout);
    note_key_enrollment(endpoint, &probe);
    probe.socket
}

#[cfg(any(unix, windows))]
fn note_key_enrollment(endpoint: &SshEndpoint, probe: &RemoteProbe) {
    match probe.key {
        Some(KeyEnrollment::Added) => log::info!(
            target: "zz_daemon::diagnostics",
            "added this machine's zz key to {endpoint}'s authorized_keys; its own zz key is {}",
            probe.pubkey.as_deref().unwrap_or("unknown"),
        ),
        Some(KeyEnrollment::Failed) => log::warn!(
            target: "zz_daemon::diagnostics",
            "could not add this machine's zz key to {endpoint}'s authorized_keys",
        ),
        Some(KeyEnrollment::Present) | None => {}
    }
}

#[derive(Debug)]
pub(crate) struct RemoteProbe {
    pub(crate) socket: Result<PathBuf, EndpointError>,
    pub(crate) key: Option<KeyEnrollment>,
    pub(crate) pubkey: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyEnrollment {
    Added,
    Present,
    Failed,
}

// Login profiles may print arbitrary noise around the probe's output; only the
// sentinel-prefixed lines are ours.
pub(crate) fn probe_value<'a>(stdout: &'a str, prefix: &str) -> Option<&'a str> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .map(str::trim)
}

pub(crate) fn parse_remote_probe_output(target: &str, output: &[u8]) -> RemoteProbe {
    let Ok(stdout) = std::str::from_utf8(output) else {
        return RemoteProbe {
            socket: Err(EndpointError::ProbeFailure {
                target: target.to_owned(),
                reason: "probe output was not valid UTF-8".to_owned(),
            }),
            key: None,
            pubkey: None,
        };
    };
    let key = match probe_value(stdout, "zz-probe-key=") {
        Some("added") => Some(KeyEnrollment::Added),
        Some("present") => Some(KeyEnrollment::Present),
        Some(_) => Some(KeyEnrollment::Failed),
        None => None,
    };
    let pubkey = probe_value(stdout, "zz-probe-pubkey=")
        .filter(|line| crate::ssh_key::is_zz_key_line(line))
        .map(str::to_owned);
    RemoteProbe {
        socket: remote_probe_socket(target, stdout),
        key,
        pubkey,
    }
}

fn remote_probe_socket(target: &str, stdout: &str) -> Result<PathBuf, EndpointError> {
    let probe_value = |prefix: &str| probe_value(stdout, prefix);
    let path = probe_value("zz-probe-socket=").unwrap_or_default();
    if path.is_empty() || !path.starts_with('/') {
        return Err(EndpointError::ProbeFailure {
            target: target.to_owned(),
            reason: "probe output did not include an absolute socket path".to_owned(),
        });
    }
    match probe_value("zz-probe-protocol=") {
        // No zz on the remote PATH says nothing about a running daemon; dial anyway.
        // A daemon that must be auto-started still fails with the install-zz message.
        Some("missing") => {
            log::debug!("zz is not on the login-shell PATH of {target}; skipping version gate");
        }
        Some("unknown") => {
            return Err(EndpointError::RemoteProtocolUnknown {
                target: target.to_owned(),
            });
        }
        Some(protocol) => {
            let daemon = protocol
                .parse::<u16>()
                .map_err(|_| EndpointError::ProbeFailure {
                    target: target.to_owned(),
                    reason: "probe protocol version was neither a number nor `unknown`".to_owned(),
                })?;
            if daemon != zz_protocol::PROTOCOL_VERSION {
                return Err(EndpointError::RemoteProtocolMismatch {
                    target: target.to_owned(),
                    daemon,
                    client: zz_protocol::PROTOCOL_VERSION,
                });
            }
        }
        None => {
            return Err(EndpointError::ProbeFailure {
                target: target.to_owned(),
                reason: "probe output did not include a protocol line".to_owned(),
            });
        }
    }
    Ok(PathBuf::from(path))
}

#[cfg(any(unix, windows))]
fn ensure_remote_daemon(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    remote_socket: &Path,
) -> Result<(), EndpointError> {
    let target = endpoint.to_string();
    let output = ssh_daemon_start_command(endpoint, session, remote_socket)
        .output_unmasked()
        .map_err(|source| EndpointError::SshSpawn {
            operation: "remote daemon start",
            source,
        })?;
    match output.status.code() {
        Some(0) => Ok(()),
        Some(REMOTE_ZZ_MISSING_STATUS) => Err(EndpointError::RemoteBinaryMissing { target }),
        Some(REMOTE_DAEMON_TIMEOUT_STATUS) => {
            Err(EndpointError::RemoteDaemonUnavailable { target })
        }
        _ => Err(EndpointError::SshFailed {
            target,
            reason: ssh_failure_reason(
                &String::from_utf8_lossy(&output.stderr),
                &output.status.to_string(),
            ),
        }),
    }
}

#[cfg(any(unix, windows))]
fn start_askpass(endpoint: &SshEndpoint, prompts: Option<SshPrompts>) -> Option<AskpassListener> {
    prompts.and_then(|prompts| match AskpassListener::start(prompts) {
        Ok(listener) => Some(listener),
        Err(error) => {
            log::warn!(
                target: "zz_daemon::askpass",
                "ssh prompts are unavailable for {endpoint}: {error}",
            );
            None
        }
    })
}

#[cfg(any(unix, windows))]
fn session_askpass(askpass: Option<&AskpassListener>) -> Option<SshAskpass<'_>> {
    askpass.map(|listener| SshAskpass {
        helper: listener.helper(),
        socket: listener.socket(),
    })
}

#[cfg(unix)]
fn shared_control_path(endpoint: &SshEndpoint) -> Option<PathBuf> {
    match control_master_path(endpoint) {
        Ok(path) => {
            discard_dead_control_master(endpoint, &path);
            Some(path)
        }
        Err(error) => {
            log::warn!(
                target: "zz_daemon::diagnostics",
                "ssh connection sharing is unavailable for {endpoint}: {error}",
            );
            None
        }
    }
}

#[cfg(any(unix, windows))]
pub fn install_remote(
    endpoint: &SshEndpoint,
    prompts: Option<SshPrompts>,
    restart_daemon: bool,
) -> Result<(), EndpointError> {
    let target = endpoint.to_string();
    if zz_protocol::app_identity::DEVELOPMENT {
        return Err(EndpointError::RemoteInstallFailed {
            target,
            reason: format!(
                "{} looks for {} there, and no release ships it",
                zz_protocol::app_identity::DISPLAY_NAME,
                zz_protocol::app_identity::DIRECTORY,
            ),
        });
    }
    let askpass = start_askpass(endpoint, prompts);
    #[cfg(unix)]
    let control_path = shared_control_path(endpoint);
    let session = SshSession {
        #[cfg(unix)]
        control_path: control_path.as_deref(),
        askpass: session_askpass(askpass.as_ref()),
    };
    let spawn_failed = |source| EndpointError::SshSpawn {
        operation: "remote install",
        source,
    };
    let mut child = ssh_install_command(endpoint, session, restart_daemon)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_unmasked()
        .map_err(spawn_failed)?;
    if let Some(mut stdin) = child.stdin.take()
        && let Err(error) = stdin.write_all(INSTALL_SCRIPT.as_bytes())
    {
        log::debug!("the remote install for {endpoint} stopped reading its script: {error}");
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

#[cfg(unix)]
fn forwarded_local_socket_path() -> PathBuf {
    let counter = FORWARD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = format!("zz-fwd-{}-{counter}.sock", std::process::id());
    let temporary_directory = std::env::var("TMPDIR")
        .ok()
        .filter(|path| !path.is_empty())
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    let candidate = temporary_directory.join(&file_name);
    if candidate.as_os_str().as_encoded_bytes().len() < MAX_FORWARD_SOCKET_PATH_BYTES {
        candidate
    } else {
        PathBuf::from("/tmp").join(file_name)
    }
}

#[cfg(unix)]
pub(crate) struct SshForward {
    child: Child,
    local_socket: PathBuf,
    endpoint: SshEndpoint,
    control_path: Option<PathBuf>,
    forwarding: OsString,
    socks_port: Option<u16>,
    askpass: Option<AskpassListener>,
}

#[cfg(unix)]
impl SshForward {
    pub(crate) fn start(
        endpoint: &SshEndpoint,
        prompts: Option<SshPrompts>,
    ) -> Result<Self, EndpointError> {
        let askpass = start_askpass(endpoint, prompts);
        let control_path = shared_control_path(endpoint);
        let session = SshSession {
            control_path: control_path.as_deref(),
            askpass: session_askpass(askpass.as_ref()),
        };

        let probed_socket = probe_remote_socket(endpoint, session)?;
        let remote_socket = endpoint.remote_socket.clone().unwrap_or(probed_socket);
        if !remote_socket.is_absolute() {
            return Err(EndpointError::InvalidRemoteSocket(remote_socket));
        }
        ensure_remote_daemon(endpoint, session, &remote_socket)?;

        let local_socket = forwarded_local_socket_path();
        let socks_port = available_socks_port();
        let child =
            ssh_forward_command(endpoint, session, &local_socket, &remote_socket, socks_port)
                .spawn_unmasked()
                .map_err(|source| EndpointError::SshSpawn {
                    operation: "socket forward",
                    source,
                })?;
        let mut forward = Self {
            child,
            forwarding: forwarding_spec(&local_socket, &remote_socket),
            local_socket,
            endpoint: endpoint.clone(),
            control_path,
            socks_port,
            askpass,
        };
        if let Err(error) = forward.wait_for_socket(endpoint) {
            let Some(port) = forward.socks_port else {
                return Err(error);
            };
            log::debug!(
                target: "zz_daemon::diagnostics",
                "ssh forward to {endpoint} failed while binding -D {}; retrying on a fresh port: {error}",
                socks_spec(port),
            );
            forward.restart_with_fresh_socks_port(endpoint, &remote_socket)?;
        }

        let mut permissions = fs::metadata(&forward.local_socket)
            .map_err(|source| forward.io_error(endpoint, "reading permissions for", source))?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&forward.local_socket, permissions)
            .map_err(|source| forward.io_error(endpoint, "setting permissions on", source))?;
        Ok(forward)
    }

    pub(crate) fn local_socket(&self) -> &Path {
        &self.local_socket
    }

    pub(crate) const fn socks_port(&self) -> Option<u16> {
        self.socks_port
    }

    fn restart_with_fresh_socks_port(
        &mut self,
        endpoint: &SshEndpoint,
        remote_socket: &Path,
    ) -> Result<(), EndpointError> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.cancel_forwards();
        let _ = fs::remove_file(&self.local_socket);
        self.socks_port = available_socks_port();
        let mut command = ssh_forward_command(
            endpoint,
            SshSession {
                control_path: self.control_path.as_deref(),
                askpass: session_askpass(self.askpass.as_ref()),
            },
            &self.local_socket,
            remote_socket,
            self.socks_port,
        );
        self.child = command
            .spawn_unmasked()
            .map_err(|source| EndpointError::SshSpawn {
                operation: "socket forward",
                source,
            })?;
        self.wait_for_socket(endpoint)
    }

    fn cancel_forwards(&self) {
        let Some(control_path) = &self.control_path else {
            return;
        };
        if !control_path.exists() {
            return;
        }
        let cancel = |forwarding| {
            let _ = ssh_control_command(&self.endpoint, control_path, "cancel", Some(forwarding))
                .status_unmasked();
        };
        cancel(("-L", self.forwarding.as_os_str()));
        if let Some(port) = self.socks_port {
            let spec = socks_spec(port);
            cancel(("-D", OsStr::new(&spec)));
        }
    }

    fn wait_for_socket(&mut self, endpoint: &SshEndpoint) -> Result<(), EndpointError> {
        for _ in 0..FORWARD_POLL_ATTEMPTS {
            if self.socket_exists(endpoint)? {
                return Ok(());
            }
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|source| self.io_error(endpoint, "checking ssh child for", source))?
            {
                if self.socket_exists(endpoint)? {
                    return Ok(());
                }
                return Err(EndpointError::ForwardExited {
                    target: endpoint.to_string(),
                    status: status.to_string(),
                    local_socket: self.local_socket.clone(),
                });
            }
            thread::sleep(FORWARD_POLL_INTERVAL);
        }
        if self.socket_exists(endpoint)? {
            return Ok(());
        }
        Err(EndpointError::ForwardTimeout {
            target: endpoint.to_string(),
            local_socket: self.local_socket.clone(),
        })
    }

    fn socket_exists(&self, endpoint: &SshEndpoint) -> Result<bool, EndpointError> {
        match fs::metadata(&self.local_socket) {
            Ok(metadata) => Ok(metadata.file_type().is_socket()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(self.io_error(endpoint, "inspecting", source)),
        }
    }

    fn io_error(
        &self,
        endpoint: &SshEndpoint,
        action: &'static str,
        source: io::Error,
    ) -> EndpointError {
        EndpointError::ForwardIo {
            target: endpoint.to_string(),
            action,
            path: self.local_socket.clone(),
            source,
        }
    }
}

#[cfg(unix)]
impl Drop for SshForward {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.cancel_forwards();
        let _ = fs::remove_file(&self.local_socket);
    }
}

#[cfg(windows)]
const FORWARD_PIPE_ROLE: &str = "zz-fwd";

/// A pipe whose 128-bit random name is its only access control: Windows named pipes have no
/// `0600`, and a default descriptor grants read access to Everyone.
#[cfg(windows)]
pub(crate) fn ssh_pipe(role: &str, serial: u64) -> io::Result<(PathBuf, LocalListener)> {
    let mut entropy = [0_u8; 16];
    getrandom::fill(&mut entropy).map_err(io::Error::other)?;
    let mut suffix = String::with_capacity(entropy.len() * 2);
    for byte in entropy {
        write!(&mut suffix, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let path = PathBuf::from(format!(
        r"\\.\pipe\{role}-{}-{serial}-{suffix}",
        std::process::id()
    ));
    let listener = LocalTransport::bind(&path)?;
    listener.set_nonblocking(true)?;
    Ok((path, listener))
}

/// Polled rather than woken by a self-connect: `interprocess` dials with an unbounded wait, so
/// poking a pipe that is already serving a client would hang the teardown.
#[cfg(windows)]
pub(crate) fn accept_until_stopped(
    listener: &LocalListener,
    stopped: &AtomicBool,
) -> Option<LocalStream> {
    loop {
        if stopped.load(Ordering::SeqCst) {
            return None;
        }
        match listener.accept() {
            Ok(stream) => return Some(stream),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(error) => {
                log::debug!(target: "zz_daemon::diagnostics", "pipe listener stopped: {error}");
                return None;
            }
        }
    }
}

/// `Path::is_absolute` asks the local platform's question, which a Windows client would answer
/// wrongly about a remote POSIX path.
#[cfg(any(windows, target_os = "ios"))]
pub(crate) fn remote_socket_is_absolute(path: &Path) -> bool {
    path.as_os_str().as_encoded_bytes().starts_with(b"/")
}

#[cfg(windows)]
fn ssh_proxy_command(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    remote_socket: &Path,
) -> Command {
    let mut command = Command::new("ssh");
    append_ssh_options(&mut command, endpoint, session);
    command
        .arg("--")
        .arg(&endpoint.host)
        .arg("sh")
        .arg("-lc")
        .arg(shell_quote(&remote_proxy_script(remote_socket)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

// Drained off-thread because ssh blocks once its stderr pipe fills.
#[cfg(windows)]
struct SshStderr {
    text: Arc<Mutex<String>>,
    drain: Option<thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl SshStderr {
    fn drain(stderr: ChildStderr) -> Self {
        let text = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&text);
        let drain = thread::Builder::new()
            .name("zz-ssh-stderr".to_owned())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    log::debug!(target: "zz_daemon::diagnostics", "ssh: {line}");
                    let mut sink = sink.lock();
                    if sink.len() < MAX_SSH_STDERR_BYTES {
                        sink.push_str(&line);
                        sink.push('\n');
                    }
                }
            })
            .ok();
        Self { text, drain }
    }

    fn settled(&mut self) -> String {
        if let Some(drain) = self.drain.take() {
            let _ = drain.join();
        }
        self.text.lock().clone()
    }
}

// The Windows ssh client cannot bind a Unix-domain socket, so the remote runs `zz proxy` and this
// end bridges its stdio to a named pipe. A TCP `-L` forward would work but a Windows loopback port
// has no owner to check, where the unix arm chmods its forwarded socket 0600.
#[cfg(windows)]
pub(crate) struct SshForward {
    child: Child,
    local_socket: PathBuf,
    stopped: Arc<AtomicBool>,
    _askpass: Option<AskpassListener>,
}

#[cfg(windows)]
impl SshForward {
    pub(crate) fn start(
        endpoint: &SshEndpoint,
        prompts: Option<SshPrompts>,
    ) -> Result<Self, EndpointError> {
        let askpass = start_askpass(endpoint, prompts);
        let session = SshSession {
            askpass: session_askpass(askpass.as_ref()),
        };

        let probed_socket = probe_remote_socket(endpoint, session)?;
        let remote_socket = endpoint.remote_socket.clone().unwrap_or(probed_socket);
        if !remote_socket_is_absolute(&remote_socket) {
            return Err(EndpointError::InvalidRemoteSocket(remote_socket));
        }
        ensure_remote_daemon(endpoint, session, &remote_socket)?;

        let (local_socket, listener) = ssh_pipe(
            FORWARD_PIPE_ROLE,
            FORWARD_COUNTER.fetch_add(1, Ordering::Relaxed),
        )
        .map_err(|source| EndpointError::ForwardIo {
            target: endpoint.to_string(),
            action: "creating",
            path: PathBuf::from(FORWARD_PIPE_ROLE),
            source,
        })?;

        let mut child = ssh_proxy_command(endpoint, session, &remote_socket)
            .spawn_unmasked()
            .map_err(|source| EndpointError::SshSpawn {
                operation: "socket proxy",
                source,
            })?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            return Err(EndpointError::ForwardIo {
                target: endpoint.to_string(),
                action: "opening a pipe to ssh for",
                path: local_socket,
                source: io::Error::other("ssh was spawned without redirected stdio"),
            });
        };

        let mut stderr = SshStderr::drain(stderr);
        let mut stdout = BufReader::new(stdout);
        let stopped = Arc::new(AtomicBool::new(false));
        let started = await_proxy_ready(
            endpoint,
            &mut child,
            &mut stdout,
            &mut stderr,
            &local_socket,
        )
        .and_then(|()| {
            spawn_bridge(listener, stdin, stdout, Arc::clone(&stopped)).map_err(|source| {
                EndpointError::ForwardIo {
                    target: endpoint.to_string(),
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
            _askpass: askpass,
        })
    }

    pub(crate) fn local_socket(&self) -> &Path {
        &self.local_socket
    }

    // Always absent: `ssh -D` is an unauthenticated loopback listener, and a Windows loopback port
    // has no owner to check against.
    #[allow(
        clippy::unused_self,
        reason = "the unix arm answers from its reserved port; both are called as SshForward::socks_port"
    )]
    pub(crate) const fn socks_port(&self) -> Option<u16> {
        None
    }
}

#[cfg(windows)]
fn await_proxy_ready(
    endpoint: &SshEndpoint,
    child: &mut Child,
    stdout: &mut BufReader<ChildStdout>,
    stderr: &mut SshStderr,
    local_socket: &Path,
) -> Result<(), EndpointError> {
    let mut discarded = 0_usize;
    let mut line = Vec::new();
    loop {
        line.clear();
        let read =
            stdout
                .read_until(b'\n', &mut line)
                .map_err(|source| EndpointError::ForwardIo {
                    target: endpoint.to_string(),
                    action: "reading the proxy handshake for",
                    path: local_socket.to_owned(),
                    source,
                })?;
        if read == 0 {
            return Err(proxy_never_started(endpoint, child, stderr));
        }
        if line.ends_with(PROXY_READY_MARKER) {
            return Ok(());
        }
        log::debug!(
            target: "zz_daemon::diagnostics",
            "remote login preamble: {}",
            String::from_utf8_lossy(&line).trim_end(),
        );
        discarded = discarded.saturating_add(read);
        if discarded > MAX_PROXY_PREAMBLE_BYTES {
            return Err(EndpointError::SshFailed {
                target: endpoint.to_string(),
                reason: "the remote kept writing to stdout instead of starting the zz proxy; \
                         check what its login shell prints"
                    .to_owned(),
            });
        }
    }
}

#[cfg(windows)]
fn proxy_never_started(
    endpoint: &SshEndpoint,
    child: &mut Child,
    stderr: &mut SshStderr,
) -> EndpointError {
    let target = endpoint.to_string();
    let status = child.wait().ok();
    match status.and_then(|status| status.code()) {
        // `exec zz` in a POSIX shell exits 127 when there is no `zz` to exec.
        Some(REMOTE_ZZ_MISSING_STATUS) => EndpointError::RemoteBinaryMissing { target },
        _ => EndpointError::SshFailed {
            target,
            reason: ssh_failure_reason(
                &stderr.settled(),
                &status.map_or_else(|| "no exit status".to_owned(), |status| status.to_string()),
            ),
        },
    }
}

#[cfg(windows)]
fn spawn_bridge(
    listener: LocalListener,
    mut stdin: ChildStdin,
    mut stdout: BufReader<ChildStdout>,
    stopped: Arc<AtomicBool>,
) -> io::Result<()> {
    thread::Builder::new()
        .name("zz-ssh-bridge".to_owned())
        .spawn(move || {
            let Some(client) = accept_until_stopped(&listener, &stopped) else {
                return;
            };
            let Ok(mut to_ssh) = client.try_clone() else {
                return;
            };
            let mut from_ssh = client;
            if thread::Builder::new()
                .name("zz-ssh-bridge-up".to_owned())
                .spawn(move || drop(pump(&mut to_ssh, &mut stdin)))
                .is_err()
            {
                return;
            }
            drop(pump(&mut stdout, &mut from_ssh));
        })
        .map(drop)
}

#[cfg(windows)]
impl Drop for SshForward {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The remote half of the Windows attach path, run as `zz proxy` over ssh.
///
/// Connects to this host's daemon socket and pumps it to stdin and stdout.
pub fn run_socket_proxy(socket: &Path) -> io::Result<()> {
    let mut from_daemon = LocalTransport::connect(socket)?;
    let mut to_daemon = from_daemon.try_clone()?;

    let mut stdout = io::stdout().lock();
    stdout.write_all(PROXY_READY_MARKER)?;
    stdout.flush()?;

    std::thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        drop(pump(&mut stdin, &mut to_daemon));
    });
    pump(&mut from_daemon, &mut stdout)
}

// Flushes every chunk: the protocol is framed binary with no newlines, and stdout is a
// `LineWriter`.
fn pump(reader: &mut impl io::Read, writer: &mut impl io::Write) -> io::Result<()> {
    let mut buffer = [0_u8; PUMP_BUFFER_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        writer.write_all(&buffer[..read])?;
        writer.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_path() {
        assert_eq!(
            Endpoint::parse("/tmp/zz.sock").unwrap(),
            Endpoint::Local(PathBuf::from("/tmp/zz.sock"))
        );
    }

    #[test]
    fn parses_unix_uri() {
        assert_eq!(
            Endpoint::parse("unix:///tmp/zz.sock").unwrap(),
            Endpoint::Local(PathBuf::from("/tmp/zz.sock"))
        );
    }

    #[test]
    fn parses_ssh_host() {
        assert_eq!(
            Endpoint::parse("ssh://desktop").unwrap(),
            Endpoint::Ssh(SshEndpoint {
                user: None,
                host: "desktop".to_owned(),
                port: None,
                remote_socket: None,
            })
        );
    }

    #[test]
    fn parses_full_ssh_uri() {
        assert_eq!(
            Endpoint::parse("ssh://user@host:2222/run/user/1000/zz/default.sock").unwrap(),
            Endpoint::Ssh(SshEndpoint {
                user: Some("user".to_owned()),
                host: "host".to_owned(),
                port: Some(2222),
                remote_socket: Some(PathBuf::from("/run/user/1000/zz/default.sock")),
            })
        );
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(matches!(
            Endpoint::parse("tcp://host:7777"),
            Err(EndpointError::UriParse { .. })
        ));
    }

    #[test]
    fn rejects_removed_quic_scheme_with_an_ssh_pointer() {
        let error = Endpoint::parse("quic://gpu:7777").unwrap_err();
        assert!(matches!(error, EndpointError::UriParse { .. }));
        assert!(error.to_string().contains("use ssh://"));
    }

    #[test]
    fn rejects_empty_ssh_host() {
        assert!(matches!(
            Endpoint::parse("ssh:///tmp/default.sock"),
            Err(EndpointError::UriParse { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn builds_forward_ssh_argv_for_user_and_port_permutations() {
        for (user, port, options) in ssh_option_cases() {
            for socks_port in [None, Some(41_080)] {
                let endpoint = test_ssh_endpoint(user, port);
                let command = ssh_forward_command(
                    &endpoint,
                    test_session(),
                    Path::new("/tmp/local.sock"),
                    Path::new("/run/user/1000/zz/default.sock"),
                    socks_port,
                );
                let mut expected = vec![
                    "-N",
                    "-o",
                    "ExitOnForwardFailure=yes",
                    "-o",
                    "StreamLocalBindMask=0177",
                    "-L",
                    "/tmp/local.sock:/run/user/1000/zz/default.sock",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
                if let Some(socks_port) = socks_port {
                    expected.extend(["-D".to_owned(), format!("127.0.0.1:{socks_port}")]);
                }
                expected.extend(shared_ssh_options());
                expected.extend(options.iter().copied().map(str::to_owned));
                expected.extend(["--", "host"].into_iter().map(str::to_owned));
                assert_eq!(command_args(&command), expected);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn control_cancel_names_the_forward_kind_it_gives_back() {
        let endpoint = test_ssh_endpoint(None, None);
        let control_path = Path::new(TEST_CONTROL_PATH);
        for (flag, spec) in [
            ("-L", "/tmp/local.sock:/run/zz.sock"),
            ("-D", "127.0.0.1:41080"),
        ] {
            let command = ssh_control_command(
                &endpoint,
                control_path,
                "cancel",
                Some((flag, OsStr::new(spec))),
            );
            let arguments = command_args(&command);
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| pair == [flag.to_owned(), spec.to_owned()]),
                "{arguments:?}",
            );
        }
        let check = command_args(&ssh_control_command(&endpoint, control_path, "check", None));
        assert!(
            !check
                .iter()
                .any(|argument| argument == "-L" || argument == "-D"),
            "{check:?}",
        );
    }

    #[cfg(unix)]
    #[test]
    fn builds_probe_ssh_argv_for_user_and_port_permutations() {
        for (user, port, options) in ssh_option_cases() {
            let endpoint = test_ssh_endpoint(user, port);
            let command = ssh_probe_command(&endpoint, test_session(), None, None);
            let mut expected = shared_ssh_options();
            expected.extend(options.into_iter().map(str::to_owned));
            expected.extend(
                ["--", "host", "sh", "-lc", &remote_socket_probe(None)]
                    .into_iter()
                    .map(str::to_owned),
            );
            assert_eq!(command_args(&command), expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn builds_autostart_ssh_argv_for_user_and_port_permutations() {
        let remote_socket = Path::new("/run/user/1000/zz/default.sock");
        for (user, port, options) in ssh_option_cases() {
            let endpoint = test_ssh_endpoint(user, port);
            let command = ssh_daemon_start_command(&endpoint, test_session(), remote_socket);
            let mut expected = shared_ssh_options();
            expected.extend(options.into_iter().map(str::to_owned));
            expected.extend(["--", "host", "sh", "-lc"].into_iter().map(str::to_owned));
            expected.push(shell_quote(&remote_daemon_start_script(remote_socket)));
            assert_eq!(command_args(&command), expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn every_ssh_child_shares_one_multiplexing_master() {
        let endpoint = test_ssh_endpoint(Some("user"), Some(2222));
        let commands = [
            command_args(&ssh_probe_command(&endpoint, test_session(), None, None)),
            command_args(&ssh_daemon_start_command(
                &endpoint,
                test_session(),
                Path::new("/run/zz.sock"),
            )),
            command_args(&ssh_forward_command(
                &endpoint,
                test_session(),
                Path::new("/tmp/local.sock"),
                Path::new("/run/zz.sock"),
                Some(41_080),
            )),
        ];
        for arguments in &commands {
            assert!(
                arguments.contains(&"ControlMaster=auto".to_owned()),
                "{arguments:?}"
            );
            assert!(
                arguments.contains(&"ControlPath=/tmp/zz-ssh/c0".to_owned()),
                "{arguments:?}",
            );
            assert!(
                arguments.contains(&format!("ControlPersist={CONTROL_PERSIST_SECONDS}")),
                "{arguments:?}",
            );
            assert!(
                !arguments
                    .iter()
                    .any(|argument| argument.contains("BatchMode")),
                "{arguments:?}",
            );
        }
    }

    #[test]
    fn a_session_without_a_master_uses_platform_safe_options() {
        let endpoint = test_ssh_endpoint(None, None);
        let command = ssh_probe_command(&endpoint, SshSession::default(), None, None);
        let mut expected = [
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "Compression=yes",
        ]
        .map(str::to_owned)
        .to_vec();
        #[cfg(windows)]
        expected.extend(["-o", "ControlMaster=no", "-o", "ControlPath=none"].map(str::to_owned));
        expected.extend(["--", "host", "sh", "-lc", &remote_socket_probe(None)].map(str::to_owned));
        assert_eq!(command_args(&command), expected);
        assert_eq!(
            command_envs(&command),
            Vec::<(String, Option<String>)>::new()
        );
    }

    #[test]
    fn remote_probe_requires_a_matching_reported_protocol() {
        let output = format!(
            "zz-probe-socket=/run/user/1000/zz/default.sock\nzz-probe-protocol={}\n",
            zz_protocol::PROTOCOL_VERSION
        );
        assert_eq!(
            parse_remote_probe_output("host", output.as_bytes())
                .socket
                .unwrap(),
            Path::new("/run/user/1000/zz/default.sock")
        );

        assert!(matches!(
            parse_remote_probe_output(
                "host",
                b"zz-probe-socket=/run/zz.sock\nzz-probe-protocol=48\n"
            )
            .socket,
            Err(EndpointError::RemoteProtocolMismatch {
                target,
                daemon: 48,
                client: zz_protocol::PROTOCOL_VERSION,
            }) if target == "host"
        ));
        assert_eq!(
            parse_remote_probe_output(
                "host",
                b"zz-probe-socket=/run/zz.sock\nzz-probe-protocol=unknown\n"
            )
            .socket
            .unwrap_err()
            .to_string(),
            "zz on host predates protocol version reporting; update it, then reconnect"
        );
        assert!(matches!(
            parse_remote_probe_output(
                "host",
                b"zz-probe-socket=/run/zz.sock\nzz-probe-protocol=banana\n"
            )
            .socket,
            Err(EndpointError::ProbeFailure { .. })
        ));
    }

    #[test]
    fn remote_probe_tolerates_login_profile_noise_and_a_pathless_zz() {
        // A chatty ~/.profile prints around the sentinel lines under `sh -lc`.
        let noisy = format!(
            "Welcome to devbox!\nzz-probe-socket=/run/zz.sock\nmotd: 3 updates\nzz-probe-protocol={}\n",
            zz_protocol::PROTOCOL_VERSION
        );
        assert_eq!(
            parse_remote_probe_output("host", noisy.as_bytes())
                .socket
                .unwrap(),
            Path::new("/run/zz.sock")
        );

        // No zz on the login-shell PATH must not block dialing a running daemon.
        assert_eq!(
            parse_remote_probe_output(
                "host",
                b"zz-probe-socket=/run/zz.sock\nzz-probe-protocol=missing\n"
            )
            .socket
            .unwrap(),
            Path::new("/run/zz.sock")
        );

        assert!(matches!(
            parse_remote_probe_output("host", b"zz-probe-socket=/run/zz.sock\n").socket,
            Err(EndpointError::ProbeFailure { .. })
        ));
        assert!(matches!(
            parse_remote_probe_output("host", b"only noise, no sentinels\n").socket,
            Err(EndpointError::ProbeFailure { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn zz_key_phase_offers_only_the_zz_identity() {
        let endpoint = test_ssh_endpoint(Some("user"), None);
        let identity = Path::new("/Users/a/Library/Application Support/zz-100%/ssh/id_ed25519");
        let line = "ssh-ed25519 AAAAzz zz@desk";
        let mut expected = [
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "IdentityFile=\"/Users/a/Library/Application Support/zz-100%%/ssh/id_ed25519\"",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "BatchMode=yes",
        ]
        .map(str::to_owned)
        .to_vec();
        expected.extend(shared_ssh_options());
        expected.extend(["-l", "user", "--", "host", "sh", "-lc"].map(str::to_owned));
        expected.push(remote_socket_probe(Some(line)));
        assert_eq!(
            command_args(&ssh_probe_command(
                &endpoint,
                test_session(),
                Some(identity),
                Some(line)
            )),
            expected
        );

        let fallback = command_args(&ssh_probe_command(
            &endpoint,
            test_session(),
            None,
            Some(line),
        ));
        assert_eq!(fallback[..shared_ssh_options().len()], shared_ssh_options());
        for option in [
            "IdentitiesOnly",
            "IdentityFile",
            "PreferredAuthentications",
            "BatchMode",
        ] {
            assert!(
                !fallback.iter().any(|argument| argument.starts_with(option)),
                "the user's own login must keep OpenSSH's default identities: {fallback:?}"
            );
        }
    }

    #[test]
    fn only_a_refused_login_falls_back_to_the_users_own() {
        for stderr in [
            "user@host: Permission denied (publickey).",
            "Host key verification failed.",
            "sign_and_send_pubkey: signing failed for ED25519-SK \"/k\": device not found",
        ] {
            assert!(refused_login(Some(255), stderr.as_bytes()), "{stderr}");
        }
        for (code, stderr) in [
            (
                Some(255),
                "ssh: connect to host desk port 22: Operation timed out",
            ),
            (
                Some(255),
                "ssh: Could not resolve hostname desk: nodename nor servname",
            ),
            (
                Some(255),
                "ssh: connect to host desk port 22: Connection refused",
            ),
            (Some(1), "mkdir: Permission denied"),
            (None, "Permission denied"),
        ] {
            assert!(!refused_login(code, stderr.as_bytes()), "{code:?} {stderr}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_probe_enrolls_this_machine_once_and_reports_the_hosts_own_key() {
        let line = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGW3/OPNnD2l/h6LfRGUUDgkTB5X9f1CU1CuD1GC4d5u zz@hub";
        let shells = ["/bin/sh", "/bin/tcsh"]
            .into_iter()
            .filter(|shell| Path::new(shell).exists());
        for shell in shells {
            let home = tempfile::tempdir().expect("temporary home");
            let probe = || {
                let output = Command::new(shell)
                    .args(["-c", &format!("sh -c {}", remote_socket_probe(Some(line)))])
                    .env("HOME", home.path())
                    .env("PATH", "/usr/bin:/bin")
                    .env_remove("XDG_DATA_HOME")
                    .env_remove("XDG_RUNTIME_DIR")
                    .output()
                    .expect("run the probe");
                assert!(output.status.success(), "{shell}: {output:?}");
                parse_remote_probe_output("host", &output.stdout)
            };

            let first = probe();
            assert!(first.socket.is_ok(), "{shell}: {:?}", first.socket);
            assert_eq!(first.key, Some(KeyEnrollment::Added), "{shell}");
            let host_key = first.pubkey.expect("the probe makes the host's own key");
            assert!(host_key.starts_with("ssh-ed25519 "), "{host_key}");

            let second = probe();
            assert_eq!(second.key, Some(KeyEnrollment::Present), "{shell}");
            assert_eq!(second.pubkey.as_deref(), Some(host_key.as_str()));

            let ssh = home.path().join(".ssh");
            let authorized = ssh.join("authorized_keys");
            assert_eq!(
                fs::read_to_string(&authorized).unwrap(),
                format!("{line}\n")
            );
            assert_eq!(
                fs::metadata(&ssh).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&authorized).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let data = if cfg!(target_os = "macos") {
                home.path().join("Library/Application Support")
            } else {
                home.path().join(".local/share")
            };
            let key_directory = data.join(zz_protocol::app_identity::DIRECTORY).join("ssh");
            assert_eq!(
                fs::metadata(&key_directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(key_directory.join("id_ed25519"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_probe_without_a_key_line_writes_nothing() {
        let home = tempfile::tempdir().expect("temporary home");
        let output = Command::new("/bin/sh")
            .args(["-c", &format!("sh -c {}", remote_socket_probe(None))])
            .env("HOME", home.path())
            .env("PATH", "/usr/bin:/bin")
            .env_remove("XDG_DATA_HOME")
            .output()
            .expect("run the probe");
        let probe = parse_remote_probe_output("host", &output.stdout);
        assert!(probe.socket.is_ok());
        assert_eq!((probe.key, probe.pubkey), (None, None));
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_probe_reports_enrollment_and_only_a_well_formed_host_key() {
        let output = format!(
            "zz-probe-key=added\nzz-probe-pubkey=ssh-ed25519 AAAAzz zz@xps\n\
             zz-probe-socket=/run/zz.sock\nzz-probe-protocol={}\n",
            zz_protocol::PROTOCOL_VERSION
        );
        let probe = parse_remote_probe_output("host", output.as_bytes());
        assert_eq!(probe.key, Some(KeyEnrollment::Added));
        assert_eq!(probe.pubkey.as_deref(), Some("ssh-ed25519 AAAAzz zz@xps"));
        assert!(probe.socket.is_ok());

        let probe = parse_remote_probe_output(
            "host",
            b"zz-probe-key=failed\nzz-probe-pubkey=command=\"id\" ssh-ed25519 AAAAzz zz@xps\n\
              zz-probe-socket=/run/zz.sock\nzz-probe-protocol=48\n",
        );
        assert_eq!(probe.key, Some(KeyEnrollment::Failed));
        assert_eq!(probe.pubkey, None);
        assert!(matches!(
            probe.socket,
            Err(EndpointError::RemoteProtocolMismatch { daemon: 48, .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_percent_in_the_control_path_is_escaped_not_expanded() {
        assert_eq!(
            control_path_option(Path::new("/tmp/zz-100%-a/c1")),
            "ControlPath=/tmp/zz-100%%-a/c1",
        );
    }

    #[cfg(unix)]
    #[test]
    fn askpass_reaches_every_ssh_child_through_the_environment() {
        let endpoint = test_ssh_endpoint(None, None);
        let helper = PathBuf::from("/opt/zz/bin/zz");
        let socket = PathBuf::from("/tmp/zz-ssh/a1");
        let session = SshSession {
            control_path: None,
            askpass: Some(SshAskpass {
                helper: &helper,
                socket: &socket,
            }),
        };
        for command in [
            ssh_probe_command(&endpoint, session, None, None),
            ssh_daemon_start_command(&endpoint, session, Path::new("/run/zz.sock")),
            ssh_forward_command(
                &endpoint,
                session,
                Path::new("/tmp/local.sock"),
                Path::new("/run/zz.sock"),
                Some(41_080),
            ),
        ] {
            let envs = command_envs(&command);
            assert!(
                envs.contains(&("SSH_ASKPASS".to_owned(), Some("/opt/zz/bin/zz".to_owned()))),
                "{envs:?}",
            );
            assert!(
                envs.contains(&("SSH_ASKPASS_REQUIRE".to_owned(), Some("force".to_owned()))),
                "{envs:?}",
            );
            assert!(
                envs.contains(&(
                    "ZZ_ASKPASS_SOCKET".to_owned(),
                    Some("/tmp/zz-ssh/a1".to_owned()),
                )),
                "{envs:?}",
            );
        }
    }

    #[test]
    fn every_remote_script_looks_where_the_installer_puts_the_cli() {
        let scripts = [
            remote_socket_probe(None),
            remote_daemon_start_script(Path::new("/run/zz.sock")),
            remote_proxy_script(Path::new("/run/zz.sock")),
        ];
        for script in scripts {
            for dir in ["$HOME/.local/bin", "/opt/homebrew/bin", "/usr/local/bin"] {
                assert!(script.contains(dir), "{dir} missing from: {script}");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn remote_probe_and_proxy_select_the_builds_executable() {
        let directory = tempfile::tempdir().expect("temporary directory");
        for name in ["zz", "zz-dev"] {
            let binary = directory.path().join(name);
            fs::write(&binary, format!("#!/bin/sh\nprintf '%s\\n' '{name}'\n"))
                .expect("write executable");
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
                .expect("executable permissions");
        }
        let path = format!("{}:/usr/bin:/bin", directory.path().display());
        let name = zz_protocol::app_identity::DIRECTORY;
        for (script, expected) in [
            (
                remote_socket_probe(None),
                format!("zz-probe-protocol={name}\n"),
            ),
            (
                shell_quote(&remote_proxy_script(Path::new("/tmp/explicit.sock"))),
                format!("{name}\n"),
            ),
        ] {
            let output = Command::new("/bin/sh")
                .args(["-c", &format!("sh -c {script}")])
                .env("PATH", &path)
                .output()
                .expect("run remote script");
            assert!(output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).ends_with(&expected));
        }
    }

    #[cfg(unix)]
    fn fake_installer(protocol: &str) -> String {
        format!(
            "case \"$*\" in \"--headless --version {version}\") ;; \
             *) echo \"error: unexpected arguments $*\" >&2; exit 1 ;; esac\n\
             mkdir -p \"$HOME/.local/bin\"\n\
             rm -f \"$HOME/.local/bin/{name}\"\n\
             printf '#!/bin/sh\\nif [ \"$1\" = protocol-version ]; then echo {protocol}; \
             else echo \"$*\" >> \"$HOME/calls\"; fi\\n' > \"$HOME/.local/bin/{name}\"\n\
             chmod 755 \"$HOME/.local/bin/{name}\"\n",
            version = env!("CARGO_PKG_VERSION"),
            name = zz_protocol::app_identity::DIRECTORY,
        )
    }

    #[cfg(unix)]
    fn run_install_script(
        shell: &str,
        home: &Path,
        path: &str,
        installer: &str,
        restart: bool,
    ) -> Result<(), EndpointError> {
        let script = shell_quote(&remote_install_script(
            Some(Path::new("/run/zz.sock")),
            restart,
        ));
        let mut child = Command::new(shell)
            .args(["-c", &format!("sh -c {script}")])
            .env("HOME", home)
            .env("PATH", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn_unmasked()
            .expect("run install script");
        let _ = child
            .stdin
            .take()
            .expect("install stdin")
            .write_all(installer.as_bytes());
        let output = child.wait_with_output().expect("install output");
        parse_remote_install_output(
            "ssh://desk",
            output.status.code(),
            &output.stdout,
            &output.stderr,
            &output.status.to_string(),
        )
    }

    #[cfg(unix)]
    #[test]
    fn remote_install_replaces_its_own_binary_and_restarts_the_daemon() {
        let shells = ["/bin/sh", "/bin/tcsh"]
            .into_iter()
            .filter(|shell| Path::new(shell).exists());
        for shell in shells {
            let home = tempfile::tempdir().expect("temporary home");
            let bin = home.path().join(".local/bin");
            fs::create_dir_all(&bin).unwrap();
            let binary = bin.join(zz_protocol::app_identity::DIRECTORY);
            fs::write(&binary, "#!/bin/sh\necho 1\n").unwrap();
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
            let path = format!("{}:/usr/bin:/bin", bin.display());
            let installer = fake_installer(&zz_protocol::PROTOCOL_VERSION.to_string());

            run_install_script(shell, home.path(), &path, &installer, true)
                .unwrap_or_else(|error| panic!("{shell}: {error}"));
            assert_eq!(
                fs::read_to_string(home.path().join("calls")).unwrap(),
                "--socket /run/zz.sock kill-server\n",
                "{shell}",
            );

            fs::remove_file(home.path().join("calls")).unwrap();
            run_install_script(shell, home.path(), &path, &installer, false).unwrap();
            assert!(!home.path().join("calls").exists(), "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn remote_install_leaves_a_zz_it_did_not_install_alone() {
        let home = tempfile::tempdir().expect("temporary home");
        let other = tempfile::tempdir().expect("other prefix");
        let name = zz_protocol::app_identity::DIRECTORY;
        let foreign = other.path().join(name);
        fs::write(&foreign, "#!/bin/sh\necho 1\n").unwrap();
        fs::set_permissions(&foreign, fs::Permissions::from_mode(0o755)).unwrap();
        let installer = fake_installer(&zz_protocol::PROTOCOL_VERSION.to_string());

        let path = format!("{}:/usr/bin:/bin", other.path().display());
        let error = run_install_script("/bin/sh", home.path(), &path, &installer, true)
            .expect_err("a zz elsewhere on PATH is not ours to replace");
        let EndpointError::RemoteInstallElsewhere { path: found, .. } = error else {
            panic!("unexpected error: {error}");
        };
        assert_eq!(found, foreign.display().to_string());

        let bin = home.path().join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink(&foreign, bin.join(name)).unwrap();
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let error = run_install_script("/bin/sh", home.path(), &path, &installer, true)
            .expect_err("a link in ~/.local/bin points at someone else's install");
        assert!(
            matches!(error, EndpointError::RemoteInstallElsewhere { .. }),
            "{error}"
        );
        assert!(!home.path().join("calls").exists());
    }

    #[cfg(unix)]
    #[test]
    fn remote_install_reports_why_it_failed() {
        let home = tempfile::tempdir().expect("temporary home");
        let bin = home.path().join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        let binary = bin.join(zz_protocol::app_identity::DIRECTORY);
        fs::write(&binary, "#!/bin/sh\necho 1\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:/usr/bin:/bin", bin.display());

        let error = run_install_script(
            "/bin/sh",
            home.path(),
            &path,
            "echo 'error: curl is missing' >&2; exit 1\n",
            true,
        )
        .expect_err("the installer failed");
        let EndpointError::RemoteInstallFailed { reason, .. } = error else {
            panic!("unexpected error: {error}");
        };
        assert_eq!(reason, "curl is missing");

        let broken = format!(
            "printf '#!/bin/sh\\necho \"GLIBC_2.39 not found\" >&2; exit 1\\n' > \"{}\"\n",
            binary.display()
        );
        let error = run_install_script("/bin/sh", home.path(), &path, &broken, true)
            .expect_err("the installed binary cannot run");
        let EndpointError::RemoteInstallFailed { reason, .. } = error else {
            panic!("unexpected error: {error}");
        };
        assert!(reason.ends_with("GLIBC_2.39 not found"), "{reason}");

        let error = run_install_script("/bin/sh", home.path(), &path, &fake_installer("1"), true)
            .expect_err("the release speaks another protocol");
        assert!(
            matches!(
                error,
                EndpointError::RemoteInstallMismatch { remote: 1, .. }
            ),
            "{error}"
        );
        assert!(!home.path().join("calls").exists());
    }

    #[cfg(unix)]
    #[test]
    fn authorized_key_script_appends_once_on_its_own_line() {
        let home = tempfile::tempdir().expect("temporary home");
        let file = home.path().join(".ssh/authorized_keys");
        let run = || {
            let script =
                authorized_key_script("ssh-ed25519 AAAAzz", "ssh-ed25519 AAAAzz zz-iphone");
            let status = Command::new("/bin/sh")
                .args(["-c", &format!("sh -c {}", shell_quote(&script))])
                .env("HOME", home.path())
                .status()
                .expect("run key script");
            assert!(status.success());
        };
        run();
        assert_eq!(
            fs::metadata(home.path().join(".ssh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "ssh-ed25519 AAAAzz zz-iphone\n"
        );
        run();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "ssh-ed25519 AAAAzz zz-iphone\n"
        );

        fs::write(&file, "ssh-rsa AAAAother laptop").unwrap();
        run();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "ssh-rsa AAAAother laptop\nssh-ed25519 AAAAzz zz-iphone\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn remote_scripts_fall_back_to_the_mac_app_bundle_cli() {
        let name = zz_protocol::app_identity::DIRECTORY;
        let app = format!("{}.app", zz_protocol::app_identity::DISPLAY_NAME);
        if ["/opt/homebrew/bin", "/usr/local/bin"]
            .iter()
            .any(|dir| Path::new(dir).join(name).exists())
            || Path::new("/Applications").join(&app).exists()
        {
            return;
        }
        let home = tempfile::tempdir().expect("temporary home");
        let macos = home
            .path()
            .join("Applications")
            .join(&app)
            .join("Contents/MacOS");
        fs::create_dir_all(&macos).expect("app bundle");
        let cli = macos.join("cli");
        fs::write(&cli, "#!/bin/sh\nprintf 'bundle %s\\n' \"$1\"\n").expect("write cli");
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755))
            .expect("executable permissions");
        let tools = home.path().join("bin");
        fs::create_dir(&tools).expect("tool directory");
        for tool in ["sh", "uname", "getconf"] {
            if let Some(path) = ["/usr/bin", "/bin"]
                .iter()
                .map(|dir| Path::new(dir).join(tool))
                .find(|path| path.exists())
            {
                std::os::unix::fs::symlink(path, tools.join(tool)).expect("tool link");
            }
        }
        for (script, expected) in [
            (
                remote_socket_probe(None),
                "zz-probe-protocol=bundle protocol-version\n",
            ),
            (
                shell_quote(&remote_proxy_script(Path::new("/tmp/explicit.sock"))),
                "bundle proxy\n",
            ),
        ] {
            let output = Command::new("/bin/sh")
                .args(["-c", &format!("sh -c {script}")])
                .env("PATH", &tools)
                .env("HOME", home.path())
                .output()
                .expect("run remote script");
            assert!(output.status.success());
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.ends_with(expected), "{script}\n{stdout}");
        }
    }

    #[test]
    fn autostart_script_starts_the_daemon_on_the_resolved_socket() {
        let script = remote_daemon_start_script(Path::new("/run/user/1000/zz/default.sock"));
        assert!(
            script.starts_with(&format!(
                "{}command -v \"$zz_cli\" >/dev/null 2>&1 || exit 127;",
                remote_path_fallback(),
            )),
            "missing zz needs its own exit status before anything but the PATH line runs: {script}"
        );
        assert!(
            script.contains(
                "setsid \"$zz_cli\" daemon --socket '/run/user/1000/zz/default.sock' >/dev/null \
                 2>&1 </dev/null &"
            ) && script.contains(
                "nohup \"$zz_cli\" daemon --socket '/run/user/1000/zz/default.sock' >/dev/null \
                 2>&1 </dev/null &"
            ),
            "the daemon must be detached and pinned to the resolved socket by either arm: \
             {script}"
        );
        assert!(
            script.ends_with("exit 3"),
            "a daemon that never listens needs its own exit status: {script}"
        );
    }

    #[test]
    fn ssh_hosts_may_not_open_with_a_dash() {
        for input in [
            "ssh://-oProxyCommand=id",
            "ssh://user@-oProxyCommand=id",
            "ssh://-",
        ] {
            let error = Endpoint::parse(input).expect_err(input);
            assert!(
                error.to_string().contains("must not start with `-`"),
                "{input}: {error}"
            );
        }
    }

    #[test]
    fn autostart_script_starts_the_daemon_without_trusting_an_existing_socket_file() {
        let script = remote_daemon_start_script(Path::new("/run/user/1000/zz/default.sock"));
        let start = script
            .find("\"$zz_cli\" daemon")
            .expect("the script must start the daemon");
        assert!(
            !script[..start].contains("[ -S "),
            "nothing may short-circuit the start on the socket file alone: {script}"
        );
    }

    #[test]
    fn autostart_script_stays_on_one_line_for_csh_logins() {
        let script = remote_daemon_start_script(Path::new("/run/user/1000/zz/default.sock"));
        assert!(!script.contains('\n'), "{script}");
    }

    #[test]
    fn autostart_script_quotes_a_remote_socket_path_containing_a_quote() {
        assert_eq!(shell_quote("o'brien"), "'o'\\''brien'");
        let script = remote_daemon_start_script(Path::new("/tmp/zz-o'brien/default.sock"));
        assert!(
            script.contains("[ -S '/tmp/zz-o'\\''brien/default.sock' ]"),
            "{script}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn shell_quoting_survives_the_login_shell_and_the_inner_sh() {
        let path = "/tmp/zz-o'brien $HOME `hostname`/default.sock";
        let script = format!("printf %s {}", shell_quote(path));
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("sh -c {}", shell_quote(&script)))
            .output()
            .expect("sh should run");
        assert_eq!(String::from_utf8_lossy(&output.stdout), path);
    }

    #[test]
    fn classifies_ssh_stderr_into_actionable_hints() {
        for (stderr, expected) in [
            (
                "user@host: Permission denied (publickey).",
                "rejected the login",
            ),
            ("Host key verification failed.", "known_hosts"),
            (
                "ssh: Could not resolve hostname desk: Name or service not known",
                "did not resolve",
            ),
            (
                "ssh: connect to host desk port 22: Connection refused",
                "Remote Login",
            ),
            (
                "ssh: connect to host desk port 22: Operation timed out",
                "timed out",
            ),
            (
                "ssh: connect to host desk port 22: No route to host",
                "not reachable",
            ),
            (
                "sign_and_send_pubkey: signing failed for ED25519-SK \"/Users/f/.ssh/id_ed25519_sk\": \
                 device not found\nuser@host: Permission denied (publickey).",
                "the security key was not touched",
            ),
            (
                "sign_and_send_pubkey: signing failed for ECDSA-SK \"/k\" from agent: invalid format",
                "the security key was not touched",
            ),
        ] {
            let reason = ssh_failure_reason(stderr, "exit status: 255");
            assert!(
                reason.contains(expected),
                "{stderr:?} should mention {expected:?}, got {reason:?}"
            );
        }
    }

    #[test]
    fn unclassified_ssh_stderr_falls_back_to_its_last_line_then_the_status() {
        assert_eq!(
            ssh_failure_reason(
                "Warning: Permanently added 'desk' to the list of known hosts.\nbanner exchange \
                 failed\n",
                "exit status: 255",
            ),
            "banner exchange failed"
        );
        assert_eq!(
            ssh_failure_reason("  \n", "exit status: 255"),
            "ssh exited with exit status: 255"
        );
    }

    #[test]
    fn ssh_failures_read_as_advice_and_parse_errors_stay_out_of_the_host_row() {
        let reason = |error: EndpointError| error.ssh_reason().expect("ssh reason");
        assert!(
            reason(EndpointError::RemoteBinaryMissing {
                target: "ssh://desk".to_owned(),
            })
            .starts_with(&format!(
                "{} is not installed on ssh://desk",
                zz_protocol::app_identity::DIRECTORY
            ))
        );
        assert!(
            reason(EndpointError::RemoteDaemonUnavailable {
                target: "ssh://desk".to_owned(),
            })
            .contains("never appeared")
        );
        assert_eq!(
            reason(EndpointError::SshFailed {
                target: "ssh://desk".to_owned(),
                reason: "the ssh connection timed out".to_owned(),
            }),
            "Could not reach ssh://desk over ssh: the ssh connection timed out"
        );
        assert!(
            reason(EndpointError::ForwardTimeout {
                target: "ssh://desk".to_owned(),
                local_socket: PathBuf::from("/tmp/zz-fwd-1-1.sock"),
            })
            .contains("forwarded socket"),
        );
        assert!(parse_error("nope://x", "bad").ssh_reason().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn forwarded_socket_path_stays_short() {
        let path = forwarded_local_socket_path();
        assert!(path.as_os_str().as_encoded_bytes().len() < 100);
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("zz-fwd-")
        );
    }

    #[cfg(unix)]
    const TEST_CONTROL_PATH: &str = "/tmp/zz-ssh/c0";

    #[cfg(unix)]
    fn test_session() -> SshSession<'static> {
        SshSession {
            control_path: Some(Path::new(TEST_CONTROL_PATH)),
            askpass: None,
        }
    }

    #[cfg(unix)]
    fn shared_ssh_options() -> Vec<String> {
        vec![
            "-o".to_owned(),
            "ConnectTimeout=10".to_owned(),
            "-o".to_owned(),
            "ServerAliveInterval=5".to_owned(),
            "-o".to_owned(),
            "ServerAliveCountMax=3".to_owned(),
            "-o".to_owned(),
            "Compression=yes".to_owned(),
            "-o".to_owned(),
            "ControlMaster=auto".to_owned(),
            "-o".to_owned(),
            format!("ControlPath={TEST_CONTROL_PATH}"),
            "-o".to_owned(),
            format!("ControlPersist={CONTROL_PERSIST_SECONDS}"),
        ]
    }

    fn ssh_option_cases() -> Vec<(Option<&'static str>, Option<u16>, Vec<&'static str>)> {
        vec![
            (None, None, vec![]),
            (Some("user"), None, vec!["-l", "user"]),
            (None, Some(2222), vec!["-p", "2222"]),
            (Some("user"), Some(2222), vec!["-p", "2222", "-l", "user"]),
        ]
    }

    fn test_ssh_endpoint(user: Option<&str>, port: Option<u16>) -> SshEndpoint {
        SshEndpoint {
            user: user.map(str::to_owned),
            host: "host".to_owned(),
            port,
            remote_socket: None,
        }
    }

    fn command_args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[cfg(unix)]
    fn probe_socket_path(environment: &[(&str, Option<&str>)]) -> String {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(format!("sh -c {}", remote_socket_probe(None)));
        for (key, value) in environment {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
        let output = command.output().expect("sh should run the probe");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("zz-probe-socket="))
            .map(str::to_owned)
            .expect("the probe should print a socket line")
    }

    #[cfg(unix)]
    #[test]
    fn remote_socket_probe_prefers_xdg_runtime_dir_then_tmpdir() {
        for (environment, expected) in [
            (
                [
                    ("XDG_RUNTIME_DIR", Some("/run/user/1000")),
                    ("TMPDIR", Some("/launchd/tmp")),
                    ("USER", Some("ada")),
                ],
                "/run/user/1000/zz/default.sock",
            ),
            (
                [
                    ("XDG_RUNTIME_DIR", None),
                    ("TMPDIR", Some("/launchd/tmp/")),
                    ("USER", Some("ada")),
                ],
                "/launchd/tmp/zz-ada/default.sock",
            ),
            (
                [
                    ("XDG_RUNTIME_DIR", None),
                    ("TMPDIR", Some("relative")),
                    ("USER", Some("ada")),
                ],
                "/tmp/zz-ada/default.sock",
            ),
        ] {
            assert_eq!(
                probe_socket_path(&environment),
                expected.replace("/zz", &format!("/{}", zz_protocol::app_identity::DIRECTORY))
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn remote_socket_probe_resolves_the_launchd_temporary_directory() {
        let expected_root = Command::new("getconf")
            .arg("DARWIN_USER_TEMP_DIR")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|root| root.starts_with('/'))
            .map_or_else(
                || "/tmp".to_owned(),
                |root| root.trim_end_matches('/').to_owned(),
            );
        assert!(
            cfg!(not(target_os = "macos")) || expected_root != "/tmp",
            "{expected_root}"
        );
        assert_eq!(
            probe_socket_path(&[
                ("XDG_RUNTIME_DIR", None),
                ("TMPDIR", None),
                ("USER", Some("ada")),
            ]),
            format!(
                "{expected_root}/{}-ada/default.sock",
                zz_protocol::app_identity::DIRECTORY
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn remote_socket_probe_prefers_existing_macos_desktop_socket() {
        let name = zz_protocol::app_identity::DIRECTORY;
        let desktop = tempfile::Builder::new()
            .prefix(&format!("{name}-"))
            .tempdir_in("/tmp")
            .expect("desktop directory");
        let desktop_name = desktop.path().file_name().unwrap().to_str().unwrap();
        let user = desktop_name.strip_prefix(&format!("{name}-")).unwrap();
        let desktop_socket = desktop.path().join("default.sock");
        let listener =
            std::os::unix::net::UnixListener::bind(&desktop_socket).expect("desktop socket");
        let temporary = tempfile::tempdir_in("/tmp").expect("SSH temporary directory");
        let ssh_directory = temporary.path().join(desktop_name);
        fs::create_dir(&ssh_directory).expect("SSH socket directory");
        let ssh_socket = ssh_directory.join("default.sock");
        let _ssh_listener =
            std::os::unix::net::UnixListener::bind(&ssh_socket).expect("SSH socket");
        let uname = temporary.path().join("uname");
        fs::write(&uname, "#!/bin/sh\nprintf 'Darwin\\n'\n").expect("uname stub");
        fs::set_permissions(&uname, fs::Permissions::from_mode(0o755)).expect("stub permissions");
        let path = format!(
            "{}:{}",
            temporary.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut environment = [
            ("XDG_RUNTIME_DIR", None),
            ("TMPDIR", temporary.path().to_str()),
            ("USER", Some(user)),
            ("PATH", Some(path.as_str())),
        ];
        assert_eq!(
            probe_socket_path(&environment),
            desktop_socket.to_str().unwrap()
        );
        environment[1].1 = None;
        assert_eq!(
            probe_socket_path(&environment),
            desktop_socket.to_str().unwrap()
        );
        environment[1].1 = temporary.path().to_str();
        environment[0].1 = Some("/run/zz-test-runtime");
        assert_eq!(
            probe_socket_path(&environment),
            format!("/run/zz-test-runtime/{name}/default.sock")
        );
        environment[0].1 = None;
        fs::write(&uname, "#!/bin/sh\nprintf 'Linux\\n'\n").expect("Linux uname stub");
        assert_eq!(
            probe_socket_path(&environment),
            ssh_socket.to_str().unwrap()
        );
        fs::write(&uname, "#!/bin/sh\nprintf 'Darwin\\n'\n").expect("Darwin uname stub");
        drop(listener);
        fs::remove_file(desktop_socket).expect("remove desktop socket");
        assert_eq!(
            probe_socket_path(&environment),
            ssh_socket.to_str().unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn remote_socket_probe_falls_back_to_slash_tmp_when_getconf_rejects_the_variable() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let stub = directory.path().join("getconf");
        fs::write(&stub, "#!/bin/sh\necho unsupported >&2\nexit 1\n").expect("getconf stub");
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("stub permissions");
        let path = format!(
            "{}:{}",
            directory.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        assert_eq!(
            probe_socket_path(&[
                ("XDG_RUNTIME_DIR", None),
                ("TMPDIR", None),
                ("USER", Some("ada")),
                ("PATH", Some(&path)),
            ]),
            format!(
                "/tmp/{}-ada/default.sock",
                zz_protocol::app_identity::DIRECTORY
            )
        );
    }

    fn command_envs(command: &Command) -> Vec<(String, Option<String>)> {
        command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }
}
