use super::*;

const DAEMON_READY_DEADLINE: Duration = Duration::from_secs(6);

static DAEMON_SPAWN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn validated_bootstrap_client_working_directory(path: PathBuf) -> Option<PathBuf> {
    let value = path.to_str()?;
    (path.is_absolute() && value.len() <= MAX_CLIENT_WORKING_DIRECTORY_BYTES).then_some(path)
}

pub(super) fn parse_daemon_bootstrap_arguments(
    arguments: &[RawText],
) -> Result<DaemonBootstrapArguments, DaemonBootstrapArgumentError> {
    if arguments.is_empty() {
        return Ok(DaemonBootstrapArguments::default());
    }
    let [server_flag, server_id, remaining @ ..] = arguments else {
        return Err(DaemonBootstrapArgumentError::ServerId);
    };
    if server_flag != DAEMON_BOOTSTRAP_SERVER_ID_ARGUMENT {
        return Err(DaemonBootstrapArgumentError::ServerId);
    }
    let server_id = server_id
        .parse::<u64>()
        .map_err(|_| DaemonBootstrapArgumentError::ServerId)?;
    let (ready_fd, remaining) = match remaining {
        [flag, fd, remaining @ ..] if flag == DAEMON_BOOTSTRAP_READY_FD_ARGUMENT => (
            Some(
                fd.parse::<i32>()
                    .ok()
                    .filter(|fd| *fd > 2)
                    .ok_or(DaemonBootstrapArgumentError::ReadyFd)?,
            ),
            remaining,
        ),
        [flag] if flag == DAEMON_BOOTSTRAP_READY_FD_ARGUMENT => {
            return Err(DaemonBootstrapArgumentError::ReadyFd);
        }
        remaining => (None, remaining),
    };
    let client_working_directory = match remaining {
        [] => None,
        [cwd_flag, cwd] if cwd_flag == DAEMON_BOOTSTRAP_CLIENT_CWD_ARGUMENT => Some(
            validated_bootstrap_client_working_directory(PathBuf::from(cwd.to_os_string()))
                .ok_or(DaemonBootstrapArgumentError::ClientWorkingDirectory)?,
        ),
        _ => return Err(DaemonBootstrapArgumentError::ClientWorkingDirectory),
    };
    Ok(DaemonBootstrapArguments {
        server_id: Some(server_id),
        ready_fd,
        client_working_directory,
    })
}

pub(super) fn daemon_is_spawnable(error: &DaemonError) -> bool {
    matches!(
        error,
        DaemonError::Io(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset
            )
    )
}

