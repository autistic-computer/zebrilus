use super::*;

#[cfg(any(target_os = "linux", not(unix), test))]
pub(super) enum ReaderMessage {
    Data {
        buffer: Vec<u8>,
        length: usize,
    },
    #[cfg(target_os = "linux")]
    Home,
    Eof,
}

impl PtyEffects {
    pub(super) fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(256),
            overflowed: false,
        }
    }

    pub(super) fn push(&mut self, response: &[u8]) {
        if self.overflowed {
            return;
        }
        if response.len() > MAX_PTY_RESPONSE_BYTES.saturating_sub(self.bytes.len()) {
            self.overflowed = true;
            return;
        }
        self.bytes.extend_from_slice(response);
    }
}

pub(super) enum Wake {
    Command(Command),
    Input(QueuedInput),
    CommandsClosed,
    Search(SearchResults),
    ChildExit(std::io::Result<ExitStatus>),
    #[cfg(unix)]
    PtyReadable,
    #[cfg(any(target_os = "linux", not(unix)))]
    PtyMessage(ReaderMessage),
    Deadline,
}

#[cfg(unix)]
pub(super) enum DirectWrite {
    Closed,
    Written,
    Queued,
}

#[cfg(unix)]
impl DirectInput {
    pub(super) fn close(&self) {
        self.state.lock().open = false;
    }

    pub(super) fn reopen(&self, ready: impl FnOnce() -> bool) {
        let mut state = self.state.lock();
        state.open = state.fd.is_some() && !state.has_pending() && ready();
    }

    #[cfg(test)]
    pub(super) fn is_open(&self) -> bool {
        self.state.lock().open
    }

    pub(super) fn take_echo(&self) -> Option<Instant> {
        if self.echo.load(Ordering::Acquire) == 0 {
            return None;
        }
        echo_note_instant(self.echo.swap(0, Ordering::AcqRel))
    }

    pub(super) fn echo_noted(&self) -> bool {
        echo_note_instant(self.echo.load(Ordering::Acquire)).is_some()
    }

    pub(super) fn write(&self, bytes: &[u8]) -> DirectWrite {
        let mut state = self.state.lock();
        if !state.open || state.has_pending() {
            return DirectWrite::Closed;
        }
        let written = u64::try_from(OUTPUT_CLOCK.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.echo
            .store(written.saturating_add(1), Ordering::Release);
        state.compact_for(bytes.len());
        state.pending.extend_from_slice(bytes);
        if state.flush_pending().is_err() || state.has_pending() {
            state.open = false;
            return DirectWrite::Queued;
        }
        DirectWrite::Written
    }
}

#[cfg(unix)]
fn echo_note_instant(note: u64) -> Option<Instant> {
    let written = OUTPUT_CLOCK.checked_add(Duration::from_nanos(note.checked_sub(1)?))?;
    (written.elapsed() < ECHO_WINDOW).then_some(written)
}

#[cfg(unix)]
impl PtyWriteState {
    fn has_pending(&self) -> bool {
        self.offset < self.pending.len()
    }

    fn flush_pending(&mut self) -> std::io::Result<()> {
        let mut budget = PTY_WRITE_BUDGET_BYTES;
        while self.has_pending() && budget != 0 {
            let Some(fd) = &self.fd else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "PTY writer is closed",
                ));
            };
            let end = self.offset.saturating_add(budget).min(self.pending.len());
            match rustix::io::write(fd, &self.pending[self.offset..end]) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "PTY writer accepted no bytes",
                    ));
                }
                Ok(written) => {
                    self.offset = self.offset.saturating_add(written);
                    budget = budget.saturating_sub(written);
                }
                Err(rustix::io::Errno::AGAIN) => return Ok(()),
                Err(rustix::io::Errno::INTR) => {}
                Err(errno) => return Err(std::io::Error::from(errno)),
            }
        }
        if !self.has_pending() {
            self.pending.clear();
            self.offset = 0;
            if self.pending.capacity() > PTY_WRITE_RETAIN_BYTES {
                self.pending = Vec::new();
            }
        }
        Ok(())
    }

    fn compact_for(&mut self, additional: usize) {
        if self.pending.capacity().saturating_sub(self.pending.len()) < additional
            && self.offset != 0
        {
            let queued = self.pending.len().saturating_sub(self.offset);
            self.pending.copy_within(self.offset.., 0);
            self.pending.truncate(queued);
            self.offset = 0;
        }
    }
}

