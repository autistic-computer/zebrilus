use super::*;

pub(super) enum FallbackFrame {
    FirstStreamed,
    Built(TerminalViewport),
    Metadata(TerminalViewport),
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SnapshotChange {
    Content,
    View,
    Overlay,
}

#[derive(Debug, Error)]
pub(super) enum WorkerError {
    #[error("thread error: {0}")]
    Thread(String),
    #[error("spawn error: {0}")]
    Spawn(String),
    #[error("PTY error: {0}")]
    Pty(String),
    #[error("terminal emulation error: {0}")]
    Ghostty(#[from] libghostty_vt::Error),
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("terminal search snapshot exceeds the {MAX_SEARCH_SNAPSHOT_BYTES}-byte limit")]
    SearchSnapshotTooLarge,
    #[error("terminal viewport metadata exceeds its 32-bit in-memory limits")]
    ViewportMetadataTooLarge,
    #[error(
        "terminal reliable-event backlog exceeded {MAX_PENDING_RELIABLE_EVENTS} messages or {MAX_PENDING_RELIABLE_EVENT_BYTES} bytes"
    )]
    EventBackpressure,
    #[error("terminal event consumer stopped")]
    EventConsumerStopped,
}

pub(super) fn warn_and_skip_view_ghostty<T>(
    context: &str,
    result: Result<T, WorkerError>,
) -> Result<Option<T>, WorkerError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(WorkerError::Ghostty(error)) => {
            log::warn!("skipping terminal {context} after emulation error: {error}");
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn normalize_view_action_result(
    result: Result<ViewActionResult, WorkerError>,
) -> Result<ViewActionResult, WorkerError> {
    match result {
        Err(error @ WorkerError::SearchSnapshotTooLarge) => {
            log::warn!("skipping terminal view action: {error}");
            Ok(ViewActionResult::Snapshot)
        }
        result => result,
    }
}

impl Publisher {
    pub(super) fn set_foreground_source(&self, source: Option<Box<ForegroundSource>>) {
        *self.state.foreground.write() = source;
        self.state.resolve_identity();
    }

    pub(super) fn set_completion(&self, completion: TerminalProcessExit) {
        self.state
            .completion
            .store(completion.encode(), Ordering::Release);
    }

    pub(super) fn set_facts(&self, facts: TerminalFacts) {
        let titled = {
            let mut latest = self.latest.write();
            let titled = latest.facts.program_title_writes != facts.program_title_writes;
            latest.facts = facts;
            titled
        };
        if titled {
            self.notify_latest();
        }
    }

    pub(super) fn set_progress_bar(&self, bar: ProgressBar) {
        let changed = {
            let mut latest = self.latest.write();
            let changed = latest.bar.state != bar.state;
            latest.bar = bar;
            changed
        };
        if changed {
            self.notify_latest();
        }
    }

    pub(super) fn frame_sink(&self) -> Option<&Arc<dyn TerminalFrameSink>> {
        self.state.sinks.frame.get()
    }

    fn output_sink(&self) -> Option<&Arc<dyn TerminalFrameSink>> {
        self.frame_sink().filter(|sink| sink.takes_output())
    }

    pub(super) fn takes_output(&self) -> bool {
        self.output_sink().is_some()
    }

    pub(super) fn output_room(&self) -> usize {
        self.output_sink()
            .map_or(usize::MAX, |sink| sink.output_room())
    }

    pub(super) fn output(&self, bytes: &Arc<[u8]>) {
        if let Some(sink) = self.output_sink() {
            sink.output(bytes);
        }
    }

    pub(super) fn set_last_command_status(&self, status: Option<i32>) {
        self.latest.write().last_command_status = status;
    }

    pub(super) fn push_shell_marks(&self, marks: Vec<ShellMark>) {
        if marks.is_empty() {
            return;
        }
        let mut latest = self.latest.write();
        for mark in marks {
            if latest.shell_marks.len() == MAX_PENDING_SHELL_MARKS {
                latest.shell_marks.pop_front();
                latest.shell_marks_dropped += 1;
            }
            latest.shell_marks.push_back(mark);
        }
    }

    pub(super) fn set_program_status(&self, status: ProgramStatus) {
        self.latest.write().program_status = Arc::new(status);
        self.notify_latest();
    }

    #[cfg(test)]
    pub(super) fn publish(&self, viewport: TerminalViewport) {
        let viewport = Arc::new(viewport);
        {
            let mut latest = self.latest.write();
            latest.size = (viewport.columns, viewport.rows);
            latest.fallback = Arc::clone(&viewport);
            latest.fallback_current = true;
            latest.by_view.clear();
            latest.epochs.clear();
            latest.copy_facts.clear();
            latest.sunk.clear();
        }
        self.notify_viewports(&viewport, 0);
    }

    pub(super) fn publish_frame(
        &self,
        fallback: FallbackFrame,
        viewports: Vec<ViewFrame>,
        copy_facts: HashMap<TerminalViewId, Arc<CopyModeFacts>>,
        sunk: Vec<TerminalViewId>,
        size: (u16, u16),
        notify: bool,
    ) -> bool {
        let mut by_view = HashMap::with_capacity(viewports.len());
        let mut epochs = HashMap::with_capacity(viewports.len());
        let mut first_streamed = None;
        for (view, viewport, epoch) in viewports {
            if let Some(epoch) = epoch {
                first_streamed.get_or_insert_with(|| Arc::clone(&viewport));
                epochs.insert(view, epoch);
            }
            by_view.insert(view, viewport);
        }
        let view_count = by_view.len();
        let (fallback, current) = match fallback {
            FallbackFrame::FirstStreamed => (
                first_streamed.expect("a streamed view published a frame"),
                true,
            ),
            FallbackFrame::Built(viewport) => (Arc::new(viewport), true),
            FallbackFrame::Metadata(viewport) => (Arc::new(viewport), false),
        };
        {
            let mut latest = self.latest.write();
            latest.fallback = Arc::clone(&fallback);
            latest.fallback_current = current;
            latest.by_view = by_view;
            latest.epochs = epochs;
            latest.copy_facts = copy_facts;
            latest.sunk = sunk;
            latest.size = size;
        }
        notify && self.notify_viewports(&fallback, view_count)
    }

    pub(super) fn refresh_fallback(
        &self,
        terminal: &Terminal<'_, '_>,
        dictionary: &mut ViewportDictionary,
        status: &SessionStatus,
        bound: Option<(u16, u16)>,
    ) -> Result<Option<(TerminalViewport, bool)>, WorkerError> {
        let previous = Arc::clone(&self.latest.read().fallback);
        let scrollbar = terminal.scrollbar()?;
        let scrollbar = ScrollbarState {
            total: u32::try_from(scrollbar.total)
                .map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
            offset: u32::try_from(scrollbar.offset)
                .map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
            len: u32::try_from(scrollbar.len).map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
        };
        let at_bottom = |state: ScrollbarState| {
            u64::from(state.offset) + u64::from(state.len) >= u64::from(state.total)
        };
        if (previous.columns, previous.rows)
            != bounded_extent((terminal.cols()?, terminal.rows()?), bound)
            || previous.mode != TerminalMode::Live
            || !previous.overlays.is_empty()
            || previous.search.is_some()
            || !at_bottom(previous.scrollbar)
            || !at_bottom(scrollbar)
        {
            return Ok(None);
        }
        let title = terminal.title().unwrap_or("zz");
        let working_directory = terminal.pwd().ok().and_then(reported_working_directory);
        let mut viewport = (*previous).clone();
        viewport.presentation =
            dictionary.shared_presentation(title, working_directory.as_deref(), None);
        viewport.status = status.clone();
        viewport.scrollbar = scrollbar;
        viewport.mouse_tracking = terminal.is_mouse_tracking()?;
        viewport.kitty_keyboard = terminal
            .kitty_keyboard_flags()
            .is_ok_and(|flags| !flags.is_empty());
        viewport.unseen_output = 0;
        let changed = viewport.presentation.title != previous.presentation.title
            || viewport.presentation.working_directory != previous.presentation.working_directory
            || viewport.status != previous.status
            || viewport.mouse_tracking != previous.mouse_tracking
            || viewport.kitty_keyboard != previous.kitty_keyboard;
        Ok(Some((viewport, changed)))
    }

    pub(super) fn latest_fallback(&self) -> Arc<TerminalViewport> {
        Arc::clone(&self.latest.read().fallback)
    }

    pub(super) fn notify_latest(&self) {
        let fallback = self.latest_fallback();
        self.notify_viewports(&fallback, 0);
    }

    pub(super) fn publish_search_string(&self, search: Option<&CopyModeSearch>) {
        let text = search.map_or("", |search| search.text.as_str());
        if self.latest.read().search_string != text {
            text.clone_into(&mut self.latest.write().search_string);
        }
    }

    fn notify_viewports(&self, viewport: &TerminalViewport, view_count: usize) -> bool {
        let notification_was_pending = self.state.notification_pending.swap(true, Ordering::AcqRel);
        let notified = notification_was_pending
            || match self.event_tx.try_send(TerminalEvent::ViewportReady {
                output_activity: false,
            }) {
                Ok(()) => {
                    self.state.notify_consumer();
                    true
                }
                Err(async_channel::TrySendError::Closed(_)) => false,
                Err(async_channel::TrySendError::Full(_)) => {
                    self.state
                        .notification_pending
                        .store(false, Ordering::Release);
                    log::error!("terminal viewport notification queue overflow");
                    false
                }
            };
        if log::log_enabled!(
            target: "zz_terminal::diagnostics::publisher",
            log::Level::Trace
        ) {
            log::trace!(
                target: "zz_terminal::diagnostics::publisher",
                "publish generation={} view_generation={} columns={} rows={} cells={} overlays={} status={:?} views={view_count} notification_was_pending={notification_was_pending} event_queue_len={} event_queue_capacity={:?} pending_reliable={} pending_reliable_bytes={}",
                viewport.generation,
                viewport.view_generation,
                viewport.columns,
                viewport.rows,
                viewport.cells.len(),
                viewport.overlays.len(),
                viewport.status,
                self.event_tx.len(),
                self.event_tx.capacity(),
                self.state.pending_reliable.load(Ordering::Acquire),
                self.state.pending_reliable_bytes.load(Ordering::Acquire),
            );
        }
        notified
    }

    fn set_status(&self, status: &SessionStatus) {
        let update = |viewport: &TerminalViewport| {
            let mut viewport = viewport.clone();
            viewport.generation = viewport.generation.saturating_add(1);
            viewport.view_generation = viewport.view_generation.saturating_add(1);
            viewport.status = status.clone();
            Arc::new(viewport)
        };
        let (fallback, view_count) = {
            let mut latest = self.latest.write();
            latest.fallback = update(&latest.fallback);
            let by_view = std::mem::take(&mut latest.by_view)
                .into_iter()
                .map(|(view, viewport)| (view, update(&viewport)))
                .collect::<HashMap<_, _>>();
            let view_count = by_view.len();
            latest.by_view = by_view;
            latest.sunk.clear();
            (Arc::clone(&latest.fallback), view_count)
        };
        self.notify_viewports(&fallback, view_count);
    }

    pub(super) fn mark_output_activity(&self) {
        self.state
            .output_activity_pending
            .store(true, Ordering::Release);
        self.state.mark_output();
    }

    pub(super) fn fail(&self, error: &WorkerError) {
        self.set_status(&SessionStatus::failed(error.to_string()));
        self.state.resolve_identity();
    }

    pub(super) fn send_reliable(&self, event: TerminalEvent) -> Result<(), WorkerError> {
        let bytes = reliable_event_bytes(&event);
        self.state
            .pending_reliable
            .try_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                (pending < MAX_PENDING_RELIABLE_EVENTS).then_some(pending + 1)
            })
            .map_err(|_| WorkerError::EventBackpressure)?;
        if self
            .state
            .pending_reliable_bytes
            .try_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                (bytes <= MAX_PENDING_RELIABLE_EVENT_BYTES.saturating_sub(pending))
                    .then_some(pending + bytes)
            })
            .is_err()
        {
            self.state.pending_reliable.fetch_sub(1, Ordering::AcqRel);
            return Err(WorkerError::EventBackpressure);
        }
        match self.event_tx.try_send(event) {
            Ok(()) => {
                self.state.notify_consumer();
                Ok(())
            }
            Err(async_channel::TrySendError::Full(_)) => {
                self.release_reliable(bytes);
                Err(WorkerError::EventBackpressure)
            }
            Err(async_channel::TrySendError::Closed(_)) => {
                self.release_reliable(bytes);
                Err(WorkerError::EventConsumerStopped)
            }
        }
    }

    fn release_reliable(&self, bytes: usize) {
        let previous = self.state.pending_reliable.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "reliable terminal event accounting underflow");
        let previous_bytes = self
            .state
            .pending_reliable_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
        debug_assert!(
            previous_bytes >= bytes,
            "reliable terminal event byte accounting underflow"
        );
    }

    pub(super) fn copy_ready(
        &self,
        view: TerminalViewId,
        copy: Box<TerminalCopyReady>,
    ) -> Result<(), WorkerError> {
        self.send_user_action(TerminalEvent::CopyReady { view, copy }, "copy result")
    }

    pub(super) fn open_uri(&self, view: TerminalViewId, uri: String) -> Result<(), WorkerError> {
        self.send_user_action(
            TerminalEvent::OpenUri(Box::new(TerminalOpenUri { view, uri })),
            "open-URI request",
        )
    }

    pub(super) fn view_closed(&self, view: TerminalViewId) -> Result<(), WorkerError> {
        self.send_reliable(TerminalEvent::ViewClosed(view))
    }

    fn clipboard_set(&self, target: ClipboardTarget, text: String) -> Result<(), WorkerError> {
        self.send_user_action(
            TerminalEvent::ClipboardSet { target, text },
            "clipboard write",
        )
    }

    pub(super) fn rename_window(&self, name: String) -> Result<(), WorkerError> {
        self.send_user_action(TerminalEvent::RenameWindow(name), "window rename")
    }

    fn bell(&self) {
        if let Err(error) = self.send_reliable(TerminalEvent::Bell)
            && matches!(error, WorkerError::EventBackpressure)
        {
            log::warn!("discarding terminal bell: reliable event backlog is full");
        }
    }

    pub(super) fn placeholder_bound(&self, token: u64, number: u32) -> Result<(), WorkerError> {
        self.send_reliable(TerminalEvent::PlaceholderBound { token, number })
    }

    pub(super) fn pending_paste_expired(&self, token: u64) -> Result<(), WorkerError> {
        self.send_reliable(TerminalEvent::PendingPasteExpired { token })
    }

    fn send_user_action(&self, event: TerminalEvent, description: &str) -> Result<(), WorkerError> {
        match self.send_reliable(event) {
            Err(WorkerError::EventBackpressure) => {
                log::warn!("discarding terminal {description}: reliable event backlog is full");
                Ok(())
            }
            result => result,
        }
    }
}

