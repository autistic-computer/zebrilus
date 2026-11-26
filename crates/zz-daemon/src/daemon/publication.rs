use super::*;

impl Shared {
    pub(super) fn publish_key_tables_if_changed(&self) {
        let (publication, reset_clients) = {
            let mut inner = self.inner.lock();
            let ServerState {
                engine, clients, ..
            } = &mut *inner;
            let reset_clients = clients
                .iter_mut()
                .filter_map(|(id, client)| client.key_engine.as_mut().map(|engine| (id, engine)))
                .filter_map(|(client, key_engine)| {
                    key_engine
                        .active_table()
                        .is_some_and(|table| !engine.keys.has_table(table))
                        .then(|| {
                            key_engine.switch_table(None);
                            *client
                        })
                })
                .collect::<Vec<_>>();
            let mut defaults = inner
                .attachment
                .attached
                .iter()
                .filter(|(_, clients)| !clients.is_empty())
                .map(|(session, _)| inner.engine.key_table_for_session(*session))
                .collect::<BTreeSet<_>>();
            if !reset_clients.is_empty() && defaults.is_empty() {
                defaults.insert("root".to_owned());
            }
            for table in defaults {
                inner.engine.keys.ensure_table(&table);
            }
            let generation = inner.engine.keys.generation();
            let publication = if generation == inner.publication.key_tables_generation
                || timers::KeyTablePublishHold::active(self)
                || inner.clients.values().all(|c| c.subscriber.is_none())
            {
                None
            } else {
                inner.publication.key_tables_generation = generation;
                let full = inner
                    .clients
                    .iter()
                    .filter_map(|(id, client)| client.subscriber.as_ref().map(|_| id))
                    .any(|client| {
                        inner
                            .client(*client)
                            .and_then(|c| c.ctrl_subscriptions.as_ref())
                            .is_none_or(|subscription| {
                                subscription.keys == zz_protocol::KeySubscription::Full
                            })
                    });
                if full {
                    let ServerState {
                        engine,
                        publication:
                            Publication {
                                key_table_generations,
                                ..
                            },
                        ..
                    } = &mut *inner;
                    Some(
                        key_table_publication(&engine.keys, key_table_generations).unwrap_or_else(
                            || EventPayload::KeyTablesPatched {
                                tables: Vec::new(),
                                removed: Vec::new(),
                            },
                        ),
                    )
                } else {
                    Some(EventPayload::KeyTablesPatched {
                        tables: Vec::new(),
                        removed: Vec::new(),
                    })
                }
            };
            (publication, reset_clients)
        };
        for client in reset_clients {
            self.sync_key_table(client, false);
        }
        if let Some(publication) = publication {
            self.publish_key_table_changes(publication);
        }
    }

    pub(super) fn send_resync(&self, client: ClientId, outbound: &OutboundMailbox) {
        outbound.forget_delivered_terminals();
        self.send_resync_inner(client, outbound, None);
    }

    pub(super) fn send_resync_inner(
        &self,
        client: ClientId,
        outbound: &OutboundMailbox,
        skip_command_output: Option<u64>,
    ) {
        self.send_resync_as(
            client,
            outbound,
            skip_command_output,
            attach::ResyncScope::Full,
        );
    }

