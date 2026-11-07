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

pub(crate) fn offers_output() -> Result<String, String> {
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
        let wanted = match settings.mode {
            Mode::Off => Err("tailnet = off".to_owned()),
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
        if report != reported {
            log::info!("{report}");
            reported = report;
        }
        let deadline = Instant::now() + RECHECK;
        while Instant::now() < deadline && !stopping() {
            thread::sleep(STOP_POLL);
        }
    }
    for (_, mut listener) in listening.drain() {
        close(&mut listener);
    }
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
