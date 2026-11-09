#![cfg(any(target_os = "linux", all(target_os = "macos", target_arch = "aarch64")))]

use std::{
    fs, io,
    net::{IpAddr, SocketAddr},
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use parking_lot::Mutex;
use tailscale::{AuthState, Config, Device, NodeInfo, keys::PersistState};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    net::{UnixListener, UnixStream},
    runtime::Runtime,
    sync::OnceCell,
    task::JoinSet,
    time::timeout,
};
pub use zz_daemon_client::tailnet::OwnState as State;

const LOGIN_POLL: Duration = Duration::from_secs(2);
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_LIMIT: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub addresses: Vec<IpAddr>,
}

pub type Admit = Arc<dyn Fn(&Peer) -> Result<(), String> + Send + Sync>;

pub struct Options {
    pub keys: PathBuf,
    pub hostname: String,
    pub control_url: Option<String>,
    pub auth_key: Option<String>,
    pub port: u16,
    pub daemon: PathBuf,
    pub dial: PathBuf,
    pub admit: Admit,
}

#[derive(Clone)]
struct Shared {
    state: Arc<Mutex<State>>,
    device: Arc<OnceCell<Device>>,
}

impl Shared {
    fn set(&self, state: State) {
        *self.state.lock() = state;
    }

    fn running(&self) -> Option<(&Device, Vec<IpAddr>)> {
        let State::Running { addresses, .. } = self.state.lock().clone() else {
            return None;
        };
        self.device.get().map(|device| (device, addresses))
    }
}

pub struct Node {
    runtime: Option<Runtime>,
    shared: Shared,
    dial: PathBuf,
}

impl Node {
    pub fn start(options: Options) -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("zz-tailnet-node")
            .enable_all()
            .build()?;
        let shared = Shared {
            state: Arc::new(Mutex::new(State::Starting)),
            device: Arc::new(OnceCell::new()),
        };
        let _ = fs::remove_file(&options.dial);
        let dial = {
            let _guard = runtime.enter();
            UnixListener::bind(&options.dial)?
        };
        fs::set_permissions(&options.dial, fs::Permissions::from_mode(0o600))?;
        let dial_path = options.dial.clone();
        runtime.spawn(serve_dials(dial, shared.clone()));
        runtime.spawn(run(options, shared.clone()));
        Ok(Self {
            runtime: Some(runtime),
            shared,
            dial: dial_path,
        })
    }

    pub fn state(&self) -> State {
        self.shared.state.lock().clone()
    }

    pub fn peer(&self, name: &str) -> Result<Peer, String> {
        let Some(runtime) = &self.runtime else {
            return Err("zz's own node is stopped".to_owned());
        };
        let Some((device, _)) = self.shared.running() else {
            return Err(format!(
                "zz's own node is not signed in ({})",
                self.state().describe()
            ));
        };
        runtime.block_on(async {
            let full = if name.contains('.') {
                name.to_owned()
            } else {
                let me = device
                    .self_node()
                    .await
                    .map_err(|error| error.to_string())?;
                me.tailnet
                    .map_or_else(|| name.to_owned(), |tailnet| format!("{name}.{tailnet}"))
            };
            match timeout(LOOKUP_TIMEOUT, device.peer_by_name(&full)).await {
                Ok(Ok(Some(node))) => Ok(peer(&node)),
                Ok(Ok(None)) => Err(format!("no machine named `{full}` on the tailnet")),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("zz's own node took too long to answer".to_owned()),
            }
        })
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(SHUTDOWN_TIMEOUT);
        }
        let _ = fs::remove_file(&self.dial);
    }
}

pub fn read_keys(path: &Path) -> io::Result<PersistState> {
    #[derive(serde::Deserialize)]
    struct KeyFile {
        key_state: PersistState,
    }
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<KeyFile>(&bytes)
            .map(|file| file.key_state)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let state = PersistState::default();
            write_keys(path, &state)?;
            Ok(state)
        }
        Err(error) => Err(error),
    }
}

fn write_keys(path: &Path, state: &PersistState) -> io::Result<()> {
    #[derive(serde::Serialize)]
    struct KeyFile<'a> {
        key_state: &'a PersistState,
    }
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("the key file has no directory"))?;
    fs::create_dir_all(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let bytes = serde_json::to_vec(&KeyFile { key_state: state }).map_err(io::Error::other)?;
    let staged = path.with_extension("json.new");
    let _ = fs::remove_file(&staged);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&staged)?;
    io::Write::write_all(&mut file, &bytes)?;
    file.sync_all()?;
    fs::rename(&staged, path)
}