#[cfg(unix)]
impl PtyWriter {
    #[cfg(test)]
    pub(super) fn new(fd: filedescriptor::FileDescriptor) -> Self {
        Self::attach(Arc::default(), fd)
    }

    pub(super) fn attach(direct: Arc<DirectInput>, fd: filedescriptor::FileDescriptor) -> Self {
        {
            let mut state = direct.state.lock();
            state.open = false;
            state.fd = Some(fd);
        }
        Self { direct }
    }

    pub(super) fn has_pending(&self) -> bool {
        self.direct.state.lock().has_pending()
    }

    #[cfg(test)]
    pub(super) fn queued_bytes(&self) -> usize {
        let state = self.direct.state.lock();
        state.pending.len().saturating_sub(state.offset)
    }

    #[cfg(test)]
    pub(super) fn pending_capacity(&self) -> usize {
        self.direct.state.lock().pending.capacity()
    }

    pub(super) fn flush_pending(&mut self) -> std::io::Result<()> {
        self.direct.state.lock().flush_pending()
    }
}

#[cfg(unix)]
impl Drop for PtyWriter {
    fn drop(&mut self) {
        let mut state = self.direct.state.lock();
        state.open = false;
        state.fd = None;
    }
}

#[cfg(unix)]
impl Write for PtyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.direct.state.lock();
        state.compact_for(buf.len());
        state.pending.extend_from_slice(buf);
        state.flush_pending()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_pending()
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
pub(super) fn wait_for_wake(
    control_rx: &Receiver<Command>,
    input_rx: Option<&Receiver<QueuedInput>>,
    search_results: &Receiver<SearchResults>,
    mut child: Option<&mut ChildExitWatch>,
    pty: Option<&filedescriptor::FileDescriptor>,
    wake_rx: &std::os::fd::OwnedFd,
    timeout: Duration,
) -> Result<Wake, WorkerError> {
    use crossbeam_channel::TryRecvError;
    use rustix::event::{PollFd, PollFlags};

    fn check_channels(
        control_rx: &Receiver<Command>,
        input_rx: Option<&Receiver<QueuedInput>>,
        search_results: &Receiver<SearchResults>,
    ) -> Result<Option<Wake>, WorkerError> {
        match control_rx.try_recv() {
            Ok(command) => return Ok(Some(Wake::Command(command))),
            Err(TryRecvError::Disconnected) => return Ok(Some(Wake::CommandsClosed)),
            Err(TryRecvError::Empty) => {}
        }
        if let Some(input_rx) = input_rx {
            match input_rx.try_recv() {
                Ok(command) => return Ok(Some(Wake::Input(command))),
                Err(TryRecvError::Disconnected | TryRecvError::Empty) => {}
            }
        }
        match search_results.try_recv() {
            Ok(result) => Ok(Some(Wake::Search(result))),
            Err(TryRecvError::Disconnected) => Err(WorkerError::Thread(
                "terminal search worker stopped".to_owned(),
            )),
            Err(TryRecvError::Empty) => Ok(None),
        }
    }

    if let Some(wake) = check_channels(control_rx, input_rx, search_results)? {
        return Ok(wake);
    }
    if let Some(status) = child.as_deref_mut().and_then(ChildExitWatch::take_ready) {
        return Ok(Wake::ChildExit(status));
    }

    let timespec = rustix::event::Timespec::try_from(timeout.min(Duration::from_hours(1)))
        .expect("a bounded timeout fits in a timespec");
    let readable = PollFlags::IN | PollFlags::HUP | PollFlags::ERR;
    let (pty_ready, wake_ready, child_ready) = {
        let mut fds = SmallVec::<[PollFd<'_>; 3]>::new();
        fds.push(PollFd::new(wake_rx, PollFlags::IN));
        let pty_index = pty.map(|pty| {
            fds.push(PollFd::new(pty, PollFlags::IN));
            fds.len() - 1
        });
        let child_index = child
            .as_deref()
            .and_then(ChildExitWatch::poll_fd)
            .map(|kqueue| {
                fds.push(PollFd::new(kqueue, PollFlags::IN));
                fds.len() - 1
            });
        match rustix::event::poll(&mut fds, Some(&timespec)) {
            Ok(_) => {
                let ready = |index: Option<usize>| {
                    index.is_some_and(|index| fds[index].revents().intersects(readable))
                };
                (ready(pty_index), ready(Some(0)), ready(child_index))
            }
            Err(rustix::io::Errno::INTR) => (false, false, false),
            Err(error) => return Err(WorkerError::Io(error.into())),
        }
    };
    if wake_ready {
        drain_wake_pipe(wake_rx)?;
        if let Some(wake) = check_channels(control_rx, input_rx, search_results)? {
            return Ok(wake);
        }
    }
    if child_ready && let Some(status) = child.and_then(ChildExitWatch::on_readable) {
        return Ok(Wake::ChildExit(status));
    }
    if pty_ready {
        return Ok(Wake::PtyReadable);
    }
    Ok(Wake::Deadline)
}

#[cfg(unix)]
pub(super) const PANE_ENVIRONMENT_KEYS: [&str; 6] = [
    "TERM",
    "COLORTERM",
    "CLAUDE_CODE_TMUX_TRUECOLOR",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "SHELL",
];

#[cfg(unix)]
impl UnixChildKiller {
    pub(super) fn kill(&mut self) -> std::io::Result<()> {
        let Some(pid) = self
            .0
            .and_then(|pid| i32::try_from(pid).ok())
            .and_then(rustix::process::Pid::from_raw)
        else {
            return Ok(());
        };
        rustix::process::kill_process(pid, rustix::process::Signal::HUP).map_err(Into::into)
    }
}

#[cfg(unix)]
fn exit_status_from_wait(status: rustix::process::WaitStatus) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from(std::process::ExitStatus::from_raw(status.as_raw()))
}

