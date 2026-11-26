use super::*;

impl Default for Geometry {
    fn default() -> Self {
        Self {
            columns: INITIAL_COLUMNS,
            rows: INITIAL_ROWS,
            cell_width_px: INITIAL_CELL_WIDTH,
            cell_height_px: INITIAL_CELL_HEIGHT,
        }
    }
}

impl Geometry {
    pub(super) fn from_size(size: TerminalSize) -> Self {
        Self {
            columns: size.columns.max(1),
            rows: size.rows.max(1),
            cell_width_px: size.cell_width_px.max(1),
            cell_height_px: size.cell_height_px.max(1),
        }
    }

    pub(super) fn pty_size(self) -> PtySize {
        let width = u32::from(self.columns).saturating_mul(self.cell_width_px);
        let height = u32::from(self.rows).saturating_mul(self.cell_height_px);
        PtySize {
            rows: self.rows,
            cols: self.columns,
            pixel_width: u16::try_from(width).unwrap_or(u16::MAX),
            pixel_height: u16::try_from(height).unwrap_or(u16::MAX),
        }
    }

    pub(super) fn size_report(self) -> SizeReportSize {
        SizeReportSize {
            rows: self.rows,
            columns: self.columns,
            cell_width: self.cell_width_px,
            cell_height: self.cell_height_px,
        }
    }
}

#[derive(Debug)]
pub(super) enum Command {
    Text {
        view: Option<TerminalViewId>,
        text: Arc<str>,
    },
    Key {
        view: Option<TerminalViewId>,
        input: Box<KeyInput>,
    },
    Resize(Geometry),
    SetWordSeparators(Box<WordSeparators>),
    SetAppearance(Arc<TerminalAppearance>),
    SetAllowPassthrough(AllowPassthrough),
    SetWrapSearch(bool),
    SetEngineKnobs(EngineKnobs),
    CaptureCopySource {
        reply: ActorReply<Result<CapturedCopySource, TerminalCaptureError>>,
    },
    SetPendingCopySource(Option<Box<CapturedCopySource>>),
    WriteDeadNotice(Option<Arc<str>>),
    ResetScreen,
    AttachView(TerminalViewId),
    DetachView(TerminalViewId),
    ReleaseView(TerminalViewId),
    ViewAction {
        view: TerminalViewId,
        action: TerminalViewAction,
    },
    PastePreparedBytes {
        view: Option<TerminalViewId>,
        bytes: Arc<[u8]>,
        bracketed: bool,
    },
    RawInput(Arc<[u8]>),
    Output(Arc<[u8]>),
    Settle {
        reply: ActorReply<()>,
    },
    Capture(Box<CaptureRequest>),
    PointerContext(Box<PointerContextRequest>),
    SemanticCapture(Box<LastCommandRequest>),
    History(Box<HistoryCommand>),
    KittyImage(Box<KittyImageRequest>),
    KittyImageGeneration(Box<KittyImageGenerationRequest>),
    PendingPasteOpened {
        token: u64,
    },
    UnbindPastedImage {
        number: u32,
    },
    Terminate,
    Shutdown,
    SetViewStream(TerminalViewId, ViewStream),
    SetViewAreas(HashMap<TerminalViewId, (u16, u16)>),
    SetPreviewWatch(bool),
    FreshViewport(Sender<Arc<TerminalViewport>>),
    Wake,
}

impl ControlSlot {
    fn release_deferred(&mut self, commands: &mut Vec<Command>) {
        if !self.deferred.iter().any(|(remaining, _)| *remaining == 0) {
            return;
        }
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.deferred)
            .into_iter()
            .partition(|(remaining, _)| *remaining == 0);
        self.deferred = waiting;
        commands.extend(ready.into_iter().map(|(_, command)| command));
    }
}