pub(super) fn reliable_event_bytes(event: &TerminalEvent) -> usize {
    let payload = match event {
        TerminalEvent::CopyReady { copy, .. } => copy
            .text
            .len()
            .saturating_add(copy.pipe.as_ref().map_or(0, String::len))
            .saturating_add(copy.buffer.as_ref().map_or(0, |buffer| match buffer {
                PasteBufferAction::Create { prefix } => prefix.as_ref().map_or(0, String::len),
                PasteBufferAction::Append => 0,
            })),
        TerminalEvent::OpenUri(open) => open.uri.len(),
        TerminalEvent::ClipboardSet { text, .. } => text.len(),
        TerminalEvent::RenameWindow(name) => name.len(),
        TerminalEvent::ViewportReady { .. }
        | TerminalEvent::ViewClosed(_)
        | TerminalEvent::Bell
        | TerminalEvent::PlaceholderBound { .. }
        | TerminalEvent::PendingPasteExpired { .. } => 0,
    };
    std::mem::size_of::<TerminalEvent>().saturating_add(payload)
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the worker thread must own its channel and publisher"
)]
pub(super) fn terminal_worker(
    control_rx: Receiver<Command>,
    input_rx: InputReceiver,
    slot: Arc<Mutex<ControlSlot>>,
    publisher: Publisher,
    max_scrollback: usize,
    appearance: Arc<TerminalAppearance>,
    spawn: TerminalSpawn,
    wake: ActorWake,
    wake_rx: WakeReceiver,
) {
    if let Err(error) = run_terminal(
        control_rx,
        input_rx,
        slot,
        publisher.clone(),
        max_scrollback,
        &appearance,
        &spawn,
        &wake,
        wake_rx,
    ) {
        report_worker_error(&publisher, &error);
    }
    publisher.set_foreground_source(None);
}