fn next_spawn_server_id() -> u64 {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let folded = u64::try_from(timestamp & u128::from(u64::MAX)).unwrap_or_default();
    folded
        ^ u64::from(std::process::id()).rotate_left(32)
        ^ DAEMON_SPAWN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn tmux_import_hint(
    interactive: bool,
    mux_exists: bool,
    donor: Option<&Path>,
) -> Option<String> {
    if !interactive || mux_exists {
        return None;
    }
    donor.map(|path| {
        format!(
            "zz does not read {}; run `zz import-tmux-config` to import it.",
            path.display()
        )
    })
}

fn spawn_daemon(
    path: &Path,
    color_scheme: Option<TerminalColorScheme>,
    mux_config_files: &[PathBuf],
) -> Result<u64, DaemonError> {
    let server_id = next_spawn_server_id();
    let executable = daemon_executable()?;
    let mut command = Command::new(&executable);
    command
        .env_remove("ZZ_TMUX_EXECUTABLE")
        .env_remove(APP_STARTUP_DIRECTORY_ENV);
    command.arg(diagnostics::SOCKET_ARGUMENT).arg(path);
    for config in mux_config_files {
        command.arg("-f").arg(config);
    }
    command
        .arg("daemon")
        .arg(DAEMON_BOOTSTRAP_SERVER_ID_ARGUMENT)
        .arg(server_id.to_string());
    #[cfg(unix)]
    let ready = readiness::Pipe::new()?;
    #[cfg(unix)]
    command
        .arg(DAEMON_BOOTSTRAP_READY_FD_ARGUMENT)
        .arg(ready.child_fd().to_string());
    if let Some(color_scheme) = color_scheme {
        command.env("ZZ_COLOR_SCHEME", color_scheme.as_str());
    }
    diagnostics::configure_spawned_process(&mut command);
    #[cfg(unix)]
    detach_daemon_session(&mut command, ready.child_fd());
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(client_working_directory) = std::env::current_dir()
        .ok()
        .and_then(validated_bootstrap_client_working_directory)
    {
        command
            .arg(DAEMON_BOOTSTRAP_CLIENT_CWD_ARGUMENT)
            .arg(client_working_directory);
    }
    if let Some(hint) = tmux_import_hint(
        std::io::stderr().is_terminal(),
        zz_daemon_client::mux_config_write_path().is_some_and(|path| path.exists()),
        zz_daemon_client::discover_tmux_config().as_deref(),
    ) {
        eprintln!("{hint}");
    }
    reap_when_exited(command.spawn_unmasked()?);
    log::debug!(
        target: "zz::diagnostics::process",
        "spawned daemon path={} child_verbose_flag_applied={}",
        path.display(),
        diagnostics::enabled(),
    );
    #[cfg(unix)]
    ready.wait(DAEMON_READY_DEADLINE);
    Ok(server_id)
}

pub(super) fn restart_daemon(
    socket_path: &Path,
    mux_config_files: &[PathBuf],
    restart: &zz_daemon::Restart,
) -> ExitCode {
    use zz_config::owned::{self, BuildInfo};

    let this = BuildInfo::current();
    let Some(executable) = owned::installed()
        .filter(|copy| copy.info.version > this.version)
        .map(|copy| copy.binary)
        .or_else(|| std::env::current_exe().ok())
    else {
        log::error!(target: "zz::diagnostics::process", "found no zz to restart the daemon with");
        return exit_code_for(CliFailure::Runtime);
    };
    let mut command = Command::new(&executable);
    command.arg(diagnostics::SOCKET_ARGUMENT).arg(socket_path);
    for config in mux_config_files {
        command.arg("-f").arg(config);
    }
    command.arg("daemon");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;

        if let Some(fd) = restart.inheritable_lock_fd() {
            command
                .arg(DAEMON_START_LOCK_FD_ARGUMENT)
                .arg(fd.to_string());
        }
        diagnostics::configure_spawned_process(&mut command);
        let error = command.exec();
        log::error!(
            target: "zz::diagnostics::process",
            "could not restart the daemon path={} error={error}",
            executable.display(),
        );
        exit_code_for(CliFailure::Runtime)
    }
    #[cfg(windows)]
    {
        let _ = restart;
        command
            .arg(DAEMON_RETRY_BIND_ARGUMENT)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        diagnostics::configure_spawned_process(&mut command);
        match command.spawn_unmasked() {
            Ok(_) => ExitCode::SUCCESS,
            Err(error) => {
                log::error!(
                    target: "zz::diagnostics::process",
                    "could not restart the daemon path={} error={error}",
                    executable.display(),
                );
                exit_code_for(CliFailure::Runtime)
            }
        }
    }
}

pub(super) fn reap_when_exited(mut child: std::process::Child) {
    let _ = thread::Builder::new()
        .name("zz-daemon-reaper".to_owned())
        .spawn(move || child.wait());
}

#[cfg(unix)]
mod readiness {
    use std::{
        io,
        os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd, RawFd},
        time::{Duration, Instant},
    };

    pub(super) struct Pipe {
        reader: OwnedFd,
        child: Option<OwnedFd>,
    }

    #[allow(
        unsafe_code,
        reason = "pipe, fcntl and poll on descriptors this function owns"
    )]
    impl Pipe {
        pub(super) fn new() -> io::Result<Self> {
            let mut fds = [0; 2];
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            let (reader, child) =
                unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
            for fd in [&reader, &child] {
                if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(Self {
                reader,
                child: Some(child),
            })
        }

        pub(super) fn child_fd(&self) -> RawFd {
            self.child
                .as_ref()
                .map_or(-1, std::os::fd::AsRawFd::as_raw_fd)
        }

        pub(super) fn wait(mut self, limit: Duration) {
            drop(self.child.take());
            let deadline = Instant::now() + limit;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return;
                }
                let mut poll = libc::pollfd {
                    fd: self.reader.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                let timeout = i32::try_from(remaining.as_millis())
                    .unwrap_or(i32::MAX)
                    .max(1);
                match unsafe { libc::poll(&raw mut poll, 1, timeout) } {
                    0 => return,
                    count if count > 0 => {
                        let mut byte = 0_u8;
                        let _ = unsafe {
                            libc::read(self.reader.as_raw_fd(), (&raw mut byte).cast(), 1)
                        };
                        return;
                    }
                    _ if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => {}
                    _ => return,
                }
            }
        }
    }
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "Command::pre_exec is the only way to give the daemon its own session"
)]
fn detach_daemon_session(command: &mut Command, ready_fd: std::os::fd::RawFd) {
    use std::os::unix::process::CommandExt as _;

    let mut inherited = InheritedDescriptors::new();
    unsafe {
        command.pre_exec(move || {
            let _ = rustix::process::setsid();
            inherited.mark_cloexec();
            if ready_fd >= 0 {
                libc::fcntl(ready_fd, libc::F_SETFD, 0);
            }
            Ok(())
        });
    }
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "Command::pre_exec is the only way to take a child off the caller's terminal"
)]
pub(crate) fn without_controlling_terminal(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;

    unsafe {
        command.pre_exec(|| {
            let _ = rustix::process::setsid();
            Ok(())
        });
    }
}

