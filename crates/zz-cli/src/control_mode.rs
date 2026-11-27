use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, IsTerminal as _, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(not(unix))]
use std::io::BufRead as _;
#[cfg(unix)]
use std::os::fd::{AsFd as _, AsRawFd as _, OwnedFd};
#[cfg(any(not(unix), test))]
use std::sync::mpsc;
#[cfg(any(not(unix), test))]
use std::thread;

use zz_daemon_client::{Endpoint, InteractiveClient};
use zz_protocol::{
    CommandInvocation, CommandResponse, ControlSourceFileEvent, EventPayload, ExecOutcome,
    MuxSnapshot, ProtocolMessage, SessionId, WindowId,
};
#[cfg(test)]
use zz_protocol::{PreparedCommand, PreparedCommandResult, RawText, ServerError, StdoutClaim};

use super::{
    SocketSelectionSource, connect_or_spawn_daemon, format_local_command_error,
    tmux_command_starts_server, tmux_label_creation_error,
};

#[cfg(test)]
const CONTROL_PARSE_SOURCE: &str = "<control>";
const DCS: &[u8] = b"\x1bP1000p";
const ST: &[u8] = b"\x1b\\";

static TERMINATION_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
struct ControlSignal(libc::sigaction);

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "install and restore the control client's SIGTERM disposition"
)]
impl ControlSignal {
    fn install() -> io::Result<Self> {
        extern "C" fn terminate(_: libc::c_int) {
            TERMINATION_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        TERMINATION_REQUESTED.store(false, std::sync::atomic::Ordering::Relaxed);
        unsafe {
            let mut previous = std::mem::zeroed();
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = terminate as *const () as usize;
            libc::sigemptyset(&raw mut action.sa_mask);
            if libc::sigaction(libc::SIGTERM, &raw const action, &raw mut previous) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(previous))
        }
    }
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "restore the disposition saved by the control signal scope"
)]
impl Drop for ControlSignal {
    fn drop(&mut self) {
        unsafe { libc::sigaction(libc::SIGTERM, &raw const self.0, std::ptr::null_mut()) };
    }
}

struct ControlReceiver {
    source: ControlSource,
    pending: VecDeque<ProtocolMessage>,
}

enum ControlSource {
    #[cfg(any(not(unix), test))]
    Channel(mpsc::Receiver<MainEvent>),
    #[cfg(unix)]
    Direct(DirectControl),
}

impl ControlReceiver {
    #[cfg(any(not(unix), test))]
    fn new(events: mpsc::Receiver<MainEvent>) -> Self {
        Self {
            source: ControlSource::Channel(events),
            pending: VecDeque::new(),
        }
    }

    #[cfg(unix)]
    fn direct(
        client: Arc<InteractiveClient>,
        stdio: bool,
        pending: VecDeque<ProtocolMessage>,
    ) -> io::Result<Self> {
        Ok(Self {
            source: ControlSource::Direct(DirectControl::new(client, stdio)?),
            pending,
        })
    }

    #[cfg(unix)]
    fn take_sync(&mut self) -> Option<u64> {
        match &mut self.source {
            ControlSource::Direct(direct) => direct.sync.take(),
            #[cfg(test)]
            ControlSource::Channel(_) => None,
        }
    }

    #[cfg(unix)]
    fn take_submitted(&mut self) -> usize {
        match &mut self.source {
            ControlSource::Direct(direct) => std::mem::take(&mut direct.input.submitted_taken),
            #[cfg(test)]
            ControlSource::Channel(_) => 0,
        }
    }

    #[cfg(unix)]
    fn idle_received(&self) -> Option<u64> {
        match &self.source {
            ControlSource::Direct(direct) => (direct.stdio
                && self.pending.is_empty()
                && direct.pending_protocol.is_none()
                && direct.sync.is_none()
                && direct.input.submitted_taken == 0
                && direct.input.bytes.is_empty()
                && !direct.input.has_event())
            .then_some(direct.received),
            #[cfg(test)]
            ControlSource::Channel(_) => None,
        }
    }

    fn receive(&mut self, timeout: Option<std::time::Duration>) -> io::Result<Option<MainEvent>> {
        self.receive_with_probe(timeout, true, true)
    }

    fn receive_with_probe(
        &mut self,
        timeout: Option<std::time::Duration>,
        mut probe_protocol: bool,
        mut poll_empty: bool,
    ) -> io::Result<Option<MainEvent>> {
        loop {
            if let Some(message) = self.pending.pop_front() {
                return Ok(Some(MainEvent::Protocol(Box::new(message))));
            }
            let event = match &mut self.source {
                #[cfg(any(not(unix), test))]
                ControlSource::Channel(events) => {
                    let _ = (probe_protocol, poll_empty);
                    if let Some(timeout) = timeout {
                        match events.recv_timeout(timeout) {
                            Ok(event) => Some(event),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                Some(MainEvent::Disconnected)
                            }
                        }
                    } else {
                        match events.try_recv() {
                            Ok(event) => Some(event),
                            Err(mpsc::TryRecvError::Empty) => None,
                            Err(mpsc::TryRecvError::Disconnected) => Some(MainEvent::Disconnected),
                        }
                    }
                }
                #[cfg(unix)]
                ControlSource::Direct(direct) => {
                    direct.receive(timeout, probe_protocol, poll_empty)?
                }
            };
            probe_protocol = true;
            poll_empty = true;
            match event {
                Some(MainEvent::Protocol(message))
                    if matches!(message.as_ref(), ProtocolMessage::Batch(_)) =>
                {
                    let ProtocolMessage::Batch(batch) = *message else {
                        unreachable!();
                    };
                    self.pending = batch
                        .messages()
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                        .into();
                }
                event => return Ok(event),
            }
        }
    }
}

#[cfg(unix)]
struct DirectControl {
    client: Arc<InteractiveClient>,
    socket: OwnedFd,
    input: ControlInput,
    readable: Vec<rustix::event::FdSetElement>,
    pending_protocol: Option<MainEvent>,
    disconnected: bool,
    prefer_stdin: bool,
    stdio: bool,
    received: u64,
    sync: Option<u64>,
}

#[cfg(unix)]
impl DirectControl {
    fn new(client: Arc<InteractiveClient>, stdio: bool) -> io::Result<Self> {
        let mut direct = Self::with_input(client, io::stdin().as_fd().try_clone_to_owned()?)?;
        direct.input.enabled = false;
        direct.input.remote = stdio;
        direct.stdio = stdio;
        Ok(direct)
    }

    fn with_input(client: Arc<InteractiveClient>, input: OwnedFd) -> io::Result<Self> {
        let socket = client.receive_fd()?;
        let input = ControlInput::new(input);
        let bound = socket.as_raw_fd().max(input.fd.as_raw_fd()) + 1;
        Ok(Self {
            client,
            socket,
            input,
            readable: vec![
                rustix::event::FdSetElement::default();
                rustix::event::fd_set_num_elements(2, bound)
            ],
            pending_protocol: None,
            disconnected: false,
            prefer_stdin: false,
            stdio: false,
            received: 0,
            sync: None,
        })
    }

    fn read_protocol(&mut self, read_socket: bool) {
        while self.pending_protocol.is_none() && !self.disconnected {
            let result = if read_socket {
                self.client.try_recv()
            } else {
                self.client.try_recv_buffered()
            };
            match result {
                Ok(Some(message)) => {
                    if self.stdio {
                        self.received += 1;
                        match *message {
                            ProtocolMessage::ControlStdin {
                                bytes,
                                submitted,
                                closed,
                                error,
                            } => {
                                self.input.bytes.extend_from_slice(&bytes);
                                self.input.presubmitted += usize::from(submitted);
                                self.input.closed |= closed || error.is_some();
                                if error.is_some() {
                                    self.input.error = error;
                                }
                                return;
                            }
                            ProtocolMessage::ControlStdioSync { next_number } => {
                                self.sync = Some(next_number);
                            }
                            message => {
                                self.pending_protocol =
                                    Some(MainEvent::Protocol(Box::new(message)));
                            }
                        }
                    } else {
                        self.pending_protocol = Some(MainEvent::Protocol(message));
                    }
                }
                Ok(None) => return,
                Err(_) => {
                    self.disconnected = true;
                    self.pending_protocol = Some(MainEvent::Disconnected);
                }
            }
        }
    }

    #[allow(
        unsafe_code,
        reason = "select only borrows the two owned open descriptors"
    )]
    fn receive(
        &mut self,
        timeout: Option<std::time::Duration>,
        probe_protocol: bool,
        poll_empty: bool,
    ) -> io::Result<Option<MainEvent>> {
        if self.prefer_stdin && self.input.has_event() {
            return Ok(self.take_ready(false));
        }
        if probe_protocol {
            self.read_protocol(true);
        } else if !poll_empty {
            self.read_protocol(self.input.has_event());
        }
        if self.input.has_event() || (self.pending_protocol.is_some() && !self.prefer_stdin) {
            return Ok(self.take_ready(false));
        }
        if !poll_empty && self.pending_protocol.is_none() && !self.disconnected {
            return Ok(None);
        }
        self.readable.fill(rustix::event::FdSetElement::default());
        if self.input.can_read() && !self.input.has_event() {
            rustix::event::fd_set_insert(&mut self.readable, self.input.fd.as_raw_fd());
        }
        if self.pending_protocol.is_none() && !self.disconnected {
            rustix::event::fd_set_insert(&mut self.readable, self.socket.as_raw_fd());
        }
        let timeout = if self.pending_protocol.is_some() || self.input.has_event() {
            std::time::Duration::ZERO
        } else {
            timeout.unwrap_or_default()
        };
        let timeout = rustix::event::Timespec {
            tv_sec: timeout.as_secs().try_into().unwrap_or(i64::MAX),
            tv_nsec: i64::from(timeout.subsec_nanos()),
        };
        let bound = rustix::event::fd_set_bound(&self.readable);
        match unsafe {
            rustix::event::select(bound, Some(&mut self.readable), None, None, Some(&timeout))
        } {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let mut input_ready = false;
        let mut socket_ready = false;
        for fd in rustix::event::FdSetIter::new(&self.readable) {
            input_ready |= fd == self.input.fd.as_raw_fd();
            socket_ready |= fd == self.socket.as_raw_fd();
        }
        if input_ready {
            self.input.read();
        }
        if socket_ready {
            self.read_protocol(true);
        }
        Ok(self.take_ready(true))
    }

    fn take_ready(&mut self, polled: bool) -> Option<MainEvent> {
        if self.input.has_event() && (self.prefer_stdin || self.pending_protocol.is_none()) {
            self.prefer_stdin = false;
            return self.input.event().map(MainEvent::Stdin);
        }
        if let Some(event) = self.pending_protocol.take() {
            self.prefer_stdin = !polled;
            return Some(event);
        }
        self.disconnected.then_some(MainEvent::Disconnected)
    }
}

