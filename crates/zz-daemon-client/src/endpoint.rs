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
use crate::askpass::{AskpassListener, AskpassMode, AskpassPrompt, SshPrompts};
#[cfg(any(unix, windows))]
use crate::transport::LocalStream;
#[cfg(windows)]
use crate::transport::{LocalListener, TransportListener as _};
use crate::transport::{LocalTransport, Transport as _, TransportStream as _};
#[cfg(any(unix, windows, test))]
use crate::unmasked::SpawnUnmasked as _;

#[cfg(any(unix, windows, test))]
pub(crate) fn remote_path_fallback() -> String {
    format!(
        "PATH=\"$PATH:$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin\"; export PATH; \
         zz_managed=\"$HOME/.local/bin/{name}\"; zz_cli={name}; \
         if [ -x \"$zz_managed\" ]; then zz_cli=\"$zz_managed\"; \
         elif ! command -v {name} >/dev/null 2>&1; then \
         for zz_apps in /Applications \"$HOME/Applications\"; do \
         if [ -x \"$zz_apps/{app}.app/Contents/MacOS/cli\" ]; \
         then zz_cli=\"$zz_apps/{app}.app/Contents/MacOS/cli\"; break; fi; done; fi; ",
        name = zz_protocol::app_identity::DIRECTORY,
        app = zz_protocol::app_identity::DISPLAY_NAME,
    )
}

#[cfg(any(unix, windows, test))]
pub(crate) fn remote_socket_probe(key_line: Option<&str>) -> String {
    shell_quote(&remote_socket_probe_script(key_line))
}

#[cfg(any(unix, windows, test))]
pub(crate) fn remote_socket_probe_script(key_line: Option<&str>) -> String {
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
pub(crate) const INSTALL_SCRIPT: &str = include_str!("../../../install.sh");
#[cfg(any(unix, windows, test))]
const REMOTE_DAEMON_POLL_ATTEMPTS: u32 = 50;
#[cfg(any(unix, windows, test))]
const REMOTE_DAEMON_POLL_INTERVAL: &str = "0.1";
#[cfg(any(unix, windows, test))]
const REMOTE_DAEMON_POLL_FALLBACK_ATTEMPTS: u32 = 5;
#[cfg(unix)]
const FORWARD_POLL_ATTEMPTS: usize = 250;
#[cfg(unix)]
const FORWARD_POLL_INTERVAL: Duration = Duration::from_millis(20);
#[cfg(unix)]
const CONTROL_PERSIST_SECONDS: u32 = 600;
#[cfg(unix)]
const LEGACY_SSH_DIRECTORY_AGE: Duration = Duration::from_mins(10);
#[cfg(unix)]
const MAX_SSH_RUNTIME_ROOT_BYTES: usize = 60;
#[cfg(unix)]
static FORWARD_COUNTER: AtomicU64 = AtomicU64::new(1);
#[cfg(windows)]
pub(crate) static FORWARD_COUNTER: AtomicU64 = AtomicU64::new(1);
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
    Tailnet(TailnetEndpoint),
    Wsl(WslEndpoint),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslEndpoint {
    pub distro: String,
    pub user: Option<String>,
    pub remote_socket: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailnetEndpoint {
    pub peer: String,
    pub port: Option<u16>,
}

impl TailnetEndpoint {
    pub const DEFAULT_PORT: u16 = if zz_protocol::app_identity::DEVELOPMENT {
        9923
    } else {
        9922
    };
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
    #[error("{reason}")]
    Tailnet { target: String, reason: String },
    #[error("failed to spawn wsl.exe for {operation}: {source}")]
    WslSpawn {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("wsl for {target} failed: {reason}")]
    WslFailed { target: String, reason: String },
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
    #[error("zz on {target} is {path}, a link to {link} that zz did not install")]
    RemoteInstallElsewhere {
        target: String,
        path: String,
        link: String,
    },
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
        "{target} stopped answering for {seconds} s; the command may have run there, check before retrying"
    )]
    StoppedAnswering { target: String, seconds: u64 },
    #[error(
        "ssh remote socket path must be absolute: {path}",
        path = .0.display()
    )]
    InvalidRemoteSocket(PathBuf),
    #[error("ssh endpoints are unsupported on this platform")]
    UnsupportedPlatform,
}

impl EndpointError {
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

    #[must_use]
    pub fn ssh_reason(&self) -> Option<String> {
        Some(match self {
            Self::SshSpawn { operation, source } => {
                format!("Could not run ssh for the {operation}: {source}")
            }
            Self::ProbeFailure { target, reason } | Self::SshFailed { target, reason } => {
                format!("Could not reach {target} over ssh: {reason}")
            }
            Self::Tailnet { target, reason } => {
                format!("Could not reach {target} over the tailnet: {reason}")
            }
            Self::WslSpawn { operation, source } => {
                format!("Could not run wsl.exe for the {operation}: {source}")
            }
            Self::WslFailed { target, reason } => {
                format!("Could not reach {target} in WSL: {reason}")
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
            Self::RemoteInstallElsewhere { target, path, link } => format!(
                "{path} on {target} links to {link}, which {name} did not install.\nRemove the \
                 link or update that install, then reconnect.",
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
            Self::StoppedAnswering { .. } => self.to_string(),
            Self::InvalidRemoteSocket(path) => format!(
                "The remote socket path must be absolute: {path}",
                path = path.display()
            ),
            Self::UnsupportedPlatform => "ssh hosts are not supported on this platform.".to_owned(),
            Self::UriParse { .. } => return None,
        })
    }

    #[must_use]
    pub fn needs_sign_in(&self) -> bool {
        match self {
            Self::AuthenticationFailed { .. }
            | Self::HostKeyRejected { .. }
            | Self::SignInExpired { .. } => true,
            #[cfg(any(unix, windows, test))]
            Self::ProbeFailure { reason, .. } | Self::SshFailed { reason, .. } => [
                SshFailure::SecurityKey,
                SshFailure::Login,
                SshFailure::HostKey,
            ]
            .into_iter()
            .any(|failure| reason == failure.hint()),
            _ => false,
        }
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
        if let Some(value) = input.strip_prefix("tailnet://") {
            return parse_tailnet_endpoint(input, value).map(Self::Tailnet);
        }
        if let Some(value) = input.strip_prefix("wsl://") {
            return parse_wsl_endpoint(input, value).map(Self::Wsl);
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

impl Endpoint {
    #[must_use]
    pub fn same_machine(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Tailnet(left), Self::Tailnet(right)) => {
                machine_label(&left.peer) == machine_label(&right.peer)
                    && left.port.unwrap_or(TailnetEndpoint::DEFAULT_PORT)
                        == right.port.unwrap_or(TailnetEndpoint::DEFAULT_PORT)
            }
            (Self::Tailnet(tailnet), Self::Ssh(ssh)) | (Self::Ssh(ssh), Self::Tailnet(tailnet)) => {
                machine_label(&tailnet.peer) == machine_label(&ssh.host)
            }
            (Self::Wsl(left), Self::Wsl(right)) => left.distro.eq_ignore_ascii_case(&right.distro),
            _ => self == other,
        }
    }

    #[must_use]
    pub fn is_address(&self, address: std::net::IpAddr) -> bool {
        let host = match self {
            Self::Ssh(endpoint) => &endpoint.host,
            Self::Tailnet(endpoint) => &endpoint.peer,
            Self::Local(_) | Self::Wsl(_) => return false,
        };
        host.parse() == Ok(address)
    }
}

fn machine_label(host: &str) -> String {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_owned();
    }
    host.split('.').next().unwrap_or(host).to_ascii_lowercase()
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
            Self::Tailnet(endpoint) => endpoint.fmt(formatter),
            Self::Wsl(endpoint) => endpoint.fmt(formatter),
        }
    }
}

impl fmt::Display for WslEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("wsl://")?;
        if let Some(user) = &self.user {
            write!(formatter, "{user}@")?;
        }
        formatter.write_str(&self.distro)?;
        if let Some(remote_socket) = &self.remote_socket {
            remote_socket.display().fmt(formatter)?;
        }
        Ok(())
    }
}

impl fmt::Display for TailnetEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("tailnet://")?;
        if self.peer.contains(':') {
            write!(formatter, "[{}]", self.peer)?;
        } else {
            formatter.write_str(&self.peer)?;
        }
        if let Some(port) = self.port {
            write!(formatter, ":{port}")?;
        }
        Ok(())
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

