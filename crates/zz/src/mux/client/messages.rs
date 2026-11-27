use super::*;

impl MuxClient {
    pub(super) fn handle_message(
        &mut self,
        host: HostId,
        message: ProtocolMessage,
        cx: &mut Context<Self>,
    ) {
        let Some(message) = self.route_host_path_list(host, message, cx) else {
            return;
        };
        let message = match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Providers(report),
                ..
            }) => {
                if let Some(connection) = self.connections.get_mut(&host) {
                    connection.providers = Some(report);
                }
                cx.notify();
                return;
            }
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentAccounts(accounts),
                ..
            }) => {
                if let Some(connection) = self.connections.get_mut(&host) {
                    connection.agent_accounts = Some(accounts);
                }
                cx.notify();
                return;
            }
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentQuotas(quotas),
                ..
            }) => {
                if let Some(connection) = self.connections.get_mut(&host) {
                    connection.agent_quotas = quotas;
                }
                cx.notify();
                return;
            }
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentLogin(login),
                ..
            }) => {
                self.observe_agent_login(host, login, cx);
                return;
            }
            ProtocolMessage::Batch(batch) => {
                match batch.messages() {
                    Ok(messages) => {
                        let attached_before = self.core.attached_session();
                        let active = host == self.attached_host;
                        if active {
                            self.core.begin_event_group();
                        }
                        if let Some(connection) = self.connections.get_mut(&host) {
                            connection.background_core.begin_event_group();
                        }
                        for message in messages {
                            self.handle_message(host, message, cx);
                        }
                        if let Some(connection) = self.connections.get_mut(&host) {
                            connection.background_core.finish_event_group();
                            connection.snapshot =
                                Some(Arc::clone(connection.background_core.snapshot()));
                            while let Some(outbound) = connection.background_core.poll_outbound() {
                                if !active
                                    && matches!(outbound, Outbound::TreeSync)
                                    && let Some(client) = &connection.client
                                {
                                    let _ = client.send(&ProtocolMessage::TreeSync);
                                }
                            }
                            while connection.background_core.poll_event().is_some() {}
                        }
                        if active {
                            self.core.finish_event_group();
                            while let Some(outbound) = self.core.poll_outbound() {
                                match outbound {
                                    Outbound::RequestFull(pane) => self.request_full_viewport(pane),
                                    Outbound::TreeSync => {
                                        if let Some(client) = &self.attached_connection().client {
                                            let _ = client.send(&ProtocolMessage::TreeSync);
                                        }
                                    }
                                }
                            }
                            let mut attaching = true;
                            while let Some(event) = self.core.poll_event() {
                                self.apply_core_event(
                                    host,
                                    event,
                                    attached_before,
                                    &mut attaching,
                                    cx,
                                );
                            }
                        }
                        cx.notify();
                    }
                    Err(error) => {
                        self.error = Some(Arc::from(error.to_string()));
                        cx.notify();
                    }
                }
                return;
            }
            message => message,
        };
        if host == self.attached_host
            && let ProtocolMessage::Event(event) = &message
            && matches!(
                event.payload,
                EventPayload::Snapshot(_)
                    | EventPayload::TreeDelta(_)
                    | EventPayload::ClientView(_)
                    | EventPayload::KeyTablesChanged { .. }
                    | EventPayload::KeyTablesPatched { .. }
                    | EventPayload::AgentParents { .. }
            )
            && let Some(connection) = self.connections.get_mut(&host)
        {
            connection.background_core.handle_message(message.clone());
            while connection.background_core.poll_event().is_some() {}
            while connection.background_core.poll_outbound().is_some() {}
        }
        if host != self.attached_host {
            match message {
                ProtocolMessage::Event(event) => match event.payload {
                    payload @ (EventPayload::Snapshot(_)
                    | EventPayload::TreeDelta(_)
                    | EventPayload::ClientView(_)
                    | EventPayload::KeyTablesChanged { .. }
                    | EventPayload::KeyTablesPatched { .. }
                    | EventPayload::AgentParents { .. }) => {
                        if let Some(connection) = self.connections.get_mut(&host) {
                            connection
                                .background_core
                                .handle_message(ProtocolMessage::Event(Event {
                                    sequence: event.sequence,
                                    payload,
                                }));
                            connection.snapshot =
                                Some(Arc::clone(connection.background_core.snapshot()));
                            while let Some(outbound) = connection.background_core.poll_outbound() {
                                if matches!(outbound, Outbound::TreeSync)
                                    && let Some(client) = &connection.client
                                {
                                    let _ = client.send(&ProtocolMessage::TreeSync);
                                }
                            }
                            while connection.background_core.poll_event().is_some() {}
                            cx.notify();
                        }
                    }
                    EventPayload::Bell { pane } => {
                        self.record_bell(host, pane);
                        cx.notify();
                    }
                    EventPayload::AppearanceChanged {
                        appearance,
                        provenance,
                    } => {
                        if let Some(connection) = self.connections.get_mut(&host) {
                            connection.appearance = Some((*appearance, provenance));
                        }
                    }
                    _ => {}
                },
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Error {
                    request_id,
                    error,
                    ..
                }) => {
                    if !self.report_command_failure(host, request_id, &error, cx) {
                        log::warn!(
                            target: "zz::diagnostics::mux",
                            "unsolicited_error host={host:?} error={error}"
                        );
                    }
                }
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Success {
                    request_id,
                    ..
                }) => {
                    if self.answer(host, request_id, Ok(())) {
                        cx.notify();
                    } else if let Some(connection) = self.connections.get(&host) {
                        connection.take_command(request_id);
                        connection.settle_command(request_id);
                    }
                }
                _ => {}
            }
            return;
        }
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let status_only = matches!(
            &message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::StatusChanged { .. },
                ..
            })
        );
        let quiet_success = self.error.is_none()
            && matches!(
                &message,
                ProtocolMessage::CommandResponse(CommandResponse::Success { .. })
            );
        let path_list = matches!(
            &message,
            ProtocolMessage::PathListBegin { .. }
                | ProtocolMessage::PathListChunk { .. }
                | ProtocolMessage::PathListGit { .. }
        );
        let terminal_frame = matches!(
            &message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { .. } | EventPayload::TerminalPatch { .. },
                ..
            })
        );
        log::trace!(
            target: "zz::diagnostics::mux",
            "handle_message begin message={message:#?}"
        );
        match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { pane, viewport },
                ..
            }) => self.apply_terminal_viewport(pane, viewport, cx),
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalPatch { pane, patch },
                ..
            }) => self.apply_terminal_patch(pane, patch, cx),
            ProtocolMessage::Event(Event {
                payload: EventPayload::CommandOutput { pane, viewport, .. },
                ..
            }) => self.apply_command_output(pane, viewport),
            message => {
                let attached_before = self.core.attached_session();
                self.core.handle_message(message);
                while let Some(outbound) = self.core.poll_outbound() {
                    match outbound {
                        Outbound::RequestFull(pane) => self.request_full_viewport(pane),
                        Outbound::TreeSync => {
                            if let Some(client) = &self.attached_connection().client {
                                let _ = client.send(&ProtocolMessage::TreeSync);
                            }
                        }
                    }
                }
                let mut attaching = false;
                while let Some(event) = self.core.poll_event() {
                    self.apply_core_event(host, event, attached_before, &mut attaching, cx);
                }
            }
        }
        log::trace!(
            target: "zz::diagnostics::mux",
            "handle_message end elapsed_us={} mux_generation={} viewports={} resync_pending={}",
            diagnostics::elapsed_us(started),
            self.core.snapshot().generation,
            self.viewports.len(),
            self.attached_connection().resync_pending,
        );
        if !status_only && !quiet_success && !path_list && !terminal_frame {
            cx.notify();
        }
    }

    pub(super) fn apply_core_event(
        &mut self,
        host: HostId,
        event: CoreEvent,
        attached_before: Option<SessionId>,
        attaching: &mut bool,
        cx: &mut Context<Self>,
    ) {
        match event {
            CoreEvent::Attached { session } => {
                *attaching = true;
                self.finish_attach(host, session, cx);
            }
            CoreEvent::SnapshotChanged => {
                self.attached_snapshot_pending = false;
                let connection = self.attached_connection_mut();
                connection.resync_pending = false;
                connection.full_requests_pending.clear();
                connection.history.clear_deferred();
                if !*attaching {
                    self.backfill_retained_history();
                }
            }
            CoreEvent::AppearanceChanged => self.adopt_core_appearance(cx),
            CoreEvent::MuxOptionsChanged => self.backfill_retained_history(),
            CoreEvent::PrefixArmed { armed } => log::info!(
                target: "zz::diagnostics::input",
                "prefix_armed_received armed={armed}"
            ),
            CoreEvent::PrefixCancelled { request_id } => {
                self.prefix_cancelled_request = self.prefix_cancelled_request.max(Some(request_id));
            }
            CoreEvent::CommandPromptChanged => {
                self.command_prompt_revision = self.command_prompt_revision.wrapping_add(1).max(1);
            }
            CoreEvent::ChooseTreeChanged => {
                self.choose_tree_revision = self.choose_tree_revision.wrapping_add(1).max(1);
                if self.core.choose_tree().is_none() {
                    self.choose_tree_closed_revision = self.choose_tree_revision;
                }
            }
            CoreEvent::ChooseBufferChanged => {
                self.choose_buffer_revision = self.choose_buffer_revision.wrapping_add(1).max(1);
            }
            CoreEvent::DisplayPanesChanged => {
                self.display_panes_revision = self.display_panes_revision.wrapping_add(1).max(1);
            }
            CoreEvent::MenuChanged => {
                self.menu_revision = self.menu_revision.wrapping_add(1).max(1);
            }
            CoreEvent::ConfirmChanged => {
                self.confirm_revision = self.confirm_revision.wrapping_add(1).max(1);
            }
            CoreEvent::PaneRemoved { pane } => self.forget_pane(pane),
            CoreEvent::Bell { pane } => self.record_bell(host, pane),
            CoreEvent::FocusSidebar => {
                self.sidebar_focus_revision = self.sidebar_focus_revision.wrapping_add(1).max(1);
                log::info!(
                    target: "zz::diagnostics::input",
                    "focus_sidebar_received revision={}",
                    self.sidebar_focus_revision
                );
            }
            CoreEvent::Detached {
                session,
                by,
                action,
            } => {
                if attached_before == Some(session) {
                    self.reset_session_state(cx);
                    let notice = match action {
                        zz_protocol::ClientExitAction::Exec { command, .. } => {
                            format!("detach-client -E needs a terminal client: {command}")
                        }
                        _ => by.map_or_else(
                            || "session ended".to_owned(),
                            |device| format!("detached by {device}"),
                        ),
                    };
                    Self::emit_notification(ClientMessageKind::Warning, notice, cx);
                }
            }
            CoreEvent::ServerStopping => {
                self.error_after_next_attach = None;
                self.reset_session_state(cx);
                if host == HostId::LOCAL {
                    self.error = Some(Arc::from("zz daemon stopped"));
                } else {
                    self.error = None;
                }
            }
            CoreEvent::CommandResponse(response) => {
                self.handle_command_response(host, response, cx);
            }
            CoreEvent::ClientMessage {
                kind,
                text,
                duration_ms,
                message_id,
                ..
            } => cx.emit(ClientNotification {
                key: Some(if text.starts_with(CONFIG_RELOADED_MESSAGE) {
                    CONFIG_RELOADED_MESSAGE.to_owned()
                } else {
                    text.clone()
                }),
                kind,
                text,
                duration_ms,
                message_id,
            }),
            CoreEvent::ClientMessageCleared { message_id } => {
                cx.emit(ClientNotificationCleared { message_id });
            }
            CoreEvent::Clipboard {
                pane,
                producer,
                target,
                text,
                ..
            } => self.write_clipboard(pane, producer, target, text, cx),
            CoreEvent::OpenUri { pane, uri } => self.route_open_uri(pane, &uri, cx),
            CoreEvent::BrowserCommand { pane, command } => {
                if let BrowserCommand::Screenshot { request_id, path } = command {
                    self.screenshot_requests.push((pane, request_id, path));
                } else {
                    self.browser_commands.entry(pane).or_default().push(command);
                }
                self.pending_commands_revision =
                    self.pending_commands_revision.wrapping_add(1).max(1);
            }
            CoreEvent::TerminalUiCommand { pane, command } => {
                self.terminal_commands
                    .entry(pane)
                    .or_default()
                    .push(command);
                self.pending_commands_revision =
                    self.pending_commands_revision.wrapping_add(1).max(1);
            }
            CoreEvent::HistoryChunk {
                pane,
                start,
                total,
                offset,
                columns,
                rows,
                dictionary,
            } => self.apply_history_chunk_event(
                pane, start, total, offset, columns, rows, dictionary, cx,
            ),
            CoreEvent::KittyImageBegin {
                pane,
                image_id,
                generation,
                width,
                height,
                total_bytes,
            } => self.begin_kitty_image(pane, image_id, generation, width, height, total_bytes),
            CoreEvent::KittyImageChunk {
                pane,
                image_id,
                generation,
                bytes,
            } => self.push_kitty_image_chunk(pane, image_id, generation, &bytes),
            CoreEvent::KittyImagesRemoved { pane, image_ids } => {
                self.remove_kitty_images(pane, &image_ids);
            }
            CoreEvent::AgentUpdates {
                pane,
                first_seq,
                items,
            } => {
                self.apply_agent_updates(pane, first_seq, items);
            }
            CoreEvent::AgentStateChanged { pane, .. } => {
                cx.emit(AgentStateChanged);
                #[cfg(feature = "agent-pane")]
                if self.watched_agents.contains(&pane)
                    && let Some(state) = self.core.agent_state(pane).cloned()
                {
                    self.agent_events.states.push((pane, state));
                    if let std::collections::btree_map::Entry::Vacant(slot) =
                        self.agent_cursors.entry(pane)
                    {
                        slot.insert(0);
                        self.request_agent_replay_from_cursor(pane);
                    }
                }
                #[cfg(not(feature = "agent-pane"))]
                let _ = pane;
            }
            CoreEvent::AgentLagged { pane, next_seq } => {
                log::warn!(
                    target: "zz::diagnostics::mux",
                    "agent lane cleared for {pane} from {next_seq}; replaying from the shell's cursor"
                );
                #[cfg(feature = "agent-pane")]
                if self.watched_agents.contains(&pane) {
                    self.request_agent_replay_from_cursor(pane);
                }
            }
            CoreEvent::Message(message) => self.handle_unreduced_message(*message, cx),
            CoreEvent::KeyTableChanged => cx.emit(KeyTableChanged),
            CoreEvent::KeyTablesChanged
            | CoreEvent::HelloReceived
            | CoreEvent::ViewportChanged { .. }
            | CoreEvent::StatusChanged
            | CoreEvent::CommandOutputChanged
            | CoreEvent::ProvidersChanged
            | CoreEvent::AgentAccountsChanged
            | CoreEvent::AgentLoginChanged { .. }
            | CoreEvent::AgentReplayed { .. }
            | CoreEvent::SimulatorChanged { .. } => {}
            CoreEvent::SimulatorCommand { pane, command } => {
                self.simulator_commands.push((pane, command));
                self.pending_commands_revision =
                    self.pending_commands_revision.wrapping_add(1).max(1);
            }
            CoreEvent::AgentSessions { pane, result, .. } => {
                cx.emit(AgentCatalogReply(result.clone()));
                #[cfg(feature = "agent-pane")]
                if self.watched_agents.contains(&pane) {
                    self.agent_events.sessions.push((pane, result));
                }
                #[cfg(not(feature = "agent-pane"))]
                let _ = (pane, result);
            }
            CoreEvent::OpenPathPicker { pane, start_dir } => {
                cx.emit(PathPickerUpdate::Open { pane, start_dir });
            }
        }
    }

    pub(super) fn adopt_core_appearance(&mut self, cx: &mut Context<Self>) {
        let previous_hash = self.appearance.stable_hash();
        let host = (
            self.core.appearance().cloned().unwrap_or_default(),
            self.core.appearance_provenance().clone(),
        );
        self.attached_connection_mut().appearance = Some(host);
        let (mut appearance, provenance) = self.shown_appearance(cx);
        let configured_font_size = appearance.font_size_points;
        let requested_primary_font = appearance.font_families.first().cloned();
        localize_terminal_font_families(
            &mut appearance,
            &provenance,
            &self.available_font_names(cx),
        );
        self.terminal_font_size_offset_points =
            apply_terminal_font_size_offset(&mut appearance, self.terminal_font_size_offset_points);
        self.appearance = Arc::new(appearance);
        crate::theme::set_terminal_appearance(Arc::clone(&self.appearance), cx);
        self.command_output_diff.invalidate();
        log::info!(
            target: "zz::diagnostics::appearance",
            "reloaded appearance previous_hash={previous_hash} hash={} scheme={} requested_primary_font={requested_primary_font:?} primary_font={:?} configured_font_size_points={configured_font_size} effective_font_size_points={} font_size_offset_points={}",
            self.appearance.stable_hash(),
            self.appearance.color_scheme.as_str(),
            self.appearance.font_families.first(),
            self.appearance.font_size_points,
            self.terminal_font_size_offset_points,
        );
    }

    pub(super) fn forget_pane(&mut self, pane: PaneId) {
        self.viewports.remove(&pane);
        self.clear_kitty_images(pane);
        let connection = self.attached_connection_mut();
        connection.full_requests_pending.remove(&pane);
        connection.history.forget(pane);
        self.browser_commands.remove(&pane);
        self.terminal_commands.remove(&pane);
        #[cfg(feature = "agent-pane")]
        self.unwatch_agent(pane);
        self.pane_images.remove(&pane);
        self.pasted_image_assemblies
            .retain(|(target, _), _| *target != pane);
        self.pending_pasted_image_previews
            .retain(|(target, _)| *target != pane);
        let mut removed_screenshots = Vec::new();
        self.screenshot_requests.retain(|(target, request_id, _)| {
            let keep = *target != pane;
            if !keep {
                removed_screenshots.push(*request_id);
            }
            keep
        });
        for request_id in removed_screenshots {
            self.respond_to_request(GuiResponse::Error {
                request_id,
                message: format!("{pane} was closed"),
            });
        }
    }

    fn route_open_uri(&mut self, pane: PaneId, uri: &str, cx: &mut Context<Self>) {
        let route = open_uri_route(
            self.core.snapshot(),
            self.core.attached_session(),
            pane,
            uri,
        );
        match route {
            OpenUriRoute::Embedded { pane: browser, url } => {
                log::debug!(
                    "routing terminal link from {pane} to embedded browser {browser}: {}",
                    diagnostic_url(&url)
                );
                self.browser_commands
                    .entry(browser)
                    .or_default()
                    .push(BrowserCommand::Navigate(url));
                self.pending_commands_revision =
                    self.pending_commands_revision.wrapping_add(1).max(1);
            }
            OpenUriRoute::PastedImage { pane, number } => self.open_pasted_image(pane, number, cx),
            OpenUriRoute::External => cx.open_url(uri),
        }
    }

    fn handle_unreduced_message(&mut self, message: ProtocolMessage, cx: &mut Context<Self>) {
        match message {
            ProtocolMessage::PastedImageBegin {
                pane,
                number,
                format,
                total_bytes,
            } => self.begin_pasted_image(pane, number, format, total_bytes),
            ProtocolMessage::PastedImageChunk {
                pane,
                number,
                bytes,
            } => self.push_pasted_image_chunk(pane, number, &bytes, cx),
            ProtocolMessage::PastedImageUnavailable { pane, number } => {
                self.pasted_image_unavailable(pane, number);
            }
            ProtocolMessage::ArtifactBegin {
                request_id,
                mime,
                total_bytes,
                reply,
            } => {
                if let Some(fetch) = self.artifact_fetches.get_mut(&request_id) {
                    let total = usize::try_from(total_bytes).unwrap_or(usize::MAX);
                    fetch.bytes.reserve(total);
                    fetch.begun = Some(ArtifactBegun { mime, total, reply });
                }
                self.artifact_progress(request_id);
            }
            ProtocolMessage::ArtifactChunk { request_id, bytes } => {
                if let Some(fetch) = self.artifact_fetches.get_mut(&request_id) {
                    fetch.bytes.extend_from_slice(&bytes);
                }
                self.artifact_progress(request_id);
            }
            ProtocolMessage::ArtifactUnavailable { request_id, reason } => {
                if let Some(fetch) = self.artifact_fetches.remove(&request_id) {
                    fetch.request.respond(Err(reason));
                }
            }
            _ => {}
        }
    }

    pub(super) fn apply_terminal_viewport(
        &mut self,
        pane: PaneId,
        viewport: TerminalViewport,
        cx: &mut Context<Self>,
    ) {
        self.kitty_image_cache(pane);
        self.viewports.insert(
            pane,
            Arc::new(RwLock::new(new_retained_viewport(
                viewport,
                &mut self.next_row_revision,
            ))),
        );
        let connection = self.attached_connection_mut();
        connection.full_requests_pending.remove(&pane);
        connection.history.forget(pane);
        self.request_history_backfill(pane);
        self.signal_terminal(pane, cx);
    }

    fn apply_terminal_patch(
        &mut self,
        pane: PaneId,
        patch: TerminalViewportPatch,
        cx: &mut Context<Self>,
    ) {
        self.kitty_image_cache(pane);
        let retained = self.viewports.get(&pane).cloned();
        let request_missing =
            retained.is_none() && snapshot_contains_pane(self.core.snapshot(), pane);
        let request_failed_patch = retained.is_some();
        let apply_result = if let Some(retained) = retained {
            let mut retained = retained.write();
            if retained.row_revisions.len() == usize::from(patch.rows) {
                apply_retained_patch(&mut retained, patch, &mut self.next_row_revision).map_err(
                    |error| {
                        log::warn!("rejected terminal patch for {pane}: {error}");
                    },
                )
            } else {
                log::warn!("terminal row revisions are out of sync for {pane}");
                Err(())
            }
        } else {
            log::warn!("received terminal patch for missing pane {pane}");
            Err(())
        };
        if apply_result.is_err() && (request_failed_patch || request_missing) {
            self.request_full_viewport(pane);
        } else if apply_result.is_ok() {
            self.request_history_backfill(pane);
            self.signal_terminal(pane, cx);
        }
    }

    fn request_full_viewport(&mut self, pane: PaneId) {
        let connection = self.attached_connection_mut();
        if connection.full_requests_pending.insert(pane)
            && let Some(client) = &connection.client
            && let Err(error) = client.request_full(pane)
        {
            connection.full_requests_pending.remove(&pane);
            log::warn!("failed to request a full terminal viewport for {pane}: {error}");
        }
    }

    fn apply_command_output(&mut self, pane: PaneId, viewport: Option<TerminalViewport>) {
        let Some(viewport) = viewport else {
            let output_pane = self
                .command_output
                .as_ref()
                .map_or(pane, |output| output.pane);
            self.command_output = None;
            self.command_output_diff.invalidate();
            self.terminal_commands.remove(&output_pane);
            return;
        };
        let Some(output) = self
            .command_output
            .as_ref()
            .filter(|output| output.pane == pane)
        else {
            self.command_output_diff.invalidate();
            self.command_output = Some(CommandOutputModel {
                pane,
                retained: Arc::new(RwLock::new(new_retained_viewport(
                    viewport,
                    &mut self.next_row_revision,
                ))),
            });
            return;
        };
        let mut retained = output.retained.write();
        let patch = TerminalViewport::diff_with_scratch(
            &retained.viewport,
            &viewport,
            &mut self.command_output_diff,
        );
        if let Some(patch) = patch {
            if let Err(error) =
                apply_retained_patch(&mut retained, patch, &mut self.next_row_revision)
            {
                log::warn!("could not retain command-output rows for {pane}: {error}");
                replace_retained_viewport(&mut retained, viewport, &mut self.next_row_revision);
                self.command_output_diff.invalidate();
            } else {
                self.command_output_diff
                    .remember_applied(&retained.viewport);
            }
        } else {
            replace_retained_viewport(&mut retained, viewport, &mut self.next_row_revision);
            self.command_output_diff.invalidate();
        }
    }

    fn record_bell(&mut self, host: HostId, pane: PaneId) {
        log::debug!(target: "zz::diagnostics::mux", "bell rang host={host:?} pane={pane}");
        self.bell_revision = self.bell_revision.wrapping_add(1).max(1);
    }
}