#[cfg(unix)]
struct ControlInput {
    enabled: bool,
    remote: bool,
    presubmitted: usize,
    submitted_taken: usize,
    fd: OwnedFd,
    bytes: Vec<u8>,
    closed: bool,
    eof_sent: bool,
    error: Option<String>,
}

#[cfg(unix)]
impl ControlInput {
    fn new(fd: OwnedFd) -> Self {
        Self {
            enabled: true,
            remote: false,
            presubmitted: 0,
            submitted_taken: 0,
            fd,
            bytes: Vec::new(),
            closed: false,
            eof_sent: false,
            error: None,
        }
    }

    fn can_read(&self) -> bool {
        self.enabled && !self.closed && !self.remote
    }

    fn has_event(&self) -> bool {
        if !self.enabled {
            return false;
        }
        self.error.is_some() || (self.closed && !self.eof_sent) || self.bytes.contains(&b'\n')
    }

    fn read(&mut self) {
        let mut bytes = [0; 4096];
        match rustix::io::read(&self.fd, &mut bytes) {
            Ok(0) => self.closed = true,
            Ok(count) => self.bytes.extend_from_slice(&bytes[..count]),
            Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {}
            Err(error) => {
                self.closed = true;
                self.error = Some(error.to_string());
            }
        }
    }

    fn event(&mut self) -> Option<StdinEvent> {
        if let Some(error) = self.error.take() {
            self.bytes.clear();
            self.eof_sent = true;
            return Some(StdinEvent::Error(error));
        }
        let bytes = if let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<_> = self.bytes.drain(..=end).collect();
            line.pop();
            line
        } else if self.closed && !self.bytes.is_empty() {
            std::mem::take(&mut self.bytes)
        } else if self.closed && !self.eof_sent {
            self.eof_sent = true;
            return Some(StdinEvent::Eof);
        } else {
            return None;
        };
        Some(match String::from_utf8(bytes) {
            Ok(line) => {
                if self.presubmitted != 0 {
                    self.presubmitted -= 1;
                    self.submitted_taken += 1;
                }
                StdinEvent::Line(line)
            }
            Err(error) => {
                self.bytes.clear();
                self.closed = true;
                self.eof_sent = true;
                StdinEvent::Error(error.to_string())
            }
        })
    }
}

fn receive_control_event(receiver: &mut ControlReceiver) -> io::Result<MainEvent> {
    receive_control_event_with_probe(receiver, true)
}

fn receive_control_event_with_probe(
    receiver: &mut ControlReceiver,
    mut probe_protocol: bool,
) -> io::Result<MainEvent> {
    loop {
        if TERMINATION_REQUESTED.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        if let Some(event) = receiver.receive_with_probe(
            Some(std::time::Duration::from_millis(20)),
            probe_protocol,
            true,
        )? {
            return Ok(event);
        }
        probe_protocol = true;
    }
}

fn receive_buffered_control_event<W: Write>(
    receiver: &mut ControlReceiver,
    output: &mut ControlWriter<W>,
) -> io::Result<MainEvent> {
    if TERMINATION_REQUESTED.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(io::Error::from(io::ErrorKind::Interrupted));
    }
    let event = if let Some(event) = receiver.receive_with_probe(None, output.block_open, false)? {
        if !matches!(
            &event,
            MainEvent::Protocol(message)
                if matches!(message.as_ref(), ProtocolMessage::CommandResponse(_))
        ) {
            output.output.flush()?;
        }
        event
    } else {
        output.output.flush()?;
        receive_control_event_with_probe(receiver, false)?
    };
    #[cfg(unix)]
    if let Some(next_number) = receiver.take_sync() {
        output.next_number = next_number;
    }
    Ok(event)
}

pub(crate) fn run(
    socket_path: &Path,
    socket_source: SocketSelectionSource,
    mux_config_files: &[PathBuf],
    no_start_server: bool,
    level: u8,
    arguments: &[zz_protocol::RawText],
) -> ExitCode {
    let commands = control_commands(arguments);
    let start_server = !no_start_server
        && commands
            .first()
            .is_some_and(|command| tmux_command_starts_server(&command.name));
    if let Some(error) = tmux_label_creation_error(socket_path, socket_source, start_server) {
        eprintln!("{}", error.message);
        return ExitCode::FAILURE;
    }
    serve(level, commands, Some(socket_path), || {
        if start_server {
            connect_or_spawn_daemon(
                socket_path,
                None,
                mux_config_files,
                |startup_config_owner| {
                    InteractiveClient::connect_control_with_startup_owner(
                        socket_path,
                        startup_config_owner,
                    )
                },
                InteractiveClient::server_hello,
            )
        } else {
            InteractiveClient::connect_control(socket_path)
        }
        .map_err(|error| format_local_command_error(socket_path, error))
    })
}

pub(crate) fn run_host(
    host: &str,
    endpoint: &Endpoint,
    level: u8,
    arguments: &[zz_protocol::RawText],
) -> ExitCode {
    serve(level, control_commands(arguments), None, || {
        InteractiveClient::connect_control_endpoint(endpoint)
            .map_err(|error| super::host_command_error_message(host, &error))
    })
}

fn control_commands(arguments: &[zz_protocol::RawText]) -> Vec<CommandInvocation> {
    if arguments.is_empty() {
        vec![CommandInvocation::new("new-session", [] as [&str; 0])]
    } else {
        super::split_command_chain(arguments)
    }
}

fn serve(
    level: u8,
    commands: Vec<CommandInvocation>,
    local_socket: Option<&Path>,
    connect: impl FnOnce() -> Result<InteractiveClient, String>,
) -> ExitCode {
    #[cfg(unix)]
    let _signal = match ControlSignal::install() {
        Ok(signal) => signal,
        Err(error) => {
            eprintln!("zz: {error}");
            return ExitCode::FAILURE;
        }
    };
    let client = match connect() {
        Ok(client) => Arc::new(client),
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    #[cfg(unix)]
    let (flags, stash) = match local_socket {
        Some(socket_path) => match start_stdio(&client) {
            Ok(started) => started,
            Err(error) => {
                eprintln!("{}", format_local_command_error(socket_path, error.into()));
                return ExitCode::FAILURE;
            }
        },
        None => (None, VecDeque::new()),
    };
    #[cfg(not(unix))]
    let (flags, stash) = {
        let _ = local_socket;
        (None::<()>, VecDeque::new())
    };
    let stdio = flags.is_some();
    let terminal = match ControlTerminal::enter(level >= 2) {
        Ok(terminal) => terminal,
        Err(error) => {
            eprintln!("zz: {error}");
            return ExitCode::FAILURE;
        }
    };
    let sink = if let Some(flags) = flags {
        ControlOutput::daemon(Arc::clone(&client), flags)
    } else {
        ControlOutput::Stdout(io::BufWriter::new(io::stdout().lock()))
    };
    let mut output = ControlWriter::new(sink, level >= 2);
    let result = output
        .start()
        .and_then(|()| drive(&client, commands, &mut output, stdio, stash));
    let result = if TERMINATION_REQUESTED.load(std::sync::atomic::Ordering::Relaxed) {
        #[cfg(unix)]
        if !stdio {
            let _ = client.shutdown();
        }
        let terminated = output.terminate().map(|()| 0);
        #[cfg(unix)]
        if stdio {
            output.output.close();
            let _ = client.shutdown();
        }
        terminated
    } else {
        result
    };
    drop(terminal);
    output.close();
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("zz: {error}");
            ExitCode::FAILURE
        }
    }
}