fn parse_wsl_endpoint(input: &str, value: &str) -> Result<WslEndpoint, EndpointError> {
    let (authority, remote_socket) = match value.split_once('/') {
        Some((authority, path)) => (authority, Some(PathBuf::from(format!("/{path}")))),
        None => (value, None),
    };
    if authority.is_empty() {
        return Err(parse_error(input, "wsl distro is empty"));
    }
    let (user, distro) = match authority.split_once('@') {
        Some((user, distro)) => {
            if user.is_empty() {
                return Err(parse_error(input, "wsl user is empty"));
            }
            if distro.contains('@') {
                return Err(parse_error(
                    input,
                    "wsl authority contains multiple `@` signs",
                ));
            }
            if user
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
            {
                return Err(parse_error(input, "wsl user contains invalid characters"));
            }
            (Some(user.to_owned()), distro)
        }
        None => (None, authority),
    };
    if distro.is_empty() {
        return Err(parse_error(input, "wsl distro is empty"));
    }
    if distro.starts_with('-') {
        return Err(parse_error(input, "wsl distro must not start with `-`"));
    }
    if distro.chars().any(|character| {
        character.is_whitespace() || character.is_control() || matches!(character, '\\' | ':')
    }) {
        return Err(parse_error(input, "wsl distro contains invalid characters"));
    }
    Ok(WslEndpoint {
        distro: distro.to_owned(),
        user,
        remote_socket,
    })
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

fn parse_tailnet_endpoint(input: &str, value: &str) -> Result<TailnetEndpoint, EndpointError> {
    let value = value.strip_suffix('/').unwrap_or(value);
    let (peer, port) = if let Some(rest) = value.strip_prefix('[') {
        let (address, rest) = rest
            .split_once(']')
            .ok_or_else(|| parse_error(input, "an IPv6 address needs a closing `]`"))?;
        if address.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(parse_error(input, "brackets hold an IPv6 address"));
        }
        let port = match rest {
            "" => None,
            rest => Some(
                rest.strip_prefix(':')
                    .ok_or_else(|| parse_error(input, "expected `:port` after `]`"))?,
            ),
        };
        (address, port)
    } else {
        match value.split_once(':') {
            Some((peer, port)) => (peer, Some(port)),
            None => (value, None),
        }
    };
    let port = port
        .map(|port| {
            port.parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| parse_error(input, "port must be a number from 1 to 65535"))
        })
        .transpose()?;
    if peer.is_empty() {
        return Err(parse_error(input, "name a tailnet machine or address"));
    }
    if !peer.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '_' | ':')
    }) {
        return Err(parse_error(
            input,
            "a tailnet machine name uses letters, digits, `-`, `_` and `.`",
        ));
    }
    Ok(TailnetEndpoint {
        peer: peer.to_owned(),
        port,
    })
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
    control: Option<&'a SshControl>,
    askpass: Option<SshAskpass<'a>>,
    identity: Option<&'a Path>,
    batch: bool,
}

#[cfg(not(windows))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct SshControl {
    path: PathBuf,
    user_persist: bool,
}

#[cfg(any(unix, windows, test))]
#[derive(Clone, Copy, Debug)]
struct SshAskpass<'a> {
    helper: &'a Path,
    socket: &'a Path,
}

#[cfg(any(unix, windows))]
pub(crate) enum SshLogin {
    Prompts(Option<SshPrompts>),
    Unattended,
}

#[cfg(any(unix, windows))]
impl SshLogin {
    fn split(self) -> (Option<SshPrompts>, bool) {
        match self {
            Self::Prompts(prompts) => (prompts, false),
            Self::Unattended => (None, true),
        }
    }
}

#[cfg(any(unix, test))]
fn forwarding_spec(local_socket: &Path, remote_socket: &Path) -> std::ffi::OsString {
    let mut forwarding = local_socket.as_os_str().to_os_string();
    forwarding.push(":");
    forwarding.push(remote_socket.as_os_str());
    forwarding
}

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
        .arg("-n")
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
    key_line: Option<&str>,
) -> Command {
    ssh_script_command(endpoint, session, &[], &remote_socket_probe(key_line))
}

