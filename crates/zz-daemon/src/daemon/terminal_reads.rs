use super::*;
use terminal_requests::CommandState;

struct Target {
    client: ClientId,
    guard_client: bool,
    pane: PaneId,
    terminal: Arc<TerminalSession>,
    state: Arc<CommandState>,
    observation: Arc<Observation>,
}

impl Target {
    fn new(
        shared: &Shared,
        client: ClientId,
        pane: PaneId,
        terminal: Arc<TerminalSession>,
        state: Arc<CommandState>,
    ) -> Arc<Self> {
        let mut inner = shared.inner.lock();
        let registered = inner.client(client).is_some();
        let observation = inner
            .pane_read_observations
            .get(&pane)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| {
                let observation = Arc::new(Observation {
                    generation: AtomicU64::new(0),
                    run: AtomicBool::new(false),
                    notify: shared.terminal_requests.notifier(),
                });
                inner
                    .pane_read_observations
                    .insert(pane, Arc::downgrade(&observation));
                observation
            });
        if let Some(sink) = shard_sink::PaneSink::of(&terminal) {
            sink.observe(&observation);
        }
        #[cfg(unix)]
        let registered = registered || shared.loop_active.load(Ordering::Acquire);
        Arc::new(Self {
            client,
            guard_client: client != ClientId(u64::MAX) && registered,
            pane,
            terminal,
            state,
            observation,
        })
    }

    fn generation(&self) -> u64 {
        self.observation.generation.load(Ordering::Acquire)
    }

    fn check(&self, shared: &Shared) -> bool {
        if self.state.continuation.ready() {
            return false;
        }
        if shared.command_queue_cancelled(self.client)
            || shared.stopping.load(Ordering::Acquire)
            || self.guard_client && shared.inner.lock().client(self.client).is_none()
        {
            self.state.resolve(Ok(Execution::default()));
            return false;
        }
        if !shared
            .inner
            .lock()
            .terminals
            .get(&self.pane)
            .is_some_and(|current| Arc::ptr_eq(current, &self.terminal))
        {
            self.state
                .resolve(Err(ServerError::PaneExited(self.pane).into()));
            return false;
        }
        true
    }
}

pub(super) struct Observation {
    generation: AtomicU64,
    run: AtomicBool,
    notify: Arc<dyn Fn() + Send + Sync>,
}

struct RunTurn(Arc<Observation>);

impl RunTurn {
    fn claim(observation: &Arc<Observation>) -> Option<Self> {
        observation
            .run
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            .then(|| Self(Arc::clone(observation)))
    }
}

impl Drop for RunTurn {
    fn drop(&mut self) {
        self.0.run.store(false, Ordering::Release);
        self.0.generation.fetch_add(1, Ordering::Release);
        (self.0.notify)();
    }
}

fn pane_last_output(shared: &Shared, pane: PaneId) -> Option<Instant> {
    shared
        .inner
        .lock()
        .terminals
        .get(&pane)
        .and_then(|terminal| terminal.last_output())
}

pub(super) fn pane_changed(inner: &mut ServerState, pane: PaneId) {
    if let Some(observation) = inner.pane_read_observations.get(&pane) {
        if let Some(observation) = observation.upgrade() {
            observation.generation.fetch_add(1, Ordering::Release);
            (observation.notify)();
        } else {
            inner.pane_read_observations.remove(&pane);
        }
    }
}

type ObserveFinish = Box<dyn FnOnce(&Arc<Shared>, Arc<Target>) + Send>;

struct OutputWait {
    target: Arc<Target>,
    generation: u64,
    deadline: Option<Instant>,
    finish: Option<ObserveFinish>,
}