fn drive(
    client: &Arc<InteractiveClient>,
    initial: Vec<CommandInvocation>,
    output: &mut ControlWriter<ControlOutput>,
    stdio: bool,
    stash: VecDeque<ProtocolMessage>,
) -> io::Result<u8> {
    #[cfg(unix)]
    let mut receiver = ControlReceiver::direct(Arc::clone(client), stdio, stash)?;
    #[cfg(not(unix))]
    let _ = (stdio, stash);
    #[cfg(not(unix))]
    let (events, channel) = mpsc::sync_channel(32);
    #[cfg(not(unix))]
    spawn_protocol_reader(Arc::clone(client), events.clone());
    #[cfg(not(unix))]
    let mut receiver = ControlReceiver::new(channel);
    let mut state = ControlState::default();
    let mut pending_stdin = VecDeque::new();
    let initial_result = execute_command_unit(
        client.as_ref(),
        &mut receiver,
        output,
        initial,
        None,
        0,
        &mut state,
        &mut pending_stdin,
        None,
    )?;
    #[cfg(unix)]
    {
        match &mut receiver.source {
            ControlSource::Direct(direct) => direct.input.enabled = true,
            #[cfg(test)]
            ControlSource::Channel(_) => unreachable!(),
        }
    }
    #[cfg(not(unix))]
    spawn_stdin_reader(events);

    if initial_result.exit.is_some() {
        finish_exit(
            output,
            initial_result.exit.reason(),
            state.wait_exit,
            false,
            &mut receiver,
            &mut pending_stdin,
        )?;
        return Ok(match initial_result.exit {
            ExitSignal::Detached => 0,
            ExitSignal::Clean => state.return_code,
            _ => initial_result.exit_code,
        });
    }
    if initial_result.exit_code != 0 || state.attached_session.is_none() {
        finish_exit(
            output,
            None,
            state.wait_exit,
            false,
            &mut receiver,
            &mut pending_stdin,
        )?;
        return Ok(completed_exit_code(initial_result.exit_code, &state));
    }

    if let Some(pending_return) = take_ready_pending_return(&mut state.pending_return) {
        return finish_control_return(
            client.as_ref(),
            pending_return,
            output,
            &mut state,
            &mut receiver,
            &mut pending_stdin,
        );
    }
    loop {
        if state.tree_sync_required {
            client.request_tree_sync().map_err(io::Error::other)?;
            state.tree_sync_required = false;
        }
        #[cfg(unix)]
        if pending_stdin.is_empty()
            && state.pending_return.is_none()
            && !state.tree_sync_required
            && state.submitted_lines == 0
            && !output.block_open
            && output.deferred.is_empty()
            && let Some(received) = receiver.idle_received()
        {
            output.output.report_idle(received, output.next_number);
        }
        let event = pending_stdin.pop_front().map_or_else(
            || receive_control_event(&mut receiver),
            |stdin| {
                if let Some(pending_return) = state.pending_return.as_mut() {
                    pending_return.consume_preceding_input();
                }
                Ok(MainEvent::Stdin(stdin))
            },
        )?;
        #[cfg(unix)]
        {
            if let Some(next_number) = receiver.take_sync() {
                output.next_number = next_number;
            }
            if matches!(event, MainEvent::Stdin(StdinEvent::Line(_))) {
                state.submitted_lines += receiver.take_submitted();
            }
        }
        match event {
            MainEvent::Stdin(StdinEvent::Line(line)) => {
                if line.is_empty() {
                    return finish_control_return(
                        client.as_ref(),
                        PendingReturn::Blank {
                            code: state.return_code,
                            preceding_input: 0,
                            observed_preceding_input: false,
                        },
                        output,
                        &mut state,
                        &mut receiver,
                        &mut pending_stdin,
                    );
                }
                if line.trim().is_empty() || line.trim_start().starts_with('#') {
                    continue;
                }
                let result = execute_command_unit(
                    client.as_ref(),
                    &mut receiver,
                    output,
                    Vec::new(),
                    Some(line),
                    1,
                    &mut state,
                    &mut pending_stdin,
                    None,
                )?;
                if result.exit.is_some() {
                    finish_exit(
                        output,
                        result.exit.reason(),
                        state.wait_exit,
                        false,
                        &mut receiver,
                        &mut pending_stdin,
                    )?;
                    return Ok(match result.exit {
                        ExitSignal::Detached => 0,
                        ExitSignal::Clean => state.return_code,
                        _ => result.exit_code,
                    });
                }
                if let Some(pending_return) = take_ready_pending_return(&mut state.pending_return) {
                    return finish_control_return(
                        client.as_ref(),
                        pending_return,
                        output,
                        &mut state,
                        &mut receiver,
                        &mut pending_stdin,
                    );
                }
            }
            MainEvent::Stdin(StdinEvent::Eof) => {
                return finish_control_return(
                    client.as_ref(),
                    PendingReturn::Eof {
                        code: state.return_code,
                        preceding_input: 0,
                        observed_preceding_input: false,
                    },
                    output,
                    &mut state,
                    &mut receiver,
                    &mut pending_stdin,
                );
            }
            MainEvent::Stdin(StdinEvent::Error(error)) => {
                return finish_control_return(
                    client.as_ref(),
                    PendingReturn::InputError {
                        message: error,
                        preceding_input: 0,
                    },
                    output,
                    &mut state,
                    &mut receiver,
                    &mut pending_stdin,
                );
            }
            MainEvent::Protocol(message) => {
                let exit = handle_protocol(*message, &mut state, output)?;
                if exit.is_some() {
                    finish_exit(
                        output,
                        exit.reason(),
                        state.wait_exit,
                        false,
                        &mut receiver,
                        &mut pending_stdin,
                    )?;
                    return Ok(match exit {
                        ExitSignal::Detached => 0,
                        ExitSignal::Clean => state.return_code,
                        _ => 1,
                    });
                }
            }
            MainEvent::Disconnected => {
                finish_exit(
                    output,
                    Some("server exited unexpectedly"),
                    state.wait_exit,
                    false,
                    &mut receiver,
                    &mut pending_stdin,
                )?;
                return Ok(1);
            }
        }
    }
}

#[cfg(test)]
fn prepared_error(commands: &[PreparedCommand]) -> Option<&ServerError> {
    commands.iter().find_map(|command| match &command.result {
        PreparedCommandResult::Ready => None,
        PreparedCommandResult::Error(error) => Some(error),
    })
}

#[cfg(test)]
struct PendingExpansion {
    variables: bool,
    request_id: u64,
}

#[cfg(test)]
impl PendingExpansion {
    fn answers(&self, message: &ProtocolMessage) -> bool {
        match message {
            ProtocolMessage::HomeDirectoryResponse { request_id, .. } => {
                !self.variables && *request_id == self.request_id
            }
            ProtocolMessage::EnvironmentResponse { request_id, .. } => {
                self.variables && *request_id == self.request_id
            }
            _ => false,
        }
    }
}

#[cfg(test)]
fn expansion_answer(message: ProtocolMessage) -> Vec<Option<String>> {
    match message {
        ProtocolMessage::HomeDirectoryResponse { homes, .. } => homes,
        ProtocolMessage::EnvironmentResponse { values, .. } => values,
        _ => Vec::new(),
    }
}

#[cfg(test)]
fn match_prepared_response(
    message: ProtocolMessage,
    request_id: u64,
) -> Result<Vec<PreparedCommand>, ProtocolMessage> {
    match message {
        ProtocolMessage::PreparedCommandList {
            request_id: response_id,
            commands,
        } if response_id == request_id => Ok(commands),
        message => Err(message),
    }
}

fn execute_command_unit<W: Write>(
    client: &InteractiveClient,
    receiver: &mut ControlReceiver,
    output: &mut ControlWriter<W>,
    commands: Vec<CommandInvocation>,
    raw_line: Option<String>,
    flags: u8,
    state: &mut ControlState,
    pending_stdin: &mut VecDeque<StdinEvent>,
    mut deferred_return: Option<PendingReturn>,
) -> io::Result<CommandResult> {
    let mut first_is_detach = commands
        .first()
        .is_some_and(|command| zz_protocol::canonical_command(&command.name) == "detach-client");
    if !first_is_detach && state.pending_return.is_none() {
        state.pending_return = deferred_return.take();
    }
    output.hold_exit();
    let names: Vec<_> = commands
        .iter()
        .map(|command| command.name.clone())
        .collect();
    if let Some(line) = raw_line {
        if state.submitted_lines == 0 {
            client
                .execute_control_line(line)
                .map_err(io::Error::other)?;
        } else {
            state.submitted_lines -= 1;
        }
        if state.attached_session.is_some() {
            submit_pending_control_lines(
                pending_stdin,
                &mut state.submitted_lines,
                state
                    .pending_return
                    .as_ref()
                    .map(PendingReturn::preceding_input),
                |line| {
                    client
                        .execute_control_line(line.to_owned())
                        .map_err(io::Error::other)
                },
            )?;
        }
    } else {
        client.execute_chain(commands).map_err(io::Error::other)?;
    }
    let mut result = CommandResult {
        exit_code: 0,
        exit: ExitSignal::None,
    };
    let mut parked = false;
    let mut completed_guards = output.command_guard_frames;
    let mut started_command: Option<StartedCommand> = None;
    loop {
        if state.tree_sync_required {
            client.request_tree_sync().map_err(io::Error::other)?;
            state.tree_sync_required = false;
        }
        match receive_buffered_control_event(receiver, output)? {
            MainEvent::Protocol(message) => match *message {
                ProtocolMessage::Event(zz_protocol::Event {
                    payload:
                        EventPayload::ControlCommandStarted {
                            request_id,
                            flags,
                            canonical_name,
                            guard,
                        },
                    ..
                }) => {
                    let flags = u8::try_from(flags).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid control command flags")
                    })?;
                    if request_id == 1 && canonical_name.as_deref() == Some("detach-client") {
                        first_is_detach = true;
                        if deferred_return.is_none() {
                            deferred_return = state.pending_return.take();
                        }
                    }
                    parked = false;
                    started_command = Some(StartedCommand {
                        request_id,
                        flags,
                        canonical_name,
                        command_guard_frames: output.command_guard_frames,
                        frame: if guard {
                            Some(output.begin_buffered_at(unix_timestamp(), flags)?)
                        } else {
                            None
                        },
                    });
                }
                ProtocolMessage::CommandQueueParked { .. } => {
                    parked = true;
                    if release_parked_queue_at_client_exit(state, pending_stdin) {
                        if let Some(frame) =
                            started_command.as_ref().and_then(|started| started.frame)
                        {
                            output.end(&frame, false)?;
                        } else if output.command_guard_frames == completed_guards {
                            output.control_command_guard("", false, flags)?;
                        }
                        output.release_exit()?;
                        return Ok(result);
                    }
                }
                ProtocolMessage::CommandResponse(response) => {
                    let request_id = response_request_id(&response);
                    let index = request_id.saturating_sub(1) as usize;
                    let started = if started_command
                        .as_ref()
                        .is_some_and(|started| started.request_id == request_id)
                    {
                        started_command.take()
                    } else {
                        None
                    };
                    let name = started
                        .as_ref()
                        .and_then(|started| started.canonical_name.as_deref())
                        .or_else(|| {
                            names
                                .get(index)
                                .map(|name| zz_protocol::canonical_command(name))
                        });
                    let updates_return_code = response_sets_return_code(name, &response);
                    let new_failure = updates_return_code && state.return_code == 0;
                    if updates_return_code {
                        state.return_code = 1;
                    }
                    settle_deferred_return(
                        result.exit == ExitSignal::Detached,
                        &mut deferred_return,
                        state,
                    );
                    if let Some(pending_return) = state.pending_return.as_mut() {
                        if (new_failure && !response_is_post_admission_callback_failure(&response))
                            || (updates_return_code && name == Some("source-file"))
                        {
                            pending_return.observe_preceding_input();
                        }
                        pending_return.refresh_code_after_preceding_input(state.return_code);
                    }
                    result.exit_code = if let Some(started) = started {
                        render_command_response(
                            output,
                            started.frame.as_ref(),
                            started.flags,
                            started.command_guard_frames,
                            response,
                        )?
                    } else {
                        response_exit_code(&response)
                    };
                    completed_guards = output.command_guard_frames;
                }
                ProtocolMessage::ExecExit(finished) => {
                    match finished.outcome {
                        ExecOutcome::Ran => {}
                        ExecOutcome::Rejected(error) => {
                            if flags == 0 {
                                output.write_line(&error.tmux_message())?;
                            } else {
                                let message = error.tmux_message();
                                output.parse_error(&if message.starts_with("parse error: ") {
                                    message
                                } else {
                                    format!("parse error: {message}")
                                })?;
                            }
                            result.exit_code = 1;
                            settle_deferred_return(false, &mut deferred_return, state);
                        }
                        ExecOutcome::Resume(_) | ExecOutcome::ServerMismatch => {
                            result.exit_code = 1;
                            result.exit = ExitSignal::Unexpected;
                        }
                    }
                    output.release_exit()?;
                    return Ok(result);
                }
                message => {
                    let signal = handle_protocol(message, state, output)?;
                    if signal.is_some() {
                        result.exit = signal;
                    }
                }
            },
            MainEvent::Stdin(stdin) => {
                if let StdinEvent::Line(line) = &stdin
                    && state.attached_session.is_some()
                    && state.pending_return.is_none()
                    && deferred_return.is_none()
                    && pending_stdin.len() == state.submitted_lines
                    && state.submitted_lines < 32
                    && !line.trim().is_empty()
                    && !line.trim_start().starts_with('#')
                {
                    client
                        .execute_control_line(line.clone())
                        .map_err(io::Error::other)?;
                    state.submitted_lines = state.submitted_lines.saturating_add(1);
                }
                capture_pending_return(
                    stdin,
                    state.return_code,
                    if first_is_detach {
                        &mut deferred_return
                    } else {
                        &mut state.pending_return
                    },
                    pending_stdin,
                    output,
                );
                if parked && release_parked_queue_at_client_exit(state, pending_stdin) {
                    if let Some(frame) = started_command.as_ref().and_then(|started| started.frame)
                    {
                        output.end(&frame, false)?;
                    } else if output.command_guard_frames == completed_guards {
                        output.control_command_guard("", false, flags)?;
                    }
                    output.release_exit()?;
                    return Ok(result);
                }
            }
            MainEvent::Disconnected => {
                if let Some(started) = started_command {
                    if result.exit == ExitSignal::Clean {
                        if let Some(frame) = started.frame {
                            output.end(&frame, false)?;
                        }
                    } else {
                        render_command_failure(
                            output,
                            started.frame.as_ref(),
                            started.flags,
                            started.command_guard_frames,
                            "server exited unexpectedly",
                        )?;
                    }
                }
                output.release_exit()?;
                if !result.exit.is_some() {
                    result.exit = ExitSignal::Unexpected;
                    result.exit_code = 1;
                }
                return Ok(result);
            }
        }
    }
}