impl PendingControl {
    pub(super) fn push_view(&mut self, command: Command) {
        let view = match &command {
            Command::AttachView(view)
            | Command::DetachView(view)
            | Command::ReleaseView(view)
            | Command::SetViewStream(view, _) => *view,
            _ => unreachable!("only view commands coalesce per view"),
        };
        let replaces = |queued: &Command| {
            matches!(
                (&command, queued),
                (Command::ReleaseView(_), _)
                    | (
                        Command::AttachView(_) | Command::DetachView(_),
                        Command::AttachView(_) | Command::DetachView(_),
                    )
                    | (Command::SetViewStream(..), Command::SetViewStream(..))
            )
        };
        self.views.retain(|queued| {
            let same = match queued {
                Command::AttachView(queued)
                | Command::DetachView(queued)
                | Command::ReleaseView(queued)
                | Command::SetViewStream(queued, _) => *queued == view,
                _ => false,
            };
            !(same && replaces(queued))
        });
        self.views.push(command);
    }

    fn take(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        if let Some(separators) = self.word_separators.take() {
            commands.push(Command::SetWordSeparators(separators));
        }
        if let Some(mode) = self.allow_passthrough.take() {
            commands.push(Command::SetAllowPassthrough(mode));
        }
        if let Some(enabled) = self.wrap_search.take() {
            commands.push(Command::SetWrapSearch(enabled));
        }
        if let Some(knobs) = self.engine_knobs.take() {
            commands.push(Command::SetEngineKnobs(knobs));
        }
        if let Some(appearance) = self.appearance.take() {
            commands.push(Command::SetAppearance(appearance));
        }
        if let Some(areas) = self.view_areas.take() {
            commands.push(Command::SetViewAreas(areas));
        }
        if let Some(geometry) = self.resize.take() {
            commands.push(Command::Resize(geometry));
        }
        commands.append(&mut self.views);
        if let Some(watch) = self.preview.take() {
            commands.push(Command::SetPreviewWatch(watch));
        }
        commands
    }
}

#[cfg(unix)]
pub(super) fn slot_is_idle(slot: &ControlSlot) -> bool {
    !slot.wake_queued && slot.in_flight == 0 && slot.deferred.is_empty()
}

pub(super) fn take_control_slot(
    slot: &Mutex<ControlSlot>,
    woke_by: Option<Command>,
    from_control: bool,
) -> Vec<Command> {
    let mut slot = slot.lock();
    slot.wake_queued = false;
    let mut commands = slot.pending.take();
    slot.release_deferred(&mut commands);
    let Some(command) = woke_by else {
        return commands;
    };
    let counted = from_control && !matches!(command, Command::Wake);
    if from_control && let Some(waker) = slot.raw_input_space_waker.take() {
        waker.wake();
    }
    commands.push(command);
    if counted {
        slot.in_flight = slot.in_flight.saturating_sub(1);
        for (remaining, _) in &mut slot.deferred {
            *remaining = remaining.saturating_sub(1);
        }
        slot.release_deferred(&mut commands);
    }
    commands
}