fn peer(node: &NodeInfo) -> Peer {
    Peer {
        id: node.stable_id.0.clone(),
        name: node.fqdn(false),
        addresses: vec![
            IpAddr::V4(node.tailnet_address.ipv4.addr()),
            IpAddr::V6(node.tailnet_address.ipv6.addr()),
        ],
    }
}

async fn run(options: Options, shared: Shared) {
    let device = match open(&options).await {
        Ok(device) => device,
        Err(reason) => {
            log::warn!("zz's own node did not start: {reason}");
            shared.set(State::Failed(reason));
            return;
        }
    };
    let device = shared.device.get_or_init(|| async { device }).await;
    loop {
        match device.is_authorized().await {
            Ok(AuthState::Authorized) => break,
            Ok(AuthState::NotAuthorized(url)) => {
                let url = url.to_string();
                if *shared.state.lock() != State::NeedsLogin(url.clone()) {
                    log::info!("zz's own node needs signing in at {url}");
                    shared.set(State::NeedsLogin(url));
                }
            }
            Err(error) => {
                log::warn!("zz's own node could not sign in: {error}");
                shared.set(State::Failed(error.to_string()));
                return;
            }
        }
        tokio::time::sleep(LOGIN_POLL).await;
    }
    let (v4, v6) = match (device.ipv4_addr().await, device.ipv6_addr().await) {
        (Ok(v4), Ok(v6)) => (IpAddr::V4(v4), IpAddr::V6(v6)),
        (Err(error), _) | (_, Err(error)) => {
            shared.set(State::Failed(format!("no tailnet address: {error}")));
            return;
        }
    };
    let (name, key) = device.self_node().await.map_or_else(
        |_| (options.hostname.clone(), String::new()),
        |node| (node.fqdn(false), node.node_key.to_string()),
    );
    let addresses = vec![v4, v6];
    let mut listening = JoinSet::new();
    for address in &addresses {
        let at = SocketAddr::new(*address, options.port);
        match device.tcp_listen(at).await {
            Ok(listener) => {
                listening.spawn(accept(
                    listener,
                    shared.clone(),
                    options.daemon.clone(),
                    Arc::clone(&options.admit),
                ));
            }
            Err(error) => log::warn!("zz's own node could not listen on {at}: {error}"),
        }
    }
    log::info!(
        "zz's own node is signed in as {name} and listening on port {} at {v4}, {v6}",
        options.port
    );
    shared.set(State::Running {
        name,
        key,
        addresses,
    });
    while listening.join_next().await.is_some() {}
}

async fn open(options: &Options) -> Result<Device, String> {
    let keys = read_keys(&options.keys)
        .map_err(|error| format!("could not read {}: {error}", options.keys.display()))?;
    let mut config = Config::from(keys);
    config.client_name = Some("zz".to_owned());
    config.requested_hostname = Some(options.hostname.clone());
    if let Some(url) = &options.control_url {
        config.control_server_url = url
            .parse()
            .map_err(|error| format!("tailnet-control-url `{url}`: {error}"))?;
    }
    Device::new(&config, options.auth_key.clone())
        .await
        .map_err(|error| error.to_string())
}

async fn accept(
    listener: tailscale::netstack::TcpListener,
    shared: Shared,
    daemon: PathBuf,
    admit: Admit,
) {
    let mut connections = JoinSet::new();
    loop {
        let stream = match listener.accept().await {
            Ok(stream) => stream,
            Err(error) => {
                log::warn!(
                    "zz's own node stopped listening on {}: {error}",
                    listener.local_addr()
                );
                return;
            }
        };
        while connections.try_join_next().is_some() {}
        connections.spawn(serve(
            stream,
            shared.clone(),
            daemon.clone(),
            Arc::clone(&admit),
        ));
    }
}

