use std::{
    collections::HashMap,
    io,
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use parking_lot::{Mutex, RwLock};
use zz_daemon_client::{
    tailnet::{self, Allow, Mode, PORT, Settings, Status},
    transport::{LocalTransport, Transport as _},
};

const RECHECK: Duration = Duration::from_secs(30);
const STOP_POLL: Duration = Duration::from_millis(250);
const ACCEPT_POLL: Duration = Duration::from_millis(100);

type Current = Arc<RwLock<Option<(Status, Allow)>>>;
type Live = Arc<Mutex<HashMap<u64, TcpStream>>>;

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);
static LATEST: RwLock<Option<Status>> = RwLock::new(None);
static WAKE: AtomicBool = AtomicBool::new(false);

const USAGE: &str =
    "usage: tailnet [hosts | status | login [AUTH-KEY] | logout | allow [NAME] | disallow NAME]";

pub(crate) fn command(arguments: &[String]) -> Result<String, String> {
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["hosts"] => offers_output(),
        ["status"] => Ok(own::state(&read_settings()).to_line()),
        ["login"] => own::login(None),
        ["login", key] => own::login(Some((*key).to_owned())),
        ["logout"] => Ok(own::logout()),
        ["allow"] => own::allowed(),
        ["allow", name] => own::allow(name),
        ["disallow", name] => own::disallow(name),
        _ => Err(USAGE.to_owned()),
    }
}

fn offers_output() -> Result<String, String> {
    if read_settings().mode == Mode::Own {
        return Err(
            "this machine uses zz's own Tailscale node, which cannot list your machines; add them by name"
                .to_owned(),
        );
    }
    LATEST
        .read()
        .as_ref()
        .map(|status| zz_daemon_client::tailnet_offer::offers_json(&tailnet::offers(status)))
        .ok_or_else(|| {
            "this machine is not on the tailnet; run `zz tailnet status` on it to see why"
                .to_owned()
        })
}

struct Listening {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    live: Live,
}

pub(crate) fn spawn(
    socket: PathBuf,
    default_socket: bool,
    stopping: impl Fn() -> bool + Send + 'static,
) -> io::Result<()> {
    thread::Builder::new()
        .name("zz-tailnet".to_owned())
        .spawn(move || supervise(&socket, default_socket, &stopping))
        .map(|_| ())
}

fn supervise(socket: &Path, default_socket: bool, stopping: &dyn Fn() -> bool) {
    let current: Current = Arc::new(RwLock::new(None));
    let mut listening: HashMap<SocketAddr, Listening> = HashMap::new();
    let mut reported = String::new();
    while !stopping() {
        let settings = read_settings();
        let own_report = own::step(&settings, socket);
        let wanted = match settings.mode {
            Mode::Off => Err("tailnet = off".to_owned()),
            Mode::Own => Err("tailnet = own".to_owned()),
            Mode::Auto if !default_socket => {
                Err("only the daemon on the default socket listens while tailnet = auto".to_owned())
            }
            Mode::Auto | Mode::On => tailnet::status()
                .map_err(|error| error.to_string())
                .and_then(|status| status.unusable().map_or(Ok(status), Err)),
        };
        let report = match wanted {
            Ok(status) => {
                let addresses: Vec<SocketAddr> = status
                    .addresses
                    .iter()
                    .map(|address| SocketAddr::new(*address, PORT))
                    .collect();
                *LATEST.write() = Some(status.clone());
                *current.write() = Some((status, settings.allow));
                listening.retain(|address, listener| {
                    let keep = addresses.contains(address);
                    if !keep {
                        close(listener);
                    }
                    keep
                });
                let mut failures = Vec::new();
                for address in addresses {
                    if listening.contains_key(&address) {
                        continue;
                    }
                    match listen(address, socket.to_path_buf(), Arc::clone(&current)) {
                        Ok(listener) => {
                            listening.insert(address, listener);
                        }
                        Err(error) => failures.push(format!("{address}: {error}")),
                    }
                }
                let mut bound: Vec<String> = listening.keys().map(ToString::to_string).collect();
                bound.sort();
                if failures.is_empty() {
                    format!(
                        "listening on the tailnet at {} (tailnet-allow = {})",
                        bound.join(", "),
                        settings.allow.as_str()
                    )
                } else {
                    format!(
                        "listening on the tailnet at [{}]; could not listen on {}",
                        bound.join(", "),
                        failures.join("; ")
                    )
                }
            }
            Err(reason) => {
                *LATEST.write() = None;
                *current.write() = None;
                for (_, mut listener) in listening.drain() {
                    close(&mut listener);
                }
                format!("not listening on the tailnet: {reason}")
            }
        };
        let report = own_report.unwrap_or(report);
        if report != reported {
            log::info!("{report}");
            reported = report;
        }
        let deadline = Instant::now() + RECHECK;
        while Instant::now() < deadline && !stopping() && !WAKE.swap(false, Ordering::AcqRel) {
            thread::sleep(STOP_POLL);
        }
    }
    for (_, mut listener) in listening.drain() {
        close(&mut listener);
    }
    own::stop();
}