#[cfg(any(unix, windows, test))]
fn ssh_script_command(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    options: &[&str],
    script: &str,
) -> Command {
    let mut command = Command::new("ssh");
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
fn login_options(command: &mut Command, session: SshSession<'_>) {
    if let Some(identity) = session.identity {
        command
            .arg("-o")
            .arg("IdentitiesOnly=yes")
            .arg("-o")
            .arg(format!(
                "IdentityFile=\"{}\"",
                identity.to_string_lossy().replace('%', "%%")
            ))
            .arg("-o")
            .arg("PreferredAuthentications=publickey");
    }
    let batch = session.batch || batch_logins();
    if batch || session.identity.is_some() {
        command.arg("-o").arg("BatchMode=yes");
    }
    if batch {
        command.arg("-o").arg(NO_SECURITY_KEYS);
    }
}

#[cfg(any(unix, windows, test))]
fn batch_logins() -> bool {
    std::env::var_os(crate::SSH_BATCH_ENVIRONMENT_VARIABLE).is_some_and(|value| !value.is_empty())
}

#[cfg(any(unix, windows))]
fn zz_key_first(
    identity: Option<&Path>,
    command: impl Fn(Option<&Path>) -> Command,
) -> io::Result<(std::process::Output, bool)> {
    if let Some(identity) = identity {
        let output = command(Some(identity)).output_unmasked()?;
        if !refused_login(output.status.code(), &output.stderr) {
            return Ok((output, true));
        }
        log::debug!(
            target: "zz_daemon::diagnostics",
            "the zz key did not sign in: {}",
            String::from_utf8_lossy(&output.stderr).trim(),
        );
    }
    Ok((command(None).output_unmasked()?, false))
}

#[cfg(any(unix, windows, test))]
fn refused_login(code: Option<i32>, stderr: &[u8]) -> bool {
    code == Some(SSH_FAILURE_STATUS)
        && SshFailure::classify(&String::from_utf8_lossy(stderr)).is_some_and(SshFailure::is_login)
}

#[cfg(any(unix, windows, test))]
const NO_SECURITY_KEYS: &str = "PubkeyAcceptedKeyTypes=-sk-*,webauthn-sk-*";

#[cfg(any(unix, windows))]
pub(crate) fn run_remote_script(
    endpoint: &SshEndpoint,
    connect_timeout_seconds: u32,
    script: &str,
) -> io::Result<std::process::Output> {
    let connect_timeout = format!("ConnectTimeout={connect_timeout_seconds}");
    let script = shell_quote(script);
    let identity = zz_identity().map(|(path, _)| path);
    let run = move |endpoint: &SshEndpoint, session: SshSession<'_>| {
        zz_key_first(identity.as_deref(), |phase| {
            ssh_script_command(
                endpoint,
                SshSession {
                    identity: phase,
                    batch: true,
                    ..session
                },
                &[&connect_timeout],
                &script,
            )
        })
        .map(|(output, _)| output)
    };
    #[cfg(unix)]
    if let Some((control, alive)) = live_control(endpoint) {
        let step = SshStep {
            endpoint: endpoint.clone(),
            control: Some(control),
            askpass: None,
            identity: None,
            batch: false,
        };
        return match step.run(alive, "remote script", move |step| {
            Ok(run(&step.endpoint, step.session()))
        }) {
            Ok(output) => output,
            Err(EndpointError::SshFailed { reason, .. }) => {
                Err(io::Error::new(io::ErrorKind::TimedOut, reason))
            }
            Err(error) => Err(io::Error::other(error)),
        };
    }
    run(endpoint, SshSession::default())
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

#[cfg(unix)]
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

#[cfg(any(unix, test))]
pub(crate) fn remote_daemon_start_script(remote_socket: &Path) -> String {
    format!(
        "{}{}",
        remote_path_fallback(),
        remote_daemon_start_body(&shell_quote(&remote_socket.to_string_lossy())),
    )
}

#[cfg(any(unix, windows, test))]
fn remote_daemon_start_body(socket: &str) -> String {
    format!(
        "command -v \"$zz_cli\" >/dev/null 2>&1 || exit {REMOTE_ZZ_MISSING_STATUS}; \
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
    )
}

#[cfg(any(unix, windows, test))]
pub(crate) fn remote_install_script(remote_socket: Option<&Path>, restart_daemon: bool) -> String {
    let restart = if restart_daemon {
        let socket = remote_socket.map_or_else(String::new, |socket| {
            format!("--socket {} ", shell_quote(&socket.to_string_lossy()))
        });
        format!(
            "if [ \"$zz_protocol\" = {protocol} ]; then \
             zz_restart=$(\"$zz_managed\" {socket}kill-server 2>&1 >/dev/null); \
             printf 'zz-install-restart=%s\\n' \"$?\"; \
             printf '%s\\n' \"$zz_restart\" | sed 's/^/zz-install-restart-error=/'; fi; ",
            protocol = zz_protocol::PROTOCOL_VERSION,
        )
    } else {
        String::new()
    };
    format!(
        "{fallback}if [ \"$(\"$zz_managed\" --version 2>/dev/null </dev/null)\" = 'zz {version}' ] \
         && [ \"$(\"$zz_managed\" protocol-version 2>/dev/null </dev/null)\" = {protocol} ]; \
         then printf 'zz-install-current=1\\n'; \
         else zz_link=$(readlink \"$zz_managed\" 2>/dev/null); case \"$zz_link\" in \
         \"\") printf 'zz-install-mode=headless\\n'; sh -s -- --headless --version {version} ;; \
         ../lib/zz/cli|\"$HOME/.local/lib/zz/cli\") printf 'zz-install-mode=desktop\\n'; \
         sh -s -- --prefix \"$HOME/.local\" --version {version} ;; \
         *) printf 'zz-install-elsewhere=%s\\nzz-install-link=%s\\n' \"$zz_managed\" \"$zz_link\"; \
         exit {REMOTE_INSTALL_ELSEWHERE_STATUS} ;; esac || exit {REMOTE_INSTALL_FAILED_STATUS}; fi; \
         zz_protocol=$(\"$zz_managed\" protocol-version) || exit {REMOTE_INSTALL_BROKEN_STATUS}; \
         {restart}printf 'zz-install-protocol=%s\\n' \"$zz_protocol\"",
        fallback = remote_path_fallback(),
        version = env!("CARGO_PKG_VERSION"),
        protocol = zz_protocol::PROTOCOL_VERSION,
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
) -> Result<RemoteInstall, EndpointError> {
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
            Some(Ok(zz_protocol::PROTOCOL_VERSION)) => Ok(RemoteInstall {
                installed: if value("zz-install-current=").is_some() {
                    RemoteInstalled::Current
                } else if value("zz-install-mode=").as_deref() == Some("desktop") {
                    RemoteInstalled::Desktop
                } else {
                    RemoteInstalled::Headless
                },
                restart: match value("zz-install-restart=").as_deref() {
                    None => RemoteRestart::NotAsked,
                    Some("0") => RemoteRestart::Restarted,
                    Some(code) => match stdout
                        .lines()
                        .filter_map(|line| line.strip_prefix("zz-install-restart-error="))
                        .map(str::trim)
                        .rfind(|line| !line.is_empty())
                    {
                        Some(reason) if daemon_was_not_running(reason) => RemoteRestart::NotRunning,
                        Some(reason) => RemoteRestart::Failed(
                            reason.strip_prefix("zz: ").unwrap_or(reason).to_owned(),
                        ),
                        None => RemoteRestart::Failed(format!("kill-server exited with {code}")),
                    },
                },
                warnings: stderr
                    .lines()
                    .filter_map(|line| line.trim().strip_prefix("warning: "))
                    .map(ToOwned::to_owned)
                    .collect(),
            }),
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
            if let Some(path) = value("zz-install-elsewhere=")
                && let Some(link) = value("zz-install-link=") =>
        {
            Err(EndpointError::RemoteInstallElsewhere { target, path, link })
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

#[cfg(any(unix, windows, test))]
fn daemon_was_not_running(message: &str) -> bool {
    message.starts_with("no server running on ")
        || (message.starts_with("error connecting to ")
            && message.ends_with("(No such file or directory)"))
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
pub(crate) fn remote_proxy_script(remote_socket: &Path) -> String {
    format!(
        "{fallback}exec \"$zz_cli\" proxy --socket {socket}",
        fallback = remote_path_fallback(),
        socket = shell_quote(&remote_socket.to_string_lossy()),
    )
}

#[cfg(any(windows, test))]
pub(crate) fn remote_connect_script(
    remote_socket: Option<&Path>,
    key_line: Option<&str>,
) -> String {
    let socket = remote_socket.map_or_else(
        || "\"$zz_dir/default.sock\"".to_owned(),
        |socket| shell_quote(&socket.to_string_lossy()),
    );
    format!(
        "{probe}; command -v \"$zz_cli\" >/dev/null 2>&1 || exit {REMOTE_ZZ_MISSING_STATUS}; \
         [ \"$(\"$zz_cli\" protocol-version 2>/dev/null)\" = {protocol} ] || exit 0; \
         zz_socket={socket}; ( {start} ) || exit $?; \
         exec \"$zz_cli\" proxy --socket \"$zz_socket\"",
        probe = remote_socket_probe_script(key_line),
        protocol = zz_protocol::PROTOCOL_VERSION,
        start = remote_daemon_start_body("\"$zz_socket\""),
    )
}

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
pub(crate) const SSH_FAILURE_STATUS: i32 = 255;
#[cfg(any(unix, windows, test))]
const SSH_CONNECT_TIMEOUT_SECONDS: u32 = 10;
#[cfg(any(unix, windows, test))]
pub(crate) const UNATTENDED_CONNECT_TIMEOUT_SECONDS: u32 = 3;

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
    WindowsShell,
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
        } else if contains("fullyqualifiederrorid")
            || contains("is not recognized as an internal or external command")
        {
            Some(Self::WindowsShell)
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
                "known_hosts has no matching key for the host: verify the host before connecting"
            }
            Self::Resolve => "the host name did not resolve",
            Self::Refused => {
                "nothing accepted the ssh connection: start sshd there, or on a Mac turn on Remote \
                 Login in System Settings › General › Sharing"
            }
            Self::Timeout => "the ssh connection timed out",
            Self::Unreachable => "the host is not reachable from this network",
            Self::WindowsShell => {
                "the host's ssh shell is PowerShell or cmd, and zz needs a Linux or macOS shell there"
            }
        }
    }
}

#[cfg(any(unix, windows, test))]
pub(crate) fn ssh_failure_hint(stderr: &str) -> Option<&'static str> {
    SshFailure::classify(stderr).map(SshFailure::hint)
}

#[cfg(any(unix, windows, test))]
pub(crate) fn ssh_failure_reason(stderr: &str, status: &str) -> String {
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
    login_options(command, session);
    let connect_timeout = if batch_logins() {
        UNATTENDED_CONNECT_TIMEOUT_SECONDS
    } else {
        SSH_CONNECT_TIMEOUT_SECONDS
    };
    command
        .arg("-o")
        .arg(format!("ConnectTimeout={connect_timeout}"))
        .arg("-o")
        .arg("ServerAliveInterval=5")
        .arg("-o")
        .arg("ServerAliveCountMax=3")
        .arg("-o")
        .arg("Compression=yes");
    #[cfg(windows)]
    command
        .arg("-o")
        .arg("ControlMaster=no")
        .arg("-o")
        .arg("ControlPath=none");
    #[cfg(not(windows))]
    if let Some(control) = session.control {
        command
            .arg("-o")
            .arg("ControlMaster=auto")
            .arg("-o")
            .arg(control_path_option(&control.path));
        if !control.user_persist {
            command
                .arg("-o")
                .arg(format!("ControlPersist={CONTROL_PERSIST_SECONDS}"));
        }
    }
    if let Some(port) = endpoint.port {
        command.arg("-p").arg(port.to_string());
    }
    if let Some(user) = &endpoint.user {
        command.arg("-l").arg(user);
    }
    if let Some(askpass) = session.askpass {
        command
            .env(crate::askpass::SSH_ASKPASS_ENV, askpass.helper)
            .env(crate::askpass::SSH_ASKPASS_REQUIRE_ENV, "force")
            .env(crate::askpass::ASKPASS_SOCKET_ENV, askpass.socket);
    }
}

#[cfg(any(unix, test))]
fn control_path_option(control_path: &Path) -> String {
    format!(
        "ControlPath={}",
        control_path.to_string_lossy().replace('%', "%%")
    )
}