    pub(super) fn send_resync_as(
        &self,
        client: ClientId,
        outbound: &OutboundMailbox,
        skip_command_output: Option<u64>,
        scope: attach::ResyncScope,
    ) {
        if self.read_client(client, |c| {
            c.is_some_and(|c| c.ctrl_subscriptions.is_some())
        }) {
            outbound.collect_control_attach();
            self.send_compact_resync(client, outbound, scope.sends_everything());
            outbound.flush_control_batch(false);
            return;
        }
        let everything = scope.sends_everything();
        let order = everything.then(|| self.snapshot_order.lock());
        let (
            snapshot,
            viewports,
            foreground_panes,
            command_prompt,
            choose_tree,
            choose_buffer,
            chooser_presentation,
            display_panes,
            menu,
            confirm,
        ) = {
            let mut inner = self.inner.lock();
            let snapshot = everything.then(|| {
                let mut snapshot = inner.engine.state.snapshot();
                let presence = snapshot_presence(&inner);
                stamp_snapshot_for_client(&inner, client, &mut snapshot, &presence);
                note_snapshot_sent(&mut inner, client, &snapshot);
                snapshot
            });
            let command_prompt = command_prompt_state(&inner, client);
            let chooser_shown = chooser_shown(&inner, client);
            if let Some(entry) = inner.client_mut(client) {
                entry.published_chooser = Default::default();
            }
            let choose_tree = inner
                .client(client)
                .and_then(|c| c.choose_tree.as_ref())
                .filter(|_| chooser_shown)
                .map(|chooser| chooser.rendered.clone());
            let choose_buffer = inner
                .client(client)
                .and_then(|c| c.choose_buffer.as_ref())
                .filter(|_| chooser_shown)
                .map(|chooser| chooser.rendered.clone());
            let display_panes = inner
                .client(client)
                .and_then(|c| c.display_panes.as_ref())
                .map(|overlay| overlay.state.clone());
            let menu = inner
                .client(client)
                .and_then(|c| c.menu.as_ref())
                .map(|menu| menu.state.clone());
            let confirm = inner
                .client(client)
                .and_then(|c| c.confirm.as_ref())
                .map(|confirm| confirm.state.clone());
            let view = TerminalViewId(client.0);
            let viewports = if client_terminal_publication_frozen(&inner, client) {
                Vec::new()
            } else {
                inner
                    .client(client)
                    .and_then(|c| c.streamed_terminals.as_ref())
                    .map_or_else(Vec::new, |streamed| {
                        streamed
                            .iter()
                            .filter_map(|(pane, kind)| {
                                terminal_viewport_for_pane(&inner, *pane, view)
                                    .filter(|(terminal, _)| {
                                        everything
                                            || !attach::attach_frame_superseded(
                                                &inner, *pane, terminal,
                                            )
                                    })
                                    .map(|(terminal, viewport)| (*pane, *kind, terminal, viewport))
                            })
                            .collect()
                    })
            };
            let foreground_panes = inner
                .client(client)
                .and_then(|c| c.visible_terminals.as_ref())
                .map_or(0, BTreeSet::len);
            (
                snapshot,
                viewports,
                foreground_panes,
                command_prompt,
                choose_tree,
                choose_buffer,
                chooser_presentation::chooser_presentation(&inner, client).map(Box::new),
                display_panes,
                menu,
                confirm,
            )
        };
        if let Some(snapshot) = snapshot {
            Self::send_event(outbound, EventPayload::Snapshot(snapshot));
        }
        drop(order);
        if everything || command_prompt.is_some() {
            Self::send_event(
                outbound,
                EventPayload::CommandPrompt {
                    state: command_prompt,
                },
            );
        }
        if everything || menu.is_some() {
            Self::send_event(outbound, EventPayload::Menu { state: menu });
        }
        if everything || confirm.is_some() {
            Self::send_event(outbound, EventPayload::Confirm { state: confirm });
        }
        if everything || choose_tree.is_some() {
            Self::send_event(outbound, EventPayload::ChooseTree { state: choose_tree });
        }
        if everything || choose_buffer.is_some() {
            Self::send_event(
                outbound,
                EventPayload::ChooseBuffer {
                    state: choose_buffer,
                },
            );
        }
        if everything || chooser_presentation.is_some() {
            Self::send_event(
                outbound,
                EventPayload::ChooserPresentation {
                    presentation: chooser_presentation,
                },
            );
        }
        if everything || display_panes.is_some() {
            Self::send_event(
                outbound,
                EventPayload::DisplayPanes {
                    state: display_panes,
                },
            );
        }
        for (pane, kind, terminal, viewport) in viewports {
            if kind == TerminalStreamKind::Foreground
                && !self.enqueue_kitty_images_for_viewport(outbound, pane, &terminal, &viewport)
            {
                continue;
            }
            let frames = &self.terminal_frames;
            match kind {
                TerminalStreamKind::Foreground => {
                    if outbound.enqueue_terminal_viewport(
                        pane,
                        &viewport,
                        TerminalDelivery::Foreground,
                        frames,
                    ) == TerminalEnqueue::NeedsFull
                    {
                        let _ = outbound.replace_terminal_viewport(pane, &viewport, frames);
                    }
                }
                TerminalStreamKind::Preview => {
                    let _ = outbound.enqueue_terminal_viewport(
                        pane,
                        &viewport,
                        TerminalDelivery::Preview { foreground_panes },
                        frames,
                    );
                }
            }
        }
        #[cfg(feature = "agent")]
        self.send_agent_resync(client, outbound);
        self.send_simulator_resync(outbound);
        let inner = self.inner.lock();
        if let Some(output) = inner.client(client).and_then(|c| c.command_output.as_ref()) {
            if skip_command_output == Some(output.output_id) {
                return;
            }
            if let Some(viewport) = output
                .terminal
                .latest_viewport_for(TerminalViewId(client.0))
            {
                let message = Self::event(EventPayload::CommandOutput {
                    output_id: output.output_id,
                    pane: output.pane,
                    viewport: Some((*viewport).clone()),
                });
                let _ = outbound.replace_command_output(&message);
            }
        } else if everything {
            Self::send_event(
                outbound,
                EventPayload::CommandOutput {
                    output_id: 0,
                    pane: client_context_pane(&inner, client).unwrap_or(PaneId(0)),
                    viewport: None,
                },
            );
        }
    }

    pub(super) fn send_preview_refreshes(&self, client: ClientId, outbound: &OutboundMailbox) {
        for pane in outbound.take_preview_refreshes() {
            self.send_full(client, pane, outbound);
        }
    }

    pub(super) fn send_full(&self, client: ClientId, pane: PaneId, outbound: &OutboundMailbox) {
        if client_terminal_publication_frozen(&self.inner.lock(), client) {
            return;
        }
        let viewport = {
            let inner = self.inner.lock();
            let Some(kind) = inner
                .client(client)
                .and_then(|c| c.streamed_terminals.as_ref())
                .and_then(|streamed| streamed.get(&pane))
                .copied()
            else {
                return;
            };
            let foreground_panes = if kind == TerminalStreamKind::Preview {
                inner
                    .client(client)
                    .and_then(|c| c.visible_terminals.as_ref())
                    .map_or(0, BTreeSet::len)
            } else {
                0
            };
            let view = TerminalViewId(client.0);
            terminal_viewport_for_pane(&inner, pane, view)
                .map(|(terminal, viewport)| (kind, foreground_panes, terminal, viewport))
        };
        if let Some((kind, foreground_panes, terminal, viewport)) = viewport {
            match kind {
                TerminalStreamKind::Foreground => {
                    if !self.enqueue_kitty_images_for_viewport(outbound, pane, &terminal, &viewport)
                    {
                        return;
                    }
                    let _ = outbound.replace_terminal_viewport(
                        pane,
                        viewport.as_ref(),
                        &self.terminal_frames,
                    );
                }
                TerminalStreamKind::Preview => {
                    outbound.suspend_terminal(pane);
                    let _ = outbound.enqueue_terminal_viewport(
                        pane,
                        viewport.as_ref(),
                        TerminalDelivery::Preview { foreground_panes },
                        &self.terminal_frames,
                    );
                }
            }
        }
    }