#[cfg(unix)]
pub(super) fn reap_child(
    pid: rustix::process::Pid,
    block: bool,
) -> Option<std::io::Result<ExitStatus>> {
    let options = if block {
        rustix::process::WaitOptions::empty()
    } else {
        rustix::process::WaitOptions::NOHANG
    };
    loop {
        match rustix::process::waitpid(Some(pid), options) {
            Ok(Some((_, status))) => return Some(Ok(exit_status_from_wait(status))),
            Ok(None) => return None,
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Some(Err(error.into())),
        }
    }
}

#[cfg(unix)]
pub(super) fn child_pid(process_id: Option<u32>) -> Result<rustix::process::Pid, WorkerError> {
    process_id
        .and_then(|pid| i32::try_from(pid).ok())
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| WorkerError::Spawn("the spawned child has no process id".to_owned()))
}

#[cfg(all(unix, not(target_os = "linux")))]
impl ChildExitWatch {
    #[cfg(target_os = "macos")]
    pub(super) fn for_shard(process_id: Option<u32>) -> Result<Self, WorkerError> {
        let pid = child_pid(process_id)?;
        let ready = reap_child(pid, false);
        Ok(Self {
            pid,
            kqueue: None,
            reaped: ready.is_some(),
            ready,
            retry: None,
        })
    }

