use super::*;

#[cfg(windows)]
const ACCEPT_WAIT_TIMEOUT: Duration = Duration::from_millis(20);

#[cfg(windows)]
pub(super) fn accept_connections<T: Transport>(
    listener: &T::Listener,
    shared: &Arc<Shared>,
) -> Result<(), DaemonError> {
    while !shared.stopping.load(Ordering::Acquire) {
        match listener.accept() {
            Ok(stream) => {
                let connection_shared = Arc::clone(shared);
                if let Err(error) = shared.connection_threads.run(Box::new(move || {
                    if let Err(error) = handle_connection(stream, &connection_shared) {
                        log::debug!("client disconnected: {error}");
                    }
                })) {
                    log::warn!("could not start client connection thread: {error}");
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                listener.wait_for_incoming(ACCEPT_WAIT_TIMEOUT)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) struct SocketGuard {
    path: PathBuf,
    armed: bool,
}

#[cfg(windows)]
pub(super) struct SocketGuard;

impl SocketGuard {
    #[cfg(unix)]
    pub(super) fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    #[cfg(windows)]
    pub(super) fn new(path: PathBuf) -> Self {
        let _ = path;
        Self
    }

    #[cfg(unix)]
    pub(super) fn release(&mut self) {
        if !std::mem::take(&mut self.armed) {
            return;
        }
        let _ = fs::remove_file(&self.path);
    }

    #[cfg(windows)]
    pub(super) fn release(&mut self) {}

    #[cfg(windows)]
    pub(super) fn disarm(&mut self) {}
}

#[cfg(unix)]
impl Drop for SocketGuard {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(all(unix, test))]
pub(super) fn handle_connection<S: TransportStream>(
    stream: S,
    shared: &Arc<Shared>,
) -> Result<(), DaemonError> {
    event_loop::serve_single(stream, shared)
}

#[cfg(windows)]
pub(super) fn handle_connection<S: TransportStream>(
    mut stream: S,
    shared: &Arc<Shared>,
) -> Result<(), DaemonError> {
    let mut inbound_frame = Vec::new();
    let first_message = match read_protocol_message_into(&mut stream, &mut inbound_frame) {
        Ok(message) => message,
        Err(error @ ProtocolError::VersionMismatch { received, .. }) => {
            best_effort_protocol_mismatch_reply(&mut stream, received);
            log::info!(
                target: "zz_daemon::diagnostics::connection",
                "rejected client protocol={received} server_protocol={PROTOCOL_VERSION}",
            );
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    };
    handle_connection_message(stream, shared, first_message)
}

#[cfg(unix)]
#[derive(Debug)]
pub(super) struct StartLock {
    path: PathBuf,
    pub(super) _file: fs::File,
}

#[cfg(unix)]
pub(super) fn start_lock_path(socket: &Path) -> PathBuf {
    let mut path = socket.as_os_str().to_owned();
    path.push(".lock");
    PathBuf::from(path)
}

#[cfg(unix)]
impl StartLock {
    #[allow(
        unsafe_code,
        reason = "the restarting daemon handed this descriptor over exec and nothing else owns it"
    )]
    pub(super) fn adopt(path: PathBuf, fd: i32) -> Option<Self> {
        use std::os::{
            fd::{BorrowedFd, FromRawFd as _},
            unix::fs::MetadataExt as _,
        };

        if fd < 0 {
            return None;
        }
        let held = fs::File::from(
            unsafe { BorrowedFd::borrow_raw(fd) }
                .try_clone_to_owned()
                .ok()?,
        )
        .metadata()
        .ok()?;
        let current = fs::metadata(&path).ok()?;
        if (held.dev(), held.ino()) != (current.dev(), current.ino()) {
            return None;
        }
        let file = unsafe { fs::File::from_raw_fd(fd) };
        rustix::io::fcntl_setfd(&file, rustix::io::FdFlags::CLOEXEC).ok()?;
        Some(Self { path, _file: file })
    }

    pub(super) fn acquire(path: PathBuf) -> Result<Self, DaemonError> {
        use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};

        loop {
            let file = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .mode(0o600)
                .open(&path)?;
            rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
                .map_err(std::io::Error::from)?;
            let held = file.metadata()?;
            match fs::metadata(&path) {
                Ok(current) if current.dev() == held.dev() && current.ino() == held.ino() => {
                    return Ok(Self { path, _file: file });
                }
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

#[cfg(unix)]
impl Drop for StartLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
pub(super) fn prepare_socket(
    path: &Path,
    ready: &mut BootstrapReady,
    handed_lock: Option<i32>,
) -> Result<StartLock, DaemonError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let lock_path = start_lock_path(path);
    let lock = match handed_lock.and_then(|fd| StartLock::adopt(lock_path.clone(), fd)) {
        Some(lock) => lock,
        None => StartLock::acquire(lock_path)?,
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while path.exists() {
        match daemon_answers_at(path) {
            Some(true) => {
                ready.signal();
                return Err(DaemonError::AlreadyRunning(path.to_owned()));
            }
            Some(false) if Instant::now() >= deadline => {
                return Err(DaemonError::AlreadyRunning(path.to_owned()));
            }
            Some(false) => thread::sleep(Duration::from_millis(20)),
            None => {
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                break;
            }
        }
    }
    Ok(lock)
}

#[cfg(unix)]
fn daemon_answers_at(path: &Path) -> Option<bool> {
    let mut stream = LocalTransport::connect(path).ok()?;
    let (answered, answer) = crossbeam_channel::bounded(1);
    let probe = thread::Builder::new()
        .name("zz-socket-probe".to_owned())
        .spawn(move || {
            let request = ProtocolMessage::Exec(zz_protocol::ExecRequest {
                protocol_version: PROTOCOL_VERSION,
                flags: zz_protocol::ExecFlags::default(),
                client_instance_id: ClientInstanceId::default(),
                origin: None,
                working_directory: None,
                tty: None,
                size: None,
                features: 0,
                startup_reentry: None,
                spawned_server_id: None,
                expect_server_id: None,
                process_id: std::process::id(),
                environment: ClientEnvironmentBlob::default(),
                commands: Vec::new(),
                raw_control_line: None,
            });
            let live = zz_protocol::write_protocol_message(&mut stream, &request).is_ok()
                && matches!(
                    zz_protocol::read_protocol_message(&mut stream),
                    Ok(ProtocolMessage::ExecExit(_))
                );
            let _ = answered.send(live);
        });
    if probe.is_err() {
        return Some(false);
    }
    Some(
        answer
            .recv_timeout(Duration::from_millis(250))
            .unwrap_or(false),
    )
}

#[cfg(windows)]
pub(super) fn prepare_socket(_: &Path, _: &mut BootstrapReady) -> Result<(), DaemonError> {
    Ok(())
}

#[cfg(unix)]
pub(super) fn restrict_socket_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(windows)]
pub(super) fn restrict_socket_permissions(_: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

pub(super) fn server_id() -> u64 {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let folded = u64::try_from(timestamp & u128::from(u64::MAX)).unwrap_or_default();
    folded ^ u64::from(std::process::id())
}