    pub(super) fn send_history(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
        start: u32,
        count: u32,
        outbound: &Arc<OutboundMailbox>,
    ) {
        let terminal = {
            let inner = self.inner.lock();
            let Some(session) = client_attached_session(&inner, client) else {
                return;
            };
            if !visible_terminal_panes(&inner, client, session).contains(&pane) {
                return;
            }
            inner.terminals.get(&pane).cloned()
        };
        let Some(terminal) = terminal else {
            return;
        };
        let request = terminal.history_request(
            start,
            count.min(MAX_HISTORY_CHUNK_ROWS),
            self.terminal_requests.notifier(),
        );
        let outbound = Arc::clone(outbound);
        let continuation = cmdq::WaitContinuation::new(None, None);
        let done = continuation.clone();
        self.terminal_requests
            .submit(request, move |shared, result| {
                let valid = {
                    let inner = shared.inner.lock();
                    inner.client(client).is_some()
                        && inner
                            .terminals
                            .get(&pane)
                            .is_some_and(|current| Arc::ptr_eq(current, &terminal))
                        && client_attached_session(&inner, client).is_some_and(|session| {
                            visible_terminal_panes(&inner, client, session).contains(&pane)
                        })
                };
                if valid && let Ok(Ok((start, rows, dictionary, scrollbar, columns))) = result {
                    Self::send_event(
                        &outbound,
                        EventPayload::HistoryChunk {
                            pane,
                            start,
                            total: scrollbar.total,
                            offset: scrollbar.offset,
                            columns,
                            rows,
                            dictionary,
                        },
                    );
                }
                done.complete();
            });
        self.terminal_requests.finish_off_loop(self, &continuation);
    }

    pub(super) fn publish_snapshot(self: &Arc<Self>) {
        self.detach_removed_sessions();
        self.publish_snapshot_state();
    }

    pub(super) fn publish_snapshot_state(&self) {
        self.publish_snapshot_state_except(None);
    }

    pub(super) fn publish_snapshot_after_detach(self: &Arc<Self>, client: ClientId) {
        self.detach_removed_sessions();
        self.status.lock().forget(client);
        self.publish_snapshot_state_except(Some(client));
    }

    fn publish_snapshot_state_except(&self, detached: Option<ClientId>) {
        self.note_published();
        self.publish_mux_snapshots_except(true, true, detached);
        self.refresh_terminal_visibility();
        #[cfg(feature = "agent")]
        self.refresh_agent_visibility();
        self.refresh_choose_trees();
        self.refresh_choose_buffers();
        self.refresh_display_panes();
    }

    pub(super) fn publish_mux_snapshots(&self) {
        self.publish_mux_snapshots_as(true, false);
    }

    pub(super) fn publish_mux_labels(&self) {
        self.publish_mux_snapshots_as(false, false);
    }

    pub(super) fn publish_mux_snapshots_as(&self, owns_generation: bool, with_status: bool) {
        self.publish_mux_snapshots_except(owns_generation, with_status, None);
    }

    pub(super) fn publish_mux_snapshots_except(
        &self,
        owns_generation: bool,
        with_status: bool,
        detached: Option<ClientId>,
    ) {
        let startup_ready = with_status.then(|| *self.startup_ready.lock());
        let order = self.snapshot_order.lock();
        let mut border_cycle_started = false;
        let (snapshots, appearance_updates, requests, simulators) = {
            let mut inner = self.inner.lock();
            let ServerState {
                engine,
                attachment:
                    Attachment {
                        window_latest_clients,
                        ..
                    },
                ..
            } = &mut *inner;
            window_latest_clients.retain(|window, _| engine.state.windows.contains_key(window));
            release_chooser_zooms(&mut inner);
            publish_agent_parents(&mut inner);
            let simulators = changed_simulator_panes(&mut inner);
            let generation = inner.engine.state.generation();
            let tracked =
                owns_generation || inner.publication.last_published_mux_generation == generation;
            if tracked {
                inner.publication.last_published_mux_generation = generation;
            }
            let appearance_updates = if inner.engine.has_window_style_settings() {
                terminal_appearance_updates(&inner)
            } else {
                Vec::new()
            };
            let mut targets = startup_ready
                .map(|_| status_targets(&inner, None, None))
                .unwrap_or_default();
            targets.retain(|client| {
                Some(*client) != detached
                    && !inner.client(*client).is_some_and(|c| c.ctrl_initializing)
            });
            if inner.clients.values().all(|c| c.subscriber.is_none()) {
                (Vec::new(), appearance_updates, Vec::new(), simulators)
            } else {
                if !targets.is_empty() {
                    inner.engine.set_format_now(unix_timestamp());
                }
                let snapshot = inner.engine.state.snapshot();
                let facts = format_hook_facts(&inner);
                let cycled = inner.border_cycling();
                let snapshots = stamped_snapshot_sends(&mut inner, tracked, &snapshot, &facts);
                border_cycle_started = !cycled && inner.border_cycling();
                let requests = status_requests(
                    &inner,
                    targets,
                    &snapshot,
                    &facts,
                    startup_ready.unwrap_or_default(),
                    &self.status_job_needs,
                );
                (snapshots, appearance_updates, requests, simulators)
            }
        };
        for (terminal, appearance) in appearance_updates {
            terminal.set_appearance(appearance);
        }
        for (subscriber, snapshot) in snapshots {
            Self::send_event(&subscriber, EventPayload::Snapshot(snapshot));
        }
        self.publish_compact_trees();
        for (pane, descriptor) in simulators {
            self.publish(EventPayload::SimulatorPane { pane, descriptor });
        }
        drop(order);
        self.publish_status_requests(&requests);
        if border_cycle_started {
            self.nudge_client_timers();
        }
    }