async fn serve(
    stream: tailscale::netstack::TcpStream,
    shared: Shared,
    daemon: PathBuf,
    admit: Admit,
) {
    let from = stream.remote_addr().ip();
    let Some((device, me)) = shared.running() else {
        return;
    };
    if me.contains(&from) {
        log::debug!("closed a tailnet connection from this node's own address {from}");
        return;
    }
    let caller = match timeout(LOOKUP_TIMEOUT, device.peer_by_tailnet_ip(from)).await {
        Ok(Ok(Some(node))) => peer(&node),
        Ok(Ok(None)) => {
            log::warn!(
                "refused a tailnet connection from {from}: no machine on the tailnet has that address"
            );
            return;
        }
        Ok(Err(error)) => {
            log::warn!("refused a tailnet connection from {from}: {error}");
            return;
        }
        Err(_) => {
            log::warn!("refused a tailnet connection from {from}: the peer lookup took too long");
            return;
        }
    };
    if let Err(reason) = admit(&caller) {
        log::warn!(
            "refused a tailnet connection from {} ({}, {from}): {reason}",
            caller.name,
            caller.id
        );
        return;
    }
    let local = match UnixStream::connect(&daemon).await {
        Ok(local) => local,
        Err(error) => {
            log::warn!("could not hand {} to the daemon: {error}", caller.name);
            return;
        }
    };
    log::info!(
        "tailnet client {} ({}) connected from {from}",
        caller.name,
        caller.id
    );
    pipe(stream, local).await;
}

async fn serve_dials(listener: UnixListener, shared: Shared) {
    let mut dials = JoinSet::new();
    while let Ok((local, _)) = listener.accept().await {
        while dials.try_join_next().is_some() {}
        dials.spawn(dial(local, shared.clone()));
    }
}

async fn dial(mut local: UnixStream, shared: Shared) {
    let request = match read_request(&mut local).await {
        Ok(request) => request,
        Err(error) => {
            log::debug!("dropped a dial request: {error}");
            return;
        }
    };
    match connect(&request, &shared).await {
        Ok(remote) => {
            if local.write_all(b"ok\n").await.is_ok() {
                pipe(remote, local).await;
            }
        }
        Err(reason) => {
            let _ = local
                .write_all(format!("error {reason}\n").as_bytes())
                .await;
        }
    }
}

async fn read_request(local: &mut UnixStream) -> io::Result<String> {
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        if local.read(&mut byte).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if byte[0] == b'\n' {
            break;
        }
        request.push(byte[0]);
        if request.len() > REQUEST_LIMIT {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
    String::from_utf8(request).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

async fn connect(request: &str, shared: &Shared) -> Result<tailscale::netstack::TcpStream, String> {
    let (name, port) = request
        .rsplit_once(' ')
        .and_then(|(name, port)| Some((name, port.parse::<u16>().ok()?)))
        .ok_or_else(|| format!("zz's own node could not read the dial request `{request}`"))?;
    let Some((device, me)) = shared.running() else {
        return Err(format!(
            "{}; run `zz tailnet status` on this machine",
            shared.state.lock().describe()
        ));
    };
    let address = match name.parse::<IpAddr>() {
        Ok(address) => address,
        Err(_) => match timeout(LOOKUP_TIMEOUT, device.peer_by_name(name)).await {
            Ok(Ok(Some(node))) => IpAddr::V4(node.tailnet_address.ipv4.addr()),
            Ok(Ok(None)) => return Err(format!("no tailnet peer named `{name}`")),
            Ok(Err(error)) => return Err(error.to_string()),
            Err(_) => return Err("zz's own node took too long to look up the peer".to_owned()),
        },
    };
    if me.contains(&address) {
        return Err(format!("{name} is this machine"));
    }
    let target = SocketAddr::new(address, port);
    match timeout(DIAL_TIMEOUT, device.tcp_connect(target)).await {
        Ok(Ok(remote)) => Ok(remote),
        Ok(Err(error)) => Err(format!(
            "nothing answers at {target} ({error}); is zz running there with tailnet on?"
        )),
        Err(_) => Err(format!(
            "nothing answers at {target} (timed out); is zz running there with tailnet on?"
        )),
    }
}

async fn pipe(
    first: impl AsyncRead + AsyncWrite + Unpin,
    second: impl AsyncRead + AsyncWrite + Unpin,
) {
    let (mut first_read, mut first_write) = tokio::io::split(first);
    let (mut second_read, mut second_write) = tokio::io::split(second);
    tokio::select! {
        _ = tokio::io::copy(&mut first_read, &mut second_write) => {}
        _ = tokio::io::copy(&mut second_read, &mut first_write) => {}
    }
}