impl terminal_requests::Pending for OutputWait {
    fn poll(&mut self, shared: &Arc<Shared>, now: Instant) -> bool {
        if self.target.state.continuation.ready() {
            return true;
        }
        let invalid = {
            let inner = shared.inner.lock();
            !inner
                .terminals
                .get(&self.target.pane)
                .is_some_and(|terminal| Arc::ptr_eq(terminal, &self.target.terminal))
                || self.target.guard_client && inner.client(self.target.client).is_none()
        };
        if self.target.generation() == self.generation
            && self.deadline.is_none_or(|deadline| now < deadline)
            && !invalid
            && !shared.command_queue_cancelled(self.target.client)
            && !shared.stopping.load(Ordering::Acquire)
        {
            return false;
        }
        self.finish.take().unwrap()(shared, Arc::clone(&self.target));
        true
    }

    fn next(&self) -> Option<Instant> {
        self.deadline
    }
}

fn observe_output(
    shared: &Arc<Shared>,
    target: Arc<Target>,
    generation: u64,
    deadline: Option<Instant>,
    finish: impl FnOnce(&Arc<Shared>, Arc<Target>) + Send + 'static,
) {
    shared.terminal_requests.push(OutputWait {
        target,
        generation,
        deadline,
        finish: Some(Box::new(finish)),
    });
}

fn timeout_deadline(started: Instant, timeout: Duration) -> Option<Instant> {
    (!timeout.is_zero()).then(|| started + timeout)
}

fn capture_screen(
    shared: &Arc<Shared>,
    target: Arc<Target>,
    options: CaptureOptions,
    finish: impl FnOnce(&Arc<Shared>, Arc<Target>, Result<String, DaemonError>) + Send + 'static,
) {
    if !target.check(shared) {
        return;
    }
    let request = target
        .terminal
        .capture_request(options, shared.terminal_requests.notifier());
    shared
        .terminal_requests
        .submit(request, move |shared, result| {
            if !target.check(shared) {
                return;
            }
            let result = result
                .map_err(TerminalCaptureError::from)
                .and_then(|result| result)
                .map_err(|error| match error {
                    TerminalCaptureError::ActorStopped => ServerError::PaneExited(target.pane),
                    TerminalCaptureError::TimedOut => {
                        ServerError::Internal("terminal capture timed out".to_owned())
                    }
                    error => ServerError::Internal(error.to_string()),
                })
                .map_err(Into::into);
            finish(shared, target, result);
        });
}

type PasteFinish = Box<dyn FnOnce(&Arc<Shared>, Arc<Target>, Result<(), DaemonError>) + Send>;

struct Typed(Option<Arc<TerminalSession>>);

impl Drop for Typed {
    fn drop(&mut self) {
        if let Some(terminal) = self.0.take()
            && shell_owns_pane(&terminal)
        {
            send_tokens(
                std::slice::from_ref(&terminal),
                &[zz_protocol::KeyToken::Raw(0x15)],
            );
        }
    }
}

struct Paste {
    bytes: Arc<[u8]>,
    tail: String,
    collapsed_before: usize,
    generation: u64,
    started: Instant,
    timeout: Duration,
    enter: bool,
    typed: Typed,
    finish: PasteFinish,
}

impl Paste {
    fn end(self, shared: &Arc<Shared>, target: Arc<Target>, result: Result<(), DaemonError>) {
        let Self { typed, finish, .. } = self;
        drop(typed);
        finish(shared, target, result);
    }

    fn not_echoed(self, shared: &Arc<Shared>, target: Arc<Target>) {
        let error = ServerError::InvalidCommand(format!(
            "{}: text not echoed within {} seconds; nothing submitted",
            target.pane,
            self.timeout.as_secs_f64()
        ));
        self.end(shared, target, Err(error.into()));
    }
}