    pub(super) fn set_terminal_preview(&self, client: ClientId, kind: ClientKind, enabled: bool) {
        if kind != ClientKind::Interactive {
            return;
        }
        let changed = {
            let mut inner = self.inner.lock();
            if client_attached_session(&inner, client).is_none() {
                let _ = inner
                    .client_mut(client)
                    .is_some_and(|client| std::mem::take(&mut client.terminal_preview));
                return;
            }
            if enabled {
                !std::mem::replace(&mut inner.client_entry(client).terminal_preview, true)
            } else {
                inner
                    .client_mut(client)
                    .is_some_and(|client| std::mem::take(&mut client.terminal_preview))
            }
        };
        if changed {
            self.refresh_terminal_visibility();
        }
    }

    pub(super) fn refresh_terminal_visibility(&self) {
        let (changes, resizes, layout_changed) = {
            let mut inner = self.inner.lock();
            sync_control_routes(&mut inner, &self.control_wake);
            let attachments = inner
                .attachment
                .attached
                .iter()
                .flat_map(|(session, clients)| clients.iter().map(|client| (*session, *client)))
                .collect::<Vec<_>>();
            let mut changes = Vec::new();
            let mut affected_panes = BTreeSet::new();
            for &(session, client) in &attachments {
                let next_visible = visible_terminal_panes(&inner, client, session);
                let next_streamed = streamed_terminal_panes(&inner, client, &next_visible);
                let previous_visible = inner
                    .client(client)
                    .and_then(|c| c.visible_terminals.as_ref())
                    .cloned()
                    .unwrap_or_default();
                let previous_streamed = inner
                    .client(client)
                    .and_then(|c| c.streamed_terminals.as_ref())
                    .cloned()
                    .unwrap_or_default();
                if next_visible == previous_visible && next_streamed == previous_streamed {
                    continue;
                }
                affected_panes.extend(previous_visible.iter().copied());
                affected_panes.extend(next_visible.iter().copied());
                let removed = previous_streamed
                    .keys()
                    .filter(|pane| !next_streamed.contains_key(pane))
                    .copied()
                    .collect::<Vec<_>>();
                let view = TerminalViewId(client.0);
                apply_view_streams(&inner, view, &previous_streamed, &next_streamed);
                let viewport_for = |pane: &PaneId| {
                    terminal_viewport_for_pane(&inner, *pane, view)
                        .map(|(terminal, viewport)| (*pane, terminal, viewport))
                };
                let newly_streamed = next_streamed
                    .iter()
                    .filter(|(pane, _)| !previous_streamed.contains_key(pane))
                    .filter_map(|(pane, kind)| {
                        viewport_for(pane)
                            .map(|(pane, terminal, viewport)| (pane, *kind, terminal, viewport))
                    })
                    .collect::<Vec<_>>();
                let newly_foreground = next_streamed
                    .iter()
                    .filter(|(pane, kind)| {
                        **kind == TerminalStreamKind::Foreground
                            && previous_streamed.get(pane) == Some(&TerminalStreamKind::Preview)
                    })
                    .filter_map(|(pane, _)| viewport_for(pane))
                    .collect::<Vec<_>>();
                if let Some(subscriber) = inner
                    .client(client)
                    .and_then(|c| c.subscriber.as_ref())
                    .cloned()
                {
                    changes.push((
                        subscriber,
                        removed,
                        newly_streamed,
                        newly_foreground,
                        next_visible.len(),
                    ));
                }
                inner
                    .client_entry(client)
                    .visible_terminals
                    .replace(next_visible);
                inner
                    .client_entry(client)
                    .streamed_terminals
                    .replace(next_streamed);
            }
            refresh_preview_watches(&mut inner);
            forget_floating_pane_reports(&mut inner);
            let layout_changed = write_back_terminal_geometries(&mut inner, &affected_panes);
            for (_, client) in attachments {
                sync_client_view_areas(&inner, client);
            }
            affected_panes.extend(inner.engine.state.windows.values().flat_map(|window| {
                window
                    .panes
                    .keys()
                    .copied()
                    .filter(|pane| window.is_floating(*pane))
            }));
            let resizes = terminal_resizes_for_panes(&inner, &affected_panes);
            (changes, resizes, layout_changed)
        };

        for (subscriber, removed, newly_streamed, newly_foreground, foreground_panes) in changes {
            for pane in removed {
                subscriber.suspend_terminal(pane);
            }
            for (pane, terminal, viewport) in newly_foreground {
                if !self.enqueue_kitty_images_for_viewport(&subscriber, pane, &terminal, &viewport)
                {
                    continue;
                }
                let frames = &self.terminal_frames;
                if subscriber.enqueue_terminal_viewport(
                    pane,
                    &viewport,
                    TerminalDelivery::Foreground,
                    frames,
                ) == TerminalEnqueue::NeedsFull
                {
                    let _ = subscriber.replace_terminal_viewport(pane, &viewport, frames);
                }
            }
            for (pane, kind, terminal, viewport) in newly_streamed {
                if kind == TerminalStreamKind::Foreground
                    && !self.enqueue_kitty_images_for_viewport(
                        &subscriber,
                        pane,
                        &terminal,
                        &viewport,
                    )
                {
                    continue;
                }
                let frames = &self.terminal_frames;
                match kind {
                    TerminalStreamKind::Foreground => {
                        if subscriber.enqueue_terminal_viewport(
                            pane,
                            &viewport,
                            TerminalDelivery::Foreground,
                            frames,
                        ) == TerminalEnqueue::NeedsFull
                        {
                            let _ = subscriber.replace_terminal_viewport(pane, &viewport, frames);
                        }
                    }
                    TerminalStreamKind::Preview => {
                        let _ = subscriber.enqueue_terminal_viewport(
                            pane,
                            &viewport,
                            TerminalDelivery::Preview { foreground_panes },
                            frames,
                        );
                    }
                }
            }
        }
        apply_terminal_resizes(resizes);
        if layout_changed {
            self.publish_mux_snapshots();
        }
    }