#[cfg(target_os = "macos")]
struct InheritedDescriptors(Vec<libc::proc_fdinfo>);

#[cfg(all(unix, not(target_os = "macos")))]
struct InheritedDescriptors(libc::c_int);

#[cfg(unix)]
fn descriptor_limit() -> usize {
    let open = rustix::process::getrlimit(rustix::process::Resource::Nofile)
        .current
        .unwrap_or(4096)
        .clamp(256, 65_536);
    usize::try_from(open).unwrap_or(4096)
}

#[cfg(target_os = "macos")]
impl InheritedDescriptors {
    fn new() -> Self {
        Self(Vec::with_capacity(descriptor_limit()))
    }

    #[allow(
        unsafe_code,
        reason = "proc_pidinfo writes into capacity reserved before the fork"
    )]
    fn mark_cloexec(&mut self) {
        let capacity = self.0.capacity();
        let bytes =
            i32::try_from(capacity * std::mem::size_of::<libc::proc_fdinfo>()).unwrap_or(i32::MAX);
        let written = unsafe {
            libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDLISTFDS,
                0,
                self.0.as_mut_ptr().cast(),
                bytes,
            )
        };
        let Ok(written) = usize::try_from(written) else {
            return;
        };
        let count = (written / std::mem::size_of::<libc::proc_fdinfo>()).min(capacity);
        for index in 0..count {
            let descriptor = unsafe { (*self.0.as_ptr().add(index)).proc_fd };
            if descriptor > 2 {
                unsafe {
                    libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC);
                }
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl InheritedDescriptors {
    fn new() -> Self {
        Self(libc::c_int::try_from(descriptor_limit()).unwrap_or(4096))
    }

    #[allow(
        unsafe_code,
        reason = "close_range and fcntl are async-signal-safe syscalls"
    )]
    fn mark_cloexec(&mut self) {
        #[cfg(target_os = "linux")]
        if unsafe {
            libc::syscall(
                libc::SYS_close_range,
                3_u32,
                u32::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            )
        } == 0
        {
            return;
        }
        for descriptor in 3..self.0 {
            unsafe {
                libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
    }
}

pub fn connect_or_spawn_daemon<T>(
    path: &Path,
    color_scheme: Option<TerminalColorScheme>,
    mux_config_files: &[PathBuf],
    connect: impl Fn(bool) -> Result<T, DaemonError>,
    server_hello: impl for<'a> Fn(&'a T) -> &'a ServerHello,
) -> Result<T, DaemonError> {
    connect_or_spawn_daemon_with_provenance(
        path,
        color_scheme,
        mux_config_files,
        connect,
        server_hello,
    )
    .map(|(client, _)| client)
}

pub fn connect_or_spawn_daemon_with_provenance<T>(
    path: &Path,
    color_scheme: Option<TerminalColorScheme>,
    mux_config_files: &[PathBuf],
    connect: impl Fn(bool) -> Result<T, DaemonError>,
    server_hello: impl for<'a> Fn(&'a T) -> &'a ServerHello,
) -> Result<(T, Option<u64>), DaemonError> {
    match connect(false) {
        Ok(client) => {
            log::debug!(
                target: "zz::diagnostics::process",
                "connected to existing daemon path={} server_hello={:#?}",
                path.display(),
                server_hello(&client),
            );
            return Ok((client, None));
        }
        Err(error) => match classify_local_connect_error(path, error) {
            error if daemon_is_spawnable(&error) => {}
            error => return Err(error),
        },
    }

    let spawned_server_id = spawn_daemon(path, color_scheme, mux_config_files)?;
    let client = connect_spawned(path, || connect(true))?;
    let provenance =
        (server_hello(&client).server_id == spawned_server_id).then_some(spawned_server_id);
    Ok((client, provenance))
}

fn connect_spawned<T>(
    path: &Path,
    connect: impl Fn() -> Result<T, DaemonError>,
) -> Result<T, DaemonError> {
    let deadline = Instant::now() + DAEMON_READY_DEADLINE;
    let mut backoff = [500, 1_000, 2_000].into_iter();
    loop {
        match connect() {
            Ok(client) => return Ok(client),
            Err(error) if Instant::now() >= deadline || !daemon_is_spawnable(&error) => {
                return Err(classify_local_connect_error(path, error));
            }
            Err(_) => thread::sleep(Duration::from_micros(backoff.next().unwrap_or(5_000))),
        }
    }
}