fn paste(
    shared: &Arc<Shared>,
    target: Arc<Target>,
    text: &str,
    timeout: Duration,
    enter: bool,
    finish: PasteFinish,
) -> Result<(), DaemonError> {
    if shared.inner.lock().engine.pane_input_off(target.pane)? {
        return Err(
            ServerError::InvalidCommand(format!("{}: pane input is off", target.pane)).into(),
        );
    }
    let bytes = prepare_paste_buffer(text.as_bytes(), b"\r", true)
        .map_err(|error| ServerError::InvalidCommand(format!("send-text: {error}")))?;
    let mut paste = Paste {
        bytes: bytes.into(),
        tail: echo_tail(text),
        collapsed_before: 0,
        generation: target.generation(),
        started: Instant::now(),
        timeout,
        enter,
        typed: Typed(None),
        finish,
    };
    capture_screen(
        shared,
        target,
        CaptureOptions {
            join_wrapped: true,
            ..CaptureOptions::default()
        },
        move |shared, target, result| {
            let screen = match result {
                Ok(screen) => screen,
                Err(error) => {
                    paste.end(shared, target, Err(error));
                    return;
                }
            };
            paste.collapsed_before = screen.matches(PASTE_COLLAPSE_MARKER).count();
            paste.generation = target.generation();
            target
                .terminal
                .paste_prepared_bytes(None, Arc::clone(&paste.bytes), true);
            if paste.enter {
                paste.typed.0 = Some(Arc::clone(&target.terminal));
            }
            paste.started = Instant::now();
            poll_paste(shared, target, paste);
        },
    );
    Ok(())
}