fn submit_pending_control_lines(
    pending: &VecDeque<StdinEvent>,
    submitted: &mut usize,
    preceding_input: Option<usize>,
    mut submit: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<()> {
    for stdin in pending
        .iter()
        .take(preceding_input.unwrap_or(usize::MAX))
        .skip(*submitted)
        .take(32usize.saturating_sub(*submitted))
    {
        let StdinEvent::Line(line) = stdin else {
            break;
        };
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            break;
        }
        submit(line)?;
        *submitted += 1;
    }
    Ok(())
}

#[derive(Default)]
struct ControlState {
    attached_session: Option<SessionId>,
    snapshot: MuxSnapshot,
    tree: MuxSnapshot,
    view: Option<zz_protocol::ClientView>,
    tree_sync_required: bool,
    last_windows: BTreeMap<SessionId, WindowId>,
    self_name: Option<String>,
    wait_exit: bool,
    new_layouts: bool,
    return_code: u8,
    pending_return: Option<PendingReturn>,
    parked_queue_released: bool,
    submitted_lines: usize,
}

impl ControlState {
    fn attach(&mut self, session: SessionId, snapshot: MuxSnapshot) {
        self.tree = snapshot.clone();
        self.attached_session = Some(session);
        self.adopt_snapshot(snapshot);
    }

    fn adopt_snapshot(&mut self, snapshot: MuxSnapshot) {
        for session in &snapshot.sessions {
            if let Some(previous) = self
                .snapshot
                .sessions
                .iter()
                .find(|previous| previous.id == session.id)
                && previous.active_window != session.active_window
            {
                self.last_windows.insert(session.id, previous.active_window);
            }
        }
        self.snapshot = snapshot;
        self.self_name = self
            .attached_session
            .and_then(|attached| {
                self.snapshot
                    .sessions
                    .iter()
                    .find(|session| session.id == attached)
            })
            .and_then(|session| session.viewers.iter().find(|viewer| viewer.is_self))
            .map(|viewer| viewer.name.clone());
    }

    fn window(
        &self,
        id: &str,
    ) -> Option<(&zz_protocol::SessionSnapshot, &zz_protocol::WindowSnapshot)> {
        let attached = self.attached_session.and_then(|attached| {
            self.snapshot
                .sessions
                .iter()
                .find(|session| session.id == attached)
        });
        attached
            .and_then(|session| {
                session
                    .windows
                    .iter()
                    .find(|window| window.id.to_string() == id)
                    .map(|window| (session, window))
            })
            .or_else(|| {
                self.snapshot.sessions.iter().find_map(|session| {
                    session
                        .windows
                        .iter()
                        .find(|window| window.id.to_string() == id)
                        .map(|window| (session, window))
                })
            })
    }

    fn mine(&self, variables: &BTreeMap<String, String>) -> bool {
        self.attached_session.is_some_and(|attached| {
            self.snapshot.sessions.iter().any(|session| {
                session.id == attached
                    && session
                        .windows
                        .iter()
                        .any(|window| variables.get("hook_window") == Some(&window.id.to_string()))
            })
        })
    }

    fn raw_window_flags(
        &self,
        session: &zz_protocol::SessionSnapshot,
        window: &zz_protocol::WindowSnapshot,
    ) -> String {
        let mut flags = String::new();
        if window.activity {
            flags.push('#');
        }
        if window.panes.values().any(|pane| pane.bell) {
            flags.push('!');
        }
        if session.active_window == window.id {
            flags.push('*');
        }
        if self.last_windows.get(&session.id) == Some(&window.id) {
            flags.push('-');
        }
        if window.zoomed_pane.is_some() {
            flags.push('Z');
        }
        if window.silence {
            flags.push('~');
        }
        flags
    }
}

fn handle_protocol<W: Write>(
    message: ProtocolMessage,
    state: &mut ControlState,
    output: &mut ControlWriter<W>,
) -> io::Result<ExitSignal> {
    match message {
        ProtocolMessage::Attached {
            session, snapshot, ..
        } => state.attach(session, snapshot),
        ProtocolMessage::Event(event) => match event.payload {
            EventPayload::Snapshot(snapshot) => {
                state.tree = snapshot.clone();
                state.adopt_snapshot(snapshot);
                state.tree_sync_required = false;
            }
            EventPayload::TreeDelta(delta) => {
                if delta.apply(&mut state.tree).is_err() {
                    state.tree_sync_required = true;
                } else {
                    let mut snapshot = state.tree.clone();
                    if let Some(view) = &state.view {
                        let _ = view.apply(&mut snapshot);
                    }
                    state.adopt_snapshot(snapshot);
                }
            }
            EventPayload::ClientView(view) => {
                let mut snapshot = state.tree.clone();
                if view.apply(&mut snapshot).is_err() {
                    state.tree_sync_required = true;
                } else {
                    state.attached_session = view.session;
                    state.adopt_snapshot(snapshot);
                    state.view = Some(view);
                }
            }
            EventPayload::HookEvent { name, variables } => {
                if state.attached_session.is_some()
                    && !output.notifications_closed
                    && let Some(line) = render_hook(state, &name, &variables)
                {
                    output.notify(line.as_bytes())?;
                }
            }
            EventPayload::PaneOutput { pane, bytes } => {
                output.pane_output(&render_pane_output(pane, &bytes))?;
            }
            EventPayload::PaneOutputState { pane, paused } => {
                output.notify(
                    format!("%{} {pane}", if paused { "pause" } else { "continue" }).as_bytes(),
                )?;
            }
            EventPayload::PaneOutputAged {
                pane,
                age_ms,
                bytes,
            } => {
                output.pane_output(&render_pane_output_aged(pane, age_ms, &bytes))?;
            }
            EventPayload::ControlFlags {
                wait_exit,
                new_layouts,
                ..
            } => {
                state.wait_exit = wait_exit;
                state.new_layouts = new_layouts;
            }
            EventPayload::SubscriptionChanged {
                name,
                session,
                window,
                window_index,
                pane,
                value,
            } => {
                output.notify(
                    render_subscription_changed(&name, session, window, window_index, pane, &value)
                        .as_bytes(),
                )?;
            }
            EventPayload::TimedClientMessage { text, .. } => {
                let mut line = b"%message ".to_vec();
                line.extend(render_message(&text));
                output.notify(&line)?;
            }
            EventPayload::ControlCommandGuard {
                output: text,
                error,
                sticky_failure,
                flags,
            }
            | EventPayload::ControlCommandGuardRaw {
                output: text,
                error,
                sticky_failure,
                flags,
            } => {
                if sticky_failure || (error && is_source_error_message(&text)) {
                    state.return_code = 1;
                }
                output.control_command_guard_bytes(text.as_bytes(), error, flags)?;
            }
            EventPayload::ControlSourceFile { event } => {
                if matches!(event, ControlSourceFileEvent::ReadError(_)) {
                    state.return_code = 1;
                }
                output.control_source_file(event)?;
            }
            EventPayload::ControlCommandOutput { output: text } => {
                output.control_command_output(&text)?;
            }
            EventPayload::CommandStdout { output: text } => {
                output.command_block_output(text.as_bytes())?;
            }
            EventPayload::StartupConfigCauses { causes } => {
                output.startup_config_causes(&causes)?;
            }
            EventPayload::ClientMessage {
                kind: zz_protocol::ClientMessageKind::Error,
                text,
                ..
            } => {
                if is_source_error_message(&text) {
                    state.return_code = 1;
                }
                output.diagnostic_error(&text)?;
            }
            EventPayload::ClientMessage { kind, text, .. }
                if kind == zz_protocol::ClientMessageKind::Warning
                    && is_source_error_message(&text) =>
            {
                state.return_code = 1;
                output.diagnostic_error(&text)?;
            }
            EventPayload::ControlConfigError { text } => {
                output.notify(format!("%config-error {text}").as_bytes())?;
            }
            EventPayload::Detached { .. } => {
                state.attached_session = None;
                return Ok(ExitSignal::Detached);
            }
            EventPayload::ServerStopping => {
                output.emit_exit(None)?;
                return Ok(ExitSignal::Clean);
            }
            EventPayload::ControlExit { reason } if reason.is_empty() => {
                output.release_exit()?;
                output.emit_exit(None)?;
                return Ok(ExitSignal::Clean);
            }
            EventPayload::ControlExit { reason } if reason == "too far behind" => {
                return Ok(ExitSignal::TooFarBehind);
            }
            _ => {}
        },
        _ => {}
    }
    Ok(ExitSignal::None)
}

