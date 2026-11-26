use super::*;

impl Shared {
    pub(super) fn open_command_output_for_pane(
        self: &Arc<Self>,
        pane: PaneId,
        title: &str,
        output: &str,
    ) {
        let clients = {
            let inner = self.inner.lock();
            attached_clients_for_pane(&inner, pane)
                .into_iter()
                .flatten()
                .copied()
                .filter(|client| {
                    inner
                        .client(*client)
                        .is_some_and(|c| c.subscriber.is_some())
                        && inner.client(*client).and_then(|c| c.kind)
                            == Some(ClientKind::Interactive)
                })
                .collect::<Vec<_>>()
        };
        for client in clients {
            let _ = self.open_command_output(client, Some(pane), title.to_owned(), output);
        }
    }

    pub(super) fn open_command_output(
        self: &Arc<Self>,
        client: ClientId,
        preferred_pane: Option<PaneId>,
        title: String,
        text: &str,
    ) -> Result<(), DaemonError> {
        if text.is_empty() {
            return Ok(());
        }
        let (pane, terminal, events) =
            self.install_command_output(client, preferred_pane, title, text, false)?;
        if let Err(error) =
            self.watch_command_output(client, pane, &terminal, events, VecDeque::new(), None)
        {
            self.close_command_output(client, &terminal);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn open_startup_command_output(
        self: &Arc<Self>,
        client: ClientId,
        preferred_pane: Option<PaneId>,
        title: String,
        text: &str,
        outbound: &Arc<OutboundMailbox>,
    ) -> Result<u64, DaemonError> {
        let (pane, terminal, events) =
            self.install_command_output(client, preferred_pane, title, text, true)?;
        let result = (|| {
            let view = TerminalViewId(client.0);
            let mut prefetched = VecDeque::new();
            let viewport = loop {
                let event = events.recv_deferred_blocking().map_err(|error| {
                    DaemonError::Thread(format!(
                        "command output event stream closed before its initial viewport: {error}"
                    ))
                })?;
                match event.event() {
                    TerminalEvent::ViewportReady { .. } => {
                        event.into_event();
                        break terminal.latest_viewport_for(view).ok_or_else(|| {
                            DaemonError::Thread(
                                "command output did not publish its initial viewport".to_owned(),
                            )
                        })?;
                    }
                    TerminalEvent::ViewClosed(closed) if *closed == view => {
                        return Err(DaemonError::Thread(
                            "command output closed before its initial viewport".to_owned(),
                        ));
                    }
                    _ => prefetched.push_back(event),
                }
            };
            let admitted_generation = viewport_generation(&viewport);
            let output_id = self.enqueue_initial_command_output(
                client,
                pane,
                &terminal,
                viewport.as_ref(),
                outbound,
            )?;
            self.watch_command_output(
                client,
                pane,
                &terminal,
                events,
                prefetched,
                Some(admitted_generation),
            )?;
            Ok(output_id)
        })();
        if result.is_err() {
            self.retire_current_command_output(client, &terminal);
        }
        result
    }

    fn install_command_output(
        self: &Arc<Self>,
        client: ClientId,
        preferred_pane: Option<PaneId>,
        title: String,
        text: &str,
        startup: bool,
    ) -> Result<(PaneId, Arc<TerminalSession>, TerminalEvents), DaemonError> {
        if text.is_empty() {
            return Err(ServerError::InvalidCommand(
                "command output requires non-empty text".to_owned(),
            )
            .into());
        }
        let text = text
            .strip_suffix('\n')
            .filter(|rest| !rest.is_empty())
            .unwrap_or(text);
        let text = bounded_command_output(text);
        let (pane, word_separators, terminal_options) = {
            let inner = self.inner.lock();
            if inner.client(client).is_none_or(|c| c.subscriber.is_none()) {
                return Err(ServerError::InvalidCommand(
                    "command output requires an interactive client".to_owned(),
                )
                .into());
            }
            let pane = preferred_pane
                .filter(|pane| inner.engine.state.window_for_pane(*pane).is_some())
                .or_else(|| client_context_pane(&inner, client))
                .ok_or_else(|| ServerError::MissingTarget("current pane".to_owned()))?;
            (
                pane,
                WordSeparators::new(inner.engine.word_separators_for_pane(pane)?),
                terminal_worker_options(
                    &inner.engine,
                    &inner.config.appearance,
                    &inner.config.config_files,
                    pane,
                )?,
            )
        };
        let terminal = Arc::new(if startup {
            TerminalSession::spawn_startup_output_view_with_appearance(
                title,
                text,
                Arc::clone(&terminal_options.appearance),
            )
        } else {
            TerminalSession::spawn_output_view_with_appearance(
                title,
                text,
                Arc::clone(&terminal_options.appearance),
            )
        });
        let events = terminal.events();
        terminal.set_wrap_search(terminal_options.wrap_search);
        terminal.set_engine_knobs(terminal_options.knobs);
        let view = TerminalViewId(client.0);

        let (replaced, display_panes_closed, command_prompt_closed) = {
            let mut inner = self.inner.lock();
            if inner.client(client).is_none_or(|c| c.subscriber.is_none()) {
                return Err(ServerError::InvalidCommand(
                    "command output requires an interactive client".to_owned(),
                )
                .into());
            }
            if inner.engine.state.window_for_pane(pane).is_none() {
                return Err(ServerError::MissingTarget(pane.to_string()).into());
            }
            let display_panes_closed = dismiss_display_panes(&mut inner, client);
            let command_prompt_closed = inner
                .client_mut(client)
                .and_then(|c| c.command_prompt.take());
            let replaced = inner
                .client_mut(client)
                .and_then(|c| c.command_output.take());
            let previous_key_table = replaced.as_ref().map_or_else(
                || {
                    let engine = inner
                        .client_entry(client)
                        .key_engine
                        .get_or_insert_default();
                    let repeat_held = engine
                        .shown_table(Instant::now())
                        .is_some_and(|(_, repeat)| repeat);
                    engine
                        .active_table()
                        .filter(|table| *table != "prefix" && !repeat_held)
                        .map(str::to_owned)
                },
                |output| output.previous_key_table.clone(),
            );
            let table = inner.engine.copy_mode_table_for_pane(pane)?.to_owned();
            inner
                .client_entry(client)
                .key_engine
                .get_or_insert_default()
                .switch_table(Some(table));
            inner.waits.next_command_output_id = inner
                .waits
                .next_command_output_id
                .checked_add(1)
                .expect("command output IDs exhausted");
            let output_id = inner.waits.next_command_output_id;
            inner
                .client_entry(client)
                .command_output
                .replace(CommandOutputSession {
                    output_id,
                    pane,
                    terminal: Arc::clone(&terminal),
                    previous_key_table,
                    parked: false,
                });
            let replaced = replaced.map(|output| {
                let subscriber = inner
                    .client(client)
                    .and_then(|c| c.subscriber.as_ref())
                    .cloned();
                (output, subscriber)
            });
            (replaced, display_panes_closed, command_prompt_closed)
        };

        terminal.set_word_separators(word_separators);
        terminal.attach_view(view);
        terminal.set_view_stream(view, ViewStream::Foreground);
        self.publish_chooser_state(client);
        if display_panes_closed {
            self.publish_to_client(client, EventPayload::DisplayPanes { state: None });
            self.settle_pane_modes(client, &mut ExecutionContext::default());
        }
        if let Some(closed) = command_prompt_closed {
            self.publish_to_client(client, EventPayload::CommandPrompt { state: None });
            if closed.freezes() {
                self.resume_client_terminals(client);
            }
        }
        if let Some(replaced) = replaced {
            self.retire_command_output(client, replaced);
        }
        Ok((pane, terminal, events))
    }

    fn watch_command_output(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        events: TerminalEvents,
        prefetched: VecDeque<DeferredTerminalEvent>,
        admitted_generation: Option<TerminalGeneration>,
    ) -> Result<(), DaemonError> {
        #[cfg(any(test, windows))]
        self.start_watcher_consumer()?;
        self.watcher_tx.register(
            watchers::Watcher::command_output(client, pane, terminal, admitted_generation),
            events,
            prefetched,
        )
    }

    pub(super) fn publish_command_output(
        &self,
        client: ClientId,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        viewport: &TerminalViewport,
    ) {
        self.publish_command_output_with_encoder(
            client,
            pane,
            terminal,
            viewport,
            OutboundMailbox::encode_message,
        );
    }

    fn enqueue_initial_command_output(
        &self,
        client: ClientId,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        viewport: &TerminalViewport,
        outbound: &Arc<OutboundMailbox>,
    ) -> Result<u64, DaemonError> {
        let current = {
            let inner = self.inner.lock();
            current_command_output_subscriber(&inner, client, pane, terminal)
        };
        let Some((output_id, subscriber)) = current else {
            return Err(DaemonError::Thread(
                "startup command output was replaced before admission".to_owned(),
            ));
        };
        if !Arc::ptr_eq(&subscriber, outbound) {
            return Err(DaemonError::Thread(
                "startup command output subscriber changed before admission".to_owned(),
            ));
        }
        {
            let inner = self.inner.lock();
            if inner.client(client).is_some_and(|c| c.ctrl_initializing)
                && current_command_output_subscriber(&inner, client, pane, terminal).is_some_and(
                    |(current_id, current)| {
                        current_id == output_id && Arc::ptr_eq(&current, &subscriber)
                    },
                )
                && outbound.is_open()
            {
                return Ok(output_id);
            }
        }
        let message = Self::event(EventPayload::CommandOutput {
            output_id,
            pane,
            viewport: Some(viewport.clone()),
        });
        let encoded = subscriber.encode_message(&message)?;
        let mut encoded = Some(encoded);
        let admitted = {
            let inner = self.inner.lock();
            current_command_output_subscriber(&inner, client, pane, terminal)
                .filter(|(current_id, current)| {
                    *current_id == output_id && Arc::ptr_eq(current, &subscriber)
                })
                .is_some_and(|_| {
                    subscriber.enqueue_encoded_reliable(
                        encoded
                            .take()
                            .expect("encoded startup command output is available"),
                    )
                })
        };
        if !admitted {
            if let Some(encoded) = encoded {
                subscriber.recycle_frame(encoded);
            }
            return Err(DaemonError::Thread(
                "startup command output was not admitted".to_owned(),
            ));
        }
        Ok(output_id)
    }

    pub(super) fn publish_command_output_with_encoder(
        &self,
        client: ClientId,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        viewport: &TerminalViewport,
        encode: impl FnOnce(&OutboundMailbox, &ProtocolMessage) -> Result<Vec<u8>, ProtocolError>,
    ) {
        let current = {
            let inner = self.inner.lock();
            current_command_output_subscriber(&inner, client, pane, terminal)
        };
        let Some((output_id, subscriber)) = current else {
            return;
        };
        let message = Self::event(EventPayload::CommandOutput {
            output_id,
            pane,
            viewport: Some(viewport.clone()),
        });
        let Ok(encoded) = encode(&subscriber, &message) else {
            log::error!("failed to encode command-output viewport");
            return;
        };

        let mut encoded = Some(encoded);
        let installed = {
            let inner = self.inner.lock();
            current_command_output_subscriber(&inner, client, pane, terminal)
                .filter(|(current_id, current)| {
                    *current_id == output_id && Arc::ptr_eq(current, &subscriber)
                })
                .is_some_and(|_| {
                    subscriber.replace_encoded_command_output(
                        output_id,
                        encoded.take().expect("encoded command output is available"),
                    )
                })
        };
        if !installed && let Some(encoded) = encoded {
            subscriber.recycle_frame(encoded);
        }
    }

    fn retire_current_command_output(&self, client: ClientId, terminal: &Arc<TerminalSession>) {
        let retired = {
            let mut inner = self.inner.lock();
            let current = inner
                .client(client)
                .and_then(|c| c.command_output.as_ref())
                .is_some_and(|output| Arc::ptr_eq(&output.terminal, terminal));
            current
                .then(|| take_command_output(&mut inner, client))
                .flatten()
        };
        if let Some(retired) = retired {
            self.retire_command_output(client, retired);
        }
    }

    pub(super) fn retire_command_output_if_exact(&self, client: ClientId, output_id: u64) {
        let retired = {
            let mut inner = self.inner.lock();
            let current = inner
                .client(client)
                .and_then(|c| c.command_output.as_ref())
                .is_some_and(|output| output.output_id == output_id);
            current
                .then(|| take_command_output(&mut inner, client))
                .flatten()
        };
        if let Some(retired) = retired {
            self.retire_command_output(client, retired);
        }
    }

    pub(super) fn close_command_output(&self, client: ClientId, terminal: &Arc<TerminalSession>) {
        let (output_id, pane, subscriber) = {
            let mut inner = self.inner.lock();
            let Some(output) = inner.client(client).and_then(|c| c.command_output.as_ref()) else {
                return;
            };
            if !Arc::ptr_eq(&output.terminal, terminal) {
                return;
            }
            let output = inner
                .client_mut(client)
                .and_then(|c| c.command_output.take())
                .expect("command output was checked above");
            if !output.parked {
                inner
                    .client_entry(client)
                    .key_engine
                    .get_or_insert_default()
                    .switch_table(output.previous_key_table);
            }
            (
                output.output_id,
                output.pane,
                inner
                    .client(client)
                    .and_then(|c| c.subscriber.as_ref())
                    .cloned(),
            )
        };
        self.sync_key_table(client, false);
        if let Some(subscriber) = subscriber {
            Self::send_event(
                &subscriber,
                EventPayload::CommandOutput {
                    output_id,
                    pane,
                    viewport: None,
                },
            );
        }
        self.publish_chooser_state(client);
    }

    pub(super) fn retire_command_output(
        &self,
        client: ClientId,
        (output, subscriber): RetiredCommandOutput,
    ) {
        self.sync_key_table(client, false);
        output.terminal.view_action(
            TerminalViewId(client.0),
            zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
        );
        if let Some(subscriber) = subscriber {
            Self::send_event(
                &subscriber,
                EventPayload::CommandOutput {
                    output_id: output.output_id,
                    pane: output.pane,
                    viewport: None,
                },
            );
        }
        self.publish_chooser_state(client);
    }

    pub(super) fn is_current_command_output(
        &self,
        client: ClientId,
        terminal: &Arc<TerminalSession>,
    ) -> bool {
        self.read_client(client, |c| {
            c.and_then(|c| c.command_output.as_ref())
                .is_some_and(|output| Arc::ptr_eq(&output.terminal, terminal))
        })
    }

    pub(super) fn begin_control_command_event_capture(
        self: &Arc<Self>,
        client: ClientId,
    ) -> ControlCommandEventCapture {
        let item_id = self.command_item.as_ref().expect("command item").lock().id;
        self.inner
            .lock()
            .waits
            .control_command_event_captures
            .entry((client, item_id))
            .or_default()
            .push(CapturedControlCommandEvents::default());
        ControlCommandEventCapture {
            shared: Arc::clone(self),
            client,
            item_id,
            active: true,
        }
    }

    pub(super) fn pop_control_command_event_capture(
        &self,
        client: ClientId,
        item_id: cmdq::ItemId,
    ) -> Option<CapturedControlCommandEvents> {
        let capture_key = (client, item_id);
        let mut inner = self.inner.lock();
        let captures = inner
            .waits
            .control_command_event_captures
            .get_mut(&capture_key)?;
        let finished = captures.pop();
        if captures.is_empty() {
            inner
                .waits
                .control_command_event_captures
                .remove(&capture_key);
        }
        finished
    }

    pub(super) fn is_capturing_control_command_events(&self, client: ClientId) -> bool {
        let capture_key = self
            .command_item
            .as_ref()
            .map(|item| (client, item.lock().id));
        self.inner
            .lock()
            .waits
            .control_command_event_captures
            .get(&capture_key.unwrap_or((client, cmdq::ItemId::NONE)))
            .is_some_and(|captures| !captures.is_empty())
    }

    pub(super) fn publish_captured_control_command_events(
        &self,
        client: ClientId,
        captured: CapturedControlCommandEvents,
    ) {
        let CapturedControlCommandEvents {
            events,
            callback_parse_events,
        } = captured;
        let mut callback_parse_events = callback_parse_events.into_iter().peekable();
        for (index, payload) in events.into_iter().enumerate() {
            let callback_parse_event = callback_parse_events
                .next_if(|(event_index, _)| *event_index == index)
                .map(|(_, event)| event);
            self.publish_to_client_with_callback_parse_event(client, payload, callback_parse_event);
        }
    }
}

fn current_command_output_subscriber(
    inner: &ServerState,
    client: ClientId,
    pane: PaneId,
    terminal: &Arc<TerminalSession>,
) -> Option<(u64, Arc<OutboundMailbox>)> {
    let output = inner
        .client(client)
        .and_then(|c| c.command_output.as_ref())?;
    if output.pane != pane || !Arc::ptr_eq(&output.terminal, terminal) {
        return None;
    }
    Some((
        output.output_id,
        inner
            .client(client)
            .and_then(|c| c.subscriber.as_ref())
            .cloned()?,
    ))
}

fn bounded_command_output(text: &str) -> String {
    let mut output = String::new();
    if append_command_prompt_output(&mut output, text) {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(COMMAND_PROMPT_OUTPUT_TRUNCATED);
    }
    output
}