fn poll_paste(shared: &Arc<Shared>, target: Arc<Target>, mut paste: Paste) {
    let generation = target.generation();
    capture_screen(
        shared,
        target,
        CaptureOptions {
            join_wrapped: true,
            ..CaptureOptions::default()
        },
        move |shared, target, result| {
            let screen = match result {
                Ok(screen) => screen,
                Err(error) => {
                    paste.end(shared, target, Err(error));
                    return;
                }
            };
            if (paste.tail.is_empty() || generation != paste.generation)
                && (collapse_whitespace(&screen).contains(&paste.tail)
                    || screen.matches(PASTE_COLLAPSE_MARKER).count() > paste.collapsed_before)
            {
                let result = if !paste.enter
                    || send_tokens(
                        std::slice::from_ref(&target.terminal),
                        &[zz_protocol::KeyToken::Named("Enter".to_owned())],
                    ) {
                    paste.typed.0 = None;
                    Ok(())
                } else {
                    Err(ServerError::InvalidCommand(format!(
                        "{}: input queue is full; text delivered but Enter not sent",
                        target.pane
                    ))
                    .into())
                };
                paste.end(shared, target, result);
            } else if !paste.timeout.is_zero() && paste.started.elapsed() >= paste.timeout {
                paste.not_echoed(shared, target);
            } else {
                observe_output(
                    shared,
                    target,
                    generation,
                    timeout_deadline(paste.started, paste.timeout),
                    move |shared, target| {
                        if target.generation() != generation {
                            poll_paste(shared, target, paste);
                        } else if target.check(shared) {
                            paste.not_echoed(shared, target);
                        }
                    },
                );
            }
        },
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn send_text(
    shared: &Arc<Shared>,
    client: ClientId,
    pane: PaneId,
    terminal: Arc<TerminalSession>,
    text: &str,
    timeout: Duration,
    enter: bool,
) -> Result<Execution, DaemonError> {
    let wait = terminal_requests::CommandWait::new(shared);
    let target = Target::new(shared, client, pane, terminal, wait.start());
    if let Err(error) = paste(
        shared,
        Arc::clone(&target),
        text,
        timeout,
        enter,
        Box::new(|_, target, result| {
            target.state.resolve(result.map(|()| Execution::default()));
        }),
    ) {
        target.state.resolve(Err(error));
    }
    wait.finish(shared, Execution::default())
}

pub(super) fn submit_agent_text(
    shared: &Arc<Shared>,
    client: ClientId,
    pane: PaneId,
    terminal: Arc<TerminalSession>,
    text: &str,
    finish: impl FnOnce(&Arc<Shared>, Result<(), DaemonError>) + Send + 'static,
) {
    let wait = terminal_requests::CommandWait::new(shared);
    let target = Target::new(shared, client, pane, terminal, wait.start());
    let finish = Arc::new(Mutex::new(Some(finish)));
    let completed = Arc::clone(&finish);
    if let Err(error) = paste(
        shared,
        Arc::clone(&target),
        text,
        SEND_TEXT_TIMEOUT,
        true,
        Box::new(move |shared, target, result| {
            completed.lock().take().unwrap()(shared, result);
            target.state.complete();
        }),
    ) {
        finish.lock().take().unwrap()(shared, Err(error));
        target.state.complete();
    }
}

const WAIT_PROMPT_SETTLE: Duration = Duration::from_millis(200);

struct Wait {
    parsed: ParsedWaitPane,
    started: Instant,
    scan_start: Option<usize>,
    park: Option<Arc<Shared>>,
    settle: Option<Instant>,
}

pub(super) fn wait_pane(
    shared: &Arc<Shared>,
    client: ClientId,
    pane: PaneId,
    terminal: Arc<TerminalSession>,
    parsed: ParsedWaitPane,
) -> Result<Execution, DaemonError> {
    let wait = terminal_requests::CommandWait::new(shared);
    let target = Target::new(shared, client, pane, terminal, wait.start());
    let scan_start = (!matches!(
        parsed.condition,
        PaneWaitCondition::Until(_) | PaneWaitCondition::Regex(_)
    ))
    .then_some(0);
    poll_wait(
        shared,
        target,
        Wait {
            parsed,
            started: Instant::now(),
            scan_start,
            park: Some(Arc::clone(shared)),
            settle: None,
        },
    );
    wait.finish(shared, Execution::default())
}

fn poll_wait(shared: &Arc<Shared>, target: Arc<Target>, mut wait: Wait) {
    let generation = target.generation();
    if !target.check(shared) {
        return;
    }
    match wait.parsed.condition {
        PaneWaitCondition::Exit => unreachable!(),
        PaneWaitCondition::Idle(dwell) => {
            let last_output = pane_last_output(shared, target.pane)
                .unwrap_or(wait.started)
                .max(wait.started);
            if wait.park.is_none() && last_output.elapsed() >= dwell {
                target.state.resolve(Ok(Execution::default()));
            } else {
                finish_wait_poll(shared, target, wait, generation);
            }
        }
        _ => {
            let quiet = target.terminal.last_output();
            let keep_idle_prompt =
                quiet.is_none_or(|output| output.elapsed() >= WAIT_PROMPT_SETTLE);
            capture_screen(
                shared,
                target,
                CaptureOptions {
                    start: CaptureBoundary::HistoryStart,
                    join_wrapped: true,
                    preserve_trailing: true,
                    skip_input: true,
                    keep_idle_prompt,
                    ..CaptureOptions::default()
                },
                move |shared, target, result| {
                    let screen = match result {
                        Ok(screen) => screen,
                        Err(error) => {
                            target.state.resolve(Err(error));
                            return;
                        }
                    };
                    let latest = target.terminal.last_output();
                    let settled = keep_idle_prompt && latest == quiet;
                    wait.settle = latest
                        .filter(|_| !settled)
                        .map(|output| output + WAIT_PROMPT_SETTLE);
                    let scan_start = *wait.scan_start.get_or_insert_with(|| {
                        screen
                            .lines()
                            .count()
                            .saturating_sub(usize::from(target.terminal.size().1))
                    });
                    if (settled || !keep_idle_prompt)
                        && let Some(line) = screen
                            .lines()
                            .rev()
                            .take(
                                screen
                                    .lines()
                                    .count()
                                    .saturating_sub(scan_start)
                                    .min(10_000),
                            )
                            .skip_while(|line| line.trim().is_empty())
                            .take(wait.parsed.tail.unwrap_or(usize::MAX))
                            .find(|line| match &wait.parsed.condition {
                                PaneWaitCondition::Until(text) => line.contains(text),
                                PaneWaitCondition::Regex(regex) => regex.is_match(line),
                                _ => false,
                            })
                    {
                        target.state.resolve(Ok(Execution {
                            output: format!("{line}\n").into(),
                            effects: Vec::new(),
                        }));
                    } else {
                        finish_wait_poll(shared, target, wait, generation);
                    }
                },
            );
        }
    }
}

fn finish_wait_poll(shared: &Arc<Shared>, target: Arc<Target>, mut wait: Wait, generation: u64) {
    if !wait.parsed.timeout.is_zero() && wait.started.elapsed() >= wait.parsed.timeout {
        let condition = match &wait.parsed.condition {
            PaneWaitCondition::Exit => "--exit".to_owned(),
            PaneWaitCondition::Idle(dwell) => format!("--idle {}", dwell.as_millis()),
            PaneWaitCondition::Until(text) => format!("--until {text:?}"),
            PaneWaitCondition::Regex(regex) => format!("--regex {:?}", regex.as_str()),
        };
        target.state.resolve(Err(DaemonError::CommandExit {
            output: format!(
                "wait-pane: timed out waiting for {condition} on {}\n",
                target.pane
            )
            .into(),
            exit_code: 124,
        }));
    } else {
        if let Some(item) = wait.park.take() {
            item.report_command_queue_park();
        }
        let idle = if let PaneWaitCondition::Idle(dwell) = wait.parsed.condition {
            Some(
                pane_last_output(shared, target.pane)
                    .unwrap_or(wait.started)
                    .max(wait.started)
                    + dwell,
            )
        } else {
            None
        };
        let deadline = timeout_deadline(wait.started, wait.parsed.timeout)
            .into_iter()
            .chain(idle)
            .chain(wait.settle)
            .min();
        observe_output(
            shared,
            target,
            generation,
            deadline,
            move |shared, target| {
                if target.generation() != generation
                    || wait.settle.is_some_and(|settle| settle <= Instant::now())
                    || matches!(
                        wait.parsed.condition,
                        PaneWaitCondition::Idle(_) | PaneWaitCondition::Exit
                    )
                {
                    poll_wait(shared, target, wait);
                } else if target.check(shared) {
                    finish_wait_poll(shared, target, wait, generation);
                }
            },
        );
    }
}

const RUN_PANE_SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "ksh", "mksh", "ash", "yash", "busybox",
];
const RUN_PANE_START_GRACE: Duration = Duration::from_secs(2);
const RUN_PANE_TYPE_FLOOR: Duration = Duration::from_millis(500);
const RUN_PANE_LINE_LIMIT: usize = 1023;

struct Run {
    parsed: ParsedRunPane,
    marker: String,
    started: Instant,
    output: String,
    collecting: bool,
    queued: bool,
    tail: String,
    start_check: Option<Instant>,
    _turn: Option<RunTurn>,
}

fn process_name(pid: u32) -> String {
    path_listing::foreground_basename(
        &zz_daemon_client::process_info::command_name(pid).unwrap_or_default(),
    )
}

fn other_group_member(terminal: &TerminalSession, group: u32) -> Option<String> {
    let shell = terminal.process_id()?;
    zz_daemon_client::process_info::descendants(shell)
        .into_iter()
        .filter(|pid| {
            *pid != group && zz_daemon_client::process_info::process_group(*pid) == Some(group)
        })
        .map(process_name)
        .rfind(|name| !name.is_empty())
}

fn foreground_shell(terminal: &TerminalSession) -> Result<Option<String>, String> {
    let Some(foreground) = terminal_foreground_process(terminal) else {
        return Ok(None);
    };
    let name = process_name(foreground);
    if name.is_empty() {
        Err(other_group_member(terminal, foreground)
            .unwrap_or_else(|| "a program that already exited".to_owned()))
    } else if !RUN_PANE_SHELLS.contains(&name.as_str()) {
        Err(name)
    } else if Some(foreground) == terminal.process_id() {
        Ok(Some(name))
    } else if terminal.tty_canonical() != Some(false) {
        Err(format!("a {name} script"))
    } else if let Some(other) = other_group_member(terminal, foreground) {
        Err(other)
    } else {
        Ok(Some(name))
    }
}

fn shell_owns_pane(terminal: &TerminalSession) -> bool {
    matches!(foreground_shell(terminal), Ok(Some(_)))
}

fn refuse_run(shared: &Shared, target: &Target, reason: &str) {
    shared.record_command_stderr(target.client, &format!("run-pane: {reason}"));
    target.state.resolve(Err(DaemonError::CommandExit {
        output: RawText::default(),
        exit_code: 3,
    }));
}

fn send_byte(target: &Target, byte: u8) -> bool {
    send_tokens(
        std::slice::from_ref(&target.terminal),
        &[zz_protocol::KeyToken::Raw(byte)],
    )
}

fn shell_rejected(screen: &str, marker: &str) -> bool {
    let typed = format!("'{marker}-BEGIN'");
    screen.contains(&typed)
        && screen
            .lines()
            .rev()
            .take_while(|line| !line.contains(&typed))
            .any(|line| {
                let line = line.to_ascii_lowercase();
                line.contains("syntax error") || line.contains("parse error")
            })
}

fn echo_is_last(screen: &str, tail: &str) -> bool {
    let mut last = screen
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(2)
        .collect::<Vec<_>>();
    last.reverse();
    collapse_whitespace(&last.join(" ")).contains(tail)
}

pub(super) fn run_pane(
    shared: &Arc<Shared>,
    client: ClientId,
    pane: PaneId,
    terminal: Arc<TerminalSession>,
    parsed: ParsedRunPane,
) -> Result<Execution, DaemonError> {
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).map_err(std::io::Error::other)?;
    let run = Run {
        parsed,
        marker: format!("ZZRUN-{:016x}", u64::from_ne_bytes(nonce)),
        started: Instant::now(),
        output: String::new(),
        collecting: false,
        queued: false,
        tail: String::new(),
        start_check: None,
        _turn: None,
    };
    let wait = terminal_requests::CommandWait::new(shared);
    let target = Target::new(shared, client, pane, terminal, wait.start());
    shared.report_command_queue_park();
    start_run(shared, target, run);
    wait.finish(shared, Execution::default())
}

