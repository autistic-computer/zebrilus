use super::*;

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