    pub(super) fn refresh_chooser_previews(&self) {
        let clients = self
            .inner
            .lock()
            .clients
            .iter()
            .filter_map(|(id, client)| client.choose_tree.as_ref().map(|_| id))
            .copied()
            .collect::<Vec<_>>();
        for client in clients {
            self.publish_chooser_presentation(client);
        }
    }

    pub(super) fn publish(&self, payload: EventPayload) {
        if let EventPayload::PaneRemoved(pane) = &payload {
            self.kitty_image_frames
                .lock()
                .retain(|key, _| key.pane != *pane);
            self.pasted_images.lock().remove(pane);
        }
        let message = Self::event(payload);
        let subscribers = self
            .inner
            .lock()
            .clients
            .values()
            .filter_map(|c| c.subscriber.as_ref())
            .cloned()
            .collect::<Vec<_>>();
        let Ok(encoded) = zz_protocol::encode_protocol_message(&message).map(Arc::<[u8]>::from)
        else {
            return;
        };
        for subscriber in subscribers {
            let _ = subscriber.enqueue_reliable_frame(&message, Arc::clone(&encoded).into(), true);
        }
    }

    pub(super) fn publish_to_control_clients(
        &self,
        payload: EventPayload,
        exclude: Option<ClientId>,
        attached_only: bool,
    ) {
        let layout_hook = matches!(&payload, EventPayload::HookEvent { name, .. } if name == "window-layout-changed");
        let _order = layout_hook.then(|| self.snapshot_order.lock());
        if layout_hook {
            self.publish_compact_trees();
        }
        let capture_item = matches!(&payload, EventPayload::HookEvent { .. })
            .then(|| self.command_item.as_ref().map(|item| item.lock().id))
            .flatten();
        let subscribers = {
            let mut inner = self.inner.lock();
            let recipients = inner
                .clients
                .iter()
                .filter(|(client, state)| {
                    state.subscriber.is_some()
                        && inner.client(**client).and_then(|c| c.kind) == Some(ClientKind::Control)
                        && Some(**client) != exclude
                        && (!attached_only
                            || inner
                                .attachment
                                .attached
                                .values()
                                .any(|attached| attached.contains(client)))
                })
                .map(|(client, _)| *client)
                .collect::<Vec<_>>();
            let mut subscribers = Vec::with_capacity(recipients.len());
            for client in recipients {
                if let Some(item) = capture_item
                    && let Some(capture) = inner
                        .waits
                        .control_command_event_captures
                        .get_mut(&(client, item))
                        .and_then(|captures| captures.last_mut())
                {
                    capture.events.push(payload.clone());
                    continue;
                }
                if let Some(subscriber) = inner.client(client).and_then(|c| c.subscriber.as_ref()) {
                    subscribers.push(Arc::clone(subscriber));
                }
            }
            subscribers
        };
        if subscribers.is_empty() {
            return;
        }
        let message = Self::event(payload);
        let Ok(encoded) = zz_protocol::encode_protocol_message(&message).map(Arc::<[u8]>::from)
        else {
            return;
        };
        for subscriber in subscribers {
            let _ = subscriber.enqueue_reliable_frame(&message, Arc::clone(&encoded).into(), true);
        }
    }

    pub(super) fn publish_to_client(&self, client: ClientId, payload: EventPayload) {
        self.publish_to_client_with_callback_parse_event(client, payload, None);
    }

    pub(super) fn publish_callback_parse_event(
        &self,
        client: ClientId,
        payload: EventPayload,
        callback_parse_event: Arc<()>,
    ) {
        self.publish_to_client_with_callback_parse_event(
            client,
            payload,
            Some(callback_parse_event),
        );
    }