fn start_run(shared: &Arc<Shared>, target: Arc<Target>, mut run: Run) {
    if !target.check(shared) {
        return;
    }
    if shared
        .inner
        .lock()
        .engine
        .state
        .pane(target.pane)
        .is_some_and(|pane| pane.dead)
    {
        refuse_run(
            shared,
            &target,
            &format!(
                "{} is dead (its program exited); nothing was typed",
                target.pane
            ),
        );
        return;
    }
    let generation = target.generation();
    let deadline = timeout_deadline(run.started, run.parsed.timeout);
    let now = Instant::now();
    let busy = format!(
        "{} is busy with another run-pane; nothing was typed",
        target.pane
    );
    if deadline.is_some_and(|deadline| now >= deadline) {
        let reason = if target.observation.run.load(Ordering::Acquire) {
            busy
        } else {
            format!(
                "{} did not echo the command within {}s; nothing was submitted",
                target.pane,
                run.parsed.timeout.as_secs_f64()
            )
        };
        refuse_run(shared, &target, &reason);
        return;
    }
    let Some(turn) = RunTurn::claim(&target.observation) else {
        observe_output(
            shared,
            target,
            generation,
            deadline,
            move |shared, target| {
                run.queued = true;
                start_run(shared, target, run);
            },
        );
        return;
    };
    if run.queued
        && deadline
            .is_some_and(|deadline| deadline.saturating_duration_since(now) < RUN_PANE_TYPE_FLOOR)
    {
        drop(turn);
        refuse_run(shared, &target, &busy);
        return;
    }
    run._turn = Some(turn);
    let shell = match foreground_shell(&target.terminal) {
        Ok(shell) => shell,
        Err(program) => {
            refuse_run(
                shared,
                &target,
                &format!(
                    "{} is running {program}, not a shell; nothing was typed",
                    target.pane
                ),
            );
            return;
        }
    };
    let eval = if shell.as_deref().is_none_or(|shell| shell == "zsh") {
        "eval"
    } else {
        "command eval"
    };
    let line = run_pane_line(&run.marker, eval, &run.parsed.command);
    if line.len() > RUN_PANE_LINE_LIMIT
        && (shell
            .as_deref()
            .is_some_and(|shell| !matches!(shell, "bash" | "zsh"))
            || target.terminal.tty_canonical() == Some(true))
    {
        refuse_run(
            shared,
            &target,
            &format!(
                "the command is {} bytes once quoted, more than {}'s shell takes on one line; put \
                 it in a script file",
                line.len(),
                target.pane
            ),
        );
        return;
    }
    run.tail = echo_tail(&line);
    let remaining = deadline.map_or(Duration::ZERO, |deadline| {
        deadline.saturating_duration_since(now)
    });
    if let Err(error) = paste(
        shared,
        Arc::clone(&target),
        &line,
        remaining,
        true,
        Box::new(move |shared, target, result| match result {
            Ok(()) => {
                run.start_check = Some(Instant::now() + RUN_PANE_START_GRACE);
                poll_run(shared, target, run);
            }
            Err(_)
                if !run.parsed.timeout.is_zero() && run.started.elapsed() >= run.parsed.timeout =>
            {
                refuse_run(
                    shared,
                    &target,
                    &format!(
                        "{} did not echo the command within {}s; nothing was submitted",
                        target.pane,
                        run.parsed.timeout.as_secs_f64()
                    ),
                );
            }
            Err(error) => target.state.resolve(Err(error)),
        }),
    ) {
        target.state.resolve(Err(error));
    }
}