pub(super) fn report_worker_error(publisher: &Publisher, error: &WorkerError) {
    log::error!("terminal worker stopped: {error}");
    if matches!(error, WorkerError::Spawn(_)) {
        publisher.set_completion(TerminalProcessExit {
            code: 1,
            signal: None,
        });
        publisher.set_status(&SessionStatus::exited(1, None));
    } else {
        publisher.fail(error);
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the worker thread owns its command-output source and channels"
)]
pub(super) fn output_view_worker(
    command_rx: Receiver<Command>,
    slot: Arc<Mutex<ControlSlot>>,
    publisher: Publisher,
    title: String,
    text: String,
    appearance: Arc<TerminalAppearance>,
    max_scrollback: usize,
    frozen: bool,
    geometry: Geometry,
) {
    let result = (|| {
        let mut actor = new_output_view(
            command_rx,
            slot,
            publisher.clone(),
            &title,
            &text,
            &appearance,
            max_scrollback,
            frozen,
            geometry,
            &ActorWake::none(),
        )?;
        loop {
            actor.on_deadline()?;
            let wake = actor.wait_for_wake()?;
            if !actor.on_wake(wake)? {
                return Ok::<(), WorkerError>(());
            }
        }
    })();
    if let Err(error) = result {
        log::error!("command output view stopped: {error}");
        publisher.fail(&error);
    }
}