#[cfg(unix)]
pub(crate) fn ssh_runtime_dir() -> io::Result<&'static Path> {
    static DIRECTORY: OnceLock<Result<SshRuntimeDir, String>> = OnceLock::new();
    match DIRECTORY.get_or_init(|| {
        let stable = stable_ssh_runtime_dir();
        remove_legacy_ssh_dirs(&ssh_runtime_root());
        match private_directory(&stable) {
            Ok(()) => return Ok(SshRuntimeDir::Stable(stable)),
            Err(error) => log::warn!(
                target: "zz_daemon::diagnostics",
                "ssh sockets fall back to a per-process directory: {} is unusable: {error}",
                stable.display(),
            ),
        }
        let directory = tempfile::Builder::new()
            .prefix(&format!("{}-", user_ssh_dir_name()))
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
        Ok(SshRuntimeDir::Process(directory))
    }) {
        Ok(directory) => Ok(directory.path()),
        Err(error) => Err(io::Error::other(error.clone())),
    }
}

#[cfg(unix)]
enum SshRuntimeDir {
    Stable(PathBuf),
    Process(tempfile::TempDir),
}

#[cfg(unix)]
impl SshRuntimeDir {
    fn path(&self) -> &Path {
        match self {
            Self::Stable(path) => path,
            Self::Process(directory) => directory.path(),
        }
    }
}

#[cfg(unix)]
fn stable_ssh_runtime_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|runtime| runtime.is_absolute())
    {
        return runtime
            .join(zz_protocol::app_identity::DIRECTORY)
            .join("ssh");
    }
    ssh_runtime_root().join(user_ssh_dir_name())
}

#[cfg(unix)]
fn user_ssh_dir_name() -> String {
    format!(
        "{}-ssh-{}",
        zz_protocol::app_identity::DIRECTORY,
        rustix::process::geteuid().as_raw()
    )
}

#[cfg(unix)]
fn private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::other("it is not a directory"));
    }
    if metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err(io::Error::other("another user owns it"));
    }
    if metadata.mode() & 0o077 != 0 {
        return Err(io::Error::other(format!(
            "its mode is {:o}, not 700",
            metadata.mode() & 0o777
        )));
    }
    if path.to_str().is_none() {
        return Err(io::Error::other("it is not valid UTF-8"));
    }
    Ok(())
}

#[cfg(unix)]
fn remove_legacy_ssh_dirs(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_str().is_some_and(legacy_ssh_dir_name) {
            continue;
        }
        let path = entry.path();
        let settled = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= LEGACY_SSH_DIRECTORY_AGE);
        if settled {
            let _ = fs::remove_dir(&path);
        }
    }
}

