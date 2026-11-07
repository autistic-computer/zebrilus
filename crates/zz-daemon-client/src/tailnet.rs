use std::{
    io::{self, Read},
    net::{IpAddr, Shutdown, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde_json::Value;
use thiserror::Error;

use crate::{
    endpoint::TailnetEndpoint,
    transport::{LocalStream, LocalTransport, Transport, TransportListener, TransportStream},
    unmasked::SpawnUnmasked as _,
};

pub const PORT: u16 = TailnetEndpoint::DEFAULT_PORT;
pub const MODE_KEY: &str = "tailnet";
pub const ALLOW_KEY: &str = "tailnet-allow";
pub const HIDE_KEY: &str = "tailnet-hide";

const CLI_TIMEOUT: Duration = Duration::from_secs(5);
const CLI_POLL: Duration = Duration::from_millis(10);
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Allow {
    #[default]
    Owner,
    Members,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    pub allow: Allow,
    pub hide: Vec<String>,
}

impl Settings {
    #[must_use]
    pub fn is_key(key: &str) -> bool {
        matches!(key, MODE_KEY | ALLOW_KEY | HIDE_KEY)
    }

    pub fn apply(&mut self, key: &str, value: &str) -> Result<(), String> {
        match (key, value) {
            (MODE_KEY, "auto") => self.mode = Mode::Auto,
            (MODE_KEY, "on") => self.mode = Mode::On,
            (MODE_KEY, "off") => self.mode = Mode::Off,
            (MODE_KEY, _) => return Err("expected auto, on or off".to_owned()),
            (ALLOW_KEY, "owner") => self.allow = Allow::Owner,
            (ALLOW_KEY, "members") => self.allow = Allow::Members,
            (ALLOW_KEY, _) => return Err("expected owner or members".to_owned()),
            (HIDE_KEY, names) => {
                self.hide = names
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            _ => return Err(format!("`{key}` is not a tailnet key")),
        }
        Ok(())
    }

    #[must_use]
    pub fn from_config(source: &str) -> Self {
        let mut settings = Self::default();
        for line in source.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if Self::is_key(key) {
                let value = value.split('#').next().unwrap_or_default().trim();
                let _ = settings.apply(key, value);
            }
        }
        settings
    }
}

impl Mode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

impl Allow {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Members => "members",
        }
    }
}