pub(super) fn apply_terminal_appearance(
    terminal: &mut Terminal<'_, '_>,
    appearance: &TerminalAppearance,
) -> Result<(), WorkerError> {
    let follows_default = [
        terminal.fg_color()? == terminal.default_fg_color()?,
        terminal.bg_color()? == terminal.default_bg_color()?,
    ];
    terminal.set_default_fg_color(Some(ghostty_color(appearance.foreground)))?;
    terminal.set_default_bg_color(Some(ghostty_color(appearance.background)))?;
    if follows_default[0] && terminal.fg_color()? != terminal.default_fg_color()? {
        terminal.vt_write(b"\x1b]110\x1b\\");
    }
    if follows_default[1] && terminal.bg_color()? != terminal.default_bg_color()? {
        terminal.vt_write(b"\x1b]111\x1b\\");
    }
    terminal.set_default_cursor_color(Some(ghostty_color(appearance.cursor_color)))?;
    terminal.set_default_cursor_style(Some(ghostty_cursor_style(appearance.cursor_style)))?;
    terminal.set_default_cursor_blink(Some(!matches!(
        appearance.cursor_blink_policy,
        CursorBlinkPolicy::Off
    )))?;
    terminal.set_default_color_palette(Some(Palette(
        (*appearance.palette.as_array()).map(ghostty_color),
    )))?;
    log::debug!(
        target: "zz_terminal::diagnostics::appearance",
        "applied appearance hash={} foreground={:?} background={:?} cursor={:?} cursor_style={:?} palette_entries=256",
        appearance.stable_hash(),
        appearance.foreground,
        appearance.background,
        appearance.cursor_color,
        appearance.cursor_style,
    );
    Ok(())
}

