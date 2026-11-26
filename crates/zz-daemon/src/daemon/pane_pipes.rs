use super::*;

impl Shared {
    pub(super) fn pipe_pane(
        self: &Arc<Self>,
        context: &ExecutionContext,
        command_name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_buffer_command_args("pipe-pane", args, &['t'], &['I', 'O', 'o'])?;
        let _serial = self.pipe_effects.lock();
        let (pane, terminal, old_pipe) = {
            let mut inner = self.inner.lock();
            let pane =
                inner
                    .engine
                    .resolve_pane(parsed.value('t'), context.window, context.pane)?;
            if inner.engine.state.pane(pane).is_some_and(|pane| pane.dead) {
                return Err(
                    ServerError::InvalidCommand("target pane has exited".to_owned()).into(),
                );
            }
            if is_agent_chat(&inner, pane) {
                return Err(agent_chat_refusal("pipe-pane", pane).into());
            }
            let terminal = inner
                .terminals
                .get(&pane)
                .cloned()
                .ok_or(ServerError::PaneExited(pane))?;
            let old_pipe = inner.jobs.pane_pipes.remove(&pane);
            (pane, terminal, old_pipe)
        };
        let _round_trips = zz_terminal::allow_actor_round_trips();
        let had_pipe = old_pipe.is_some();
        if let Some(pipe) = old_pipe {
            stop_pane_pipe(pipe);
        }
        let Some(command) = parsed
            .positional
            .first()
            .filter(|command| !command.is_empty())
        else {
            self.refresh_status();
            return Ok(Execution::default());
        };
        if parsed.has('o') && had_pipe {
            self.refresh_status();
            return Ok(Execution::default());
        }
        let pipe_input = parsed.has('I');
        let pipe_output = parsed.has('O') || !pipe_input;
        let (token, command) = {
            let mut inner = self.inner.lock();
            inner.jobs.next_pipe_token = inner.jobs.next_pipe_token.wrapping_add(1).max(1);
            inner.engine.set_format_now(unix_timestamp());
            let target = ExecutionContext::for_pane(&inner.engine.state, pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            let facts = borrowed_format_hook_facts(&inner);
            let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                &facts,
                context.format_variables(),
            )
            .with_command_item(command_name);
            let command = inner.engine.expand_pane_format_time(
                command,
                &target,
                context.session,
                context.target_format_client(),
                &mut hooks,
            );
            (inner.jobs.next_pipe_token, command)
        };
        let mut process = shell_process(&command);
        let failed = |error: std::io::Error| {
            ServerError::InvalidCommand(format!("could not start pipe-pane command: {error}"))
        };
        #[cfg(unix)]
        let (mut child, child_input, child_output) = {
            let io = |piped| {
                if piped {
                    jobs::Io::Piped
                } else {
                    jobs::Io::Null
                }
            };
            let spawned = jobs::spawn(
                &mut process,
                jobs::Environment::Inherited,
                [io(pipe_output), io(pipe_input), jobs::Io::Null],
            )
            .map_err(failed)?;
            (spawned.child, spawned.stdin, spawned.stdout)
        };
        #[cfg(windows)]
        let (child, child_input, child_output) = {
            process
                .stdin(if pipe_output {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stdout(if pipe_input {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stderr(Stdio::null());
            let mut child = process.spawn_unmasked().map_err(failed)?;
            let child_input = child.stdin.take();
            let child_output = child.stdout.take();
            (child, child_input, child_output)
        };
        let pid = child.id();
        let (tap_output, tap) = if pipe_output {
            #[cfg(unix)]
            let output = {
                let wake = Arc::clone(&self.pipe_jobs.wake);
                shard_sink::PipeFeed::new(move || wake.wake())
            };
            #[cfg(windows)]
            let output = shard_sink::PipeFeed::new(|| {});
            let reader = output.reader();
            (Some(output), Some(reader))
        } else {
            (None, None)
        };
        let route = tap_output.clone();
        #[cfg(unix)]
        {
            let cancel = Arc::new(AtomicBool::new(false));
            let terminal_target = Arc::new(Mutex::new(Arc::clone(&terminal)));
            let valid = {
                let mut inner = self.inner.lock();
                let valid = !self.stopping.load(Ordering::Acquire)
                    && inner
                        .terminals
                        .get(&pane)
                        .is_some_and(|current| Arc::ptr_eq(current, &terminal))
                    && inner.engine.state.pane(pane).is_some_and(|pane| !pane.dead);
                if valid {
                    inner.jobs.pane_pipes.insert(
                        pane,
                        PanePipe {
                            token,
                            pid,
                            terminal: Arc::clone(&terminal_target),
                            tap_output,
                            cancel: Arc::clone(&cancel),
                            jobs: self.pipe_jobs.clone(),
                        },
                    );
                    if let Some(sink) = shard_sink::PaneSink::of(&terminal) {
                        sink.set_pipe(route);
                    }
                }
                valid
            };
            if !valid {
                child.terminate();
                return Err(ServerError::PaneExited(pane).into());
            }
            let mut descriptors = Vec::new();
            if let Some(input) = child_input {
                descriptors.push(jobs::Descriptor {
                    fd: input,
                    read: false,
                    input: None,
                    socket: false,
                });
            }
            if let Some(output) = child_output {
                descriptors.push(jobs::Descriptor {
                    fd: output,
                    read: true,
                    input: None,
                    socket: false,
                });
            }
            let weak = Arc::downgrade(&self.server_owner());
            let launch = jobs::Launch {
                child,
                descriptors,
                policy: jobs::CompletionPolicy::ChildExit,
                deadline: None,
                process_group: true,
                output_limit: None,
                stream: None,
                pipe: Some(jobs::PipeIo::new(
                    terminal_target,
                    tap,
                    std::task::Waker::from(Arc::clone(&self.pipe_jobs.wake)),
                )),
                cancel: Some(cancel),
                complete: Box::new(move |_| {
                    if let Some(shared) = weak.upgrade() {
                        shared.pipe_finished(pane, token);
                    }
                }),
            };
            if let Err(error) = self.pipe_jobs.launch(launch) {
                self.pipe_finished(pane, token);
                return Err(error);
            }
        }
        #[cfg(windows)]
        {
            let process = Arc::new(Mutex::new(Some(child)));
            let stop = Arc::new(AtomicBool::new(false));
            let terminal_target = Arc::new(Mutex::new(Arc::clone(&terminal)));
            let (start, started) = crossbeam_channel::bounded(1);
            let worker_process = Arc::clone(&process);
            let worker_stop = Arc::clone(&stop);
            let worker_terminal = Arc::clone(&terminal_target);
            let weak = Arc::downgrade(&self.server_owner());
            let worker = thread::Builder::new()
                .name(format!("zz-pipe-{}", pane.0))
                .spawn(move || {
                    if started.recv().is_err() {
                        return;
                    }
                    run_pane_pipe(
                        &weak,
                        pane,
                        token,
                        &worker_terminal,
                        &worker_process,
                        &worker_stop,
                        child_input,
                        child_output,
                        tap,
                    );
                })
                .map_err(|error| {
                    terminate_managed_process(&process);
                    DaemonError::Thread(error.to_string())
                })?;
            let valid = {
                let mut inner = self.inner.lock();
                let valid = !self.stopping.load(Ordering::Acquire)
                    && inner
                        .terminals
                        .get(&pane)
                        .is_some_and(|current| Arc::ptr_eq(current, &terminal))
                    && inner.engine.state.pane(pane).is_some_and(|pane| !pane.dead);
                if valid {
                    inner.jobs.pane_pipes.insert(
                        pane,
                        PanePipe {
                            token,
                            pid,
                            terminal: terminal_target,
                            tap_output,
                            process: Arc::clone(&process),
                            stop: Arc::clone(&stop),
                            thread: Some(worker),
                        },
                    );
                    if let Some(sink) = shard_sink::PaneSink::of(&terminal) {
                        sink.set_pipe(route);
                    }
                }
                valid
            };
            if !valid {
                drop(start);
                stop.store(true, Ordering::Release);
                terminate_managed_process(&process);
                return Err(ServerError::PaneExited(pane).into());
            }
            let _ = start.send(());
        }
        drop(_serial);
        self.refresh_status();
        Ok(Execution::default())
    }

    fn pipe_finished(self: &Arc<Self>, pane: PaneId, token: u64) {
        let removed = {
            let mut inner = self.inner.lock();
            if inner
                .jobs
                .pane_pipes
                .get(&pane)
                .is_some_and(|pipe| pipe.token == token)
            {
                inner.jobs.pane_pipes.remove(&pane);
                true
            } else {
                false
            }
        };
        if removed {
            self.refresh_status();
        }
    }

    pub(super) fn spawn_copy_pipe(
        self: &Arc<Self>,
        pane: PaneId,
        client: ClientId,
        command: String,
        data: String,
    ) {
        let (subscriber, environment) = {
            let inner = self.inner.lock();
            let subscriber = (inner.client(client).and_then(|c| c.kind)
                == Some(ClientKind::Interactive))
            .then(|| {
                inner
                    .client(client)
                    .and_then(|c| c.subscriber.as_ref())
                    .cloned()
            })
            .flatten();
            let session = inner
                .engine
                .state
                .window_for_pane(pane)
                .and_then(|window| inner.engine.state.windows.get(&window))
                .map(|window| window.session);
            let environment = CopyPipeEnvironment {
                variables: inner.engine.job_environment(session),
                default_terminal: inner.engine.default_terminal_for_spawn().to_owned(),
                tmux: tmux_environment(&self.socket_path, session),
                zz_socket: self.socket_path.clone(),
            };
            (subscriber, environment)
        };
        let rejection = if command.is_empty() {
            Some("copy-pipe command is empty".to_owned())
        } else if command.len() > MAX_COPY_PIPE_COMMAND_BYTES {
            Some(format!(
                "copy-pipe command exceeds {MAX_COPY_PIPE_COMMAND_BYTES} bytes"
            ))
        } else if data.len() > MAX_COPY_PIPE_BYTES {
            Some(format!(
                "copy-pipe input exceeds {MAX_COPY_PIPE_BYTES} bytes"
            ))
        } else {
            let mut inner = self.inner.lock();
            if inner.jobs.active_copy_pipes >= MAX_COPY_PIPE_PROCESSES {
                Some(format!(
                    "copy-pipe process limit reached ({MAX_COPY_PIPE_PROCESSES})"
                ))
            } else {
                inner.jobs.active_copy_pipes += 1;
                None
            }
        };
        if let Some(message) = rejection {
            self.publish_copy_pipe_message(pane, subscriber.as_deref(), message);
            return;
        }

        #[cfg(unix)]
        {
            let shared = self.server_owner();
            let permit = CopyPipePermit {
                shared: Arc::downgrade(&shared),
            };
            let owner = Arc::downgrade(&shared);
            let worker_subscriber = subscriber.clone();
            let complete = Box::new(move |completion| {
                drop(permit);
                if let Err(error) = copy_pipe_result(completion, COPY_PIPE_TIMEOUT)
                    && let Some(shared) = owner.upgrade()
                {
                    shared.publish_copy_pipe_message(
                        pane,
                        worker_subscriber.as_deref(),
                        format!("copy-pipe failed: {error}"),
                    );
                }
            });
            match launch_copy_pipe(&command, data, &environment, COPY_PIPE_TIMEOUT, complete)
                .and_then(|launch| {
                    self.pipe_jobs
                        .launch(launch)
                        .map_err(|error| error.to_string())
                }) {
                Ok(()) => {}
                Err(error) => self.publish_copy_pipe_message(
                    pane,
                    subscriber.as_deref(),
                    format!("copy-pipe failed: {error}"),
                ),
            }
            drop(command);
        }
        #[cfg(windows)]
        {
            let shared = self.server_owner();
            let worker_subscriber = subscriber.clone();
            let permit = CopyPipePermit {
                shared: Arc::downgrade(&self.server_owner()),
            };
            if let Err(error) = thread::Builder::new()
                .name("zz-copy-pipe".to_owned())
                .spawn(move || {
                    let result = run_copy_pipe(&command, &data, &environment);
                    drop(permit);
                    if let Err(error) = result {
                        shared.publish_copy_pipe_message(
                            pane,
                            worker_subscriber.as_deref(),
                            format!("copy-pipe failed: {error}"),
                        );
                    }
                })
            {
                self.publish_copy_pipe_message(
                    pane,
                    subscriber.as_deref(),
                    format!("copy-pipe failed to start worker: {error}"),
                );
            }
        }
    }

    fn publish_copy_pipe_message(
        &self,
        pane: PaneId,
        subscriber: Option<&OutboundMailbox>,
        text: String,
    ) {
        log::warn!("{text}");
        let payload = EventPayload::ClientMessage {
            pane: Some(pane),
            kind: ClientMessageKind::Error,
            text,
        };
        if let Some(subscriber) = subscriber {
            Self::send_event(subscriber, payload);
        } else {
            let subscribers = {
                let inner = self.inner.lock();
                attached_clients_for_pane(&inner, pane)
                    .into_iter()
                    .flatten()
                    .filter(|client| {
                        inner.client(**client).and_then(|c| c.kind) == Some(ClientKind::Interactive)
                    })
                    .filter_map(|client| {
                        inner
                            .client(*client)
                            .and_then(|c| c.subscriber.as_ref())
                            .cloned()
                    })
                    .collect::<Vec<_>>()
            };
            for subscriber in subscribers {
                Self::send_event(&subscriber, payload.clone());
            }
        }
    }
}

pub(super) const MAX_COPY_PIPE_PROCESSES: usize = 8;

const MAX_COPY_PIPE_BYTES: usize = 32 * 1024 * 1024;

const MAX_COPY_PIPE_COMMAND_BYTES: usize = 8 * 1024;

pub(super) const COPY_PIPE_TIMEOUT: Duration = Duration::from_secs(30);

struct CopyPipePermit {
    shared: Weak<Shared>,
}

impl Drop for CopyPipePermit {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            let mut inner = shared.inner.lock();
            inner.jobs.active_copy_pipes = inner.jobs.active_copy_pipes.saturating_sub(1);
        }
    }
}

pub(super) struct CopyPipeEnvironment {
    pub(super) variables: Vec<(RawText, Option<RawText>)>,
    pub(super) default_terminal: String,
    pub(super) tmux: String,
    pub(super) zz_socket: PathBuf,
}

#[cfg(any(test, windows))]
pub(super) fn run_copy_pipe(
    command: &str,
    data: &str,
    environment: &CopyPipeEnvironment,
) -> Result<(), String> {
    run_copy_pipe_with_timeout(command, data, environment, COPY_PIPE_TIMEOUT)
}

#[cfg(windows)]
pub(super) fn run_copy_pipe_with_timeout(
    command: &str,
    data: &str,
    environment: &CopyPipeEnvironment,
    timeout: Duration,
) -> Result<(), String> {
    let mut input = tempfile::tempfile()
        .map_err(|error| format!("could not stage input in a temporary file: {error}"))?;
    input
        .write_all(data.as_bytes())
        .map_err(|error| format!("could not stage input: {error}"))?;
    input
        .seek(SeekFrom::Start(0))
        .map_err(|error| format!("could not rewind staged input: {error}"))?;

    let mut process = shell_process(command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;

        process.process_group(0);
    }
    configure_shell_job_environment(
        &mut process,
        &environment.variables,
        &environment.default_terminal,
        false,
        &environment.tmux,
        environment.zz_socket.as_os_str(),
        None,
        None,
    );
    process
        .stdin(Stdio::from(input))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = process
        .spawn_unmasked()
        .map_err(|error| format!("could not start process: {error}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("process exited unsuccessfully ({status})")),
            Ok(None) if started.elapsed() >= timeout => {
                let (kill_error, wait_error) = terminate_copy_pipe(&mut child);
                if let Some(error) = kill_error {
                    return Err(format!(
                        "process timed out after {} seconds and could not be terminated: {error}",
                        timeout.as_secs()
                    ));
                }
                if let Some(error) = wait_error {
                    return Err(format!(
                        "process timed out after {} seconds and could not be reaped: {error}",
                        timeout.as_secs()
                    ));
                }
                return Err(format!(
                    "process timed out after {} seconds",
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(COPY_PIPE_POLL_INTERVAL),
            Err(error) => {
                terminate_copy_pipe(&mut child);
                return Err(format!("could not query process status: {error}"));
            }
        }
    }
}

#[cfg(unix)]
pub(super) fn launch_copy_pipe(
    command: &str,
    data: String,
    environment: &CopyPipeEnvironment,
    timeout: Duration,
    complete: Box<dyn FnOnce(jobs::Completion) + Send>,
) -> Result<jobs::Launch, String> {
    let mut process = shell_process(command);
    configure_shell_job_environment(
        &mut process,
        &environment.variables,
        &environment.default_terminal,
        false,
        &environment.tmux,
        environment.zz_socket.as_os_str(),
        None,
        None,
    );
    let spawned = jobs::spawn(
        &mut process,
        jobs::Environment::Cleared,
        [jobs::Io::Piped, jobs::Io::Null, jobs::Io::Null],
    )
    .map_err(|error| format!("could not start process: {error}"))?;
    Ok(jobs::Launch {
        child: spawned.child,
        descriptors: vec![jobs::Descriptor {
            fd: spawned.stdin.unwrap(),
            read: false,
            input: Some(data.into_bytes()),
            socket: false,
        }],
        policy: jobs::CompletionPolicy::ChildExit,
        deadline: Some(Instant::now() + timeout),
        process_group: true,
        output_limit: None,
        stream: None,
        pipe: None,
        cancel: None,
        complete,
    })
}

#[cfg(unix)]
fn copy_pipe_result(completion: jobs::Completion, timeout: Duration) -> Result<(), String> {
    if completion.cancelled {
        return Err(format!(
            "process timed out after {} seconds",
            timeout.as_secs()
        ));
    }
    if let Some(error) = completion.error {
        return Err(error.to_string());
    }
    match completion.status {
        Some(status) if status.success() => Ok(()),
        Some(status) => Err(format!("process exited unsuccessfully ({status})")),
        None => Err("process exited without status".into()),
    }
}

#[cfg(all(test, unix))]
pub(super) fn run_copy_pipe_with_timeout(
    command: &str,
    data: &str,
    environment: &CopyPipeEnvironment,
    timeout: Duration,
) -> Result<(), String> {
    let mut poll = mio::Poll::new().map_err(|error| error.to_string())?;
    let mut registry = jobs::JobRegistry::default();
    let mut token = 4;
    let (done, completed) = mpsc::channel();
    let launch = launch_copy_pipe(
        command,
        data.to_owned(),
        environment,
        timeout,
        Box::new(move |completion| {
            let _ = done.send(completion);
        }),
    )?;
    let id = registry
        .register(poll.registry(), &mut token, launch)
        .map_err(|error| error.to_string())?;
    let mut events = mio::Events::with_capacity(16);
    while registry.contains(id) {
        registry.turn(poll.registry(), Instant::now());
        registry.child_signal(poll.registry());
        let timeout = registry
            .next(Instant::now())
            .map_or(COPY_PIPE_POLL_INTERVAL, |deadline| {
                deadline.saturating_duration_since(Instant::now())
            })
            .min(COPY_PIPE_POLL_INTERVAL);
        match poll.poll(&mut events, Some(timeout)) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        }
        for event in &events {
            registry.ready(
                poll.registry(),
                event.token(),
                event.is_readable() || event.is_read_closed(),
                event.is_writable() || event.is_write_closed(),
            );
        }
    }
    copy_pipe_result(
        completed.recv().map_err(|error| error.to_string())?,
        timeout,
    )
}

#[cfg(windows)]
fn pane_pipe_process_exited(process: &Mutex<Option<Child>>) -> bool {
    let mut process = process.lock();
    let Some(child) = process.as_mut() else {
        return true;
    };
    match child.try_wait() {
        Ok(Some(_)) => {
            process.take();
            true
        }
        Ok(None) => false,
        Err(_) => {
            let mut child = process.take().expect("pipe process was present");
            drop(process);
            let _ = terminate_copy_pipe(&mut child);
            true
        }
    }
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn run_pane_pipe(
    shared: &std::sync::Weak<Shared>,
    pane: PaneId,
    token: u64,
    terminal: &Arc<Mutex<Arc<TerminalSession>>>,
    process: &Arc<Mutex<Option<Child>>>,
    stop: &Arc<AtomicBool>,
    mut child_input: Option<std::process::ChildStdin>,
    child_output: Option<std::process::ChildStdout>,
    tap: Option<shard_sink::PipeReader>,
) {
    let input_worker = child_output.map(|mut output| {
        let terminal = Arc::clone(terminal);
        let stop = Arc::clone(stop);
        thread::Builder::new()
            .name(format!("zz-pipe-input-{}", pane.0))
            .spawn(move || {
                let mut buffer = vec![0_u8; 64 * 1024];
                while !stop.load(Ordering::Acquire) {
                    match output.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(length) => {
                            let target = Arc::clone(&terminal.lock());
                            if !target.send_raw_input(Arc::from(&buffer[..length])) {
                                break;
                            }
                        }
                    }
                }
            })
    });
    let input_worker = match input_worker {
        Some(Ok(worker)) => Some(worker),
        Some(Err(_)) => {
            stop.store(true, Ordering::Release);
            None
        }
        None => None,
    };
    let mut exited = false;
    while !stop.load(Ordering::Acquire) {
        let Some(tap) = tap.as_ref() else {
            if pane_pipe_process_exited(process) {
                exited = true;
                break;
            }
            thread::sleep(COPY_PIPE_POLL_INTERVAL);
            continue;
        };
        match tap.recv_timeout(COPY_PIPE_POLL_INTERVAL) {
            Ok(bytes) => {
                let Some(input) = child_input.as_mut() else {
                    break;
                };
                if input.write_all(&bytes).is_err() {
                    exited = pane_pipe_process_exited(process);
                    break;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if pane_pipe_process_exited(process) {
                    exited = true;
                    break;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(tap);
    drop(child_input);
    if !exited {
        terminate_managed_process(process);
    }
    if let Some(worker) = input_worker {
        let _ = worker.join();
    }
    if let Some(shared) = shared.upgrade() {
        shared.pipe_finished(pane, token);
    }
}

pub(super) struct PanePipe {
    token: u64,
    pub(super) pid: u32,
    pub(super) terminal: Arc<Mutex<Arc<TerminalSession>>>,
    pub(super) tap_output: Option<Arc<shard_sink::PipeFeed>>,
    #[cfg(unix)]
    pub(super) cancel: Arc<AtomicBool>,
    #[cfg(unix)]
    pub(super) jobs: pipe_jobs::Client,
    #[cfg(windows)]
    process: Arc<Mutex<Option<Child>>>,
    #[cfg(windows)]
    stop: Arc<AtomicBool>,
    #[cfg(windows)]
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for PanePipe {
    fn drop(&mut self) {
        if let Some(output) = &self.tap_output {
            output.end();
        }
    }
}

#[cfg(not(unix))]
pub(super) fn terminate_copy_pipe(child: &mut Child) -> (Option<String>, Option<std::io::Error>) {
    #[cfg(windows)]
    let tree_result = {
        let pid = child.id().to_string();
        std::process::Command::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status_unmasked()
            .map_err(|error| format!("could not run taskkill: {error}"))
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("taskkill exited unsuccessfully ({status})"))
                }
            })
    };
    #[cfg(not(any(unix, windows)))]
    let tree_result = Err("process-tree termination is unsupported".to_owned());
    let child_error = child.kill().err();
    let kill_error = tree_result.err().map(|tree_error| match child_error {
        Some(child_error) => {
            format!("{tree_error}; could not terminate immediate child: {child_error}")
        }
        None => tree_error,
    });
    let wait_error = child.wait().err();
    (kill_error, wait_error)
}

#[cfg(not(unix))]
pub(super) fn terminate_managed_process(process: &Mutex<Option<Child>>) {
    let Some(mut child) = process.lock().take() else {
        return;
    };
    let _ = terminate_copy_pipe(&mut child);
}

#[cfg(unix)]
pub(super) fn stop_pane_pipe(pipe: PanePipe) {
    if let Some(pid) = rustix::process::Pid::from_raw(pipe.pid as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    pipe.cancel.store(true, Ordering::Release);
    pipe.jobs.wake.wake();
    drop(pipe);
}

#[cfg(windows)]
pub(super) fn stop_pane_pipe(mut pipe: PanePipe) {
    pipe.stop.store(true, Ordering::Release);
    terminate_managed_process(&pipe.process);
    if let Some(worker) = pipe.thread.take()
        && worker.join().is_err()
    {
        log::error!("pipe-pane worker panicked for pane process {}", pipe.pid);
    }
}