fn read_settings() -> Settings {
    zz_config::discover_config_path(&zz_config::config_candidates())
        .and_then(|path| zz_config::read_config_source(&path).ok())
        .map(|source| Settings::from_config(&source))
        .unwrap_or_default()
}

fn listen(address: SocketAddr, socket: PathBuf, current: Current) -> io::Result<Listening> {
    let listener = TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let accept_stop = Arc::clone(&stop);
    let live: Live = Arc::default();
    let accept_live = Arc::clone(&live);
    let thread = thread::Builder::new()
        .name("zz-tailnet-accept".to_owned())
        .spawn(move || {
            while !accept_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if stream.set_nonblocking(false).is_err() {
                            continue;
                        }
                        let socket = socket.clone();
                        let current = Arc::clone(&current);
                        let live = Arc::clone(&accept_live);
                        let _ = thread::Builder::new()
                            .name("zz-tailnet-client".to_owned())
                            .spawn(move || serve(stream, &socket, &current, &live));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(ACCEPT_POLL);
                    }
                    Err(error) => {
                        log::warn!("stopped listening on the tailnet at {address}: {error}");
                        break;
                    }
                }
            }
        })?;
    Ok(Listening {
        stop,
        thread: Some(thread),
        live,
    })
}

fn close(listener: &mut Listening) {
    listener.stop.store(true, Ordering::Release);
    if let Some(thread) = listener.thread.take() {
        let _ = thread.join();
    }
    for (_, stream) in listener.live.lock().drain() {
        let _ = stream.shutdown(Shutdown::Both);
    }
}

fn serve(stream: TcpStream, socket: &Path, current: &Current, live: &Live) {
    let Ok(peer) = stream.peer_addr() else {
        return;
    };
    let Some((me, allow)) = current.read().clone() else {
        return;
    };
    let from = unmapped(peer.ip());
    if me.addresses.contains(&from) {
        log::debug!("closed a tailnet connection from this machine's own address {peer}");
        return;
    }
    let caller = match tailnet::whois(from) {
        Ok(caller) => caller,
        Err(error) => {
            log::warn!(
                "refused a tailnet connection from {peer}: Tailscale could not say who it is ({error})"
            );
            return;
        }
    };
    if let Err(reason) = tailnet::admit(allow, &me, &caller, from) {
        log::warn!(
            "refused a tailnet connection from {} ({}, {peer}): {reason}",
            caller.host,
            caller.login
        );
        return;
    }
    let local = match LocalTransport::connect(socket) {
        Ok(local) => local,
        Err(error) => {
            log::warn!("could not hand {} to the daemon: {error}", caller.host);
            return;
        }
    };
    log::info!(
        "tailnet client {} ({}) connected from {peer}",
        caller.host,
        caller.login
    );
    let id = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
    if let Ok(handle) = stream.try_clone() {
        live.lock().insert(id, handle);
    }
    let _ = tailnet::bridge(stream, local);
    live.lock().remove(&id);
}

fn unmapped(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        IpAddr::V4(_) => address,
    }
}

#[cfg(all(
    feature = "tailnet-own",
    any(target_os = "linux", all(target_os = "macos", target_arch = "aarch64"))
))]
mod own {
    use std::{
        fmt::Write as _,
        io,
        path::{Path, PathBuf},
        sync::{Arc, atomic::Ordering},
    };