    pub(super) fn new(process_id: Option<u32>) -> Result<Self, WorkerError> {
        use rustix::event::kqueue::{Event, EventFilter, EventFlags, ProcessEvents};

        let pid = child_pid(process_id)?;
        let kqueue =
            rustix::event::kqueue::kqueue().map_err(|error| WorkerError::Io(error.into()))?;
        let change = [Event::new(
            EventFilter::Proc {
                pid,
                flags: ProcessEvents::EXIT,
            },
            EventFlags::ADD | EventFlags::ONESHOT,
            std::ptr::null_mut(),
        )];
        #[allow(
            unsafe_code,
            reason = "a process filter names no descriptor that could close under the kqueue"
        )]
        let registered = unsafe {
            rustix::event::kqueue::kevent(&kqueue, &change, &mut [] as &mut [Event; 0], None)
        };
        match registered {
            Ok(_) => {
                let ready = reap_child(pid, false);
                Ok(Self {
                    pid,
                    kqueue: ready.is_none().then_some(kqueue),
                    reaped: ready.is_some(),
                    ready,
                    #[cfg(target_os = "macos")]
                    retry: None,
                })
            }
            Err(rustix::io::Errno::SRCH) => {
                let ready = reap_child(pid, false).or_else(|| reap_child(pid, true));
                Ok(Self {
                    pid,
                    kqueue: None,
                    reaped: ready.is_some(),
                    ready,
                    #[cfg(target_os = "macos")]
                    retry: None,
                })
            }
            Err(error) => Err(WorkerError::Io(error.into())),
        }
    }

    pub(super) fn poll_fd(&self) -> Option<&std::os::fd::OwnedFd> {
        self.kqueue.as_ref()
    }

    pub(super) fn take_ready(&mut self) -> Option<std::io::Result<ExitStatus>> {
        self.ready.take()
    }

    pub(super) fn on_readable(&mut self) -> Option<std::io::Result<ExitStatus>> {
        use rustix::event::kqueue::Event;

        let exited = self.kqueue.as_ref().is_some_and(|kqueue| {
            let mut events = Vec::<Event>::with_capacity(1);
            #[allow(
                unsafe_code,
                reason = "reading events registers nothing, so no descriptor can dangle"
            )]
            let read = unsafe {
                rustix::event::kqueue::kevent(kqueue, &[], &mut events, Some(Duration::ZERO))
            };
            read.is_ok_and(|count| count > 0)
        });
        let Some(status) = reap_child(self.pid, exited) else {
            #[cfg(target_os = "macos")]
            if self.kqueue.is_none() {
                self.retry = Some(Instant::now() + SHARD_CHILD_RETRY);
            }
            return None;
        };
        self.kqueue = None;
        self.reaped = true;
        Some(status)
    }

    #[cfg(target_os = "macos")]
    pub(super) fn retry_due(&mut self) -> Option<std::io::Result<ExitStatus>> {
        if self.retry.is_none_or(|due| due > Instant::now()) {
            return None;
        }
        self.retry = None;
        let status = reap_child(self.pid, false)?;
        self.reaped = true;
        Some(status)
    }

    pub(super) fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> Option<std::io::Result<ExitStatus>> {
        use rustix::event::{PollFd, PollFlags};

        if let Some(status) = self.ready.take() {
            return Some(status);
        }
        self.kqueue.as_ref()?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = reap_child(self.pid, false) {
                self.kqueue = None;
                self.reaped = true;
                return Some(status);
            }
            let kqueue = self.kqueue.as_ref()?;
            let remaining = deadline.checked_duration_since(Instant::now())?;
            let timespec = rustix::event::Timespec::try_from(remaining)
                .expect("a bounded timeout fits in a timespec");
            let mut fds = [PollFd::new(kqueue, PollFlags::IN)];
            match rustix::event::poll(&mut fds, Some(&timespec)) {
                Ok(0) => return None,
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(_) => return None,
            }
            if let Some(status) = self.on_readable() {
                return Some(status);
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
impl Drop for ChildExitWatch {
    fn drop(&mut self) {
        if self.reaped || reap_child(self.pid, false).is_some() {
            return;
        }
        let pid = self.pid;
        if let Err(error) = thread::Builder::new()
            .name("zz-child-reap".into())
            .spawn(move || {
                let _ = reap_child(pid, true);
            })
        {
            log::warn!(
                "could not wait for pane child {}: {error}",
                pid.as_raw_nonzero()
            );
        }
    }
}

#[cfg(unix)]
pub(super) fn drain_wake_pipe(wake_rx: &std::os::fd::OwnedFd) -> Result<(), WorkerError> {
    let mut drained = [0_u8; 64];
    loop {
        match rustix::io::read(wake_rx, &mut drained) {
            Ok(0) | Err(rustix::io::Errno::AGAIN) => return Ok(()),
            Ok(read) if read < drained.len() => return Ok(()),
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(WorkerError::Io(error.into())),
        }
    }
}

#[cfg(not(unix))]
pub(super) fn wait_for_wake(
    control_rx: &Receiver<Command>,
    input_rx: Option<&Receiver<QueuedInput>>,
    search_results: &Receiver<SearchResults>,
    child_exit: &Receiver<std::io::Result<ExitStatus>>,
    output_rx: &Receiver<ReaderMessage>,
    timeout: Duration,
) -> Result<Wake, WorkerError> {
    if let Some(input_rx) = input_rx {
        crossbeam_channel::select_biased! {
            recv(control_rx) -> message => Ok(message.map_or(Wake::CommandsClosed, Wake::Command)),
            recv(input_rx) -> message => message.map(Wake::Input).map_err(|_| {
                WorkerError::Thread("terminal input channel stopped".to_owned())
            }),
            recv(search_results) -> result => result.map(Wake::Search).map_err(|_| {
                WorkerError::Thread("terminal search worker stopped".to_owned())
            }),
            recv(output_rx) -> message => Ok(Wake::PtyMessage(message.unwrap_or(ReaderMessage::Eof))),
            recv(child_exit) -> status => status.map(Wake::ChildExit).map_err(|_| {
                WorkerError::Thread("terminal child waiter stopped".to_owned())
            }),
            default(timeout) => Ok(Wake::Deadline),
        }
    } else {
        crossbeam_channel::select_biased! {
            recv(control_rx) -> message => Ok(message.map_or(Wake::CommandsClosed, Wake::Command)),
            recv(search_results) -> result => result.map(Wake::Search).map_err(|_| {
                WorkerError::Thread("terminal search worker stopped".to_owned())
            }),
            recv(output_rx) -> message => Ok(Wake::PtyMessage(message.unwrap_or(ReaderMessage::Eof))),
            recv(child_exit) -> status => status.map(Wake::ChildExit).map_err(|_| {
                WorkerError::Thread("terminal child waiter stopped".to_owned())
            }),
            default(timeout) => Ok(Wake::Deadline),
        }
    }
}

#[cfg(any(target_os = "linux", not(unix)))]
impl BufferReturn {
    pub(super) fn give(&self, buffer: Vec<u8>) {
        let _ = self.buffers.try_send(buffer);
        #[cfg(target_os = "linux")]
        self.gather.returned();
    }
}

#[cfg(any(not(unix), test))]
#[allow(
    clippy::needless_pass_by_value,
    reason = "the reader thread owns the final sender so disconnect signals shutdown"
)]
pub(super) fn read_pty(
    mut reader: Box<dyn Read + Send>,
    pending_output: Box<dyn Fn() -> usize + Send>,
    output: Sender<ReaderMessage>,
    recycled: Receiver<Vec<u8>>,
    wake: ActorWake,
) {
    let mut eof = false;
    while !eof {
        let Ok(mut buffer) = recycled.recv() else {
            return;
        };
        debug_assert_eq!(buffer.len(), PTY_READ_BUFFER_BYTES);
        let mut length = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                log::debug!("PTY reader reached EOF: {error}");
                break;
            }
        };
        let mut pending = pending_output();
        while length < buffer.len() && pending > 0 {
            let want = pending.min(buffer.len() - length);
            match reader.read(&mut buffer[length..length + want]) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(read) => {
                    length += read;
                    pending = pending.saturating_sub(read);
                    if pending == 0 {
                        pending = pending_output();
                    }
                }
                Err(error) => {
                    log::debug!("PTY reader reached EOF: {error}");
                    eof = true;
                    break;
                }
            }
        }
        if output.send(ReaderMessage::Data { buffer, length }).is_err() {
            return;
        }
        wake.notify();
    }
    let _ = output.send(ReaderMessage::Eof);
    wake.notify();
}

