use super::*;

impl std::fmt::Debug for TerminalSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TerminalSession")
            .finish_non_exhaustive()
    }
}

impl TerminalSession {
    #[must_use]
    pub fn spawn(
        max_scrollback: usize,
        appearance: Arc<TerminalAppearance>,
        spawn: TerminalSpawn,
    ) -> Self {
        Self::spawn_with_shard(max_scrollback, appearance, spawn, shard::choose())
    }

    pub(super) fn spawn_with_shard(
        max_scrollback: usize,
        appearance: Arc<TerminalAppearance>,
        spawn: TerminalSpawn,
        shard: Result<Option<shard::ShardHandle>, WorkerError>,
    ) -> Self {
        let max_scrollback = max_scrollback.min(MAX_HISTORY_LIMIT);
        let (command_tx, command_rx) = command_channel();
        let (input_tx, input_rx) = input_channel();
        let (wake, wake_rx) = match &shard {
            Ok(Some(shard)) => {
                #[cfg(unix)]
                let wake_rx = None;
                #[cfg(not(unix))]
                let wake_rx = ();
                (shard.wake.for_actor(), wake_rx)
            }
            _ => actor_wake(),
        };
        let (alive, liveness) = crossbeam_channel::bounded(0);
        let slot = Arc::new(Mutex::new(ControlSlot::default()));
        let commands = CommandSender {
            queues: Arc::new(CommandQueues {
                control: command_tx,
                input: Some(input_tx),
                liveness,
                slot: Arc::clone(&slot),
                wake: wake.clone(),
            }),
        };
        let event_state = Arc::new(EventQueueState::new());
        let (event_tx, events) = terminal_event_channel(&event_state);
        let latest = Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank_with_appearance(
                spawn
                    .initial_size
                    .map_or(INITIAL_COLUMNS, |size| size.columns),
                spawn.initial_size.map_or(INITIAL_ROWS, |size| size.rows),
                SessionStatus::Starting,
                &appearance,
            ),
        )));
        let publisher = Publisher {
            event_tx,
            latest: Arc::clone(&latest),
            state: event_state,
        };

        let worker_publisher = publisher.clone();
        let appearance_hash = appearance.stable_hash();
        let initial_separators = spawn.word_separators.clone().unwrap_or_default();
        match shard {
            Ok(Some(shard)) => {
                if let Err(error) = shard.launch(shard::PaneLaunch {
                    control_rx: command_rx,
                    slot,
                    publisher: worker_publisher,
                    max_scrollback,
                    appearance,
                    kind: shard::LaunchKind::Pty {
                        input_rx,
                        spawn: Box::new(spawn),
                    },
                    alive,
                    wake,
                }) {
                    publisher.fail(&error);
                }
            }
            Ok(None) => {
                if let Err(error) =
                    thread::Builder::new()
                        .name("zz-terminal".into())
                        .spawn(move || {
                            terminal_worker(
                                command_rx,
                                input_rx,
                                slot,
                                worker_publisher,
                                max_scrollback,
                                appearance,
                                spawn,
                                wake,
                                wake_rx,
                            );
                            drop(alive);
                        })
                {
                    publisher.fail(&WorkerError::Thread(error.to_string()));
                }
            }
            Err(error) => publisher.fail(&error),
        }

        Self {
            commands,
            events,
            latest,
            max_scrollback,
            word_separators: RwLock::new(initial_separators),
            applied_appearance: AtomicU64::new(appearance_hash),
            terminating: AtomicBool::new(false),
            preview_pending: AtomicBool::new(false),
            retired: AtomicBool::new(false),
        }
    }

    #[must_use]
    pub fn spawn_output_view(title: String, text: String) -> Self {
        Self::spawn_output_view_with_appearance(
            title,
            text,
            Arc::new(TerminalAppearance::default()),
        )
    }

    #[must_use]
    pub fn spawn_output_view_with_appearance(
        title: String,
        text: String,
        appearance: Arc<TerminalAppearance>,
    ) -> Self {
        Self::spawn_surface_with_appearance(
            title,
            text,
            appearance,
            MAX_OUTPUT_VIEW_SCROLLBACK,
            true,
            None,
        )
    }

    #[must_use]
    pub fn spawn_startup_output_view_with_appearance(
        title: String,
        text: String,
        appearance: Arc<TerminalAppearance>,
    ) -> Self {
        Self::spawn_surface_with_appearance(
            title,
            text,
            appearance,
            MAX_STARTUP_OUTPUT_VIEW_SCROLLBACK,
            true,
            None,
        )
    }

    #[must_use]
    pub fn spawn_empty_with_appearance(
        max_scrollback: usize,
        appearance: Arc<TerminalAppearance>,
    ) -> Self {
        Self::spawn_surface_with_appearance(
            String::new(),
            String::new(),
            appearance,
            max_scrollback.min(MAX_HISTORY_LIMIT),
            false,
            None,
        )
    }

    #[must_use]
    pub fn spawn_empty_pane(
        max_scrollback: usize,
        appearance: Arc<TerminalAppearance>,
        size: Option<TerminalSize>,
    ) -> Self {
        Self::spawn_surface_with_appearance(
            String::new(),
            EMPTY_PANE_MODES.to_owned(),
            appearance,
            max_scrollback.min(MAX_HISTORY_LIMIT),
            false,
            size,
        )
    }

    fn spawn_surface_with_appearance(
        title: String,
        text: String,
        appearance: Arc<TerminalAppearance>,
        max_scrollback: usize,
        frozen: bool,
        size: Option<TerminalSize>,
    ) -> Self {
        #[cfg(unix)]
        let shard = shard::choose();
        #[cfg(not(unix))]
        let shard: Result<Option<shard::ShardHandle>, WorkerError> = Ok(None);
        Self::spawn_surface_with_shard(title, text, appearance, max_scrollback, frozen, size, shard)
    }

    pub(super) fn spawn_surface_with_shard(
        title: String,
        text: String,
        appearance: Arc<TerminalAppearance>,
        max_scrollback: usize,
        frozen: bool,
        size: Option<TerminalSize>,
        shard: Result<Option<shard::ShardHandle>, WorkerError>,
    ) -> Self {
        let wake = match &shard {
            Ok(Some(shard)) => shard.wake.for_actor(),
            _ => ActorWake::none(),
        };
        let (command_tx, command_rx) = command_channel();
        let (alive, liveness) = crossbeam_channel::bounded(0);
        let slot = Arc::new(Mutex::new(ControlSlot::default()));
        let commands = CommandSender {
            queues: Arc::new(CommandQueues {
                control: command_tx,
                input: None,
                liveness,
                slot: Arc::clone(&slot),
                wake: wake.clone(),
            }),
        };
        let event_state = Arc::new(EventQueueState::new());
        event_state.resolve_identity();
        let (event_tx, events) = terminal_event_channel(&event_state);
        let geometry = size.map(Geometry::from_size).unwrap_or_default();
        let latest = Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank_with_appearance(
                geometry.columns,
                geometry.rows,
                SessionStatus::Starting,
                &appearance,
            ),
        )));
        let publisher = Publisher {
            event_tx,
            latest: Arc::clone(&latest),
            state: event_state,
        };

        let worker_publisher = publisher.clone();
        let appearance_hash = appearance.stable_hash();
        match shard {
            Ok(Some(shard)) => {
                if let Err(error) = shard.launch(shard::PaneLaunch {
                    control_rx: command_rx,
                    slot,
                    publisher: worker_publisher,
                    max_scrollback,
                    appearance,
                    kind: shard::LaunchKind::Surface {
                        title,
                        text,
                        frozen,
                        geometry,
                    },
                    alive,
                    wake,
                }) {
                    publisher.fail(&error);
                }
            }
            Ok(None) => {
                if let Err(error) = thread::Builder::new()
                    .name(if frozen {
                        "zz-output-view".into()
                    } else {
                        "zz-empty-pane".into()
                    })
                    .spawn(move || {
                        output_view_worker(
                            command_rx,
                            slot,
                            worker_publisher,
                            title,
                            text,
                            appearance,
                            max_scrollback,
                            frozen,
                            geometry,
                        );
                        drop(alive);
                    })
                {
                    publisher.fail(&WorkerError::Thread(error.to_string()));
                }
            }
            Err(error) => publisher.fail(&error),
        }

        Self {
            commands,
            events,
            latest,
            max_scrollback,
            word_separators: RwLock::new(WordSeparators::default()),
            applied_appearance: AtomicU64::new(appearance_hash),
            terminating: AtomicBool::new(false),
            preview_pending: AtomicBool::new(false),
            retired: AtomicBool::new(false),
        }
    }

    #[must_use]
    pub const fn max_scrollback(&self) -> usize {
        self.max_scrollback
    }

    #[must_use]
    pub fn foreground_process_id(&self) -> Option<u32> {
        self.events.state.foreground.read().as_ref()?.process_id()
    }

    #[must_use]
    pub fn last_output(&self) -> Option<Instant> {
        self.events.state.last_output()
    }

    pub fn take_output_since_check(&self) -> bool {
        self.events.state.take_output_since_check()
    }

    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.events
            .state
            .foreground
            .read()
            .as_ref()?
            .shell_process_id()
    }

    #[must_use]
    pub fn tty_canonical(&self) -> Option<bool> {
        self.events.state.foreground.read().as_ref()?.canonical()
    }

    pub fn wait_for_identity(&self, timeout: Duration) -> bool {
        let state = &self.events.state;
        let deadline = Instant::now() + timeout;
        let mut resolved = state.identity.state.lock();
        debug_assert!(!ROUND_TRIPS_FORBIDDEN.with(Cell::get));
        while !resolved.ready {
            if state
                .identity
                .ready
                .wait_until(&mut resolved, deadline)
                .timed_out()
            {
                break;
            }
        }
        resolved.ready
    }

    pub fn identity_request(
        &self,
        timeout: Duration,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> TerminalRequest<bool> {
        let (reply, request) = self.commands.reply_token(timeout, notify);
        let mut identity = self.events.state.identity.state.lock();
        identity.replies.push(reply);
        if identity.ready {
            let reply = identity.replies.pop().unwrap();
            drop(identity);
            let _ = reply.send(true);
        }
        request
    }

    #[must_use]
    pub fn completion(&self) -> Option<TerminalProcessExit> {
        TerminalProcessExit::decode(self.events.state.completion.load(Ordering::Acquire))
    }

    pub fn terminate(&self) {
        if !self.terminating.swap(true, Ordering::AcqRel) {
            self.send_command(Command::Terminate);
        }
    }

    #[must_use]
    pub fn tty(&self) -> Option<PathBuf> {
        self.events.state.foreground.read().as_ref()?.tty.clone()
    }

    pub fn set_word_separators(&self, separators: WordSeparators) {
        *self.word_separators.write() = separators.clone();
        self.commands.with_slot(|slot| {
            slot.pending.word_separators = Some(Box::new(separators));
            true
        });
    }

    pub fn set_appearance(&self, appearance: Arc<TerminalAppearance>) {
        let hash = appearance.stable_hash();
        if self.applied_appearance.swap(hash, Ordering::AcqRel) == hash {
            return;
        }
        self.commands.with_slot(|slot| {
            slot.pending.appearance = Some(appearance);
            true
        });
    }

    pub fn set_allow_passthrough(&self, enabled: bool) {
        self.commands.with_slot(|slot| {
            slot.pending.allow_passthrough = Some(if enabled {
                AllowPassthrough::All
            } else {
                AllowPassthrough::Off
            });
            true
        });
    }

    pub fn set_wrap_search(&self, enabled: bool) {
        self.commands.with_slot(|slot| {
            slot.pending.wrap_search = Some(enabled);
            true
        });
    }

    pub fn set_engine_knobs(&self, knobs: EngineKnobs) {
        self.commands.with_slot(|slot| {
            slot.pending.engine_knobs = Some(knobs);
            true
        });
    }

    pub fn capture_copy_source(&self) -> Result<CapturedCopySource, TerminalCaptureError> {
        self.commands.request(|reply| Command::CaptureCopySource {
            reply: reply.into(),
        })?
    }

    pub fn write_dead_notice(&self, text: Option<Arc<str>>) {
        self.send_command(Command::WriteDeadNotice(text));
    }

    pub fn set_pending_copy_source(&self, source: Option<Box<CapturedCopySource>>) {
        self.send_command(Command::SetPendingCopySource(source));
    }

    pub fn reset_screen(&self) {
        self.send_command(Command::ResetScreen);
    }

    #[must_use]
    pub fn word_separators(&self) -> WordSeparators {
        self.word_separators.read().clone()
    }

    #[must_use]
    pub fn events(&self) -> TerminalEvents {
        self.events.clone()
    }

    #[must_use]
    pub fn progress_bar(&self) -> ProgressBar {
        self.latest.read().bar
    }

    #[must_use]
    pub fn program_status(&self) -> Arc<ProgramStatus> {
        Arc::clone(&self.latest.read().program_status)
    }

    #[must_use]
    pub fn last_command_status(&self) -> Option<i32> {
        self.latest.read().last_command_status
    }

    #[must_use]
    pub fn take_shell_marks(&self) -> Vec<ShellMark> {
        let mut latest = self.latest.write();
        if latest.shell_marks.is_empty() {
            return Vec::new();
        }
        latest.shell_marks.drain(..).collect()
    }

    #[must_use]
    pub fn shell_marks_dropped(&self) -> u64 {
        self.latest.read().shell_marks_dropped
    }

    #[must_use]
    pub fn facts(&self) -> TerminalFacts {
        self.latest.read().facts
    }

    #[must_use]
    pub fn latest_viewport(&self) -> Arc<TerminalViewport> {
        Arc::clone(&self.latest.read().fallback)
    }

    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        self.latest.read().size
    }

    #[must_use]
    pub fn latest_viewport_for(&self, view: TerminalViewId) -> Option<Arc<TerminalViewport>> {
        self.latest.read().by_view.get(&view).cloned()
    }

    #[must_use]
    pub fn latest_viewports(&self) -> HashMap<TerminalViewId, Arc<TerminalViewport>> {
        self.latest.read().by_view.clone()
    }

    #[must_use]
    pub fn latest_view_frames(&self) -> Vec<ViewFrame> {
        self.latest_frames().0
    }

    #[must_use]
    pub fn latest_frames(&self) -> (Vec<ViewFrame>, Vec<TerminalViewId>) {
        let latest = self.latest.read();
        let mut frames = latest
            .by_view
            .iter()
            .map(|(view, viewport)| {
                (
                    *view,
                    Arc::clone(viewport),
                    latest.epochs.get(view).copied(),
                )
            })
            .collect::<Vec<_>>();
        frames.sort_by_key(|(view, _, _)| view.0);
        (frames, latest.sunk.clone())
    }

    pub fn install_frame_sink(&self, sink: Arc<dyn TerminalFrameSink>) {
        let _ = self.events.state.sinks.frame.set(sink);
    }

    #[must_use]
    pub fn frame_sink(&self) -> Option<&Arc<dyn TerminalFrameSink>> {
        self.events.state.sinks.frame.get()
    }

    #[must_use]
    pub fn latest_viewport_is_current(&self) -> bool {
        self.latest.read().fallback_current
    }

    pub fn retire(&self) {
        self.retired.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }

    pub fn take_preview_ready(&self) -> bool {
        self.preview_pending.load(Ordering::Acquire)
            && self.latest_viewport_is_current()
            && self.preview_pending.swap(false, Ordering::AcqRel)
    }

    #[must_use]
    pub fn copy_mode_facts(&self, view: TerminalViewId) -> Option<Arc<CopyModeFacts>> {
        self.latest.read().copy_facts.get(&view).cloned()
    }

    #[must_use]
    pub fn pane_search_string(&self) -> String {
        self.latest.read().search_string.clone()
    }

    pub fn kitty_image(&self, image_id: u32) -> Result<Option<KittyImage>, KittyImageRequestError> {
        self.commands
            .request(|reply| {
                Command::KittyImage(Box::new(KittyImageRequest {
                    image_id,
                    reply: reply.into(),
                }))
            })
            .map_err(Into::into)
    }

    pub fn kitty_image_generation(
        &self,
        image_id: u32,
    ) -> Result<Option<u64>, KittyImageRequestError> {
        self.commands
            .request(|reply| {
                Command::KittyImageGeneration(Box::new(KittyImageGenerationRequest {
                    image_id,
                    reply: reply.into(),
                }))
            })
            .map_err(Into::into)
    }

    #[must_use]
    pub fn diagnostics(&self) -> TerminalSessionDiagnostics {
        let viewport = self.latest_viewport();
        let (_, pending_pty_input_bytes) = self.commands.pending_input();
        TerminalSessionDiagnostics {
            command_queue_len: self.commands.len(),
            command_queue_capacity: self.commands.capacity(),
            pending_pty_input_bytes,
            event_queue_len: self.events.receiver.len(),
            event_queue_capacity: self.events.receiver.capacity(),
            pending_reliable_events: self.events.state.pending_reliable.load(Ordering::Acquire),
            pending_reliable_bytes: self
                .events
                .state
                .pending_reliable_bytes
                .load(Ordering::Acquire),
            viewport_notification_pending: self
                .events
                .state
                .notification_pending
                .load(Ordering::Acquire),
            max_scrollback: self.max_scrollback,
            viewport_generation: viewport.generation,
            viewport_view_generation: viewport.view_generation,
            viewport_dictionary_generation: viewport.dictionary_generation,
            viewport_columns: viewport.columns,
            viewport_rows: viewport.rows,
            viewport_cells: viewport.cells.len(),
            viewport_cell_bytes: std::mem::size_of_val(viewport.cells.as_ref()),
            viewport_overlays: viewport.overlays.len(),
            viewport_strong_count: Arc::strong_count(&viewport),
            viewport_cell_arc_strong_count: Arc::strong_count(&viewport.cells),
            viewport_dictionary_arc_strong_count: Arc::strong_count(&viewport.dictionary),
            viewport_overlay_arc_strong_count: Arc::strong_count(&viewport.overlays),
            viewport_styles: viewport.dictionary.styles.len(),
            viewport_graphemes: viewport.dictionary.grapheme_offsets.len(),
            viewport_grapheme_bytes: viewport.dictionary.grapheme_bytes.len(),
            viewport_title: Arc::clone(&viewport.presentation.title),
            viewport_status: viewport.status.clone(),
        }
    }

    pub fn send_text(&self, text: impl Into<Arc<str>>) {
        self.try_send_text(text);
    }

    pub fn try_send_text(&self, text: impl Into<Arc<str>>) -> bool {
        self.send_command(Command::Text {
            view: None,
            text: text.into(),
        })
    }

    pub fn send_text_for_view(&self, view: TerminalViewId, text: impl Into<Arc<str>>) {
        self.send_command(Command::Text {
            view: Some(view),
            text: text.into(),
        });
    }

    pub fn send_key(&self, input: KeyInput) {
        self.try_send_key(input);
    }

    pub fn try_send_key(&self, input: KeyInput) -> bool {
        self.send_command(Command::Key {
            view: None,
            input: Box::new(input),
        })
    }

    pub fn send_key_for_view(&self, view: TerminalViewId, input: KeyInput) {
        #[cfg(unix)]
        if self.commands.write_direct_key(&input) {
            return;
        }
        self.send_command(Command::Key {
            view: Some(view),
            input: Box::new(input),
        });
    }

    pub fn resize(&self, columns: u16, rows: u16, cell_width_px: u32, cell_height_px: u32) {
        self.commands.with_slot(|slot| {
            let requested = slot.requested_geometry;
            let pixels = |reported: u32, previous: Option<u32>| match reported {
                0 => previous.unwrap_or(1),
                reported => reported,
            };
            let geometry = Geometry {
                columns: columns.max(1),
                rows: rows.max(1),
                cell_width_px: pixels(cell_width_px, requested.map(|kept| kept.cell_width_px)),
                cell_height_px: pixels(cell_height_px, requested.map(|kept| kept.cell_height_px)),
            };
            if requested == Some(geometry) {
                return false;
            }
            slot.requested_geometry = Some(geometry);
            slot.pending.resize = Some(geometry);
            true
        });
    }

    pub fn attach_view(&self, view: TerminalViewId) {
        self.commands.with_slot(|slot| {
            slot.known_views.insert(view);
            slot.pending.push_view(Command::AttachView(view));
            true
        });
    }

    pub fn detach_view(&self, view: TerminalViewId) {
        self.commands.with_slot(|slot| {
            if !slot.known_views.contains(&view) {
                return false;
            }
            slot.pending.push_view(Command::DetachView(view));
            true
        });
    }

    pub fn release_view(&self, view: TerminalViewId) {
        self.commands.with_slot(|slot| {
            if slot.view_areas.remove(&view).is_some() {
                slot.pending.view_areas = Some(slot.view_areas.clone());
            }
            if !slot.known_views.remove(&view) {
                return false;
            }
            slot.pending.push_view(Command::ReleaseView(view));
            true
        });
    }

    pub fn set_view_area(&self, view: TerminalViewId, area: Option<(u16, u16)>) {
        self.commands.with_slot(|slot| {
            let changed = match area {
                Some(area) => slot.view_areas.insert(view, area) != Some(area),
                None => slot.view_areas.remove(&view).is_some(),
            };
            if changed {
                slot.pending.view_areas = Some(slot.view_areas.clone());
            }
            changed
        });
    }

    #[doc(hidden)]
    #[must_use]
    pub fn known_view_count(&self) -> usize {
        self.commands.queues.slot.lock().known_views.len()
    }

    pub fn set_view_stream(&self, view: TerminalViewId, stream: ViewStream) {
        self.commands.with_slot(|slot| {
            if stream.is_on() {
                slot.known_views.insert(view);
            } else if !slot.known_views.contains(&view) {
                return false;
            }
            slot.pending.push_view(Command::SetViewStream(view, stream));
            true
        });
    }

    pub fn set_preview_watch(&self, watch: bool) {
        self.preview_pending.store(watch, Ordering::Release);
        self.commands.with_slot(|slot| {
            slot.pending.preview = Some(watch);
            true
        });
    }

    pub fn fresh_viewport(&self) -> Arc<TerminalViewport> {
        self.commands
            .request(Command::FreshViewport)
            .unwrap_or_else(|_| self.latest_viewport())
    }

    pub fn view_action(&self, view: TerminalViewId, action: TerminalViewAction) {
        self.send_command(Command::ViewAction { view, action });
    }

    pub fn paste_prepared_bytes(
        &self,
        view: Option<TerminalViewId>,
        bytes: Arc<[u8]>,
        bracketed: bool,
    ) {
        self.send_command(Command::PastePreparedBytes {
            view,
            bytes,
            bracketed,
        });
    }

    pub fn send_raw_input(&self, bytes: Arc<[u8]>) -> bool {
        self.send_command(Command::RawInput(bytes))
    }

    pub fn send_raw_input_notified(&self, bytes: Arc<[u8]>, waker: &std::task::Waker) -> bool {
        self.commands.queues.slot.lock().raw_input_space_waker = Some(waker.clone());
        if self.send_raw_input(bytes) {
            self.commands.queues.slot.lock().raw_input_space_waker = None;
            true
        } else {
            false
        }
    }

    #[must_use]
    pub fn output_wake(&self) -> OutputWake {
        OutputWake(self.commands.queues.wake.clone())
    }

    pub fn settle(&self) -> bool {
        self.commands
            .request(|reply| Command::Settle {
                reply: reply.into(),
            })
            .is_ok()
    }

    pub fn settle_request(&self, notify: Arc<dyn Fn() + Send + Sync>) -> TerminalRequest<()> {
        self.commands
            .request_token(notify, |reply| Command::Settle { reply })
    }

    pub fn copy_source_request(
        &self,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> TerminalRequest<Result<CapturedCopySource, TerminalCaptureError>> {
        self.commands
            .request_token(notify, |reply| Command::CaptureCopySource { reply })
    }

    pub fn capture_request(
        &self,
        options: CaptureOptions,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> TerminalRequest<Result<String, TerminalCaptureError>> {
        self.commands.request_token(notify, |reply| {
            Command::Capture(Box::new(CaptureRequest { options, reply }))
        })
    }

    pub fn history_request(
        &self,
        start: u32,
        count: u32,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> TerminalRequest<Result<HistoryCapture, TerminalCaptureError>> {
        self.commands.request_token(notify, |reply| {
            Command::History(Box::new(HistoryCommand {
                start,
                count,
                reply,
            }))
        })
    }

    pub fn capture_last_command_request(
        &self,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> TerminalRequest<Result<LastCommandCapture, TerminalCaptureError>> {
        self.commands.request_token(notify, |reply| {
            Command::SemanticCapture(Box::new(LastCommandRequest { reply }))
        })
    }

    pub fn pointer_context_request(
        &self,
        view: TerminalViewId,
        column: u16,
        row: u16,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> TerminalRequest<PointerContext> {
        self.commands.request_token(notify, |reply| {
            Command::PointerContext(Box::new(PointerContextRequest {
                view,
                column,
                row,
                reply,
            }))
        })
    }

    pub fn kitty_image_request(
        &self,
        image_id: u32,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> TerminalRequest<Option<KittyImage>> {
        self.commands.request_token(notify, |reply| {
            Command::KittyImage(Box::new(KittyImageRequest { image_id, reply }))
        })
    }

    pub fn kitty_image_generation_request(
        &self,
        image_id: u32,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> TerminalRequest<Option<u64>> {
        self.commands.request_token(notify, |reply| {
            Command::KittyImageGeneration(Box::new(KittyImageGenerationRequest { image_id, reply }))
        })
    }

    pub fn open_pending_paste(&self, token: u64) {
        self.send_command(Command::PendingPasteOpened { token });
    }

    pub fn unbind_pasted_image(&self, number: u32) {
        self.send_command(Command::UnbindPastedImage { number });
    }

    pub fn pointer_context(
        &self,
        view: TerminalViewId,
        column: u16,
        row: u16,
    ) -> Result<PointerContext, TerminalCaptureError> {
        self.commands
            .request(|reply| {
                Command::PointerContext(Box::new(PointerContextRequest {
                    view,
                    column,
                    row,
                    reply: reply.into(),
                }))
            })
            .map_err(Into::into)
    }

    pub fn capture(&self, options: CaptureOptions) -> Result<String, TerminalCaptureError> {
        self.commands.request(|reply| {
            Command::Capture(Box::new(CaptureRequest {
                options,
                reply: reply.into(),
            }))
        })?
    }

    pub fn capture_frozen_frame(
        &self,
        options: CaptureOptions,
    ) -> Result<String, TerminalCaptureError> {
        match self.capture(options) {
            Err(TerminalCaptureError::ActorStopped) => self.capture_retained_frame(options),
            result => result,
        }
    }

    pub fn capture_retained_frame(
        &self,
        options: CaptureOptions,
    ) -> Result<String, TerminalCaptureError> {
        capture_viewport(&self.latest_viewport(), options)
    }

    #[allow(
        clippy::type_complexity,
        reason = "the tuple is the established public history-page response"
    )]
    pub fn history(
        &self,
        start: u32,
        count: u32,
    ) -> Result<
        (
            u32,
            Vec<Vec<PackedCell>>,
            TerminalDictionary,
            ScrollbarState,
            u16,
        ),
        TerminalCaptureError,
    > {
        self.commands.request(|reply| {
            Command::History(Box::new(HistoryCommand {
                start,
                count,
                reply: reply.into(),
            }))
        })?
    }

    pub fn capture_last_command(&self) -> Result<LastCommandCapture, TerminalCaptureError> {
        self.commands.request(|reply| {
            Command::SemanticCapture(Box::new(LastCommandRequest {
                reply: reply.into(),
            }))
        })?
    }

    pub fn feed(&self, bytes: Arc<[u8]>) -> bool {
        self.send_command(Command::Output(bytes))
    }

    pub(super) fn send_command(&self, command: Command) -> bool {
        let started = diagnostic_timer();
        let queue_before = if started.is_some() {
            self.commands.len()
        } else {
            0
        };
        log::trace!(
            target: "zz_terminal::diagnostics::command",
            "enqueue begin queue_len={queue_before} queue_capacity={:?} command={command:#?}",
            self.commands.capacity(),
        );
        let result = self.commands.send(command);
        let success = result.is_ok();
        if let Err(crossbeam_channel::TrySendError::Full(command)) = &result {
            let (pending_commands, pending_bytes) = self.commands.pending_input();
            log::warn!(
                "rejected terminal PTY input command={} payload_bytes={} pending_commands={} pending_bytes={} limit_bytes={}",
                command.name(),
                command.pty_input_payload().unwrap_or(0),
                pending_commands,
                pending_bytes,
                MAX_PENDING_PTY_INPUT_BYTES,
            );
        }
        log::trace!(
            target: "zz_terminal::diagnostics::command",
            "enqueue end success={} queue_before={} queue_after={} elapsed_us={}",
            success,
            queue_before,
            self.commands.len(),
            diagnostic_elapsed_us(started),
        );
        success
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if !self.terminating.load(Ordering::Acquire)
            && let Err(crossbeam_channel::TrySendError::Full(command)) =
                self.commands.try_send(Command::Shutdown)
        {
            self.commands.defer(command);
        }
    }
}