impl Command {
    pub(super) const fn name(&self) -> &'static str {
        match self {
            Self::Text { .. } => "text",
            Self::Key { .. } => "key",
            Self::Resize(_) => "resize",
            Self::SetWordSeparators(_) => "set-word-separators",
            Self::SetAppearance(_) => "set-appearance",
            Self::SetAllowPassthrough(_) => "set-allow-passthrough",
            Self::SetWrapSearch(_) => "set-wrap-search",
            Self::SetEngineKnobs(_) => "set-engine-knobs",
            Self::CaptureCopySource { .. } => "capture-copy-source",
            Self::SetPendingCopySource(_) => "set-pending-copy-source",
            Self::WriteDeadNotice(_) => "write-dead-notice",
            Self::AttachView(_) => "attach-view",
            Self::DetachView(_) => "detach-view",
            Self::ReleaseView(_) => "release-view",
            Self::ViewAction { .. } => "view-action",
            Self::PastePreparedBytes { .. } => "paste-prepared-bytes",
            Self::RawInput(_) => "raw-input",
            Self::Output(_) => "output",
            Self::Settle { .. } => "settle",
            Self::Capture(_) => "capture",
            Self::PointerContext(_) => "pointer-context",
            Self::SemanticCapture(_) => "semantic-capture",
            Self::History(_) => "history",
            Self::KittyImage(_) => "kitty-image",
            Self::KittyImageGeneration(_) => "kitty-image-generation",
            Self::PendingPasteOpened { .. } => "pending-paste-opened",
            Self::UnbindPastedImage { .. } => "unbind-pasted-image",
            Self::ResetScreen => "reset-screen",
            Self::Terminate => "terminate",
            Self::Shutdown => "shutdown",
            Self::SetViewStream(..) => "set-view-stream",
            Self::SetViewAreas(_) => "set-view-areas",
            Self::SetPreviewWatch(_) => "set-preview-watch",
            Self::FreshViewport(_) => "fresh-viewport",
            Self::Wake => "wake",
        }
    }

    pub(super) fn pty_input_payload(&self) -> Option<usize> {
        Some(match self {
            Self::Text { text, .. } => text.len(),
            Self::Key { input, .. } => input.text.as_deref().map_or(0, str::len),
            Self::ViewAction { action, .. } => match action {
                TerminalViewAction::Mouse(_)
                | TerminalViewAction::ScrollWheel { .. }
                | TerminalViewAction::Focus(_) => 0,
                TerminalViewAction::Paste(text) => text.len(),
                _ => return None,
            },
            Self::PastePreparedBytes { bytes, .. } => bytes.len(),
            Self::PendingPasteOpened { .. } => 0,
            _ => return None,
        })
    }

    fn pty_input_bytes(&self) -> Option<usize> {
        self.pty_input_payload()
            .map(|payload| payload.saturating_add(PTY_INPUT_COMMAND_FLOOR_BYTES))
    }
}

pub(super) fn command_channel() -> (Sender<Command>, Receiver<Command>) {
    crossbeam_channel::bounded(MAX_PENDING_ACTOR_COMMANDS)
}

#[derive(Default)]
pub(super) struct InputAdmission {
    commands: usize,
    bytes: usize,
    closed: bool,
    overflow: VecDeque<(Command, usize)>,
    refill: Option<Sender<QueuedInput>>,
}

impl InputAdmission {
    fn refill(&mut self, admission: &Arc<Mutex<Self>>) {
        if self.closed {
            self.close();
            return;
        }
        let Some((command, bytes)) = self.overflow.pop_front() else {
            return;
        };
        let queued = QueuedInput {
            command,
            permit: InputPermit {
                admission: Some(Arc::clone(admission)),
                bytes,
            },
        };
        let sender = self
            .refill
            .as_ref()
            .expect("an input overflow keeps its refill sender");
        match sender.try_send(queued) {
            Ok(()) => self.commands += 1,
            Err(error) => {
                let mut queued = error.into_inner();
                queued.permit.admission = None;
                self.bytes = self.bytes.saturating_sub(bytes);
                log::warn!(
                    "rejected terminal PTY input command={} from the overflow",
                    queued.command.name()
                );
            }
        }
        if self.overflow.is_empty() {
            self.overflow = VecDeque::new();
            self.refill = None;
        }
    }

    fn close(&mut self) {
        self.closed = true;
        for (_, bytes) in self.overflow.drain(..) {
            self.bytes = self.bytes.saturating_sub(bytes);
        }
        self.refill = None;
    }
}

pub(super) struct InputPermit {
    admission: Option<Arc<Mutex<InputAdmission>>>,
    bytes: usize,
}

impl Drop for InputPermit {
    fn drop(&mut self) {
        let Some(admission) = self.admission.take() else {
            return;
        };
        let mut state = admission.lock();
        state.commands = state.commands.saturating_sub(1);
        state.bytes = state.bytes.saturating_sub(self.bytes);
        state.refill(&admission);
    }
}