#[cfg(any(target_os = "linux", not(unix)))]
pub(super) fn consume_pty_output(
    terminal: &mut Terminal<'_, '_>,
    passthrough: &mut PassthroughFilter,
    engine: &mut EngineOutput<'_>,
    publisher: &Publisher,
    buffer: Vec<u8>,
    length: usize,
    recycled: &BufferReturn,
) -> usize {
    log::trace!(
        target: "zz_terminal::diagnostics::pty",
        "read length={length} bytes={:?} text={:?}",
        &buffer[..length],
        String::from_utf8_lossy(&buffer[..length]),
    );
    if publisher.takes_output() {
        publisher.output(&Arc::from(&buffer[..length]));
    }
    let parsed = feed_pty_output(terminal, passthrough, engine, &buffer[..length]);
    recycled.give(buffer);
    parsed
}

pub(super) fn feed_pty_output(
    terminal: &mut Terminal<'_, '_>,
    passthrough: &mut PassthroughFilter,
    engine: &mut EngineOutput<'_>,
    bytes: &[u8],
) -> usize {
    let EngineOutput {
        filter,
        knobs,
        renames,
        bar,
        last_command_status,
    } = engine;
    if !bytes.is_empty() {
        filter.count_output();
    }
    passthrough.write(bytes, |unwrapped| {
        filter.write(
            unwrapped,
            *knobs,
            terminal,
            renames,
            bar,
            last_command_status,
        );
    })
}