    use parking_lot::{Mutex, RwLock};
    use zz_daemon_client::tailnet::{self, Allow, Mode, OwnState, PORT, Settings};
    use zz_tailnet::{Node, Options, Peer};

    use super::{WAKE, read_settings};

    static NODE: RwLock<Option<Arc<Node>>> = RwLock::new(None);
    static LOGIN_KEY: Mutex<Option<String>> = Mutex::new(None);
    static LOGOUT: Mutex<bool> = Mutex::new(false);

    fn wake() {
        WAKE.store(true, Ordering::Release);
    }

    fn own_directory() -> Option<PathBuf> {
        zz_daemon_client::user_data::platform_data_dir().map(|data| {
            data.join(zz_protocol::app_identity::DIRECTORY)
                .join("tailnet")
        })
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Allowed {
        id: String,
        name: String,
    }

    fn read_allowed(path: &Path) -> Vec<Allowed> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let (id, name) = line.trim().split_once(' ')?;
                Some(Allowed {
                    id: id.to_owned(),
                    name: name.trim().to_owned(),
                })
            })
            .collect()
    }

    fn write_allowed(path: &Path, allowed: &[Allowed]) -> io::Result<()> {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
            zz_daemon_client::user_data::restrict_directory_to_current_user(directory)?;
        }
        let mut text = String::new();
        for entry in allowed {
            let _ = writeln!(text, "{} {}", entry.id, entry.name);
        }
        let staged = path.with_extension("new");
        std::fs::write(&staged, text)?;
        zz_daemon_client::user_data::restrict_to_current_user(&staged)?;
        std::fs::rename(staged, path)
    }

    fn matches_entry(entry: &Allowed, name: &str) -> bool {
        entry.id == name || entry.name == name || entry.name.split('.').next() == Some(name)
    }

    pub(super) fn step(settings: &Settings, socket: &Path) -> Option<String> {
        if std::mem::take(&mut *LOGOUT.lock()) {
            stop();
            if let Some(directory) = own_directory() {
                let keys = directory.join("keys.json");
                match std::fs::remove_file(&keys) {
                    Ok(()) => log::info!("signed zz's own node out and deleted {}", keys.display()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => log::warn!("could not delete {}: {error}", keys.display()),
                }
            }
        }
        if settings.mode != Mode::Own {
            stop();
            return None;
        }
        let key = LOGIN_KEY.lock().take();
        let running = NODE
            .read()
            .as_ref()
            .is_some_and(|node| matches!(node.state(), OwnState::Running { .. }));
        if key.is_some() && !running {
            stop();
        }
        if NODE.read().is_none() {
            match start(settings, socket, key) {
                Ok(node) => *NODE.write() = Some(Arc::new(node)),
                Err(error) => return Some(format!("zz's own node did not start: {error}")),
            }
        }
        NODE.read().as_ref().map(|node| node.state().describe())
    }

    fn start(settings: &Settings, socket: &Path, auth_key: Option<String>) -> Result<Node, String> {
        let directory = own_directory()
            .ok_or_else(|| "no application-data directory for its keys".to_owned())?;
        let hostname = settings.hostname.clone().unwrap_or_else(|| {
            format!(
                "zz-{}",
                zz_daemon_client::short_device_name().unwrap_or_else(|| "machine".to_owned())
            )
        });
        let allowed_path = directory.join("allow");
        Node::start(Options {
            keys: directory.join("keys.json"),
            hostname,
            control_url: settings.control_url.clone(),
            auth_key,
            port: PORT,
            daemon: socket.to_path_buf(),
            dial: tailnet::dial_socket(socket),
            admit: Arc::new(move |peer: &Peer| match read_settings().allow {
                Allow::Members => Ok(()),
                Allow::Owner => {
                    if read_allowed(&allowed_path).iter().any(|entry| entry.id == peer.id) {
                        Ok(())
                    } else {
                        Err(format!(
                            "not on the allow list; run `zz tailnet allow {}` on this machine to let it in",
                            peer.name
                        ))
                    }
                }
            }),
        })
        .map_err(|error| error.to_string())
    }

    pub(super) fn stop() {
        let node = NODE.write().take();
        drop(node);
    }

    pub(super) fn state(settings: &Settings) -> OwnState {
        if let Some(node) = NODE.read().as_ref() {
            return node.state();
        }
        if settings.mode == Mode::Own {
            OwnState::Starting
        } else {
            OwnState::Off(format!("tailnet = {}", settings.mode.as_str()))
        }
    }

    pub(super) fn login(key: Option<String>) -> Result<String, String> {
        if read_settings().mode != Mode::Own {
            return Err("set tailnet = own in zz/config first".to_owned());
        }
        if key.is_some() {
            *LOGIN_KEY.lock() = key;
        }
        wake();
        Ok(state(&read_settings()).to_line())
    }

    pub(super) fn logout() -> String {
        *LOGOUT.lock() = true;
        wake();
        "signing zz's own node out".to_owned()
    }

    fn allowed_path() -> Result<std::path::PathBuf, String> {
        own_directory()
            .map(|directory| directory.join("allow"))
            .ok_or_else(|| "no application-data directory for the allow list".to_owned())
    }

    pub(super) fn allowed() -> Result<String, String> {
        let allowed = read_allowed(&allowed_path()?);
        if allowed.is_empty() {
            return Ok(
                "nobody yet; run `zz tailnet allow NAME` for each machine that may attach"
                    .to_owned(),
            );
        }
        Ok(allowed
            .iter()
            .map(|entry| format!("{} ({})", entry.name, entry.id))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    pub(super) fn allow(name: &str) -> Result<String, String> {
        let node = NODE.read().clone().ok_or_else(|| {
            "zz's own node is not running; set tailnet = own and run `zz tailnet login`".to_owned()
        })?;
        let peer = node.peer(name)?;
        let path = allowed_path()?;
        let mut allowed = read_allowed(&path);
        allowed.retain(|entry| entry.id != peer.id);
        allowed.push(Allowed {
            id: peer.id.clone(),
            name: peer.name.clone(),
        });
        write_allowed(&path, &allowed).map_err(|error| error.to_string())?;
        Ok(format!("allowed {} ({})", peer.name, peer.id))
    }

    pub(super) fn disallow(name: &str) -> Result<String, String> {
        let path = allowed_path()?;
        let mut allowed = read_allowed(&path);
        let before = allowed.len();
        allowed.retain(|entry| !matches_entry(entry, name));
        if allowed.len() == before {
            return Err(format!("`{name}` is not on the allow list"));
        }
        write_allowed(&path, &allowed).map_err(|error| error.to_string())?;
        Ok(format!("removed {name} from the allow list"))
    }
}

#[cfg(not(all(
    feature = "tailnet-own",
    any(target_os = "linux", all(target_os = "macos", target_arch = "aarch64"))
)))]
mod own {
    use std::path::Path;

    use zz_daemon_client::tailnet::{Mode, OwnState, Settings};

    const MISSING: &str =
        "zz's own Tailscale node is not built into this zz; it runs on Linux and arm64 macOS";

    pub(super) fn step(settings: &Settings, _: &Path) -> Option<String> {
        (settings.mode == Mode::Own).then(|| MISSING.to_owned())
    }

    pub(super) fn stop() {}

    pub(super) fn state(settings: &Settings) -> OwnState {
        if settings.mode == Mode::Own {
            OwnState::Failed(MISSING.to_owned())
        } else {
            OwnState::Off(format!("tailnet = {}", settings.mode.as_str()))
        }
    }

    pub(super) fn login(_: Option<String>) -> Result<String, String> {
        Err(MISSING.to_owned())
    }

    pub(super) fn logout() -> String {
        "zz's own node is not built into this zz; nothing to sign out".to_owned()
    }

    pub(super) fn allowed() -> Result<String, String> {
        Err(MISSING.to_owned())
    }

    pub(super) fn allow(_: &str) -> Result<String, String> {
        Err(MISSING.to_owned())
    }

    pub(super) fn disallow(_: &str) -> Result<String, String> {
        Err(MISSING.to_owned())
    }
}