fn render_hook(
    state: &ControlState,
    name: &str,
    variables: &BTreeMap<String, String>,
) -> Option<String> {
    let value = |key| variables.get(key).map(String::as_str);
    match name {
        "window-linked" => Some(format!(
            "{} {}",
            if state.mine(variables) {
                "%window-add"
            } else {
                "%unlinked-window-add"
            },
            value("hook_window")?
        )),
        "window-unlinked" => Some(format!(
            "{} {}",
            if state.mine(variables) {
                "%window-close"
            } else {
                "%unlinked-window-close"
            },
            value("hook_window")?
        )),
        "window-renamed" => Some(format!(
            "{} {} {}",
            if state.mine(variables) {
                "%window-renamed"
            } else {
                "%unlinked-window-renamed"
            },
            value("hook_window")?,
            value("hook_window_name")?
        )),
        "window-pane-changed" => Some(format!(
            "%window-pane-changed {} {}",
            value("hook_window")?,
            value("hook_pane")?
        )),
        "window-layout-changed" => {
            if !state.mine(variables) {
                return None;
            }
            let window_id = value("hook_window")?;
            let (session, window) = state.window(window_id)?;
            let layout = |dump: &str| {
                if state.new_layouts {
                    dump.to_owned()
                } else {
                    zz_mux::legacy_layout(dump)
                }
            };
            let flags = state.raw_window_flags(session, window);
            if let Some(unzoomed) = variables.get(zz_protocol::UNZOOMED_LAYOUT_VARIABLE) {
                let unzoomed = layout(unzoomed);
                return Some(format!(
                    "%layout-change {window_id} {unzoomed} {unzoomed} {}",
                    flags.replace('Z', "")
                ));
            }
            Some(format!(
                "%layout-change {window_id} {} {} {flags}",
                layout(&window.layout_dump),
                layout(&window.visible_layout_dump),
            ))
        }
        "session-created" | "session-closed" => Some("%sessions-changed".to_owned()),
        "session-renamed" => Some(format!(
            "%session-renamed {} {}",
            value("hook_session")?,
            value("hook_session_name")?
        )),
        "session-window-changed" => Some(format!(
            "%session-window-changed {} {}",
            value("hook_session")?,
            value("hook_window")?
        )),
        "client-session-changed" => {
            let client = value("hook_client")?;
            if state.self_name.as_deref() == Some(client) {
                Some(format!(
                    "%session-changed {} {}",
                    value("hook_session")?,
                    value("hook_session_name")?
                ))
            } else {
                Some(format!(
                    "%client-session-changed {client} {} {}",
                    value("hook_session")?,
                    value("hook_session_name")?
                ))
            }
        }
        "client-detached" => Some(format!("%client-detached {}", value("hook_client")?)),
        "pane-mode-changed" => Some(format!("%pane-mode-changed {}", value("hook_pane")?)),
        "paste-buffer-changed" => Some(format!(
            "%paste-buffer-changed {}",
            value("hook_paste_buffer")?
        )),
        "paste-buffer-deleted" => Some(format!(
            "%paste-buffer-deleted {}",
            value("hook_paste_buffer")?
        )),
        _ => None,
    }
}

fn render_subscription_changed(
    name: &str,
    session: SessionId,
    window: Option<WindowId>,
    window_index: Option<u32>,
    pane: Option<zz_protocol::PaneId>,
    value: &str,
) -> String {
    match (window, window_index, pane) {
        (Some(window), Some(index), Some(pane)) => {
            format!("%subscription-changed {name} {session} {window} {index} {pane} : {value}")
        }
        (Some(window), Some(index), None) => {
            format!("%subscription-changed {name} {session} {window} {index} - : {value}")
        }
        _ => format!("%subscription-changed {name} {session} - - - : {value}"),
    }
}

fn render_pane_output(pane: zz_protocol::PaneId, bytes: &[u8]) -> Vec<u8> {
    let mut line = format!("%output {pane} ").into_bytes();
    append_output_bytes(&mut line, bytes);
    line
}

fn render_pane_output_aged(pane: zz_protocol::PaneId, age_ms: u64, bytes: &[u8]) -> Vec<u8> {
    let mut line = format!("%extended-output {pane} {age_ms} : ").into_bytes();
    append_output_bytes(&mut line, bytes);
    line
}

fn append_output_bytes(line: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        if *byte < 0x20 || *byte == b'\\' {
            line.extend([
                b'\\',
                b'0' + (byte >> 6),
                b'0' + ((byte >> 3) & 7),
                b'0' + (byte & 7),
            ]);
        } else {
            line.push(*byte);
        }
    }
}

fn render_message(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut rendered = Vec::with_capacity(bytes.len());
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            0 => {
                rendered.extend_from_slice(b"\\0");
                if bytes.get(index + 1).is_some_and(u8::is_ascii_digit) && bytes[index + 1] < b'8' {
                    rendered.extend_from_slice(b"00");
                }
            }
            b'\r' => rendered.extend_from_slice(b"\\r"),
            8 => rendered.extend_from_slice(b"\\b"),
            7 => rendered.extend_from_slice(b"\\a"),
            11 => rendered.extend_from_slice(b"\\v"),
            12 => rendered.extend_from_slice(b"\\f"),
            b'\t' | b'\n' | b' ' | b'!'..=b'~' | 0x80..=0xff => rendered.push(byte),
            _ => rendered.extend([
                b'\\',
                b'0' + (byte >> 6),
                b'0' + ((byte >> 3) & 7),
                b'0' + (byte & 7),
            ]),
        }
    }
    rendered
}

fn is_source_error_message(text: &str) -> bool {
    let mut lines = text.lines();
    lines.next().is_some_and(is_source_error_line) && lines.all(is_source_error_line)
}

fn is_source_error_line(text: &str) -> bool {
    text.starts_with("No such file or directory: ")
        || text.starts_with("Invalid argument: ")
        || text.starts_with("Cannot allocate memory: ")
        || text.starts_with("Pattern syntax error")
        || text == "too many nested files"
        || text
            .strip_prefix("stream did not contain valid UTF-8: ")
            .is_some_and(|path| Path::new(path).is_absolute())
        || text.split_once("): ").is_some_and(|(error, path)| {
            error
                .rsplit_once(" (os error ")
                .is_some_and(|(_, code)| code.parse::<i32>().is_ok())
                && Path::new(path).is_absolute()
        })
}

#[cfg(test)]
fn response_aborts_line(response: &CommandResponse) -> bool {
    matches!(response, CommandResponse::Error { .. })
}

fn response_sets_return_code(canonical_name: Option<&str>, response: &CommandResponse) -> bool {
    match response {
        CommandResponse::Error { error, .. } => !error.is_command_parse(),
        CommandResponse::Success { exit_code, .. } => {
            canonical_name == Some("source-file") && *exit_code != 0
        }
    }
}

fn response_is_post_admission_callback_failure(response: &CommandResponse) -> bool {
    matches!(
        response,
        CommandResponse::Error { error, .. } if error.is_post_admission_callback()
    )
}

fn response_request_id(response: &CommandResponse) -> u64 {
    match response {
        CommandResponse::Success { request_id, .. } | CommandResponse::Error { request_id, .. } => {
            *request_id
        }
    }
}

fn response_exit_code(response: &CommandResponse) -> u8 {
    match response {
        CommandResponse::Success { exit_code, .. } => *exit_code,
        CommandResponse::Error { .. } => 1,
    }
}

fn render_command_response<W: Write>(
    output: &mut ControlWriter<W>,
    frame: Option<&Frame>,
    flags: u8,
    command_guard_frames: u64,
    response: CommandResponse,
) -> io::Result<u8> {
    if let Some(frame) = frame {
        return output.response(frame, response);
    }
    let exit_code = response_exit_code(&response);
    if matches!(response, CommandResponse::Error { .. })
        && output.command_guard_frames == command_guard_frames
    {
        let frame = output.begin(flags)?;
        output.response(&frame, response)
    } else {
        Ok(exit_code)
    }
}

fn render_command_failure<W: Write>(
    output: &mut ControlWriter<W>,
    frame: Option<&Frame>,
    flags: u8,
    command_guard_frames: u64,
    error: &str,
) -> io::Result<()> {
    if let Some(frame) = frame {
        return output.error(frame, error);
    }
    if output.command_guard_frames == command_guard_frames {
        let frame = output.begin(flags)?;
        output.error(&frame, error)?;
    }
    Ok(())
}

fn completed_exit_code(command_exit_code: u8, state: &ControlState) -> u8 {
    if command_exit_code == 0 {
        state.return_code
    } else {
        command_exit_code
    }
}

fn capture_pending_return<W: Write>(
    stdin: StdinEvent,
    return_code: u8,
    pending_return: &mut Option<PendingReturn>,
    pending_stdin: &mut VecDeque<StdinEvent>,
    output: &mut ControlWriter<W>,
) {
    if pending_return.is_some() {
        pending_stdin.push_back(stdin);
        return;
    }
    match PendingReturn::from_stdin(stdin, return_code, pending_stdin.len()) {
        Ok(return_event) => {
            if return_event.discards_pane_output() {
                output.begin_exit_drain(return_event.closes_notifications());
            }
            *pending_return = Some(return_event);
        }
        Err(stdin) => pending_stdin.push_back(stdin),
    }
}

fn release_parked_queue_at_client_exit(
    state: &mut ControlState,
    pending_stdin: &mut VecDeque<StdinEvent>,
) -> bool {
    let Some(
        PendingReturn::Eof {
            preceding_input, ..
        }
        | PendingReturn::Blank {
            preceding_input, ..
        },
    ) = state.pending_return.as_mut()
    else {
        return false;
    };
    *preceding_input = 0;
    pending_stdin.clear();
    state.submitted_lines = 0;
    state.parked_queue_released = true;
    true
}

fn take_ready_pending_return(pending_return: &mut Option<PendingReturn>) -> Option<PendingReturn> {
    if pending_return
        .as_ref()
        .is_some_and(|pending_return| !pending_return.has_preceding_input())
    {
        pending_return.take()
    } else {
        None
    }
}