pub(super) fn connect_command_client(
    path: &Path,
    mux_config_files: &[PathBuf],
    start_server: bool,
) -> Result<CommandClient, DaemonError> {
    match CommandClient::connect(path) {
        Ok(client) => Ok(client),
        Err(error) if start_server && daemon_is_spawnable(&error) => {
            spawn_and_connect_command_client(path, mux_config_files).map(|(client, _)| client)
        }
        Err(error) if start_server => Err(classify_local_connect_error(path, error)),
        Err(error) => Err(error),
    }
}

pub(super) fn spawn_and_connect_command_client(
    path: &Path,
    mux_config_files: &[PathBuf],
) -> Result<(CommandClient, u64), DaemonError> {
    let spawned_server_id = spawn_daemon(path, None, mux_config_files)?;
    let client = connect_spawned(path, || CommandClient::connect(path))?;
    Ok((client, spawned_server_id))
}

pub fn connect_interactive_client_with_config(
    path: &Path,
    color_scheme: TerminalColorScheme,
    mux_config_files: &[PathBuf],
    capabilities: &[&str],
    attach: Option<&zz_protocol::AttachOperation>,
) -> Result<InteractiveClient, DaemonError> {
    connect_or_spawn_daemon(
        path,
        Some(color_scheme),
        mux_config_files,
        |_| {
            InteractiveClient::connect_endpoint_with_prompts_and_attach(
                &Endpoint::Local(path.to_path_buf()),
                Some(color_scheme),
                None,
                capabilities,
                attach.cloned(),
                false,
            )
        },
        InteractiveClient::server_hello,
    )
}

pub fn connect_interactive_client_with_config_and_terminal(
    path: &Path,
    color_scheme: TerminalColorScheme,
    mux_config_files: &[PathBuf],
    client_has_terminal: bool,
) -> Result<InteractiveClient, DaemonError> {
    connect_or_spawn_daemon(
        path,
        Some(color_scheme),
        mux_config_files,
        |_| {
            InteractiveClient::connect_with_color_scheme_and_terminal(
                path,
                color_scheme,
                client_has_terminal,
            )
        },
        InteractiveClient::server_hello,
    )
}

pub fn connect_terminal_surface_client_with_config(
    path: &Path,
    color_scheme: TerminalColorScheme,
    mux_config_files: &[PathBuf],
    _client_has_terminal: bool,
    attach: Option<&zz_protocol::AttachOperation>,
) -> Result<InteractiveClient, DaemonError> {
    connect_or_spawn_daemon(
        path,
        Some(color_scheme),
        mux_config_files,
        |_| {
            InteractiveClient::connect_endpoint_with_prompts_and_attach(
                &Endpoint::Local(path.to_path_buf()),
                None,
                None,
                &[
                    zz_daemon_client::CLIENT_EXITS_ON_DETACH_CAPABILITY,
                    zz_protocol::TTY_INPUT_CAPABILITY,
                ],
                attach.cloned(),
                true,
            )
        },
        InteractiveClient::server_hello,
    )
}

pub fn daemon_executable() -> io::Result<PathBuf> {
    #[cfg(windows)]
    if let Some(copy) = owned_daemon_executable() {
        return Ok(copy);
    }
    let executable = std::env::current_exe()?;
    match executable.canonicalize() {
        Ok(executable) => Ok(daemon_executable_from(&executable)),
        #[cfg(target_os = "linux")]
        Err(_) => Ok(PathBuf::from("/proc/self/exe")),
        #[cfg(not(target_os = "linux"))]
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn owned_daemon_executable() -> Option<PathBuf> {
    use zz_config::owned::{self, BuildInfo, Launch};

    let copy = owned::installed()?;
    owned::hands_over(&BuildInfo::current(), &copy.info, Launch::from_env(true))
        .then_some(copy.binary)
}

#[must_use]
pub(super) fn daemon_executable_from(executable: &Path) -> PathBuf {
    let name = if cfg!(windows) { "cli.exe" } else { "cli" };
    if executable.file_name() != Some(std::ffi::OsStr::new(name))
        && executable.parent().and_then(Path::file_name) != Some(std::ffi::OsStr::new("bin"))
    {
        let cli = executable.with_file_name(name);
        if cli.is_file() {
            return cli;
        }
    }
    executable.to_path_buf()
}