pub(super) const fn ghostty_color(value: Color) -> RgbColor {
    RgbColor {
        r: value.r,
        g: value.g,
        b: value.b,
    }
}

const fn ghostty_cursor_style(value: CursorStyle) -> GhosttyCursorStyle {
    match value {
        CursorStyle::Bar => GhosttyCursorStyle::Bar,
        CursorStyle::Block => GhosttyCursorStyle::Block,
        CursorStyle::Underline => GhosttyCursorStyle::Underline,
        CursorStyle::BlockHollow => GhosttyCursorStyle::BlockHollow,
    }
}

pub(super) const fn ghostty_color_scheme(value: TerminalColorScheme) -> ColorScheme {
    match value {
        TerminalColorScheme::Light => ColorScheme::Light,
        TerminalColorScheme::Dark => ColorScheme::Dark,
    }
}

pub(super) fn new_terminal<'alloc: 'cb, 'cb>(
    cols: u16,
    rows: u16,
    history_limit: usize,
) -> Result<Terminal<'alloc, 'cb>, libghostty_vt::Error> {
    let history_limit = history_limit.min(MAX_HISTORY_LIMIT);
    let mut terminal = Terminal::new(cols, rows)?;
    terminal
        .set_scrollback_max_bytes(Some(scrollback_backstop_bytes(history_limit, cols)))?
        .set_scrollback_max_lines(Some(history_limit))?;
    Ok(terminal)
}

const SCROLLBACK_BACKSTOP_BYTES_PER_CELL: usize = 128;
pub(super) const SCROLLBACK_BACKSTOP_FLOOR: usize = 64 * 1024 * 1024;
pub(super) const SCROLLBACK_BACKSTOP_CAP: usize = 1024 * 1024 * 1024;

pub(super) fn scrollback_backstop_bytes(history_limit: usize, columns: u16) -> usize {
    history_limit
        .saturating_mul(usize::from(columns.max(1)))
        .saturating_mul(SCROLLBACK_BACKSTOP_BYTES_PER_CELL)
        .clamp(SCROLLBACK_BACKSTOP_FLOOR, SCROLLBACK_BACKSTOP_CAP)
}

pub(super) fn clipboard_write_request<'a>(
    location: ClipboardLocation,
    contents: impl Iterator<Item = ClipboardContent<'a>>,
) -> Result<(ClipboardTarget, String), ClipboardWriteError> {
    let mut selected = None;
    for content in contents {
        if content.mime == CLIPBOARD_TEXT_MIME {
            selected = Some(content);
            break;
        }
        selected.get_or_insert(content);
    }
    let content = selected.ok_or(ClipboardWriteError::Unsupported)?;
    if content.data.len() > MAX_CLIPBOARD_WRITE_BYTES {
        return Err(ClipboardWriteError::Denied);
    }
    let target = match location {
        ClipboardLocation::Standard => ClipboardTarget::Clipboard,
        ClipboardLocation::Selection | ClipboardLocation::Primary => ClipboardTarget::Primary,
    };
    Ok((target, String::from_utf8_lossy(content.data).into_owned()))
}

pub(super) fn register_clipboard_write(
    terminal: &mut Terminal<'_, '_>,
    publisher: Publisher,
) -> Result<(), WorkerError> {
    terminal.on_clipboard_write(move |_, write| {
        let result = clipboard_write_request(write.location(), write.contents()).and_then(
            |(target, text)| {
                publisher
                    .clipboard_set(target, text)
                    .map_err(|_| ClipboardWriteError::Busy)
            },
        );
        write.reply(result, false);
    })?;
    Ok(())
}

pub(super) fn register_bell(
    terminal: &mut Terminal<'_, '_>,
    publisher: Publisher,
) -> Result<(), WorkerError> {
    terminal.on_bell(move |_| publisher.bell())?;
    Ok(())
}

pub(super) fn register_device_attributes(
    terminal: &mut Terminal<'_, '_>,
) -> Result<(), WorkerError> {
    terminal.on_device_attributes(|_| {
        Some(DeviceAttributes {
            primary: PrimaryDeviceAttributes::new(
                ConformanceLevel::VT220,
                &[DeviceAttributeFeature::ANSI_COLOR],
            ),
            secondary: SecondaryDeviceAttributes {
                device_type: DeviceType::VT220,
                firmware_version: 1,
                rom_cartridge: 0,
            },
            tertiary: TertiaryDeviceAttributes { unit_id: 0 },
        })
    })?;
    Ok(())
}