fn timeout_run(shared: &Shared, target: &Target, run: Run) {
    shared.record_command_stderr(
        target.client,
        &format!(
            "run-pane: timed out after {}s on {}; the command keeps running",
            run.parsed.timeout.as_secs_f64(),
            target.pane
        ),
    );
    target.state.resolve(Err(DaemonError::CommandExit {
        output: run.output.into(),
        exit_code: 125,
    }));
}

fn poll_run(shared: &Arc<Shared>, target: Arc<Target>, mut run: Run) {
    let generation = target.generation();
    capture_screen(
        shared,
        target,
        CaptureOptions {
            start: CaptureBoundary::Relative(
                -i64::try_from(RUN_PANE_CAPTURE_LINES).unwrap_or(i64::MAX),
            ),
            join_wrapped: true,
            preserve_trailing: true,
            ..CaptureOptions::default()
        },
        move |shared, target, result| {
            let screen = match result {
                Ok(screen) => screen,
                Err(error) => {
                    target.state.resolve(Err(error));
                    return;
                }
            };
            let (captured, exit_code, begin) =
                run_pane_result(&screen, &run.marker, run.collecting);
            if let Some(captured) = captured {
                run.collecting = true;
                run.start_check = None;
                captured.clone_into(&mut run.output);
            }
            if let Some(exit_code) = exit_code {
                if begin == RunStart::Dropped {
                    let limit = shared
                        .inner
                        .lock()
                        .engine
                        .history_limit_for_pane(target.pane)
                        .unwrap_or_default();
                    let cut = if limit > RUN_PANE_CAPTURE_LINES {
                        format!("run-pane keeps the last {RUN_PANE_CAPTURE_LINES} lines")
                    } else {
                        format!("{} history-limit {limit}", target.pane)
                    };
                    shared.record_command_stderr(
                        target.client,
                        &format!(
                            "run-pane: output truncated to its last {} lines ({cut}); redirect to \
                             a file for all of it",
                            run.output.lines().count()
                        ),
                    );
                } else if begin == RunStart::Joined {
                    shared.record_command_stderr(
                        target.client,
                        &format!(
                            "run-pane: {}'s shell joined the command to an unfinished line (one \
                             ending in && or \\, for example); the output may include text from \
                             that line",
                            target.pane
                        ),
                    );
                }
                target.state.resolve(if exit_code == 0 {
                    Ok(Execution {
                        output: run.output.into(),
                        effects: Vec::new(),
                    })
                } else {
                    Err(DaemonError::CommandExit {
                        output: run.output.into(),
                        exit_code,
                    })
                });
                return;
            }
            let timed_out =
                !run.parsed.timeout.is_zero() && run.started.elapsed() >= run.parsed.timeout;
            if run
                .start_check
                .is_some_and(|check| timed_out || Instant::now() >= check)
            {
                run.start_check = None;
                if shell_rejected(&screen, &run.marker) && shell_owns_pane(&target.terminal) {
                    refuse_run(
                        shared,
                        &target,
                        &format!(
                            "{}'s shell rejected the line (it was inside an unfinished construct, \
                             such as case x in); nothing ran",
                            target.pane
                        ),
                    );
                    return;
                }
                if echo_is_last(&screen, &run.tail)
                    && shell_owns_pane(&target.terminal)
                    && send_byte(&target, 0x03)
                {
                    refuse_run(
                        shared,
                        &target,
                        &format!(
                            "{}'s shell did not start the command within {}s (it was waiting \
                             for more input, such as an unclosed quote); sent C-c, nothing ran",
                            target.pane,
                            if run.parsed.timeout.is_zero() {
                                RUN_PANE_START_GRACE
                            } else {
                                RUN_PANE_START_GRACE.min(run.parsed.timeout)
                            }
                            .as_secs_f64()
                        ),
                    );
                    return;
                }
            }
            if timed_out {
                timeout_run(shared, &target, run);
            } else {
                let deadline = timeout_deadline(run.started, run.parsed.timeout)
                    .into_iter()
                    .chain(run.start_check)
                    .min();
                observe_output(
                    shared,
                    target,
                    generation,
                    deadline,
                    move |shared, target| {
                        if target.generation() != generation || run.start_check.is_some() {
                            poll_run(shared, target, run);
                        } else if target.check(shared) {
                            timeout_run(shared, &target, run);
                        }
                    },
                );
            }
        },
    );
}