impl std::ops::Deref for InputReceiver {
    type Target = Receiver<QueuedInput>;

    fn deref(&self) -> &Self::Target {
        &self.commands
    }
}

impl InputReceiver {
    #[cfg(unix)]
    pub(super) fn is_idle(&self) -> bool {
        let admission = self.admission.lock();
        admission.commands == 0 && admission.overflow.is_empty()
    }
}

impl Drop for InputReceiver {
    fn drop(&mut self) {
        self.admission.lock().close();
        while self.commands.try_recv().is_ok() {}
    }
}

impl InputSender {
    pub(super) fn try_send(
        &self,
        command: Command,
    ) -> Result<(), crossbeam_channel::TrySendError<Command>> {
        let bytes = command
            .pty_input_bytes()
            .expect("only PTY input commands use the input queue");
        let result = {
            let mut admission = self.admission.lock();
            if admission.closed {
                return Err(crossbeam_channel::TrySendError::Disconnected(command));
            }
            if admission.commands >= self.max_commands || !admission.overflow.is_empty() {
                let charge = command
                    .pty_input_payload()
                    .unwrap_or(0)
                    .saturating_add(PTY_INPUT_OVERFLOW_ENTRY_BYTES);
                if charge > self.max_bytes.saturating_sub(admission.bytes) {
                    return Err(crossbeam_channel::TrySendError::Full(command));
                }
                admission.bytes += charge;
                admission.overflow.push_back((command, charge));
                admission
                    .refill
                    .get_or_insert_with(|| self.commands.clone());
                return Ok(());
            }
            if bytes > self.max_bytes.saturating_sub(admission.bytes) {
                return Err(crossbeam_channel::TrySendError::Full(command));
            }
            admission.commands += 1;
            admission.bytes += bytes;
            let queued = QueuedInput {
                command,
                permit: InputPermit {
                    admission: Some(Arc::clone(&self.admission)),
                    bytes,
                },
            };
            self.commands.try_send(queued)
        };
        result.map_err(|error| match error {
            crossbeam_channel::TrySendError::Full(queued) => {
                let QueuedInput { command, permit } = queued;
                drop(permit);
                crossbeam_channel::TrySendError::Full(command)
            }
            crossbeam_channel::TrySendError::Disconnected(queued) => {
                let QueuedInput { command, permit } = queued;
                drop(permit);
                crossbeam_channel::TrySendError::Disconnected(command)
            }
        })
    }

    const fn capacity(&self) -> usize {
        self.max_commands
    }

    pub(super) fn pending(&self) -> (usize, usize) {
        let admission = self.admission.lock();
        (
            admission.commands + admission.overflow.len(),
            admission.bytes,
        )
    }
}

pub(super) fn input_channel() -> (InputSender, InputReceiver) {
    input_channel_with_limits(MAX_PENDING_PTY_INPUT_COMMANDS, MAX_PENDING_PTY_INPUT_BYTES)
}

pub(super) fn input_channel_with_limits(
    max_commands: usize,
    max_bytes: usize,
) -> (InputSender, InputReceiver) {
    let (commands, receiver) = crossbeam_channel::bounded(max_commands);
    let admission = Arc::new(Mutex::new(InputAdmission::default()));
    #[cfg(unix)]
    let direct = Arc::new(DirectInput::default());
    (
        InputSender {
            commands,
            admission: Arc::clone(&admission),
            max_commands,
            max_bytes,
            #[cfg(unix)]
            direct: Arc::clone(&direct),
        },
        InputReceiver {
            commands: receiver,
            admission,
            #[cfg(unix)]
            direct,
        },
    )
}

impl ActorWake {
    pub(super) const fn none() -> Self {
        Self {
            ready: None,
            #[cfg(unix)]
            pipe: None,
            #[cfg(unix)]
            pending: None,
            #[cfg(not(unix))]
            channel: None,
        }
    }