pub(super) fn new_output_view(
    command_rx: Receiver<Command>,
    slot: Arc<Mutex<ControlSlot>>,
    publisher: Publisher,
    title: &str,
    text: &str,
    appearance: &TerminalAppearance,
    max_scrollback: usize,
    frozen: bool,
    geometry: Geometry,
    wake: &ActorWake,
) -> Result<surface_actor::SurfaceActor<'static, 'static>, WorkerError> {
    install_kitty_png_decoder();
    let mut terminal = new_terminal(geometry.columns, geometry.rows, max_scrollback)?;
    let reported_color_scheme = Rc::new(Cell::new(ghostty_color_scheme(appearance.color_scheme)));
    let color_scheme_source = Rc::clone(&reported_color_scheme);
    terminal.on_color_scheme(move |_| Some(color_scheme_source.get()))?;
    apply_terminal_appearance(&mut terminal, appearance)?;
    register_bell(&mut terminal, publisher.clone())?;
    if frozen {
        write_output_view_content(&mut terminal, title, text);
    } else {
        terminal.vt_write(text.as_bytes());
    }
    let mut actor = surface_actor::SurfaceActor::new(
        command_rx,
        slot,
        publisher,
        SurfaceTerminal {
            terminal,
            geometry,
            frames: Frames::new(appearance)?,
            active_views: ActiveTerminalViews::new(),
            inactive_views: InactiveTerminalViews::new(),
            word_separators: WordSeparators::default(),
            wrap_search: true,
            mode_keys_vi: false,
            reported_color_scheme,
            max_scrollback,
            status: SessionStatus::Running,
            pending_commands: Vec::new(),
            captures: VecDeque::new(),
            pending_copy_source: None,
            pane_search: None,
            search: Some(SearchWorker::spawn(wake.clone())),
        },
        EngineFilter::default(),
        frozen,
    )?;
    if !frozen {
        actor.publish_started()?;
    }
    Ok(actor)
}

fn write_output_view_content(terminal: &mut Terminal<'_, '_>, title: &str, text: &str) {
    let title = title
        .chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect::<String>();
    terminal.vt_write(format!("\x1b]2;{title}\x07").as_bytes());
    for (index, line) in text.split('\n').enumerate() {
        if index != 0 {
            terminal.vt_write(b"\r\n");
        }
        terminal.vt_write(line.strip_suffix('\r').unwrap_or(line).as_bytes());
    }
}

pub(super) fn output_view_state(
    terminal: &mut Terminal<'_, '_>,
) -> Result<TerminalViewState, WorkerError> {
    let mut view = TerminalViewState::for_screen(terminal.active_screen()?);
    let active = view.active_mut();
    enter_copy_mode(
        terminal,
        &mut active.selection,
        &mut active.copy_mode,
        false,
        false,
        None,
        false,
    )?;
    let mode = active
        .copy_mode
        .as_mut()
        .expect("entering output view creates a frozen revision");
    mode.kind = FrozenModeKind::View;
    mode.line_numbers = 0;
    mode.cursor = PointCoordinate { x: 0, y: 0 };
    mode.viewport_offset = 0;
    Ok(view)
}