#[derive(Debug, Error)]
pub enum TailnetError {
    #[error("no working tailscale command ({0})")]
    Cli(String),
    #[error("tailscale printed something zz could not read: {0}")]
    Parse(String),
    #[error("{0}")]
    Unusable(String),
    #[error("no tailnet peer named `{0}`")]
    UnknownPeer(String),
    #[error("nothing answers at {target} ({reason}); is zz running there with tailnet on?")]
    Unreachable { target: SocketAddr, reason: String },
    #[error("{0}")]
    Io(#[from] io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub backend: String,
    pub tun: bool,
    pub user_id: u64,
    pub login: String,
    pub host: String,
    pub tags: Vec<String>,
    pub tailnet: String,
    pub addresses: Vec<IpAddr>,
    pub peers: Vec<Peer>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub host: String,
    pub dns_name: String,
    pub os: String,
    pub online: bool,
    pub user_id: u64,
    pub login: String,
    pub tags: Vec<String>,
    pub addresses: Vec<IpAddr>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caller {
    pub host: String,
    pub user_id: u64,
    pub login: String,
    pub tags: Vec<String>,
}

impl Status {
    #[must_use]
    pub fn unusable(&self) -> Option<String> {
        if self.backend != "Running" {
            return Some(format!("Tailscale is {}", self.backend));
        }
        if !self.tun {
            return Some(
                "Tailscale runs in userspace-networking mode, so this machine has no tailnet address to listen on"
                    .to_owned(),
            );
        }
        if self.user_id == 0 {
            return Some("Tailscale reported no account".to_owned());
        }
        if self.addresses.is_empty() {
            return Some("Tailscale reported no tailnet address".to_owned());
        }
        None
    }

    #[must_use]
    pub fn is_personal(&self) -> bool {
        self.tailnet.contains('@')
    }

    #[must_use]
    pub fn peer(&self, name: &str) -> Option<&Peer> {
        let name = name.trim_end_matches('.');
        if let Ok(address) = name.parse::<IpAddr>() {
            return self
                .peers
                .iter()
                .find(|peer| peer.addresses.contains(&address));
        }
        self.peers
            .iter()
            .find(|peer| {
                let dns_name = peer.dns_name.trim_end_matches('.');
                dns_name.eq_ignore_ascii_case(name)
                    || dns_name
                        .split('.')
                        .next()
                        .is_some_and(|label| label.eq_ignore_ascii_case(name))
            })
            .or_else(|| {
                self.peers
                    .iter()
                    .find(|peer| peer.host.eq_ignore_ascii_case(name))
            })
    }
}

pub fn admit(allow: Allow, me: &Status, caller: &Caller, from: IpAddr) -> Result<(), String> {
    if me.addresses.contains(&from) {
        return Err("it comes from this machine's own tailnet address".to_owned());
    }
    if caller.user_id == 0 && caller.tags.is_empty() {
        return Err("Tailscale did not say who it is".to_owned());
    }
    match allow {
        Allow::Members => Ok(()),
        Allow::Owner if me.is_personal() => {
            if !caller.tags.is_empty() || caller.login == me.tailnet {
                Ok(())
            } else {
                Err(format!(
                    "{} is not the owner of the tailnet {}",
                    caller.login, me.tailnet
                ))
            }
        }
        Allow::Owner => {
            if !me.tags.is_empty() {
                Err(
                    "this machine is tagged, so it has no owner to match; set tailnet-allow = members"
                        .to_owned(),
                )
            } else if !caller.tags.is_empty() {
                Err("it is a tagged device".to_owned())
            } else if caller.user_id == me.user_id {
                Ok(())
            } else {
                Err(format!("it belongs to {}", caller.login))
            }
        }
    }
}

pub fn parse_status(json: &str) -> Result<Status, TailnetError> {
    let root: Value =
        serde_json::from_str(json).map_err(|error| TailnetError::Parse(error.to_string()))?;
    let me = &root["Self"];
    let user_id = me["UserID"].as_u64().unwrap_or_default();
    let login_of = |id: u64| {
        root["User"][id.to_string()]["LoginName"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let peers = root["Peer"]
        .as_object()
        .map(|peers| {
            peers
                .values()
                .map(|peer| {
                    let user_id = peer["UserID"].as_u64().unwrap_or_default();
                    Peer {
                        id: text(&peer["ID"]),
                        host: text(&peer["HostName"]),
                        dns_name: text(&peer["DNSName"]),
                        os: text(&peer["OS"]),
                        online: peer["Online"].as_bool().unwrap_or_default(),
                        user_id,
                        login: login_of(user_id),
                        tags: strings(&peer["Tags"]),
                        addresses: addresses(&peer["TailscaleIPs"]),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Status {
        backend: text(&root["BackendState"]),
        tun: root["TUN"].as_bool().unwrap_or_default(),
        user_id,
        login: login_of(user_id),
        host: text(&me["HostName"]),
        tags: strings(&me["Tags"]),
        tailnet: text(&root["CurrentTailnet"]["Name"]),
        addresses: addresses(&root["TailscaleIPs"]),
        peers,
    })
}

pub fn parse_whois(json: &str) -> Result<Caller, TailnetError> {
    let root: Value =
        serde_json::from_str(json).map_err(|error| TailnetError::Parse(error.to_string()))?;
    Ok(Caller {
        host: text(&root["Node"]["ComputedName"]),
        user_id: root["UserProfile"]["ID"].as_u64().unwrap_or_default(),
        login: text(&root["UserProfile"]["LoginName"]),
        tags: strings(&root["Node"]["Tags"]),
    })
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn addresses(value: &Value) -> Vec<IpAddr> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|address| address.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn status() -> Result<Status, TailnetError> {
    parse_status(&run_cli(&["status", "--json"])?)
}

pub fn whois(address: IpAddr) -> Result<Caller, TailnetError> {
    parse_whois(&run_cli(&["whois", "--json", &address.to_string()])?)
}

struct Candidate {
    program: PathBuf,
    be_cli: bool,
}

fn candidates() -> Vec<Candidate> {
    let mut candidates = vec![Candidate {
        program: PathBuf::from("tailscale"),
        be_cli: false,
    }];
    #[cfg(target_os = "macos")]
    {
        for program in ["/opt/homebrew/bin/tailscale", "/usr/local/bin/tailscale"] {
            candidates.push(Candidate {
                program: PathBuf::from(program),
                be_cli: false,
            });
        }
        candidates.push(Candidate {
            program: PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale"),
            be_cli: true,
        });
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    for program in [
        "/usr/bin/tailscale",
        "/usr/local/bin/tailscale",
        "/snap/bin/tailscale",
    ] {
        candidates.push(Candidate {
            program: PathBuf::from(program),
            be_cli: false,
        });
    }
    #[cfg(windows)]
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(Candidate {
            program: PathBuf::from(program_files).join(r"Tailscale\tailscale.exe"),
            be_cli: false,
        });
    }
    candidates
}

fn run_cli(arguments: &[&str]) -> Result<String, TailnetError> {
    let mut failures = Vec::new();
    for candidate in candidates() {
        match run_candidate(&candidate, arguments) {
            Ok(output) => return Ok(output),
            Err(reason) => failures.push(format!("{}: {reason}", candidate.program.display())),
        }
    }
    Err(TailnetError::Cli(failures.join("; ")))
}

fn run_candidate(candidate: &Candidate, arguments: &[&str]) -> Result<String, String> {
    let mut command = Command::new(&candidate.program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if candidate.be_cli {
        command.env("TAILSCALE_BE_CLI", "1");
    }
    let mut child = command
        .spawn_unmasked()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => "not found".to_owned(),
            _ => error.to_string(),
        })?;
    let stdout = read_in_background(child.stdout.take());
    let stderr = read_in_background(child.stderr.take());
    let status = wait_bounded(&mut child)?;
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        Err(stderr
            .lines()
            .find(|line| !line.starts_with("Warning:") && !line.trim().is_empty())
            .unwrap_or("failed")
            .to_owned())
    }
}

fn read_in_background(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    })
}

fn wait_bounded(child: &mut Child) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("no answer within {}s", CLI_TIMEOUT.as_secs()));
        }
        thread::sleep(CLI_POLL);
    }
}

pub fn bridge(tcp: TcpStream, local: LocalStream) -> io::Result<()> {
    let _ = tcp.set_nodelay(true);
    let mut tcp_read = tcp.try_clone()?;
    let mut local_write = local.try_clone()?;
    let upstream = thread::Builder::new()
        .name("zz-tailnet-pump".to_owned())
        .spawn(move || {
            let _ = io::copy(&mut tcp_read, &mut local_write);
            let _ = TransportStream::shutdown(&local_write);
        })?;
    let mut local_read = local;
    let mut tcp_write = tcp;
    let _ = io::copy(&mut local_read, &mut tcp_write);
    let _ = tcp_write.shutdown(Shutdown::Both);
    let _ = upstream.join();
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovered {
    pub id: String,
    pub label: String,
    pub os: String,
    pub endpoint: TailnetEndpoint,
}

impl Peer {
    #[must_use]
    pub fn label(&self) -> &str {
        self.dns_name
            .split('.')
            .next()
            .filter(|label| !label.is_empty())
            .unwrap_or(&self.host)
    }

    fn dial_address(&self) -> Option<IpAddr> {
        self.addresses
            .iter()
            .find(|address| address.is_ipv4())
            .or_else(|| self.addresses.first())
            .copied()
    }
}

#[must_use]
pub fn mine(status: &Status) -> Vec<&Peer> {
    status
        .peers
        .iter()
        .filter(|peer| {
            let caller = Caller {
                host: peer.host.clone(),
                user_id: peer.user_id,
                login: peer.login.clone(),
                tags: peer.tags.clone(),
            };
            peer.online
                && peer
                    .dial_address()
                    .is_some_and(|address| admit(Allow::Owner, status, &caller, address).is_ok())
        })
        .collect()
}

#[must_use]
pub fn discover(status: &Status) -> Vec<Discovered> {
    let candidates = mine(status);
    let mut found: Vec<Discovered> = thread::scope(|scope| {
        let probes: Vec<_> = candidates
            .into_iter()
            .map(|peer| {
                scope.spawn(move || {
                    let address = SocketAddr::new(peer.dial_address()?, PORT);
                    TcpStream::connect_timeout(&address, PROBE_TIMEOUT).ok()?;
                    Some(Discovered {
                        id: peer.id.clone(),
                        label: peer.label().to_owned(),
                        os: peer.os.clone(),
                        endpoint: TailnetEndpoint {
                            peer: peer.dns_name.trim_end_matches('.').to_owned(),
                            port: None,
                        },
                    })
                })
            })
            .collect();
        probes
            .into_iter()
            .filter_map(|probe| probe.join().ok().flatten())
            .collect()
    });
    found.sort_by(|left, right| left.label.cmp(&right.label));
    found
}

pub fn resolve(endpoint: &TailnetEndpoint) -> Result<SocketAddr, TailnetError> {
    let port = endpoint.port.unwrap_or(PORT);
    if let Ok(address) = endpoint.peer.parse::<IpAddr>() {
        return Ok(SocketAddr::new(address, port));
    }
    let status = status()?;
    if let Some(reason) = status.unusable() {
        return Err(TailnetError::Unusable(reason));
    }
    let peer = status
        .peer(&endpoint.peer)
        .ok_or_else(|| TailnetError::UnknownPeer(endpoint.peer.clone()))?;
    let address = peer
        .addresses
        .iter()
        .find(|address| address.is_ipv4())
        .or_else(|| peer.addresses.first())
        .ok_or_else(|| TailnetError::UnknownPeer(endpoint.peer.clone()))?;
    Ok(SocketAddr::new(*address, port))
}

pub struct TailnetForward {
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    _directory: Option<tempfile::TempDir>,
}

impl TailnetForward {
    pub fn start(endpoint: &TailnetEndpoint) -> Result<Self, TailnetError> {
        let target = resolve(endpoint)?;
        let first = TcpStream::connect_timeout(&target, DIAL_TIMEOUT).map_err(|error| {
            TailnetError::Unreachable {
                target,
                reason: error.to_string(),
            }
        })?;
        let first = Arc::new(parking_lot::Mutex::new(Some(first)));
        let (directory, socket) = forward_socket()?;
        let listener = LocalTransport::bind(&socket)?;
        let stop = Arc::new(AtomicBool::new(false));
        let accept_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("zz-tailnet-forward".to_owned())
            .spawn(move || {
                while let Ok(local) = listener.accept() {
                    if accept_stop.load(Ordering::Acquire) {
                        break;
                    }
                    let dialed = first.lock().take();
                    let _ = thread::Builder::new()
                        .name("zz-tailnet-dial".to_owned())
                        .spawn(move || {
                            let tcp = dialed.map_or_else(
                                || TcpStream::connect_timeout(&target, DIAL_TIMEOUT),
                                Ok,
                            );
                            match tcp {
                                Ok(tcp) => {
                                    let _ = bridge(tcp, local);
                                }
                                Err(error) => {
                                    log::debug!(
                                        "could not reach {target} over the tailnet: {error}"
                                    );
                                }
                            }
                        });
                }
            })?;
        Ok(Self {
            socket,
            stop,
            thread: Some(thread),
            _directory: directory,
        })
    }

    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for TailnetForward {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = LocalTransport::connect(&self.socket);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(unix)]
fn forward_socket() -> io::Result<(Option<tempfile::TempDir>, PathBuf)> {
    let directory = tempfile::Builder::new().prefix("zz-tn-").tempdir()?;
    let socket = directory.path().join("s");
    Ok((Some(directory), socket))
}

#[cfg(windows)]
fn forward_socket() -> io::Result<(Option<tempfile::TempDir>, PathBuf)> {
    let mut entropy = [0_u8; 8];
    getrandom::fill(&mut entropy).map_err(io::Error::other)?;
    Ok((
        None,
        PathBuf::from(format!(
            r"\\.\pipe\zz-tailnet-{:016x}",
            u64::from_le_bytes(entropy)
        )),
    ))
}

#[cfg(test)]
#[path = "tailnet_tests.rs"]
mod tests;