    pub(super) fn publish_to_client_with_callback_parse_event(
        &self,
        client: ClientId,
        payload: EventPayload,
        callback_parse_event: Option<Arc<()>>,
    ) {
        if let Some(slot) = published_chooser_slot(&payload) {
            let mut inner = self.inner.lock();
            if let Some(entry) = inner.client_mut(client) {
                if entry.published_chooser[slot].as_ref() == Some(&payload) {
                    return;
                }
                entry.published_chooser[slot] = Some(payload.clone());
                if slot != PUBLISHED_PRESENTATION_SLOT {
                    entry.published_chooser[PUBLISHED_PRESENTATION_SLOT] = None;
                }
            }
        }
        if matches!(
            &payload,
            EventPayload::ControlCommandGuard { .. }
                | EventPayload::ControlCommandGuardRaw { .. }
                | EventPayload::HookEvent { .. }
                | EventPayload::PaneOutputState { .. }
                | EventPayload::ControlSourceFile { .. }
                | EventPayload::ControlCommandOutput { .. }
                | EventPayload::ControlConfigError { .. }
                | EventPayload::ClientMessage { .. }
                | EventPayload::ServerStopping
        ) || matches!(&payload, EventPayload::ControlExit { reason } if reason.is_empty())
        {
            let capture_key = self
                .command_item
                .as_ref()
                .map(|item| (client, item.lock().id));
            let mut inner = self.inner.lock();
            if let Some(capture) = inner
                .waits
                .control_command_event_captures
                .get_mut(&capture_key.unwrap_or((client, cmdq::ItemId::NONE)))
                .and_then(|captures| captures.last_mut())
            {
                if let Some(callback_parse_event) = callback_parse_event {
                    capture
                        .callback_parse_events
                        .push((capture.events.len(), callback_parse_event));
                }
                capture.events.push(payload);
                return;
            }
        }
        let subscriber =
            self.read_client(client, |c| c.and_then(|c| c.subscriber.as_ref()).cloned());
        if let Some(subscriber) = subscriber {
            Self::send_event(&subscriber, payload);
        }
    }

    pub(super) fn publish_owned_client_message(&self, publication: OwnedClientMessagePublication) {
        let OwnedClientMessagePublication {
            client,
            pane,
            kind,
            text,
            duration_ms,
            message_id,
        } = publication;
        let inner = self.inner.lock();
        if inner
            .client(client)
            .and_then(|c| c.message.as_ref())
            .is_none_or(|message| message.token != message_id)
        {
            return;
        }
        let Some(subscriber) = inner.client(client).and_then(|c| c.subscriber.as_ref()) else {
            return;
        };
        Self::send_event(
            subscriber,
            EventPayload::TimedClientMessage {
                pane,
                kind,
                text,
                duration_ms,
                message_id,
            },
        );
    }

    pub(super) fn publish_deferred_client_event(&self, event: DeferredClientEvent) {
        match event {
            DeferredClientEvent::Unowned { client, payload } => {
                self.publish_to_client(client, *payload);
            }
            DeferredClientEvent::Owned(publication) => {
                self.publish_owned_client_message(publication);
            }
        }
    }