pub(super) fn refresh_output_view(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<(), WorkerError> {
    let Some(mode) = view.active_mut().copy_mode.as_mut() else {
        return Ok(());
    };
    let old_total = mode.revision.total_rows();
    let old_maximum = mode.revision.maximum_offset();
    let old_cursor = mode.cursor;
    let old_offset = mode.viewport_offset;
    let revision = ModeRevision::capture(terminal)?;
    let scale = |value: u32, old_limit: u32, new_limit: u32| {
        if old_limit == 0 {
            0
        } else {
            u32::try_from(
                u64::from(value)
                    .saturating_mul(u64::from(new_limit))
                    .saturating_div(u64::from(old_limit)),
            )
            .unwrap_or(new_limit)
            .min(new_limit)
        }
    };
    mode.cursor = revision.clamp_point(PointCoordinate {
        x: old_cursor.x.min(revision.columns.saturating_sub(1)),
        y: scale(
            old_cursor.y,
            old_total.saturating_sub(1),
            revision.total_rows().saturating_sub(1),
        ),
    });
    mode.viewport_offset = scale(old_offset, old_maximum, revision.maximum_offset());
    mode.revision = revision;
    mode.selection = None;
    mode.mark = None;
    mode.last_jump = None;
    mode.selecting = false;
    view.search_snapshot = None;
    if view.search.is_some() {
        complete_view_search(terminal, view)?;
    }
    Ok(())
}

pub(super) fn resize_copy_modes(
    views: &mut ActiveTerminalViews,
    columns: u16,
    rows: u16,
    worker: &mut SearchWorker,
) -> Result<(), WorkerError> {
    for (view_id, view) in views {
        let request_id = worker.cancel(*view_id);
        for state in [&mut view.primary, &mut view.alternate] {
            let Some(mode) = state.copy_mode.as_mut() else {
                continue;
            };
            let (revision, cursor) = mode.revision.resized(columns, rows, mode.cursor)?;
            mode.revision = revision;
            mode.cursor = mode.revision.clamp_point(cursor);
            mode.viewport_offset = mode.cursor.y.min(mode.revision.maximum_offset());
            mode.selection = None;
            mode.selecting = false;
            mode.recentre = None;
            rebuild_copy_search(
                mode,
                &mut state.search,
                &mut state.search_snapshot,
                request_id,
            );
        }
    }
    Ok(())
}

fn rebuild_copy_search(
    mode: &mut CopyModeState,
    search: &mut SearchSlot,
    search_snapshot: &mut Option<Arc<HistorySearchSnapshot>>,
    request_id: u64,
) {
    if let Some(origin) = mode.incremental_origin.as_mut() {
        origin.row = mode.cursor.y;
        origin.viewport_offset = mode.viewport_offset;
    }
    *search_snapshot = None;
    if mode.search_marks
        && let Some(previous) = search.take()
    {
        let mut rebuilt = mode
            .revision
            .search
            .search(&previous.query, request_id, || false)
            .expect("copy search rebuild");
        rebuilt.current = rebuilt
            .matches
            .iter()
            .position(|found| found.contains(mode.cursor, mode.revision.columns));
        mode.search_count = Some((
            u32::try_from(rebuilt.matches.len()).unwrap_or(u32::MAX),
            false,
        ));
        *search_snapshot = Some(Arc::clone(&mode.revision.search));
        *search = Some(Box::new(rebuilt));
    } else {
        mode.search_marks = false;
        mode.search_count = None;
        *search = None;
    }
}

pub(super) fn settle_copy_search(
    view_id: TerminalViewId,
    worker: &mut SearchWorker,
    copy_mode: &mut CopyModeSlot,
    search: &mut SearchSlot,
    search_origin: &mut Option<PointCoordinate>,
    search_snapshot: &mut Option<Arc<HistorySearchSnapshot>>,
    revision_before: Option<u64>,
) {
    let Some(before) = revision_before else {
        return;
    };
    match copy_mode.as_deref_mut() {
        None => drop_view_search(view_id, worker, search, search_origin, search_snapshot),
        Some(mode) if mode.revision.id != before => {
            let request_id = worker.cancel(view_id);
            rebuild_copy_search(mode, search, search_snapshot, request_id);
        }
        Some(_) => {}
    }
}

pub(super) fn refresh_frozen_view_appearance(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<bool, WorkerError> {
    let Some(mode) = view.active_mut().copy_mode.as_mut() else {
        return Ok(false);
    };
    if mode.revision.matches_terminal_appearance(terminal)? {
        return Ok(false);
    }

    let revision = mode.revision.with_appearance(terminal)?;
    mode.cursor = revision.clamp_point(mode.cursor);
    mode.viewport_offset = mode.viewport_offset.min(revision.maximum_offset());
    if let Some(selection) = mode.selection.as_mut() {
        selection.anchor = revision.clamp_point(selection.anchor);
        selection.focus = revision.clamp_point(selection.focus);
    }
    if let Some(mark) = mode.mark.as_mut() {
        *mark = revision.clamp_point(*mark);
    }
    mode.revision = revision;
    view.search_snapshot = None;
    if view.search.is_some() {
        complete_view_search(terminal, view)?;
    }
    Ok(true)
}

pub(super) fn terminal_command(spawn: &TerminalSpawn) -> CommandBuilder {
    match spawn.command.as_deref() {
        Some(argv) if argv.len() >= 2 => {
            let mut command = CommandBuilder::new(&argv[0]);
            command.args(&argv[1..]);
            command
        }
        #[cfg(windows)]
        Some([shell_command]) => {
            let mut command = CommandBuilder::new("cmd.exe");
            let shell = command.get_shell();
            command.args([
                "/D",
                "/V:ON",
                "/C",
                shell.as_str(),
                "/D",
                "/S",
                "/C",
                "!ZZ_PANE_COMMAND!",
            ]);
            command.env(
                "ZZ_PANE_COMMAND",
                format!("\"set ZZ_PANE_COMMAND=& {shell_command}\""),
            );
            command
        }
        #[cfg(not(windows))]
        Some([shell_command]) => {
            let mut command = CommandBuilder::new(spawn.shell.as_deref().unwrap_or("/bin/sh"));
            command.args(["-c", shell_command]);
            command
        }
        None if spawn.non_login_shell => {
            CommandBuilder::new(spawn.shell.as_deref().unwrap_or("/bin/sh"))
        }
        _ => crate::shell_integration::default_shell_command(spawn.shell.as_deref()),
    }
}

fn run_terminal(
    control_rx: Receiver<Command>,
    input_rx: InputReceiver,
    slot: Arc<Mutex<ControlSlot>>,
    publisher: Publisher,
    max_scrollback: usize,
    appearance: &TerminalAppearance,
    spawn: &TerminalSpawn,
    wake: &ActorWake,
    wake_rx: WakeReceiver,
) -> Result<(), WorkerError> {
    let actor = PaneActor::spawn(
        control_rx,
        input_rx,
        slot,
        publisher,
        max_scrollback,
        appearance,
        spawn,
        wake,
        wake_rx,
        false,
        #[cfg(target_os = "linux")]
        pty_gather_enabled()
            .then(|| PtyGather::start("zz-pty-gather".to_owned()))
            .transpose()?,
    )?;
    let mut actor = shard::Actor::Live(Box::new(actor));
    loop {
        let Some(next) = actor.on_deadline()? else {
            return Ok(());
        };
        actor = next;
        actor.end_turn();
        let wake = actor.wait_for_wake()?;
        actor.begin_turn();
        let Some(next) = actor.on_wake(wake)? else {
            return Ok(());
        };
        actor = next;
    }
}

pub(super) fn reply_before_dead_notice(
    terminal: &mut Terminal<'_, '_>,
    class_hints: &ClassHints,
    active_views: &ActiveTerminalViews,
    word_separators: &WordSeparators,
    slot: &Mutex<ControlSlot>,
    command: Command,
) -> Option<Command> {
    match command {
        Command::Capture(request) => {
            let CaptureRequest { options, reply } = *request;
            let mut copy_modes = active_views
                .values()
                .filter_map(|view| view.copy_mode.as_deref());
            let mode = match (copy_modes.next(), copy_modes.next()) {
                (Some(mode), None) => Some(mode),
                _ => None,
            };
            let _ = reply.send(capture_terminal(terminal, mode, options));
        }
        Command::History(request) => {
            let HistoryCommand {
                start,
                count,
                reply,
            } = *request;
            let _ = reply.send(capture_history(terminal, start, count, class_hints, None));
        }
        Command::CaptureCopySource { reply } => {
            let _ = reply.send(
                capture_copy_source(terminal).map_err(|_| TerminalCaptureError::ActorStopped),
            );
        }
        Command::SemanticCapture(request) => {
            let _ = request.reply.send(capture_last_command(terminal));
        }
        Command::PointerContext(request) => {
            let PointerContextRequest {
                view,
                column,
                row,
                reply,
            } = *request;
            let mode = active_views
                .get(&view)
                .and_then(|view| view.copy_mode.as_deref());
            let _ = reply.send(
                pointer_context(terminal, mode, column, row, word_separators).unwrap_or_default(),
            );
        }
        command => return Some(command),
    }
    complete_dead_notice_command(slot);
    None
}

pub(super) fn complete_dead_notice_command(slot: &Mutex<ControlSlot>) {
    let mut slot = slot.lock();
    slot.in_flight = slot.in_flight.saturating_sub(1);
    for (remaining, _) in &mut slot.deferred {
        *remaining = remaining.saturating_sub(1);
    }
}

pub(super) fn write_dead_notice(
    terminal: &mut Terminal<'_, '_>,
    text: &str,
) -> Result<(), WorkerError> {
    if text.is_empty() {
        terminal.vt_write(b"\x1b[?25l");
        return Ok(());
    }
    let columns = usize::from(terminal.cols()?);
    let rows = terminal.rows()?;
    let mut drawn = String::with_capacity(text.len());
    let mut width = 0_usize;
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\x1b' {
            drawn.push(character);
            for control in characters.by_ref() {
                drawn.push(control);
                if control == 'm' {
                    break;
                }
            }
            continue;
        }
        let advance = usize::from(libghostty_vt::unicode::codepoint_width(character));
        if width + advance > columns {
            break;
        }
        width += advance;
        drawn.push(character);
    }
    terminal.vt_write(format!("\x1b[r\x1b[{rows};1H\n").as_bytes());
    terminal.vt_write(drawn.as_bytes());
    terminal.vt_write(b"\x1b[0m\x1b[?25l");
    Ok(())
}