#[cfg(any(target_os = "linux", not(unix), test))]
pub(super) fn drain_pty_output_burst(
    output: &Receiver<ReaderMessage>,
    first_buffer: Vec<u8>,
    first_length: usize,
    limit_turn: bool,
    max_chunks: usize,
    mut consume: impl FnMut(Vec<u8>, usize),
) -> Option<ReaderMessage> {
    let started = limit_turn.then(Instant::now);
    let mut bytes = first_length;
    consume(first_buffer, first_length);

    for _ in 1..PTY_BUFFER_POOL_SIZE.min(max_chunks) {
        if limit_turn
            && (bytes >= PTY_DRAIN_TURN_BYTES
                || started.is_some_and(|started| started.elapsed() >= PTY_DRAIN_TURN_TIME))
        {
            break;
        }
        match output.try_recv() {
            Ok(ReaderMessage::Data { buffer, length }) => {
                bytes += length;
                consume(buffer, length);
            }
            Ok(message) => return Some(message),
            Err(_) => break,
        }
    }
    None
}

pub(super) fn drain_effects(
    effects: &RefCell<PtyEffects>,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    let mut effects = effects.borrow_mut();
    if !effects.bytes.is_empty() {
        writer.write_all(&effects.bytes)?;
        effects.bytes.clear();
    }
    let overflowed = std::mem::take(&mut effects.overflowed);
    drop(effects);
    if overflowed {
        log::warn!("terminal PTY responses exceeded the {MAX_PTY_RESPONSE_BYTES}-byte limit");
    }
    writer.flush()?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn pty_effects_pending(effects: &RefCell<PtyEffects>) -> bool {
    let effects = effects.borrow();
    !effects.bytes.is_empty() || effects.overflowed
}

#[cfg(unix)]
pub(super) fn drain_effects_if_writer_ready(
    effects: &RefCell<PtyEffects>,
    writer: &mut PtyWriter,
) -> Result<(), WorkerError> {
    if writer.has_pending() || !pty_effects_pending(effects) {
        return Ok(());
    }
    drain_effects(effects, writer)
}