#[cfg(unix)]
fn legacy_ssh_dir_name(name: &str) -> bool {
    name.strip_prefix("zz-ssh-").is_some_and(|suffix| {
        suffix.len() == 6
            && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
            && !suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
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
    Ok(ssh_runtime_dir()?.join(format!("c{:016x}", endpoint_hash(endpoint, None))))
}

#[cfg(unix)]
pub(crate) fn kept_forward_path(endpoint: &SshEndpoint) -> io::Result<PathBuf> {
    Ok(ssh_runtime_dir()?.join(format!(
        "c{:016x}.sock",
        endpoint_hash(endpoint, endpoint.remote_socket.as_deref())
    )))
}

#[cfg(unix)]
fn endpoint_hash(endpoint: &SshEndpoint, remote_socket: Option<&Path>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    endpoint.user.hash(&mut hasher);
    endpoint.host.hash(&mut hasher);
    endpoint.port.hash(&mut hasher);
    if let Some(remote_socket) = remote_socket {
        remote_socket.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(unix)]
fn control_master_alive(endpoint: &SshEndpoint, control_path: &Path) -> bool {
    control_path.exists()
        && ssh_control_command(endpoint, control_path, "check", &[])
            .status_unmasked()
            .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn ssh_control_command(
    endpoint: &SshEndpoint,
    control_path: &Path,
    request: &str,
    forwarding: &[(&str, &OsStr)],
) -> Command {
    let mut command = Command::new("ssh");
    command
        .arg("-O")
        .arg(request)
        .arg("-o")
        .arg(control_path_option(control_path));
    for (flag, spec) in forwarding {
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

#[cfg(unix)]
fn probe_remote_socket(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
) -> Result<(PathBuf, Option<PathBuf>), EndpointError> {
    let (output, identity) = probe_login(endpoint, session, false)?;
    if !output.status.success() {
        return Err(probe_ssh_failure(endpoint, &output));
    }
    let probe = parse_remote_probe_output(&endpoint.to_string(), &output.stdout);
    note_key_enrollment(endpoint, session, &probe);
    probe.socket.map(|socket| (socket, identity))
}

#[cfg(any(unix, windows))]
fn sign_in(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
) -> Result<Option<PathBuf>, EndpointError> {
    use std::io::IsTerminal as _;

    #[cfg(not(windows))]
    let shared = session.control.is_some();
    #[cfg(windows)]
    let shared = false;
    if !shared {
        return zz_key_signs_in(endpoint, session);
    }
    let shows_login = session.askpass.is_none()
        && !session.batch
        && !batch_logins()
        && io::stderr().is_terminal();
    let (output, identity) = probe_login(endpoint, session, shows_login)?;
    if output.status.code() == Some(SSH_FAILURE_STATUS) {
        return Err(probe_ssh_failure(endpoint, &output));
    }
    let probe = parse_remote_probe_output(&endpoint.to_string(), &output.stdout);
    note_key_enrollment(endpoint, session, &probe);
    Ok(identity)
}

#[cfg(any(unix, windows))]
fn zz_key_signs_in(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
) -> Result<Option<PathBuf>, EndpointError> {
    let Some((path, _)) = zz_identity() else {
        return Ok(None);
    };
    let output = ssh_probe_command(
        endpoint,
        SshSession {
            identity: Some(&path),
            ..session
        },
        None,
    )
    .output_unmasked()
    .map_err(|source| EndpointError::SshSpawn {
        operation: "remote socket probe",
        source,
    })?;
    if refused_login(output.status.code(), &output.stderr) {
        return Ok(None);
    }
    if output.status.code() == Some(SSH_FAILURE_STATUS) {
        return Err(probe_ssh_failure(endpoint, &output));
    }
    Ok(Some(path))
}

#[cfg(any(unix, windows))]
fn probe_login(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    shows_login: bool,
) -> Result<(std::process::Output, Option<PathBuf>), EndpointError> {
    let identity = zz_identity();
    let key_line = identity
        .as_ref()
        .filter(|_| configured_fleet_endpoint(endpoint))
        .map(|(_, line)| line.as_str());
    let path = identity.as_ref().map(|(path, _)| path.as_path());
    let (output, signed_in) = zz_key_first(path, |phase| {
        let mut command = ssh_probe_command(
            endpoint,
            SshSession {
                identity: phase,
                ..session
            },
            key_line,
        );
        if shows_login && phase.is_none() {
            command.stderr(Stdio::inherit());
        }
        command
    })
    .map_err(|source| EndpointError::SshSpawn {
        operation: "remote socket probe",
        source,
    })?;
    Ok((output, path.filter(|_| signed_in).map(Path::to_owned)))
}

#[cfg(any(unix, windows))]
fn probe_ssh_failure(endpoint: &SshEndpoint, output: &std::process::Output) -> EndpointError {
    EndpointError::SshFailed {
        target: endpoint.to_string(),
        reason: ssh_failure_reason(
            &String::from_utf8_lossy(&output.stderr),
            &output.status.to_string(),
        ),
    }
}

#[cfg(any(unix, windows))]
fn configured_fleet_endpoint(endpoint: &SshEndpoint) -> bool {
    crate::configured_fleet_hosts().is_ok_and(|(hosts, _)| {
        hosts.iter().any(|host| {
            matches!(&host.endpoint, Endpoint::Ssh(configured)
                if configured.user == endpoint.user
                    && configured.host == endpoint.host
                    && configured.port == endpoint.port)
        })
    })
}

#[cfg(any(unix, windows))]
fn note_key_enrollment(endpoint: &SshEndpoint, session: SshSession<'_>, probe: &RemoteProbe) {
    match probe.key {
        Some(KeyEnrollment::Added) => {
            log::debug!(
                target: "zz_daemon::diagnostics",
                "added this machine's zz key to {endpoint}'s authorized_keys; its own zz key is {}",
                probe.pubkey.as_deref().unwrap_or("unknown"),
            );
            if let Some(askpass) = session.askpass {
                let notice = AskpassPrompt::new(AskpassMode::KeyAdded, endpoint.to_string());
                if let Err(error) = crate::askpass::notify(askpass.socket, &notice) {
                    log::debug!(
                        target: "zz_daemon::askpass",
                        "could not tell the window about {endpoint}'s new key: {error}",
                    );
                }
            }
        }
        Some(KeyEnrollment::Failed) => log::debug!(
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

#[cfg(unix)]
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
fn live_control(endpoint: &SshEndpoint) -> Option<(SshControl, bool)> {
    let control = configured_control(endpoint)?;
    let alive = control_master_alive(endpoint, &control.path);
    if !alive && control.path.exists() {
        let _ = fs::remove_file(&control.path);
    }
    Some((control, alive))
}

#[cfg(unix)]
fn configured_control(endpoint: &SshEndpoint) -> Option<SshControl> {
    ssh_config_command(endpoint)
        .output_unmasked()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| user_control(&String::from_utf8_lossy(&output.stdout)))
        .or_else(|| match control_master_path(endpoint) {
            Ok(path) => Some(SshControl {
                path,
                user_persist: false,
            }),
            Err(error) => {
                log::warn!(
                    target: "zz_daemon::diagnostics",
                    "ssh connection sharing is unavailable for {endpoint}: {error}",
                );
                None
            }
        })
}

#[cfg(unix)]
fn ssh_config_command(endpoint: &SshEndpoint) -> Command {
    let mut command = Command::new("ssh");
    command.arg("-G");
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
        .stderr(Stdio::null());
    command
}

#[cfg(unix)]
fn user_control(config: &str) -> Option<SshControl> {
    let value = |key: &str| {
        config.lines().find_map(|line| {
            let (name, value) = line.split_once(' ')?;
            name.eq_ignore_ascii_case(key).then(|| value.trim())
        })
    };
    if !matches!(value("controlmaster"), Some("auto" | "true" | "yes")) {
        return None;
    }
    let path = value("controlpath")
        .filter(|path| path.starts_with('/') && !path.contains(char::is_whitespace))?;
    Some(SshControl {
        path: PathBuf::from(path),
        user_persist: !matches!(value("controlpersist"), None | Some("no" | "false")),
    })
}

#[cfg(any(unix, windows))]
pub fn run_remote_cli(
    endpoint: &SshEndpoint,
    arguments: &[String],
) -> Result<std::process::ExitStatus, EndpointError> {
    use std::io::IsTerminal as _;

    let mut command = remote_cli_command(endpoint, arguments)?;
    if !io::stdin().is_terminal() {
        command.stdin(Stdio::null());
    }
    command
        .status_unmasked()
        .map_err(|source| EndpointError::SshSpawn {
            operation: "remote command",
            source,
        })
}

#[cfg(any(unix, windows))]
pub fn remote_cli_output(
    endpoint: &SshEndpoint,
    arguments: &[String],
) -> Result<std::process::Output, EndpointError> {
    remote_cli_command(endpoint, arguments)?
        .stdin(Stdio::null())
        .output_unmasked()
        .map_err(|source| EndpointError::SshSpawn {
            operation: "remote command",
            source,
        })
}

#[cfg(any(unix, windows))]
fn remote_cli_command(
    endpoint: &SshEndpoint,
    arguments: &[String],
) -> Result<Command, EndpointError> {
    #[cfg(unix)]
    let (control, answered) = riding_control(endpoint);
    #[cfg(not(unix))]
    let answered = false;
    let session = SshSession {
        #[cfg(unix)]
        control: control.as_ref(),
        ..SshSession::default()
    };
    let identity = if answered {
        None
    } else {
        sign_in(endpoint, session)?
    };
    let session = SshSession {
        identity: identity.as_deref(),
        ..session
    };
    Ok(ssh_cli_command(endpoint, session, arguments))
}

#[cfg(unix)]
fn riding_control(endpoint: &SshEndpoint) -> (Option<SshControl>, bool) {
    let Some((control, alive)) = live_control(endpoint) else {
        return (None, false);
    };
    if !alive {
        return (Some(control), false);
    }
    let step = SshStep {
        endpoint: endpoint.clone(),
        control: Some(control),
        askpass: None,
        identity: None,
        batch: false,
    };
    let answered = match step.run(true, "remote command", master_answers) {
        Ok(()) => true,
        Err(error) => {
            log::debug!(
                target: "zz_daemon::diagnostics",
                "{error}; signing in again",
            );
            false
        }
    };
    (step.control, answered)
}

#[cfg(unix)]
fn master_answers(step: &SshStep) -> Result<(), EndpointError> {
    let mut command = Command::new("ssh");
    command.arg("-T");
    append_ssh_options(&mut command, &step.endpoint, step.session());
    command
        .arg("--")
        .arg(&step.endpoint.host)
        .arg("exit")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status_unmasked()
        .map(drop)
        .map_err(|source| EndpointError::SshSpawn {
            operation: "remote command",
            source,
        })
}

#[cfg(any(unix, windows, test))]
fn ssh_cli_command(
    endpoint: &SshEndpoint,
    session: SshSession<'_>,
    arguments: &[String],
) -> Command {
    let mut script = format!("{}exec \"$zz_cli\"", remote_path_fallback());
    for argument in arguments {
        script.push(' ');
        script.push_str(&shell_quote(argument));
    }
    let mut command = Command::new("ssh");
    command.arg("-T");
    append_ssh_options(&mut command, endpoint, session);
    command
        .arg("--")
        .arg(&endpoint.host)
        .arg("sh")
        .arg("-lc")
        .arg(shell_quote(&script));
    command
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteInstall {
    pub installed: RemoteInstalled,
    pub restart: RemoteRestart,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteInstalled {
    Current,
    Headless,
    Desktop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteRestart {
    NotAsked,
    Restarted,
    NotRunning,
    Failed(String),
}

impl RemoteInstall {
    #[must_use]
    pub fn summary(&self) -> String {
        let name = zz_protocol::app_identity::DIRECTORY;
        let version = env!("CARGO_PKG_VERSION");
        let installed = match self.installed {
            RemoteInstalled::Current => format!("already {name} {version}"),
            RemoteInstalled::Headless => format!("{name} {version} installed (headless)"),
            RemoteInstalled::Desktop => format!("{name} {version} installed (desktop)"),
        };
        match &self.restart {
            RemoteRestart::NotAsked => installed,
            RemoteRestart::Restarted => format!("{installed}, daemon restarted"),
            RemoteRestart::NotRunning => format!("{installed}, no daemon was running"),
            RemoteRestart::Failed(reason) => {
                format!("{installed}, but its daemon did not stop: {reason}")
            }
        }
    }

    #[must_use]
    pub fn notice(&self, name: &str) -> (zz_protocol::ClientMessageKind, String) {
        let kind = if matches!(self.restart, RemoteRestart::Failed(_)) {
            zz_protocol::ClientMessageKind::Error
        } else if self.warnings.is_empty() {
            zz_protocol::ClientMessageKind::Success
        } else {
            zz_protocol::ClientMessageKind::Warning
        };
        let text = std::iter::once(format!("{name}: {}", self.summary()))
            .chain(self.warnings.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n");
        (kind, text)
    }
}

#[cfg(any(unix, windows))]
pub fn install_remote(
    endpoint: &SshEndpoint,
    prompts: Option<SshPrompts>,
    restart_daemon: bool,
) -> Result<RemoteInstall, EndpointError> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        refuse_development_install(endpoint)?;
        crate::russh_client::install_remote(endpoint, prompts, restart_daemon)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    install_remote_with(endpoint, prompts, restart_daemon, false)
}

#[cfg(any(unix, windows))]
pub fn install_remote_batch(
    endpoint: &SshEndpoint,
    restart_daemon: bool,
) -> Result<RemoteInstall, EndpointError> {
    install_remote_with(endpoint, None, restart_daemon, true)
}

#[cfg(any(unix, windows))]
pub(crate) fn refuse_development_install_target(target: &str) -> Result<(), EndpointError> {
    if zz_protocol::app_identity::DEVELOPMENT {
        return Err(EndpointError::RemoteInstallFailed {
            target: target.to_owned(),
            reason: format!(
                "{} looks for {} there, and no release ships it",
                zz_protocol::app_identity::DISPLAY_NAME,
                zz_protocol::app_identity::DIRECTORY,
            ),
        });
    }
    Ok(())
}

#[cfg(any(unix, windows))]
fn refuse_development_install(endpoint: &SshEndpoint) -> Result<(), EndpointError> {
    refuse_development_install_target(&endpoint.to_string())
}

#[cfg(any(unix, windows))]
fn install_remote_with(
    endpoint: &SshEndpoint,
    prompts: Option<SshPrompts>,
    restart_daemon: bool,
    batch: bool,
) -> Result<RemoteInstall, EndpointError> {
    let target = endpoint.to_string();
    refuse_development_install(endpoint)?;
    let askpass = start_askpass(endpoint, prompts);
    #[cfg(unix)]
    let (control, answered) = riding_control(endpoint);
    #[cfg(not(unix))]
    let answered = false;
    let session = SshSession {
        #[cfg(unix)]
        control: control.as_ref(),
        askpass: session_askpass(askpass.as_ref()),
        identity: None,
        batch,
    };
    let identity = if answered {
        None
    } else {
        sign_in(endpoint, session)?
    };
    let session = SshSession {
        identity: identity.as_deref(),
        ..session
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
fn forwarded_local_socket_path() -> io::Result<PathBuf> {
    let counter = FORWARD_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(ssh_runtime_dir()?.join(format!("fwd-{}-{counter}.sock", std::process::id())))
}

#[cfg(unix)]
pub(crate) fn connect_kept<T>(
    endpoint: &SshEndpoint,
    login: SshLogin,
    resendable: bool,
    mut connect: impl FnMut(&Path, LocalStream) -> Result<T, DaemonError>,
) -> Result<(T, Option<SshForward>), DaemonError> {
    let kept = kept_forward_path(endpoint).map_err(|source| EndpointError::ForwardIo {
        target: endpoint.to_string(),
        action: "making a directory for",
        path: ssh_runtime_root(),
        source,
    })?;
    let mut dial = |socket: &Path| -> Result<T, DaemonError> {
        let stream = LocalTransport::connect(socket)?;
        let bound = stream.try_clone()?;
        bound.set_timeout(Some(MASTER_STEP_TIMEOUT))?;
        let connected = connect(socket, stream)?;
        bound.set_timeout(None)?;
        Ok(connected)
    };
    let stalled = match dial(&kept) {
        Ok(connected) => return Ok((connected, None)),
        Err(error) if !kept_forward_failed(&error) => return Err(error),
        Err(error) if !resendable && link_stalled(&error) => {
            return Err(give_up_on_stalled_link(endpoint).into());
        }
        Err(error) => {
            log::debug!(
                target: "zz_daemon::diagnostics",
                "the kept forward to {endpoint} did not answer ({error}); making it again",
            );
            link_stalled(&error)
        }
    };
    let lock = KeptForwardLock::acquire(&kept);
    if lock.contended
        && let Ok(connected) = dial(&kept)
    {
        return Ok((connected, None));
    }
    if stalled {
        abandon_stalled_master(endpoint, &kept);
    }
    let forward = SshForward::keep(endpoint, login, &kept)?;
    let connected = dial(forward.local_socket())?;
    Ok((connected, (!forward.kept).then_some(forward)))
}

#[cfg(unix)]
pub(crate) fn link_stalled(error: &DaemonError) -> bool {
    match error {
        DaemonError::Io(error) | DaemonError::Protocol(zz_protocol::ProtocolError::Io(error)) => {
            matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            )
        }
        _ => false,
    }
}

#[cfg(unix)]
fn abandon_stalled_master(endpoint: &SshEndpoint, kept: &Path) {
    log::debug!(
        target: "zz_daemon::diagnostics",
        "the ssh master for {endpoint} gave no answer within {} s; stopping it",
        MASTER_STEP_TIMEOUT.as_secs(),
    );
    if let Some(control) = configured_control(endpoint) {
        let _ = ssh_control_command(endpoint, &control.path, "stop", &[]).status_unmasked();
    }
    let _ = fs::remove_file(kept);
}

#[cfg(unix)]
fn kept_forward_failed(error: &DaemonError) -> bool {
    match error {
        DaemonError::Io(_) | DaemonError::Protocol(_) | DaemonError::IncompatibleDaemon { .. } => {
            true
        }
        DaemonError::Server(error) => {
            matches!(error, zz_protocol::ServerError::ProtocolMismatch { .. })
        }
        _ => false,
    }
}

#[cfg(unix)]
pub(crate) fn give_up_on_stalled_link(endpoint: &SshEndpoint) -> EndpointError {
    if let Ok(kept) = kept_forward_path(endpoint) {
        let lock = KeptForwardLock::acquire(&kept);
        if !lock.contended {
            abandon_stalled_master(endpoint, &kept);
        }
    }
    stopped_answering(endpoint)
}

#[cfg(unix)]
pub(crate) fn stopped_answering(endpoint: &SshEndpoint) -> EndpointError {
    EndpointError::StoppedAnswering {
        target: endpoint.to_string(),
        seconds: MASTER_STEP_TIMEOUT.as_secs(),
    }
}

#[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
pub(crate) fn remake_kept_forward(
    endpoint: &SshEndpoint,
    stalled: bool,
    answers: impl FnOnce(&Path) -> bool,
) -> Result<(PathBuf, Option<SshForward>), EndpointError> {
    let kept = kept_forward_path(endpoint).map_err(|source| EndpointError::ForwardIo {
        target: endpoint.to_string(),
        action: "making a directory for",
        path: ssh_runtime_root(),
        source,
    })?;
    let lock = KeptForwardLock::acquire(&kept);
    if lock.contended && answers(&kept) {
        return Ok((kept, None));
    }
    if stalled {
        abandon_stalled_master(endpoint, &kept);
    }
    let forward = SshForward::keep(endpoint, SshLogin::Prompts(None), &kept)?;
    let socket = forward.local_socket.clone();
    Ok((socket, (!forward.kept).then_some(forward)))
}

#[cfg(unix)]
struct KeptForwardLock {
    _file: Option<fs::File>,
    contended: bool,
}

#[cfg(unix)]
impl KeptForwardLock {
    fn acquire(kept: &Path) -> Self {
        use std::os::unix::fs::OpenOptionsExt as _;

        let mut path = kept.as_os_str().to_owned();
        path.push(".lock");
        let Ok(file) = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(PathBuf::from(path))
        else {
            return Self {
                _file: None,
                contended: false,
            };
        };
        let contended = match file.try_lock() {
            Err(fs::TryLockError::WouldBlock) => file.lock().is_ok(),
            Ok(()) | Err(fs::TryLockError::Error(_)) => false,
        };
        Self {
            _file: Some(file),
            contended,
        }
    }
}

#[cfg(unix)]
pub(crate) const MASTER_STEP_TIMEOUT: Duration = Duration::from_secs(15);

#[cfg(unix)]
#[derive(Clone)]
struct SshStep {
    endpoint: SshEndpoint,
    control: Option<SshControl>,
    askpass: Option<(PathBuf, PathBuf)>,
    identity: Option<PathBuf>,
    batch: bool,
}

#[cfg(unix)]
impl SshStep {
    fn session(&self) -> SshSession<'_> {
        SshSession {
            control: self.control.as_ref(),
            askpass: self
                .askpass
                .as_ref()
                .map(|(helper, socket)| SshAskpass { helper, socket }),
            identity: self.identity.as_deref(),
            batch: self.batch,
        }
    }

    fn run<T: Send + 'static>(
        &self,
        riding: bool,
        operation: &'static str,
        step: impl FnOnce(&Self) -> Result<T, EndpointError> + Send + 'static,
    ) -> Result<T, EndpointError> {
        self.run_within(MASTER_STEP_TIMEOUT, riding, operation, step)
    }

    fn run_within<T: Send + 'static>(
        &self,
        timeout: Duration,
        riding: bool,
        operation: &'static str,
        step: impl FnOnce(&Self) -> Result<T, EndpointError> + Send + 'static,
    ) -> Result<T, EndpointError> {
        let Some(control) = self.control.as_ref().filter(|_| riding) else {
            return step(self);
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let owned = self.clone();
        thread::Builder::new()
            .name("zz-ssh-step".to_owned())
            .spawn(move || {
                let _ = sender.send(step(&owned));
            })
            .map_err(|source| EndpointError::SshSpawn { operation, source })?;
        receiver.recv_timeout(timeout).unwrap_or_else(|_| {
            let _ =
                ssh_control_command(&self.endpoint, &control.path, "stop", &[]).status_unmasked();
            Err(EndpointError::SshFailed {
                target: self.endpoint.to_string(),
                reason: format!(
                    "the {operation} got no answer over the open ssh connection within {} s",
                    timeout.as_secs()
                ),
            })
        })
    }
}

#[cfg(unix)]
pub(crate) struct SshForward {
    child: Option<Child>,
    local_socket: PathBuf,
    endpoint: SshEndpoint,
    control: Option<SshControl>,
    master: bool,
    forwarding: OsString,
    socks_port: Option<u16>,
    askpass: Option<AskpassListener>,
    identity: Option<PathBuf>,
    kept: bool,
    batch: bool,
}

#[cfg(unix)]
impl SshForward {
    pub(crate) fn start(endpoint: &SshEndpoint, login: SshLogin) -> Result<Self, EndpointError> {
        Self::open_new(endpoint, login, None)
    }

    fn keep(endpoint: &SshEndpoint, login: SshLogin, kept: &Path) -> Result<Self, EndpointError> {
        Self::open_new(endpoint, login, Some(kept))
    }

    fn open_new(
        endpoint: &SshEndpoint,
        login: SshLogin,
        kept: Option<&Path>,
    ) -> Result<Self, EndpointError> {
        let (prompts, batch) = login.split();
        let askpass = start_askpass(endpoint, prompts);
        let (control, riding) =
            live_control(endpoint).map_or((None, false), |(control, alive)| (Some(control), alive));
        let mut step = SshStep {
            endpoint: endpoint.clone(),
            control,
            askpass: askpass
                .as_ref()
                .map(|listener| (listener.helper().to_owned(), listener.socket().to_owned())),
            identity: None,
            batch,
        };

        let (probed_socket, identity) = step.run(riding, "remote socket probe", |step| {
            probe_remote_socket(&step.endpoint, step.session())
        })?;
        step.identity = identity;
        let remote_socket = endpoint.remote_socket.clone().unwrap_or(probed_socket);
        if !remote_socket.is_absolute() {
            return Err(EndpointError::InvalidRemoteSocket(remote_socket));
        }
        let master = step
            .control
            .as_ref()
            .is_some_and(|control| riding || control_master_alive(endpoint, &control.path));
        let start_socket = remote_socket.clone();
        step.run(master, "remote daemon start", move |step| {
            ensure_remote_daemon(&step.endpoint, step.session(), &start_socket)
        })?;

        let socks = kept.is_none();
        let kept = kept.filter(|_| master);
        let local_socket = match kept {
            Some(kept) => kept.to_owned(),
            None => forwarded_local_socket_path().map_err(|source| EndpointError::ForwardIo {
                target: endpoint.to_string(),
                action: "making a directory for",
                path: ssh_runtime_root(),
                source,
            })?,
        };
        let mut forward = Self {
            child: None,
            forwarding: forwarding_spec(&local_socket, &remote_socket),
            local_socket,
            endpoint: endpoint.clone(),
            control: step.control,
            master,
            socks_port: socks.then(available_socks_port).flatten(),
            askpass,
            identity: step.identity,
            kept: kept.is_some(),
            batch,
        };
        if forward.kept {
            forward.cancel_forwards();
            let _ = fs::remove_file(&forward.local_socket);
        }
        if let Err(error) = forward.open(&remote_socket) {
            let Some(port) = forward.socks_port else {
                return Err(error);
            };
            log::debug!(
                target: "zz_daemon::diagnostics",
                "ssh forward to {endpoint} failed while binding -D {}; retrying on a fresh port: {error}",
                socks_spec(port),
            );
            forward.restart_with_fresh_socks_port(&remote_socket)?;
        }

        let mut permissions = fs::metadata(&forward.local_socket)
            .map_err(|source| forward.io_error("reading permissions for", source))?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&forward.local_socket, permissions)
            .map_err(|source| forward.io_error("setting permissions on", source))?;
        Ok(forward)
    }

    pub(crate) fn local_socket(&self) -> &Path {
        &self.local_socket
    }

    pub(crate) const fn socks_port(&self) -> Option<u16> {
        self.socks_port
    }

    fn open(&mut self, remote_socket: &Path) -> Result<(), EndpointError> {
        let Some(control_path) = self
            .control
            .as_ref()
            .filter(|_| self.master)
            .map(|control| control.path.clone())
        else {
            let session = SshSession {
                control: self.control.as_ref(),
                askpass: session_askpass(self.askpass.as_ref()),
                identity: self.identity.as_deref(),
                batch: self.batch,
            };
            let child = ssh_forward_command(
                &self.endpoint,
                session,
                &self.local_socket,
                remote_socket,
                self.socks_port,
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn_unmasked()
            .map_err(|source| EndpointError::SshSpawn {
                operation: "socket forward",
                source,
            })?;
            self.child = Some(child);
            return self.wait_for_socket();
        };
        let socks = self.socks_port.map(socks_spec);
        let mut forwarding = vec![("-L", self.forwarding.as_os_str())];
        if let Some(socks) = &socks {
            forwarding.push(("-D", OsStr::new(socks)));
        }
        let output = ssh_control_command(&self.endpoint, &control_path, "forward", &forwarding)
            .stderr(Stdio::piped())
            .output_unmasked()
            .map_err(|source| EndpointError::SshSpawn {
                operation: "socket forward",
                source,
            })?;
        if !output.status.success() {
            log::debug!(
                target: "zz_daemon::diagnostics",
                "ssh -O forward to {} failed: {}",
                self.endpoint,
                String::from_utf8_lossy(&output.stderr).trim(),
            );
            return Err(EndpointError::ForwardExited {
                target: self.endpoint.to_string(),
                status: output.status.to_string(),
                local_socket: self.local_socket.clone(),
            });
        }
        self.wait_for_socket()
    }

    fn restart_with_fresh_socks_port(&mut self, remote_socket: &Path) -> Result<(), EndpointError> {
        self.stop_child();
        self.cancel_forwards();
        let _ = fs::remove_file(&self.local_socket);
        self.socks_port = available_socks_port();
        self.open(remote_socket)
    }

    fn stop_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn cancel_forwards(&self) {
        let Some(control_path) = self.control.as_ref().map(|control| &control.path) else {
            return;
        };
        if !control_path.exists() {
            return;
        }
        let cancel = |forwarding| {
            let _ = ssh_control_command(&self.endpoint, control_path, "cancel", &[forwarding])
                .status_unmasked();
        };
        cancel(("-L", self.forwarding.as_os_str()));
        if let Some(port) = self.socks_port {
            let spec = socks_spec(port);
            cancel(("-D", OsStr::new(&spec)));
        }
    }

    fn wait_for_socket(&mut self) -> Result<(), EndpointError> {
        for _ in 0..FORWARD_POLL_ATTEMPTS {
            if self.socket_exists()? {
                return Ok(());
            }
            let exited = match &mut self.child {
                Some(child) => child
                    .try_wait()
                    .map_err(|source| self.io_error("checking ssh child for", source))?,
                None => None,
            };
            if let Some(status) = exited {
                if self.socket_exists()? {
                    return Ok(());
                }
                return Err(EndpointError::ForwardExited {
                    target: self.endpoint.to_string(),
                    status: status.to_string(),
                    local_socket: self.local_socket.clone(),
                });
            }
            thread::sleep(FORWARD_POLL_INTERVAL);
        }
        if self.socket_exists()? {
            return Ok(());
        }
        Err(EndpointError::ForwardTimeout {
            target: self.endpoint.to_string(),
            local_socket: self.local_socket.clone(),
        })
    }

    fn socket_exists(&self) -> Result<bool, EndpointError> {
        match fs::metadata(&self.local_socket) {
            Ok(metadata) => Ok(metadata.file_type().is_socket()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(self.io_error("inspecting", source)),
        }
    }

    fn io_error(&self, action: &'static str, source: io::Error) -> EndpointError {
        EndpointError::ForwardIo {
            target: self.endpoint.to_string(),
            action,
            path: self.local_socket.clone(),
            source,
        }
    }
}

#[cfg(unix)]
impl Drop for SshForward {
    fn drop(&mut self) {
        if self.kept {
            return;
        }
        self.stop_child();
        self.cancel_forwards();
        let _ = fs::remove_file(&self.local_socket);
    }
}

#[cfg(windows)]
pub(crate) fn connect_kept<T>(
    endpoint: &SshEndpoint,
    login: SshLogin,
    _resendable: bool,
    mut connect: impl FnMut(&Path, LocalStream) -> Result<T, DaemonError>,
) -> Result<(T, Option<SshForward>), DaemonError> {
    let forward = SshForward::start(endpoint, login)?;
    let socket = forward.local_socket();
    let connected = connect(socket, LocalTransport::connect(socket)?)?;
    Ok((connected, Some(forward)))
}

#[cfg(windows)]
const FORWARD_PIPE_ROLE: &str = "zz-fwd";

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

#[cfg(any(windows, target_os = "ios", target_os = "android"))]
pub(crate) fn remote_socket_is_absolute(path: &Path) -> bool {
    path.as_os_str().as_encoded_bytes().starts_with(b"/")
}

#[cfg(windows)]
fn ssh_connect_command(endpoint: &SshEndpoint, session: SshSession<'_>, script: &str) -> Command {
    let mut command = Command::new("ssh");
    append_ssh_options(&mut command, endpoint, session);
    command
        .arg("--")
        .arg(&endpoint.host)
        .arg("sh")
        .arg("-lc")
        .arg(shell_quote(script))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[cfg(windows)]
pub(crate) struct SshStderr {
    text: Arc<Mutex<String>>,
    drain: Option<thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl SshStderr {
    pub(crate) fn drain(stderr: ChildStderr) -> Self {
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

    pub(crate) fn settled(&mut self) -> String {
        if let Some(drain) = self.drain.take() {
            let _ = drain.join();
        }
        self.text.lock().clone()
    }
}

#[cfg(windows)]
pub(crate) struct SshForward {
    child: Child,
    local_socket: PathBuf,
    stopped: Arc<AtomicBool>,
    _askpass: Option<AskpassListener>,
}

#[cfg(windows)]
impl SshForward {
    pub(crate) fn start(endpoint: &SshEndpoint, login: SshLogin) -> Result<Self, EndpointError> {
        let (prompts, batch) = login.split();
        let askpass = start_askpass(endpoint, prompts);
        let session = SshSession {
            askpass: session_askpass(askpass.as_ref()),
            batch,
            ..SshSession::default()
        };
        let identity = zz_identity();
        let key_line = identity
            .as_ref()
            .filter(|_| configured_fleet_endpoint(endpoint))
            .map(|(_, line)| line.as_str());
        let script = remote_connect_script(endpoint.remote_socket.as_deref(), key_line);
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

        let mut phase = identity.as_ref().map(|(path, _)| path.as_path());
        loop {
            let session = SshSession {
                identity: phase,
                ..session
            };
            let mut child = ssh_connect_command(endpoint, session, &script)
                .spawn_unmasked()
                .map_err(|source| EndpointError::SshSpawn {
                    operation: "socket proxy",
                    source,
                })?;
            end_with_this_process(&child);
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
            let preamble = match await_proxy_ready(endpoint, &mut stdout, &local_socket) {
                Ok(ProxyStart::Ready(preamble)) => preamble,
                Ok(ProxyStart::Exited(preamble)) => {
                    let status = child.wait().ok();
                    let stderr = stderr.settled();
                    let code = status.and_then(|status| status.code());
                    if phase.is_some() && refused_login(code, stderr.as_bytes()) {
                        log::debug!(
                            target: "zz_daemon::diagnostics",
                            "the zz key did not sign in: {}",
                            stderr.trim(),
                        );
                        phase = None;
                        continue;
                    }
                    return Err(connect_failure(endpoint, status, &preamble, &stderr));
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            };
            let probe = parse_remote_probe_output(&endpoint.to_string(), &preamble);
            note_key_enrollment(endpoint, session, &probe);
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
            return Ok(Self {
                child,
                local_socket,
                stopped,
                _askpass: askpass,
            });
        }
    }

    pub(crate) fn local_socket(&self) -> &Path {
        &self.local_socket
    }

    #[allow(
        clippy::unused_self,
        reason = "the unix arm answers from its reserved port; both are called as SshForward::socks_port"
    )]
    pub(crate) const fn socks_port(&self) -> Option<u16> {
        None
    }
}

#[cfg(windows)]
pub(crate) enum ProxyStart {
    Ready(Vec<u8>),
    Exited(Vec<u8>),
}

#[cfg(windows)]
fn await_proxy_ready(
    endpoint: &SshEndpoint,
    stdout: &mut BufReader<ChildStdout>,
    local_socket: &Path,
) -> Result<ProxyStart, EndpointError> {
    await_proxy_ready_for(&endpoint.to_string(), stdout, local_socket)
}

#[cfg(windows)]
pub(crate) fn await_proxy_ready_for(
    target: &str,
    stdout: &mut BufReader<ChildStdout>,
    local_socket: &Path,
) -> Result<ProxyStart, EndpointError> {
    let mut preamble = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read =
            stdout
                .read_until(b'\n', &mut line)
                .map_err(|source| EndpointError::ForwardIo {
                    target: target.to_owned(),
                    action: "reading the proxy handshake for",
                    path: local_socket.to_owned(),
                    source,
                })?;
        if read == 0 {
            return Ok(ProxyStart::Exited(preamble));
        }
        if line.ends_with(PROXY_READY_MARKER) {
            return Ok(ProxyStart::Ready(preamble));
        }
        if !line.starts_with(b"zz-probe-") {
            log::debug!(
                target: "zz_daemon::diagnostics",
                "remote login preamble: {}",
                String::from_utf8_lossy(&line).trim_end(),
            );
        }
        preamble.extend_from_slice(&line);
        if preamble.len() > MAX_PROXY_PREAMBLE_BYTES {
            return Err(EndpointError::SshFailed {
                target: target.to_owned(),
                reason: "the remote kept writing to stdout instead of starting the zz proxy; \
                         check what its login shell prints"
                    .to_owned(),
            });
        }
    }
}

#[cfg(windows)]
fn connect_failure(
    endpoint: &SshEndpoint,
    status: Option<std::process::ExitStatus>,
    preamble: &[u8],
    stderr: &str,
) -> EndpointError {
    let target = endpoint.to_string();
    let code = status.and_then(|status| status.code());
    if code != Some(SSH_FAILURE_STATUS)
        && preamble
            .windows(b"zz-probe-protocol=".len())
            .any(|window| window == b"zz-probe-protocol=")
        && let Err(error) = parse_remote_probe_output(&target, preamble).socket
    {
        return error;
    }
    match code {
        Some(REMOTE_ZZ_MISSING_STATUS) => EndpointError::RemoteBinaryMissing { target },
        Some(REMOTE_DAEMON_TIMEOUT_STATUS) => EndpointError::RemoteDaemonUnavailable { target },
        _ => EndpointError::SshFailed {
            target,
            reason: ssh_failure_reason(
                stderr,
                &status.map_or_else(|| "no exit status".to_owned(), |status| status.to_string()),
            ),
        },
    }
}

#[cfg(windows)]
pub(crate) fn end_with_this_process(child: &Child) {
    use std::{os::windows::io::AsRawHandle as _, sync::OnceLock};

    use windows::Win32::{
        Foundation::HANDLE,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
    };

    static JOB: OnceLock<Option<usize>> = OnceLock::new();
    let job = JOB.get_or_init(|| {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).ok()?;
        #[allow(
            unsafe_code,
            reason = "job objects are raw Win32 entry points with no safe wrapper"
        )]
        let job = unsafe {
            let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).ok()?;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size,
            )
            .ok()?;
            job
        };
        Some(job.0 as usize)
    });
    let Some(job) = *job else {
        return;
    };
    #[allow(
        unsafe_code,
        reason = "AssignProcessToJobObject is a raw Win32 entry point with no safe wrapper"
    )]
    let assigned =
        unsafe { AssignProcessToJobObject(HANDLE(job as *mut _), HANDLE(child.as_raw_handle())) };
    if let Err(error) = assigned {
        log::debug!(
            target: "zz_daemon::diagnostics",
            "ssh will outlive zz if zz is killed: {error}",
        );
    }
}

#[cfg(windows)]
pub(crate) fn spawn_bridge(
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
mod parse_tests;
#[cfg(test)]
mod ssh_tests;