#[cfg(all(test, unix))]
#[path = "terminal_reads_e10_tests.rs"]
mod e10_tests;

#[cfg(all(test, unix))]
#[path = "terminal_reads_e05_tests.rs"]
mod tests;

#[cfg(unix)]
pub(super) struct MouseBinding {
    pub(super) client: ClientId,
    pub(super) kind: ClientKind,
    pub(super) pane: PaneId,
    pub(super) terminal: Option<Arc<TerminalSession>>,
    pub(super) context: ExecutionContext,
    pub(super) commands: Vec<CommandInvocation>,
    pub(super) repeat_binding: bool,
    pub(super) done: cmdq::WaitContinuation,
}

#[cfg(unix)]
impl Shared {
    pub(super) fn resume_mouse_binding(self: &Arc<Self>, mut binding: MouseBinding) {
        let valid = {
            let inner = self.inner.lock();
            inner.client(binding.client).is_some()
                && client_is_attached_to_pane(&inner, binding.client, binding.pane)
                && binding.terminal.as_ref().is_none_or(|terminal| {
                    inner
                        .terminals
                        .get(&binding.pane)
                        .is_some_and(|current| Arc::ptr_eq(current, terminal))
                })
        };
        if !valid {
            binding.done.complete();
            return;
        }
        binding.context.set_repeat_binding(binding.repeat_binding);
        let client = binding.client;
        let kind = binding.kind;
        let title = binding.commands.first().map_or_else(
            || "command output".to_owned(),
            |command| {
                if binding.commands.len() == 1 {
                    command.name.clone()
                } else {
                    "command output".to_owned()
                }
            },
        );
        self.enqueue_inserted_task(
            client,
            kind,
            &binding.context,
            &InsertedCommandSource::Commands(binding.commands),
            "<mouse-binding>",
            None,
            false,
            None,
            Box::new(move |shared, context, result| {
                match result {
                    Ok(result) => shared.route_background_inserted_output(
                        client,
                        kind,
                        context,
                        title,
                        &result.output,
                    ),
                    Err(error) => {
                        shared.publish_background_command_error(client, context, &error, false);
                    }
                }
                shared.sync_key_table(client, false);
                binding.done.complete();
                shared.accept_wake.wake();
            }),
        );
    }
}