#[cfg(test)]
fn settle_preparation_error_return(
    prepared_return: &mut Option<PendingReturn>,
    state_return: &mut Option<PendingReturn>,
) -> Option<PendingReturn> {
    if state_return.is_none() {
        *state_return = prepared_return.take();
    }
    take_ready_pending_return(state_return)
}

fn settle_deferred_return(
    caller_detached: bool,
    deferred_return: &mut Option<PendingReturn>,
    state: &mut ControlState,
) {
    if caller_detached {
        deferred_return.take();
    } else if state.pending_return.is_none() {
        state.pending_return = deferred_return.take();
    }
}

fn finish_control_return<W: Write + ControlClose>(
    client: &InteractiveClient,
    pending_return: PendingReturn,
    output: &mut ControlWriter<W>,
    state: &mut ControlState,
    receiver: &mut ControlReceiver,
    pending_stdin: &mut VecDeque<StdinEvent>,
) -> io::Result<u8> {
    if pending_return.discards_pane_output() {
        output.begin_exit_drain(pending_return.closes_notifications());
    }
    let code = pending_return.code();
    let (input_closed, input_error) = match pending_return {
        PendingReturn::Blank { .. } => (false, None),
        PendingReturn::Eof { .. } => (true, None),
        PendingReturn::InputError { message, .. } => (true, Some(message)),
    };
    if let Some(error) = input_error.as_deref() {
        output.output.close_sink();
        eprintln!("zz: {error}");
    }
    let _ = client.detach();
    if input_error.is_none() && !state.parked_queue_released {
        drain_before_exit(receiver, state, output, pending_stdin)?;
    }
    finish_exit(
        output,
        None,
        state.wait_exit,
        input_closed,
        receiver,
        pending_stdin,
    )?;
    Ok(code)
}

fn drain_before_exit<W: Write>(
    receiver: &mut ControlReceiver,
    state: &mut ControlState,
    output: &mut ControlWriter<W>,
    pending_stdin: &mut VecDeque<StdinEvent>,
) -> io::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return Ok(());
        };
        if TERMINATION_REQUESTED.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        match receiver.receive(Some(remaining.min(std::time::Duration::from_millis(20))))? {
            Some(MainEvent::Protocol(message)) => {
                if handle_protocol(*message, state, output)?.is_some() {
                    return Ok(());
                }
            }
            Some(MainEvent::Stdin(input)) => pending_stdin.push_back(input),
            Some(MainEvent::Disconnected) => {
                return Ok(());
            }
            None => {}
        }
    }
}

#[cfg(not(unix))]
fn spawn_protocol_reader(client: Arc<InteractiveClient>, events: mpsc::SyncSender<MainEvent>) {
    let _ = thread::Builder::new()
        .name("zz-control-protocol".to_owned())
        .spawn(move || {
            loop {
                if let Ok(message) = client.recv() {
                    if forward_protocol_message(message, &events).is_err() {
                        break;
                    }
                } else {
                    let _ = events.send(MainEvent::Disconnected);
                    break;
                }
            }
        });
}

#[cfg(any(not(unix), test))]
fn forward_protocol_message(
    message: ProtocolMessage,
    events: &mpsc::SyncSender<MainEvent>,
) -> Result<(), ()> {
    events
        .send(MainEvent::Protocol(Box::new(message)))
        .map_err(|_| ())
}

#[cfg(not(unix))]
fn spawn_stdin_reader(events: mpsc::SyncSender<MainEvent>) {
    let _ = thread::Builder::new()
        .name("zz-control-stdin".to_owned())
        .spawn(move || {
            let mut stdin = io::stdin().lock();
            loop {
                let mut bytes = Vec::new();
                match stdin.read_until(b'\n', &mut bytes) {
                    Ok(0) => {
                        let _ = events.send(MainEvent::Stdin(StdinEvent::Eof));
                        break;
                    }
                    Ok(_) => {
                        if bytes.last() == Some(&b'\n') {
                            bytes.pop();
                        }
                        let line = match String::from_utf8(bytes) {
                            Ok(line) => line,
                            Err(error) => {
                                let _ = events
                                    .send(MainEvent::Stdin(StdinEvent::Error(error.to_string())));
                                break;
                            }
                        };
                        if events
                            .send(MainEvent::Stdin(StdinEvent::Line(line)))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        let _ = events.send(MainEvent::Stdin(StdinEvent::Error(error.to_string())));
                        break;
                    }
                }
            }
        });
}

fn finish_exit<W: Write>(
    output: &mut ControlWriter<W>,
    reason: Option<&str>,
    wait_exit: bool,
    input_closed: bool,
    receiver: &mut ControlReceiver,
    pending_stdin: &mut VecDeque<StdinEvent>,
) -> io::Result<()> {
    output.emit_exit(reason)?;
    if wait_exit && !input_closed {
        wait_for_exit_input(receiver, pending_stdin);
    }
    output.finish()
}

fn wait_for_exit_input(receiver: &mut ControlReceiver, pending_stdin: &mut VecDeque<StdinEvent>) {
    loop {
        let event = pending_stdin
            .pop_front()
            .map(MainEvent::Stdin)
            .or_else(|| receive_control_event(receiver).ok());
        match event {
            Some(MainEvent::Stdin(StdinEvent::Line(line))) if line.is_empty() => return,
            Some(MainEvent::Stdin(StdinEvent::Eof | StdinEvent::Error(_))) | None => return,
            Some(
                MainEvent::Stdin(StdinEvent::Line(_))
                | MainEvent::Protocol(_)
                | MainEvent::Disconnected,
            ) => {}
        }
    }
}

#[cfg(test)]
fn parse_line(
    line: &str,
    homes: &BTreeMap<String, String>,
    variables: &BTreeMap<String, String>,
) -> ParsedLine {
    if line.is_empty() {
        return ParsedLine::Return;
    }
    let parsed = zz_mux::parse_config_with_expansions(CONTROL_PARSE_SOURCE, line, homes, variables);
    if let Some(diagnostic) = parsed.diagnostics.first() {
        return ParsedLine::Error(format!("parse error: {}", diagnostic.message));
    }
    if parsed.commands.is_empty() {
        ParsedLine::Ignore
    } else {
        ParsedLine::Commands(parsed.commands)
    }
}

#[cfg(unix)]
fn stdio_passable() -> bool {
    use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
    let Ok(stdin) = io::stdin().as_fd().try_clone_to_owned() else {
        return false;
    };
    std::fs::File::from(stdin).metadata().is_ok_and(|metadata| {
        let kind = metadata.file_type();
        kind.is_socket() || (kind.is_fifo() && (cfg!(target_os = "linux") || metadata.nlink() == 0))
    })
}

#[cfg(unix)]
struct StdioFlags([Option<rustix::fs::OFlags>; 3]);

#[cfg(unix)]
impl StdioFlags {
    fn capture() -> Self {
        Self([
            rustix::fs::fcntl_getfl(io::stdin()).ok(),
            rustix::fs::fcntl_getfl(io::stdout()).ok(),
            rustix::fs::fcntl_getfl(io::stderr()).ok(),
        ])
    }

    fn restore(&self) {
        let [stdin, stdout, stderr] = self.0;
        if let Some(flags) = stdin {
            let _ = rustix::fs::fcntl_setfl(io::stdin(), flags);
        }
        if let Some(flags) = stdout {
            let _ = rustix::fs::fcntl_setfl(io::stdout(), flags);
        }
        if let Some(flags) = stderr {
            let _ = rustix::fs::fcntl_setfl(io::stderr(), flags);
        }
    }
}

#[cfg(unix)]
fn start_stdio(
    client: &InteractiveClient,
) -> io::Result<(Option<StdioFlags>, VecDeque<ProtocolMessage>)> {
    let mut stash = VecDeque::new();
    if std::env::var_os("ZZ_CONTROL_RELAY").is_some_and(|value| value == "1")
        || !client
            .server_hello()
            .capabilities
            .iter()
            .any(|capability| capability == zz_protocol::CONTROL_STDIO_CAPABILITY)
        || !stdio_passable()
    {
        return Ok((None, stash));
    }
    let flags = StdioFlags::capture();
    let frame = zz_protocol::encode_protocol_message(&ProtocolMessage::ControlStdio)
        .map_err(io::Error::other)?;
    let socket = client.receive_fd()?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    let fds = [stdin.as_fd(), stdout.as_fd()];
    let mut space = [std::mem::MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(2))];
    let mut ancillary = rustix::net::SendAncillaryBuffer::new(&mut space);
    ancillary.push(rustix::net::SendAncillaryMessage::ScmRights(&fds));
    let mut sent = rustix::net::sendmsg(
        &socket,
        &[io::IoSlice::new(&frame)],
        &mut ancillary,
        rustix::net::SendFlags::empty(),
    )?;
    while sent < frame.len() {
        sent += rustix::io::write(&socket, &frame[sent..])?;
    }
    loop {
        match client.recv().map_err(io::Error::other)? {
            ProtocolMessage::ControlStdioSync { next_number: 0 } => {
                return Ok((Some(flags), stash));
            }
            ProtocolMessage::ControlStdioClosed => return Ok((None, stash)),
            ProtocolMessage::Batch(batch) => stash.extend(
                batch
                    .messages()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            ),
            message => stash.push_back(message),
        }
    }
}

enum ControlOutput {
    Stdout(io::BufWriter<io::StdoutLock<'static>>),
    #[cfg(unix)]
    Daemon {
        client: Arc<InteractiveClient>,
        buffer: Vec<u8>,
        reported: Option<(u64, u64)>,
        direct: Option<io::BufWriter<io::StdoutLock<'static>>>,
        flags: StdioFlags,
    },
}

impl ControlOutput {
    #[cfg(unix)]
    fn daemon(client: Arc<InteractiveClient>, flags: StdioFlags) -> Self {
        Self::Daemon {
            client,
            buffer: Vec::new(),
            reported: None,
            direct: None,
            flags,
        }
    }

    #[cfg(not(unix))]
    fn daemon(_client: Arc<InteractiveClient>, _flags: ()) -> Self {
        Self::Stdout(io::BufWriter::new(io::stdout().lock()))
    }