    pub(super) fn for_actor(&self) -> Self {
        Self {
            ready: Some(Arc::new(AtomicBool::new(true))),
            ..self.clone()
        }
    }

    pub(super) fn notify(&self) {
        if let Some(ready) = &self.ready {
            ready.store(true, Ordering::Release);
        }
        #[cfg(not(unix))]
        if let Some(channel) = &self.channel {
            let _ = channel.try_send(());
        }
        #[cfg(unix)]
        if let Some(pipe) = &self.pipe
            && !hold_wake(pipe, self.pending.as_ref())
        {
            wake_pipe(pipe, self.pending.as_ref());
        }
    }
}

#[cfg(unix)]
pub(super) type WakeReceiver = Option<Result<std::os::fd::OwnedFd, rustix::io::Errno>>;
#[cfg(not(unix))]
pub(super) type WakeReceiver = ();

#[cfg(unix)]
pub(super) fn write_actor_wake(
    mut write: impl FnMut() -> Result<usize, rustix::io::Errno>,
) -> Result<(), rustix::io::Errno> {
    loop {
        match write() {
            Ok(_) | Err(rustix::io::Errno::AGAIN) => return Ok(()),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
pub(super) fn configured_actor_wake_pipe()
-> Result<(std::os::fd::OwnedFd, std::os::fd::OwnedFd), rustix::io::Errno> {
    let (read, write) = rustix::pipe::pipe()?;
    rustix::io::fcntl_setfd(&read, rustix::io::FdFlags::CLOEXEC)?;
    rustix::io::fcntl_setfd(&write, rustix::io::FdFlags::CLOEXEC)?;
    rustix::io::ioctl_fionbio(&read, true)?;
    rustix::io::ioctl_fionbio(&write, true)?;
    Ok((read, write))
}

pub(super) fn actor_wake() -> (ActorWake, WakeReceiver) {
    #[cfg(unix)]
    {
        match configured_actor_wake_pipe() {
            Ok((read, write)) => (
                ActorWake {
                    ready: None,
                    pipe: Some(Arc::new(write)),
                    pending: None,
                },
                Some(Ok(read)),
            ),
            Err(error) => (ActorWake::none(), Some(Err(error))),
        }
    }
    #[cfg(not(unix))]
    (ActorWake::none(), ())
}

pub(super) enum ActorReply<T> {
    Sync(Sender<T>),
    Async {
        reply: Option<Sender<Result<T, TerminalRequestError>>>,
        deadline: Instant,
        notify: Arc<dyn Fn() + Send + Sync>,
    },
}

impl<T> std::fmt::Debug for ActorReply<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ActorReply")
    }
}

impl<T> From<Sender<T>> for ActorReply<T> {
    fn from(reply: Sender<T>) -> Self {
        Self::Sync(reply)
    }
}

impl<T> ActorReply<T> {
    pub(super) fn send(mut self, value: T) -> Result<(), ()> {
        match &mut self {
            Self::Sync(reply) => reply.send(value).map_err(|_| ()),
            Self::Async {
                reply,
                deadline,
                notify,
            } => {
                let value = if Instant::now() <= *deadline {
                    Ok(value)
                } else {
                    Err(TerminalRequestError::TimedOut)
                };
                let result = reply.take().unwrap().try_send(value).map_err(|_| ());
                notify();
                result
            }
        }
    }
}

impl<T> Drop for ActorReply<T> {
    fn drop(&mut self) {
        if let Self::Async { reply, notify, .. } = self
            && let Some(reply) = reply.take()
        {
            let _ = reply.try_send(Err(TerminalRequestError::ActorStopped));
            notify();
        }
    }
}

impl CommandSender {
    pub(super) fn with_slot(&self, update: impl FnOnce(&mut ControlSlot) -> bool) {
        let mut slot = self.queues.slot.lock();
        if slot.in_flight == 0 {
            let wake = update(&mut slot) && !std::mem::replace(&mut slot.wake_queued, true);
            drop(slot);
            self.close_direct_input();
            if wake {
                let _ = self.try_send(Command::Wake);
            }
            return;
        }
        let earlier = std::mem::take(&mut slot.pending);
        update(&mut slot);
        let later = std::mem::replace(&mut slot.pending, earlier).take();
        let in_flight = slot.in_flight;
        slot.deferred
            .extend(later.into_iter().map(|command| (in_flight, command)));
        drop(slot);
        self.close_direct_input();
    }

    fn close_direct_input(&self) {
        #[cfg(unix)]
        if let Some(input) = &self.queues.input {
            input.direct.close();
        }
    }

    #[cfg(unix)]
    pub(super) fn write_direct_key(&self, input: &KeyInput) -> bool {
        let Some(queue) = &self.queues.input else {
            return false;
        };
        let Some(bytes) = direct_key_bytes(input) else {
            return false;
        };
        match queue.direct.write(bytes) {
            DirectWrite::Closed => false,
            DirectWrite::Written => true,
            DirectWrite::Queued => {
                let _ = self.try_send(Command::Wake);
                true
            }
        }
    }

    fn counts_in_flight(&self, command: &Command) -> bool {
        let counted = !matches!(command, Command::Wake)
            && (command.pty_input_bytes().is_none() || self.queues.input.is_none());
        if counted {
            self.queues.slot.lock().in_flight += 1;
        }
        counted
    }

    fn abandon_in_flight(&self) {
        let mut slot = self.queues.slot.lock();
        slot.in_flight = slot.in_flight.saturating_sub(1);
        let limit = slot.in_flight;
        let mut released = false;
        for (remaining, _) in &mut slot.deferred {
            *remaining = (*remaining).min(limit);
            released |= *remaining == 0;
        }
        drop(slot);
        if released {
            let _ = self.try_send(Command::Wake);
        }
    }

    pub(super) fn send(
        &self,
        command: Command,
    ) -> Result<(), crossbeam_channel::TrySendError<Command>> {
        let result = if command.pty_input_bytes().is_some()
            && let Some(input) = &self.queues.input
        {
            input.try_send(command)
        } else {
            let counted = self.counts_in_flight(&command);
            let result = match self.queues.control.try_send(command) {
                Err(crossbeam_channel::TrySendError::Full(command)) => {
                    #[cfg(unix)]
                    release_held_wakes();
                    self.queues
                        .control
                        .send(command)
                        .map_err(|error| crossbeam_channel::TrySendError::Disconnected(error.0))
                }
                result => result,
            };
            if counted && result.is_err() {
                self.abandon_in_flight();
            }
            result
        };
        if result.is_ok() {
            self.close_direct_input();
            self.queues.wake.notify();
        }
        result
    }

    fn send_timeout(
        &self,
        command: Command,
        timeout: Duration,
    ) -> Result<(), crossbeam_channel::SendTimeoutError<Command>> {
        let result = if command.pty_input_bytes().is_some()
            && let Some(input) = &self.queues.input
        {
            input.try_send(command).map_err(|error| match error {
                crossbeam_channel::TrySendError::Full(command) => {
                    crossbeam_channel::SendTimeoutError::Timeout(command)
                }
                crossbeam_channel::TrySendError::Disconnected(command) => {
                    crossbeam_channel::SendTimeoutError::Disconnected(command)
                }
            })
        } else {
            let counted = self.counts_in_flight(&command);
            let result = match self.queues.control.try_send(command) {
                Ok(()) => Ok(()),
                Err(crossbeam_channel::TrySendError::Full(command)) => {
                    #[cfg(unix)]
                    release_held_wakes();
                    self.queues.control.send_timeout(command, timeout)
                }
                Err(crossbeam_channel::TrySendError::Disconnected(command)) => {
                    Err(crossbeam_channel::SendTimeoutError::Disconnected(command))
                }
            };
            if counted && result.is_err() {
                self.abandon_in_flight();
            }
            result
        };
        if result.is_ok() {
            self.close_direct_input();
            self.queues.wake.notify();
        }
        result
    }

    pub(super) fn reply_token<T>(
        &self,
        timeout: Duration,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> (ActorReply<T>, TerminalRequest<T>) {
        let (reply, response) = crossbeam_channel::bounded(1);
        let deadline = Instant::now() + timeout;
        (
            ActorReply::Async {
                reply: Some(reply),
                deadline,
                notify,
            },
            TerminalRequest {
                response,
                deadline,
                queues: Arc::clone(&self.queues),
            },
        )
    }

    pub(super) fn request_token<T>(
        &self,
        notify: Arc<dyn Fn() + Send + Sync>,
        command: impl FnOnce(ActorReply<T>) -> Command,
    ) -> TerminalRequest<T> {
        let (reply, request) = self.reply_token(CAPTURE_TIMEOUT, notify);
        if let Err(crossbeam_channel::TrySendError::Full(command)) = self.try_send(command(reply)) {
            self.defer(command);
        }
        request
    }

    pub(super) fn defer(&self, command: Command) {
        let mut slot = self.queues.slot.lock();
        let in_flight = slot.in_flight;
        slot.deferred.push((in_flight, command));
        let wake = in_flight == 0 && !std::mem::replace(&mut slot.wake_queued, true);
        drop(slot);
        self.close_direct_input();
        if wake {
            let _ = self.try_send(Command::Wake);
        }
    }

    pub(super) fn request<T>(
        &self,
        command: impl FnOnce(Sender<T>) -> Command,
    ) -> Result<T, ActorRequestError> {
        let (reply, response) = crossbeam_channel::bounded(1);
        let command = command(reply);
        debug_assert!(
            !ROUND_TRIPS_FORBIDDEN.with(Cell::get),
            "{} waits on the pane actor while actor round trips are forbidden",
            command.name()
        );
        let started = Instant::now();
        self.send_timeout(command, CAPTURE_TIMEOUT)
            .map_err(|error| match error {
                crossbeam_channel::SendTimeoutError::Timeout(_) => ActorRequestError::TimedOut,
                crossbeam_channel::SendTimeoutError::Disconnected(_) => {
                    ActorRequestError::ActorStopped
                }
            })?;
        #[cfg(unix)]
        release_held_wakes();
        crossbeam_channel::select_biased! {
            recv(response) -> reply => reply.map_err(|_| ActorRequestError::ActorStopped),
            recv(self.queues.liveness) -> _ => {
                response.try_recv().map_err(|_| ActorRequestError::ActorStopped)
            }
            default(CAPTURE_TIMEOUT.saturating_sub(started.elapsed())) => {
                Err(ActorRequestError::TimedOut)
            }
        }
    }

    pub(super) fn try_send(
        &self,
        command: Command,
    ) -> Result<(), crossbeam_channel::TrySendError<Command>> {
        let result = if command.pty_input_bytes().is_some()
            && let Some(input) = &self.queues.input
        {
            input.try_send(command)
        } else {
            let counted = self.counts_in_flight(&command);
            let result = self.queues.control.try_send(command);
            if counted && result.is_err() {
                self.abandon_in_flight();
            }
            result
        };
        if result.is_ok() {
            self.close_direct_input();
            self.queues.wake.notify();
        }
        result
    }

    pub(super) fn len(&self) -> usize {
        self.queues
            .control
            .len()
            .saturating_add(self.pending_input().0)
    }

    pub(super) fn capacity(&self) -> Option<usize> {
        let control = self.queues.control.capacity()?;
        let input = self.queues.input.as_ref().map_or(0, InputSender::capacity);
        Some(control.saturating_add(input))
    }

    pub(super) fn pending_input(&self) -> (usize, usize) {
        self.queues
            .input
            .as_ref()
            .map_or((0, 0), InputSender::pending)
    }
}