    pub(super) fn control_client_is_connected(&self, client: ClientId) -> bool {
        let inner = self.inner.lock();
        inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Control)
            && inner.client(client).is_some_and(|c| c.subscriber.is_some())
    }

    pub(super) fn publish_terminal_negotiation(&self) {
        let recipients = {
            let mut inner = self.inner.lock();
            let clients = inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.subscriber.as_ref().map(|_| *id))
                .collect::<Vec<_>>();
            clients
                .into_iter()
                .filter_map(|client| {
                    take_terminal_negotiation(&mut inner, client).map(|payload| (client, payload))
                })
                .collect::<Vec<_>>()
        };
        for (client, payload) in recipients {
            self.publish_to_client(client, payload);
        }
    }

    pub(super) fn publish_terminal_negotiation_to(
        &self,
        client: ClientId,
        outbound: &Arc<OutboundMailbox>,
    ) -> bool {
        let mut inner = self.inner.lock();
        let Some(payload) = take_terminal_negotiation(&mut inner, client) else {
            return outbound.is_open();
        };
        outbound.enqueue_reliable(&Self::event(payload))
    }

    pub(super) fn publish_for_pane(&self, pane: PaneId, payload: &EventPayload) {
        let subscribers = {
            let inner = self.inner.lock();
            let Some(window) = inner.engine.state.window_for_pane(pane) else {
                return;
            };
            let session = inner.engine.state.windows[&window].session;
            inner
                .attachment
                .attached
                .get(&session)
                .into_iter()
                .flatten()
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

    pub(super) fn publish_terminal_for_pane(
        self: &Arc<Self>,
        pane: PaneId,
        client: ClientId,
        base: Option<&TerminalViewport>,
        current: &TerminalViewport,
        terminal: &Arc<TerminalSession>,
        fanout: &mut PaneFrameFanout,
    ) {
        let (subscriber, kind, foreground_panes, unclaimed, mode_event, kill_pending) = {
            let mut inner = self.inner.lock();
            let (unclaimed, mode_changed) =
                reconcile_copy_session(&mut inner, pane, client, current.mode);
            let kill_pending = !inner.modes.copy_kill_panes.is_empty();
            let attached = inner
                .engine
                .state
                .window_for_pane(pane)
                .map(|window| inner.engine.state.windows[&window].session)
                .is_some_and(|session| {
                    inner
                        .attachment
                        .attached
                        .get(&session)
                        .is_some_and(|clients| clients.contains(&client))
                });
            let kind = (!client_terminal_publication_frozen(&inner, client))
                .then(|| {
                    inner
                        .client(client)
                        .and_then(|c| c.streamed_terminals.as_ref())
                        .and_then(|streamed| streamed.get(&pane))
                        .copied()
                })
                .flatten()
                .filter(|kind| attached || *kind == TerminalStreamKind::Preview);
            let subscriber = kind.and_then(|_| {
                inner
                    .client(client)
                    .and_then(|c| c.subscriber.as_ref())
                    .cloned()
            });
            let foreground_panes = if kind == Some(TerminalStreamKind::Preview) {
                inner
                    .client(client)
                    .and_then(|c| c.visible_terminals.as_ref())
                    .map_or(0, BTreeSet::len)
            } else {
                0
            };
            let mode_event = mode_changed
                .then(|| hook_events::copy_mode_exit_hook_events(pane, &inner.engine))
                .filter(|events| !events.is_empty());
            (
                subscriber,
                kind,
                foreground_panes,
                unclaimed,
                mode_event,
                kill_pending,
            )
        };
        if let Some(terminal) = unclaimed {
            terminal.view_action(
                TerminalViewId(client.0),
                zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
            );
        }
        if kill_pending {
            if let Some(effects) = &self.watcher_effects {
                effects
                    .lock()
                    .push(Box::new(Shared::reap_copy_mode_kill_panes));
            } else {
                self.reap_copy_mode_kill_panes();
            }
        }
        if let Some(subscriber) = subscriber {
            let kind = kind.expect("a terminal subscriber has a stream kind");
            if kind == TerminalStreamKind::Foreground
                && !self.enqueue_kitty_images_for_viewport(&subscriber, pane, terminal, current)
            {
                return;
            }
            let delivery = match kind {
                TerminalStreamKind::Foreground => TerminalDelivery::Foreground,
                TerminalStreamKind::Preview => TerminalDelivery::Preview { foreground_panes },
            };
            let frames = &self.terminal_frames;
            let result = fanout.enqueue(&subscriber, frames, pane, base, current, delivery);
            if result == TerminalEnqueue::NeedsFull {
                match kind {
                    TerminalStreamKind::Foreground => {
                        let _ = subscriber.replace_terminal_viewport(pane, current, frames);
                    }
                    TerminalStreamKind::Preview => {
                        let _ =
                            subscriber.enqueue_terminal_viewport(pane, current, delivery, frames);
                    }
                }
            }
        }
        if let Some(events) = mode_event {
            self.nudge_client_timers();
            self.run_event_hooks(events);
        }
    }

    pub(super) fn resume_client_terminals(self: &Arc<Self>, client: ClientId) {
        let (outbound, panes) = {
            let inner = self.inner.lock();
            sync_terminal_freeze(&inner, client);
            if client_terminal_publication_frozen(&inner, client) {
                return;
            }
            let Some(outbound) = inner
                .client(client)
                .and_then(|c| c.subscriber.as_ref())
                .cloned()
            else {
                return;
            };
            let panes = inner
                .client(client)
                .and_then(|c| c.streamed_terminals.as_ref())
                .map(|streamed| streamed.keys().copied().collect::<BTreeSet<_>>())
                .unwrap_or_default();
            (outbound, panes)
        };
        for pane in panes {
            self.send_full(client, pane, &outbound);
        }
    }

    pub(super) fn next_sequence() -> u64 {
        static SEQUENCE: AtomicU64 = AtomicU64::new(1);
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    }

    pub(super) fn event(payload: EventPayload) -> ProtocolMessage {
        ProtocolMessage::Event(Event {
            sequence: Self::next_sequence(),
            payload,
        })
    }

    pub(super) fn send_event(outbound: &OutboundMailbox, payload: EventPayload) {
        let _ = outbound.enqueue_reliable(&Self::event(payload));
    }
}

pub(super) const MAX_HISTORY_CHUNK_ROWS: u32 = 512;

fn key_table_publication(
    keys: &zz_protocol::KeyTables,
    published: &mut BTreeMap<String, u64>,
) -> Option<EventPayload> {
    let changed = keys
        .table_generations()
        .filter(|(name, generation)| published.get(*name) != Some(generation))
        .collect::<Vec<_>>();
    let removed = published
        .keys()
        .filter(|name| !keys.has_table(name))
        .cloned()
        .collect::<Vec<_>>();
    if changed.is_empty() && removed.is_empty() {
        return None;
    }
    for name in &removed {
        published.remove(name);
    }
    for (name, generation) in &changed {
        if let Some(current) = published.get_mut(*name) {
            *current = *generation;
        } else {
            published.insert((*name).to_owned(), *generation);
        }
    }
    Some(EventPayload::KeyTablesPatched {
        tables: changed
            .into_iter()
            .filter_map(|(name, _)| keys.snapshot_table(name))
            .collect(),
        removed,
    })
}

fn release_chooser_zooms(inner: &mut ServerState) {
    let released = inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.chooser_zoom.as_ref().map(|value| (id, value)))
        .filter(|(client, _)| {
            inner
                .client(**client)
                .is_none_or(|c| c.choose_tree.is_none())
                && inner
                    .client(**client)
                    .is_none_or(|c| c.choose_buffer.is_none())
        })
        .map(|(client, window)| (*client, *window))
        .collect::<Vec<_>>();
    for (client, window) in released {
        inner.client_mut(client).and_then(|c| c.chooser_zoom.take());
        if let Some(pane) = inner
            .engine
            .state
            .windows
            .get(&window)
            .and_then(|window| window.zoomed_pane)
        {
            let _ = inner.engine.state.toggle_zoom(pane);
        }
    }
}

const PUBLISHED_PRESENTATION_SLOT: usize = 2;

fn published_chooser_slot(payload: &EventPayload) -> Option<usize> {
    match payload {
        EventPayload::ChooseTree { .. } => Some(0),
        EventPayload::ChooseBuffer { .. } => Some(1),
        EventPayload::ChooserPresentation { .. } => Some(PUBLISHED_PRESENTATION_SLOT),
        _ => None,
    }
}