    #[cfg(unix)]
    fn send(&mut self, idle: Option<(u64, u64)>, close: bool) -> io::Result<()> {
        let Self::Daemon {
            client,
            buffer,
            direct,
            flags,
            ..
        } = self
        else {
            return Ok(());
        };
        if let Some(direct) = direct {
            direct.write_all(buffer)?;
            buffer.clear();
            return direct.flush();
        }
        let message = ProtocolMessage::ControlWrite {
            bytes: std::mem::take(buffer),
            idle,
            close,
        };
        if client.send(&message).is_err() {
            let ProtocolMessage::ControlWrite { bytes, .. } = message else {
                unreachable!();
            };
            flags.restore();
            let mut stdout = io::BufWriter::new(io::stdout().lock());
            stdout.write_all(&bytes)?;
            stdout.flush()?;
            *direct = Some(stdout);
        }
        Ok(())
    }

    #[cfg(unix)]
    fn report_idle(&mut self, received: u64, next_number: u64) {
        let Self::Daemon {
            buffer, reported, ..
        } = self
        else {
            return;
        };
        if *reported == Some((received, next_number)) && buffer.is_empty() {
            return;
        }
        *reported = Some((received, next_number));
        let _ = self.send(Some((received, next_number)), false);
    }

    #[cfg(unix)]
    fn close(&mut self) {
        let Self::Daemon { client, direct, .. } = self else {
            return;
        };
        if direct.is_some() {
            let _ = self.flush();
            return;
        }
        let client = Arc::clone(client);
        if self.send(None, true).is_ok()
            && let Self::Daemon { direct: None, .. } = self
            && let Ok(socket) = client.receive_fd()
        {
            loop {
                match client.try_recv() {
                    Ok(Some(message))
                        if matches!(*message, ProtocolMessage::ControlStdioClosed) =>
                    {
                        break;
                    }
                    Ok(Some(_)) => {}
                    Err(_) => break,
                    Ok(None) => {
                        let mut poll = [rustix::event::PollFd::new(
                            &socket,
                            rustix::event::PollFlags::IN,
                        )];
                        let _ = rustix::event::poll(&mut poll, None);
                    }
                }
            }
        }
        if let Self::Daemon { direct, flags, .. } = self
            && direct.is_none()
        {
            flags.restore();
            *direct = Some(io::BufWriter::new(io::stdout().lock()));
        }
    }

    #[cfg(not(unix))]
    fn close(&mut self) {}
}

impl Write for ControlOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout(stdout) => stdout.write(bytes),
            #[cfg(unix)]
            Self::Daemon {
                direct: Some(direct),
                ..
            } => direct.write(bytes),
            #[cfg(unix)]
            Self::Daemon { buffer, .. } => {
                buffer.extend_from_slice(bytes);
                Ok(bytes.len())
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout(stdout) => stdout.flush(),
            #[cfg(unix)]
            Self::Daemon { buffer, direct, .. } => {
                if direct.is_none() && buffer.is_empty() {
                    return Ok(());
                }
                self.send(None, false)
            }
        }
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

enum DeferredOutput {
    Notification(Vec<u8>),
    PaneOutput(Vec<u8>),
    Exit(Option<String>),
    DiagnosticError {
        time: u64,
        text: String,
    },
    ControlCommandGuard {
        time: u64,
        output: Vec<u8>,
        error: bool,
        flags: u8,
    },
    ControlSourceFile(ControlSourceFileEvent),
    ControlCommandOutput(String),
}

trait ControlClose {
    fn close_sink(&mut self);
}

impl ControlClose for ControlOutput {
    fn close_sink(&mut self) {
        self.close();
    }
}

struct ControlWriter<W: Write> {
    output: W,
    double: bool,
    next_number: u64,
    command_guard_frames: u64,
    block_open: bool,
    open_frame: Option<Frame>,
    deferred: VecDeque<DeferredOutput>,
    trailing_causes: Vec<String>,
    exit_draining: bool,
    notifications_closed: bool,
    exit_requested: bool,
    exit_held: bool,
    st_sent: bool,
}

impl<W: Write> ControlWriter<W> {
    fn new(output: W, double: bool) -> Self {
        Self {
            output,
            double,
            next_number: 1,
            command_guard_frames: 0,
            block_open: false,
            open_frame: None,
            deferred: VecDeque::new(),
            trailing_causes: Vec::new(),
            exit_draining: false,
            notifications_closed: false,
            exit_requested: false,
            exit_held: false,
            st_sent: false,
        }
    }

    fn start(&mut self) -> io::Result<()> {
        if self.double {
            self.output.write_all(DCS)?;
            self.output.flush()?;
        }
        Ok(())
    }

    fn notify(&mut self, line: &[u8]) -> io::Result<()> {
        if self.block_open {
            self.deferred
                .push_back(DeferredOutput::Notification(line.to_vec()));
            return Ok(());
        }
        self.output.write_all(line)?;
        self.output.write_all(b"\n")?;
        self.output.flush()
    }

    fn pane_output(&mut self, line: &[u8]) -> io::Result<()> {
        if self.exit_draining {
            return Ok(());
        }
        if self.block_open {
            self.deferred
                .push_back(DeferredOutput::PaneOutput(line.to_vec()));
            return Ok(());
        }
        self.output.write_all(line)?;
        self.output.write_all(b"\n")?;
        self.output.flush()
    }

    fn begin_exit_drain(&mut self, close_notifications: bool) {
        self.exit_draining = true;
        self.notifications_closed |= close_notifications;
        self.deferred
            .retain(|deferred| !matches!(deferred, DeferredOutput::PaneOutput(_)));
    }

    fn startup_config_causes(&mut self, causes: &[String]) -> io::Result<()> {
        if self.block_open {
            self.trailing_causes.extend_from_slice(causes);
            return Ok(());
        }
        for cause in causes {
            self.notify(format!("%config-error {cause}").as_bytes())?;
        }
        Ok(())
    }

    fn flush_deferred(&mut self) -> io::Result<()> {
        let mut exit = None;
        while let Some(deferred) = self.deferred.pop_front() {
            match deferred {
                DeferredOutput::Notification(line) | DeferredOutput::PaneOutput(line) => {
                    self.output.write_all(&line)?;
                    self.output.write_all(b"\n")?;
                }
                DeferredOutput::Exit(reason) => exit = Some(reason),
                DeferredOutput::DiagnosticError { time, text } => {
                    let frame = self.allocate_frame(time, 1)?;
                    self.write_frame_begin(&frame)?;
                    self.write_line(&text)?;
                    self.write_frame_end(&frame, true)?;
                }
                DeferredOutput::ControlCommandGuard {
                    time,
                    output,
                    error,
                    flags,
                } => self.write_control_command_guard(time, &output, error, flags)?,
                DeferredOutput::ControlSourceFile(event) => {
                    self.write_control_source_file(event)?;
                }
                DeferredOutput::ControlCommandOutput(output) => self.payload(output.as_bytes())?,
            }
        }
        for cause in std::mem::take(&mut self.trailing_causes) {
            self.output.write_all(b"%config-error ")?;
            self.output.write_all(cause.as_bytes())?;
            self.output.write_all(b"\n")?;
        }
        if let Some(reason) = exit {
            if self.exit_held {
                self.deferred.push_back(DeferredOutput::Exit(reason));
            } else {
                self.write_exit(reason.as_deref())?;
            }
        }
        Ok(())
    }

    fn hold_exit(&mut self) {
        self.exit_held = true;
    }

    fn release_exit(&mut self) -> io::Result<()> {
        self.exit_held = false;
        if !self.block_open {
            self.flush_deferred()?;
            self.output.flush()?;
        }
        Ok(())
    }

    fn diagnostic_error(&mut self, text: &str) -> io::Result<()> {
        self.diagnostic_error_at(unix_timestamp(), text)
    }

    fn diagnostic_error_at(&mut self, time: u64, text: &str) -> io::Result<()> {
        if self.block_open {
            self.deferred.push_back(DeferredOutput::DiagnosticError {
                time,
                text: text.to_owned(),
            });
            return Ok(());
        }
        let frame = self.allocate_frame(time, 1)?;
        self.write_frame_begin(&frame)?;
        self.write_line(text)?;
        self.write_frame_end(&frame, true)?;
        self.output.flush()
    }

    fn control_command_guard(&mut self, output: &str, error: bool, flags: u8) -> io::Result<()> {
        self.control_command_guard_at(unix_timestamp(), output, error, flags)
    }

    fn control_command_guard_at(
        &mut self,
        time: u64,
        output: &str,
        error: bool,
        flags: u8,
    ) -> io::Result<()> {
        self.control_command_guard_bytes_at(time, output.as_bytes(), error, flags)
    }

    fn control_command_guard_bytes(
        &mut self,
        output: &[u8],
        error: bool,
        flags: u8,
    ) -> io::Result<()> {
        self.control_command_guard_bytes_at(unix_timestamp(), output, error, flags)
    }

    fn control_command_guard_bytes_at(
        &mut self,
        time: u64,
        output: &[u8],
        error: bool,
        flags: u8,
    ) -> io::Result<()> {
        if self.block_open {
            self.deferred
                .push_back(DeferredOutput::ControlCommandGuard {
                    time,
                    output: output.to_vec(),
                    error,
                    flags,
                });
            return Ok(());
        }
        self.write_control_command_guard(time, output, error, flags)?;
        self.output.flush()
    }

    fn write_control_command_guard(
        &mut self,
        time: u64,
        output: &[u8],
        error: bool,
        flags: u8,
    ) -> io::Result<()> {
        let frame = self.allocate_frame(time, flags)?;
        self.write_frame_begin(&frame)?;
        self.payload(output)?;
        self.write_frame_end(&frame, error)?;
        self.command_guard_frames = self.command_guard_frames.saturating_add(1);
        Ok(())
    }

    fn control_source_file(&mut self, event: ControlSourceFileEvent) -> io::Result<()> {
        if self.block_open {
            self.deferred
                .push_back(DeferredOutput::ControlSourceFile(event));
            return Ok(());
        }
        self.write_control_source_file(event)?;
        self.output.flush()
    }

    fn write_control_source_file(&mut self, event: ControlSourceFileEvent) -> io::Result<()> {
        match event {
            ControlSourceFileEvent::ReadError(text) => self.write_line(&text),
            ControlSourceFileEvent::Complete => {
                self.next_number = self.next_number.saturating_add(1);
                Ok(())
            }
        }
    }

    fn command_block_output(&mut self, output: &[u8]) -> io::Result<()> {
        self.payload(output)?;
        self.output.flush()
    }

    fn control_command_output(&mut self, output: &str) -> io::Result<()> {
        if self.block_open {
            self.deferred
                .push_back(DeferredOutput::ControlCommandOutput(output.to_owned()));
            return Ok(());
        }
        self.payload(output.as_bytes())?;
        self.output.flush()
    }