#[cfg(unix)]
pub(super) fn signal_number(description: &str) -> Option<u8> {
    if let Some(number) = description
        .strip_prefix("Signal ")
        .and_then(|number| number.parse().ok())
        .or_else(|| {
            description
                .rsplit_once(':')
                .and_then(|(_, number)| number.trim().parse().ok())
        })
    {
        return Some(number);
    }
    match description {
        "Hangup" => Some(1),
        "Interrupt" => Some(2),
        "Quit" => Some(3),
        "Illegal instruction" => Some(4),
        "Trace/breakpoint trap" | "Trace trap" => Some(5),
        "Aborted" | "Abort trap" => Some(6),
        "Bus error" => Some(7),
        "Floating point exception" => Some(8),
        "Killed" => Some(9),
        "Segmentation fault" => Some(11),
        "Broken pipe" => Some(13),
        "Alarm clock" => Some(14),
        "Terminated" => Some(15),
        _ => None,
    }
}

#[cfg(not(unix))]
pub(super) fn signal_number(_description: &str) -> Option<u8> {
    None
}

#[cfg(unix)]
pub(super) fn signal_terminal_process_groups(
    master: &unix_pty::UnixMaster,
    shell_process_id: Option<u32>,
    signal: rustix::process::Signal,
) {
    let shell = shell_process_id
        .filter(|group| *group != 0)
        .and_then(|group| i32::try_from(group).ok())
        .and_then(rustix::process::Pid::from_raw);
    let foreground = master
        .process_group_leader()
        .and_then(rustix::process::Pid::from_raw)
        .filter(|group| Some(*group) != shell);
    if let Some(group) = foreground {
        let _ = rustix::process::kill_process_group(group, signal);
    }
    if let Some(shell) = shell
        && rustix::process::kill_process_group(shell, signal) == Err(rustix::io::Errno::SRCH)
    {
        let _ = rustix::process::kill_process(shell, signal);
    }
}