pub(super) fn reconcile_copy_session(
    inner: &mut ServerState,
    pane: PaneId,
    client: ClientId,
    mode: TerminalMode,
) -> (Option<Arc<TerminalSession>>, bool) {
    match (
        mode,
        inner
            .client_mut(client)
            .and_then(|c| c.copy_session.as_mut()),
    ) {
        (TerminalMode::Copy { .. }, Some(session)) if session.pane == pane => {
            if !session.exiting {
                session.observed = true;
            }
            (None, false)
        }
        (TerminalMode::Copy { .. }, _) => (inner.terminals.get(&pane).cloned(), false),
        (TerminalMode::Live, Some(session))
            if session.pane == pane
                && (session.observed || session.scroll_exit || session.exiting) =>
        {
            let kill = session.kill;
            let announced = session.exiting;
            exit_copy_session(inner, client);
            if kill {
                inner.modes.copy_kill_panes.push(pane);
            }
            (None, !announced)
        }
        _ => (None, false),
    }
}

fn status_requests(
    inner: &ServerState,
    targets: Vec<ClientId>,
    snapshot: &MuxSnapshot,
    facts: &FormatHookFacts,
    startup_ready: bool,
    job_needs: &crate::status::StatusJobNeeds,
) -> Vec<StatusRequest> {
    if targets.is_empty() {
        return Vec::new();
    }
    let option_snapshot = inner.engine.cached_format_option_snapshot();
    let job_needs = job_needs.lock();
    let mut line_needs = BTreeMap::new();
    targets
        .into_iter()
        .map(|client| {
            status_request_with(
                inner,
                client,
                snapshot,
                option_snapshot.clone(),
                facts.clone(),
                startup_ready,
                job_needs.get(&client).copied().unwrap_or_default(),
                &mut line_needs,
            )
        })
        .collect()
}

fn note_snapshot_sent(inner: &mut ServerState, client: ClientId, snapshot: &MuxSnapshot) {
    inner
        .client_entry(client)
        .published_snapshot
        .replace((snapshot.content_digest(), snapshot.generation));
}

fn changed_simulator_panes(inner: &mut ServerState) -> Vec<(PaneId, SimulatorDescriptor)> {
    let current = simulator_panes(inner);
    let changed = current
        .iter()
        .filter(|(pane, descriptor)| {
            inner.artifact_state.published_simulators.get(pane) != Some(descriptor)
        })
        .map(|(pane, descriptor)| (*pane, descriptor.clone()))
        .collect();
    inner.artifact_state.published_simulators = current;
    changed
}

fn stamped_snapshot_sends(
    inner: &mut ServerState,
    tracked: bool,
    snapshot: &MuxSnapshot,
    facts: &FormatHookFacts,
) -> Vec<(Arc<OutboundMailbox>, MuxSnapshot)> {
    let presence = snapshot_presence(inner);
    let mut sends = Vec::with_capacity(
        inner
            .clients
            .values()
            .filter(|c| c.subscriber.is_some())
            .count(),
    );
    let mut unchanged = Vec::new();
    let mut restamp = false;
    for (client, subscriber) in inner.clients.iter().filter_map(|(id, client)| {
        client
            .subscriber
            .as_ref()
            .map(|subscriber| (id, subscriber))
    }) {
        if inner
            .client(*client)
            .is_some_and(|c| c.ctrl_subscriptions.is_some())
        {
            continue;
        }
        let mut client_snapshot = snapshot.clone();
        stamp_snapshot_for_client_with(inner, *client, &mut client_snapshot, &presence, facts);
        let digest = client_snapshot.content_digest();
        let sent = inner
            .client(*client)
            .and_then(|c| c.published_snapshot.as_ref())
            .copied();
        if sent == Some((digest, snapshot.generation)) {
            unchanged.push(*client);
            continue;
        }
        restamp |= sent.is_some_and(|(sent_digest, sent_generation)| {
            sent_digest != digest && sent_generation == snapshot.generation
        });
        sends.push((*client, Arc::clone(subscriber), client_snapshot, digest));
    }
    let generation = if restamp {
        inner.engine.state.bump_generation();
        let generation = inner.engine.state.generation();
        if tracked {
            inner.publication.last_published_mux_generation = generation;
        }
        for client in unchanged {
            if let Some((_, sent_generation)) = inner
                .client_mut(client)
                .and_then(|c| c.published_snapshot.as_mut())
            {
                *sent_generation = generation;
            }
        }
        generation
    } else {
        snapshot.generation
    };
    sends
        .into_iter()
        .map(|(client, subscriber, mut client_snapshot, digest)| {
            client_snapshot.generation = generation;
            inner
                .client_entry(client)
                .published_snapshot
                .replace((digest, generation));
            (subscriber, client_snapshot)
        })
        .collect()
}

fn terminal_viewport_for_pane(
    inner: &ServerState,
    pane: PaneId,
    view: TerminalViewId,
) -> Option<(Arc<TerminalSession>, Arc<TerminalViewport>)> {
    let terminal = inner.terminals.get(&pane)?;
    let viewport = terminal.latest_viewport_for(view).or_else(|| {
        inner
            .engine
            .state
            .pane(pane)
            .is_some_and(|pane| pane.dead)
            .then(|| terminal.latest_viewport())
    })?;
    Some((Arc::clone(terminal), viewport))
}

fn forget_floating_pane_reports(inner: &mut ServerState) {
    let floats = inner
        .engine
        .state
        .windows
        .values()
        .flat_map(|window| {
            window
                .panes
                .keys()
                .copied()
                .filter(|pane| window.is_floating(*pane) && window.zoomed_pane != Some(*pane))
        })
        .collect::<Vec<_>>();
    for pane in floats {
        inner.panes.terminal_geometries.remove(&pane);
        inner.panes.reported_pane_places.remove(&pane);
    }
}