    fn begin(&mut self, flags: u8) -> io::Result<Frame> {
        self.begin_at(unix_timestamp(), flags)
    }

    fn begin_at(&mut self, time: u64, flags: u8) -> io::Result<Frame> {
        let frame = self.begin_buffered_at(time, flags)?;
        self.output.flush()?;
        Ok(frame)
    }

    fn begin_buffered_at(&mut self, time: u64, flags: u8) -> io::Result<Frame> {
        let frame = self.allocate_frame(time, flags)?;
        self.block_open = true;
        self.open_frame = Some(frame);
        self.write_frame_begin(&frame)?;
        Ok(frame)
    }

    fn allocate_frame(&mut self, time: u64, flags: u8) -> io::Result<Frame> {
        let number = self.next_number;
        self.next_number = self.next_number.saturating_add(1);
        let mut body = [0; 46];
        let mut remaining = body.as_mut_slice();
        writeln!(remaining, "{time} {number} {flags}")?;
        let length = (46 - remaining.len()) as u8;
        Ok(Frame { body, length })
    }

    fn write_frame_begin(&mut self, frame: &Frame) -> io::Result<()> {
        self.output.write_all(b"%begin ")?;
        self.output
            .write_all(&frame.body[..usize::from(frame.length)])
    }

    fn write_frame_end(&mut self, frame: &Frame, error: bool) -> io::Result<()> {
        let marker: &[u8] = if error { b"%error " } else { b"%end " };
        self.output.write_all(marker)?;
        self.output
            .write_all(&frame.body[..usize::from(frame.length)])
    }

    fn response(&mut self, frame: &Frame, response: CommandResponse) -> io::Result<u8> {
        match response {
            CommandResponse::Success {
                output, exit_code, ..
            } => {
                self.payload(output.as_bytes())?;
                self.end(frame, false)?;
                Ok(exit_code)
            }
            CommandResponse::Error { error, output, .. } => {
                self.payload(output.as_bytes())?;
                self.write_line(&error.tmux_message())?;
                self.end(frame, true)?;
                Ok(1)
            }
        }
    }

    fn parse_error(&mut self, error: &str) -> io::Result<()> {
        let frame = self.begin(1)?;
        self.write_line(error)?;
        self.end(&frame, true)
    }

    fn error(&mut self, frame: &Frame, error: &str) -> io::Result<()> {
        self.write_line(error)?;
        self.end(frame, true)
    }

    fn payload(&mut self, output: &[u8]) -> io::Result<()> {
        if !output.is_empty() {
            self.output.write_all(output)?;
            if output.last() != Some(&b'\n') {
                self.output.write_all(b"\n")?;
            }
        }
        Ok(())
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.output.write_all(line.as_bytes())?;
        self.output.write_all(b"\n")
    }

    fn end(&mut self, frame: &Frame, error: bool) -> io::Result<()> {
        self.write_frame_end(frame, error)?;
        self.block_open = false;
        self.open_frame = None;
        self.flush_deferred()?;
        self.output.flush()
    }

    fn emit_exit(&mut self, reason: Option<&str>) -> io::Result<()> {
        if self.exit_requested {
            return Ok(());
        }
        self.exit_requested = true;
        if self.block_open || self.exit_held {
            self.deferred
                .push_back(DeferredOutput::Exit(reason.map(str::to_owned)));
            return Ok(());
        }
        self.flush_deferred()?;
        self.write_exit(reason)?;
        self.output.flush()
    }

    fn write_exit(&mut self, reason: Option<&str>) -> io::Result<()> {
        match reason {
            Some(reason) => writeln!(self.output, "%exit {reason}")?,
            None => self.output.write_all(b"%exit\n")?,
        }
        Ok(())
    }

    fn close(&mut self)
    where
        W: ControlClose,
    {
        if self.double && !self.st_sent {
            let _ = self.output.write_all(ST);
            self.st_sent = true;
        }
        let _ = self.output.flush();
        self.output.close_sink();
    }

    fn terminate(&mut self) -> io::Result<()> {
        self.exit_held = false;
        if let Some(frame) = self.open_frame {
            self.end(&frame, false)?;
        }
        self.emit_exit(None)?;
        if self.st_sent {
            self.output.flush()
        } else {
            self.finish()
        }
    }

    fn finish(&mut self) -> io::Result<()> {
        if self.double {
            self.output.write_all(ST)?;
            self.st_sent = true;
        }
        self.output.flush()
    }
}

impl<W: Write> Drop for ControlWriter<W> {
    fn drop(&mut self) {
        if self.double && !self.st_sent {
            let _ = self.output.write_all(ST);
            let _ = self.output.flush();
        }
    }
}

#[derive(Clone, Copy)]
struct Frame {
    body: [u8; 46],
    length: u8,
}

struct StartedCommand {
    request_id: u64,
    flags: u8,
    canonical_name: Option<String>,
    command_guard_frames: u64,
    frame: Option<Frame>,
}

struct CommandResult {
    exit_code: u8,
    exit: ExitSignal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ExitSignal {
    #[default]
    None,
    Clean,
    Detached,
    TooFarBehind,
    Unexpected,
}

impl ExitSignal {
    fn is_some(self) -> bool {
        self != Self::None
    }

    fn reason(self) -> Option<&'static str> {
        match self {
            Self::TooFarBehind => Some("too far behind"),
            Self::Unexpected => Some("server exited unexpectedly"),
            Self::None | Self::Clean | Self::Detached => None,
        }
    }
}

enum MainEvent {
    Protocol(Box<ProtocolMessage>),
    Stdin(StdinEvent),
    Disconnected,
}

enum StdinEvent {
    Line(String),
    Eof,
    Error(String),
}

enum PendingReturn {
    Blank {
        code: u8,
        preceding_input: usize,
        observed_preceding_input: bool,
    },
    Eof {
        code: u8,
        preceding_input: usize,
        observed_preceding_input: bool,
    },
    InputError {
        message: String,
        preceding_input: usize,
    },
}

impl PendingReturn {
    fn from_stdin(
        stdin: StdinEvent,
        return_code: u8,
        preceding_input: usize,
    ) -> Result<Self, StdinEvent> {
        match stdin {
            StdinEvent::Line(line) if line.is_empty() => Ok(Self::Blank {
                code: return_code,
                preceding_input,
                observed_preceding_input: false,
            }),
            StdinEvent::Eof => Ok(Self::Eof {
                code: return_code,
                preceding_input,
                observed_preceding_input: false,
            }),
            StdinEvent::Error(message) => Ok(Self::InputError {
                message,
                preceding_input: 0,
            }),
            stdin @ StdinEvent::Line(_) => Err(stdin),
        }
    }

    fn code(&self) -> u8 {
        match self {
            Self::Blank { code, .. } | Self::Eof { code, .. } => *code,
            Self::InputError { .. } => 1,
        }
    }

    fn refresh_code_after_preceding_input(&mut self, return_code: u8) {
        match self {
            Self::Blank {
                code,
                observed_preceding_input: true,
                ..
            }
            | Self::Eof {
                code,
                observed_preceding_input: true,
                ..
            } => *code = return_code,
            Self::Blank { .. } | Self::Eof { .. } | Self::InputError { .. } => {}
        }
    }

    fn discards_pane_output(&self) -> bool {
        matches!(self, Self::Blank { .. } | Self::Eof { .. })
    }

    fn closes_notifications(&self) -> bool {
        matches!(self, Self::Blank { .. })
    }

    fn has_preceding_input(&self) -> bool {
        self.preceding_input() != 0
    }

    fn preceding_input(&self) -> usize {
        match self {
            Self::Blank {
                preceding_input, ..
            }
            | Self::Eof {
                preceding_input, ..
            }
            | Self::InputError {
                preceding_input, ..
            } => *preceding_input,
        }
    }

    fn consume_preceding_input(&mut self) {
        match self {
            Self::Blank {
                preceding_input,
                observed_preceding_input,
                ..
            }
            | Self::Eof {
                preceding_input,
                observed_preceding_input,
                ..
            } => {
                if *preceding_input != 0 {
                    *preceding_input -= 1;
                    *observed_preceding_input = true;
                }
            }
            Self::InputError {
                preceding_input, ..
            } => *preceding_input = preceding_input.saturating_sub(1),
        }
    }

    fn observe_preceding_input(&mut self) {
        match self {
            Self::Blank {
                observed_preceding_input,
                ..
            }
            | Self::Eof {
                observed_preceding_input,
                ..
            } => *observed_preceding_input = true,
            Self::InputError { .. } => {}
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg(test)]
enum ParsedLine {
    Return,
    Ignore,
    Commands(Vec<CommandInvocation>),
    Error(String),
}

struct ControlTerminal {
    #[cfg(unix)]
    original: Option<rustix::termios::Termios>,
}

impl ControlTerminal {
    fn enter(enabled: bool) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use rustix::termios::{
                ControlModes, InputModes, LocalModes, OptionalActions, OutputModes,
                SpecialCodeIndex,
            };

            if !enabled || !io::stdin().is_terminal() {
                return Ok(Self { original: None });
            }
            let original = rustix::termios::tcgetattr(io::stdin())?;
            let mut raw = original.clone();
            raw.make_raw();
            raw.input_modes = InputModes::ICRNL | InputModes::IXANY;
            raw.output_modes = OutputModes::OPOST | OutputModes::ONLCR;
            #[cfg(target_os = "macos")]
            {
                raw.local_modes = LocalModes::from_bits_retain(libc::NOKERNINFO);
            }
            #[cfg(not(target_os = "macos"))]
            {
                raw.local_modes = LocalModes::empty();
            }
            raw.control_modes = ControlModes::CREAD | ControlModes::CS8 | ControlModes::HUPCL;
            raw.special_codes[SpecialCodeIndex::VMIN] = 1;
            raw.special_codes[SpecialCodeIndex::VTIME] = 0;
            rustix::termios::tcsetattr(io::stdin(), OptionalActions::Now, &raw)?;
            Ok(Self {
                original: Some(original),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = enabled;
            Ok(Self {})
        }
    }
}

impl Drop for ControlTerminal {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(original) = self.original.as_ref() {
            let _ = rustix::termios::tcsetattr(
                io::stdin(),
                rustix::termios::OptionalActions::Flush,
                original,
            );
        }
    }
}

#[cfg(test)]
mod exit_tests;
#[cfg(test)]
mod frames_tests;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod notifications_tests;
