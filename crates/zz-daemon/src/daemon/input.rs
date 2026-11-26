use super::*;

impl Shared {
    pub(super) fn input(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        input: InputMessage,
    ) -> Result<(), DaemonError> {
        self.execution_item()
            .input_in_item(client, kind, context, input)
    }

    fn input_in_item(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        input: InputMessage,
    ) -> Result<(), DaemonError> {
        let layout_generation = match &input {
            InputMessage::ResizeTerminalV2 {
                layout_generation, ..
            }
            | InputMessage::ClientTerminalSizeV2 {
                layout_generation, ..
            } => Some(*layout_generation),
            _ => None,
        };
        let input = match input {
            InputMessage::Key {
                pane,
                input,
                text_follows: false,
            } => match self.plain_key_generation(client, pane, &input) {
                Some(generation) => return self.input_plain_key(client, pane, input, generation),
                None => InputMessage::Key {
                    pane,
                    input,
                    text_follows: false,
                },
            },
            input => input,
        };
        let Some(input) = ctrl::normalize_resize(&mut self.inner.lock(), client, input) else {
            return Ok(());
        };
        let started = diagnostic_timer();
        log::trace!(
            target: "zz_daemon::diagnostics::input",
            "dispatch begin client={client} kind={kind:?} context={context:#?} input={input:#?}"
        );
        let (
            ignores_message_input,
            dismisses_message,
            early_input_activity,
            input_rejected,
            read_only,
            dismiss_display_panes,
        ) = {
            let mut inner = self.inner.lock();
            let read_only = inner.attachment.client_flags.contains(client);
            let read_only_rejected = read_only && read_only_blocks_input(&input);
            let mouse_rejected = terminal_mouse_rejected(&inner, client, &input);
            let chooser_pane = inner
                .client(client)
                .and_then(|c| c.choose_tree.as_ref())
                .map(|chooser| chooser.source_pane)
                .or_else(|| {
                    inner
                        .client(client)
                        .and_then(|c| c.choose_buffer.as_ref())
                        .map(|chooser| chooser.source_pane)
                });
            let display_pane = inner
                .client(client)
                .and_then(|c| c.display_panes.as_ref())
                .map(|overlay| overlay.source_pane);
            let early_input_activity = match &input {
                InputMessage::TerminalView { action, .. }
                    if chooser_pane.is_some()
                        && terminal_view_action_is_modal_input(action)
                        && (!read_only || read_only_rejected) =>
                {
                    chooser_pane.map(EarlyInputActivity::WithoutBell)
                }
                InputMessage::TerminalView { action, .. }
                    if read_only_rejected
                        && display_pane.is_some()
                        && terminal_view_action_is_modal_input(action) =>
                {
                    display_pane.map(EarlyInputActivity::WithoutBell)
                }
                InputMessage::TerminalView { pane, action }
                    if read_only_rejected && terminal_view_action_is_modal_input(action) =>
                {
                    Some(EarlyInputActivity::WithoutBell(*pane))
                }
                InputMessage::TerminalView { pane, action }
                    if mouse_rejected && terminal_view_action_is_input(action) =>
                {
                    Some(EarlyInputActivity::Full(*pane))
                }
                _ => None,
            };
            let ignores_message_input = !read_only
                && inner.client(client).is_some_and(|c| c.message.is_some())
                && inner.client(client).is_some_and(|c| c.message_ignore_keys)
                && input_is_ignored_by_client_message(&input);
            if ignores_message_input
                && let InputMessage::Key {
                    pane,
                    input,
                    text_follows,
                } = &input
                && *text_follows
                && matches!(
                    input.action,
                    zz_terminal::KeyAction::Press | zz_terminal::KeyAction::Repeat
                )
            {
                let character = committed_character(input, *text_follows);
                enqueue_pending_committed_text_locked(
                    &mut inner,
                    client,
                    *pane,
                    CommittedTextLane::Terminal,
                    character,
                );
            }
            (
                ignores_message_input,
                inner.client(client).is_some_and(|c| c.message.is_some())
                    && !inner.attachment.client_flags.contains(client)
                    && input_dismisses_client_message(&input),
                early_input_activity,
                read_only_rejected || mouse_rejected,
                read_only,
                !read_only
                    && display_pane.is_some()
                    && matches!(
                        &input,
                        InputMessage::TerminalView { action, .. }
                            if terminal_view_action_dismisses_display_panes(action)
                    ),
            )
        };
        if ignores_message_input {
            if let InputMessage::ClientFocus { focused } = &input
                && let Some((_, _, events)) =
                    self.client_focus_report(client, kind, context, *focused, false)
            {
                self.run_event_hooks(events);
            }
            if let InputMessage::Text { pane, text } = &input {
                let matched = self.match_pending_committed_text(
                    client,
                    *pane,
                    CommittedTextLane::Terminal,
                    text.is_empty(),
                );
                if !text.is_empty() {
                    let _ = self.filter_suppressed_text(client, text, matched);
                }
            }
            if let InputMessage::Key { input, .. } = &input
                && input.action == zz_terminal::KeyAction::Release
            {
                let _ = self.key_decision(client, &input_key_name(input), true);
            }
            return Ok(());
        }
        if dismiss_display_panes {
            self.end_display_panes(client, context);
        }
        if let Some(activity) = early_input_activity {
            match activity {
                EarlyInputActivity::WithoutBell(pane) => {
                    self.note_terminal_input_without_bell(client, pane);
                }
                EarlyInputActivity::Full(pane) => {
                    self.note_terminal_input(client, pane);
                }
            }
        }
        if input_rejected {
            return Ok(());
        }
        if dismisses_message {
            self.dismiss_client_message(client);
        }
        let generation = self.inner.lock().engine.state.generation();
        let resize_split = matches!(&input, InputMessage::ResizeSplit { .. });
        let copy_mode_change = matches!(
            &input,
            InputMessage::TerminalView { action, .. }
                if terminal_view_action_enters_copy_mode(action)
                    || terminal_view_action_exits_copy_mode(action)
        );
        let hook_state_before = (!context.no_hooks
            && (matches!(
                &input,
                InputMessage::ResizeTerminal { .. } | InputMessage::ResizeSplit { .. }
            ) || copy_mode_change))
            .then(|| {
                let mut inner = self.inner.lock();
                let copy_modes = copy_mode_change.then(|| active_copy_mode_panes(&inner));
                (hook_events::HookScope::open(&mut inner.engine), copy_modes)
            });
        let pane_focus_before = (!context.no_hooks)
            .then(|| hook_events::InputFocusScope::open(self, &mut self.inner.lock()));
        let mut resize_report_applied = true;
        let result = (|| -> Result<(), DaemonError> {
            match input {
                InputMessage::ResizeTerminalV2 { .. }
                | InputMessage::ClientTerminalSizeV2 { .. } => {
                    unreachable!("resize normalized at entry")
                }
                InputMessage::Text { pane, text } => {
                    let matched = self.match_pending_committed_text(
                        client,
                        pane,
                        CommittedTextLane::Terminal,
                        text.is_empty(),
                    );
                    if matched.is_none() {
                        self.reject_unattached_input(client, pane)?;
                    }
                    if text.is_empty() {
                        return Ok(());
                    }
                    let filtered_text = self.filter_suppressed_text(client, &text, matched);
                    let text = filtered_text.as_ref();
                    if text.is_empty() {
                        return Ok(());
                    }
                    if read_only {
                        if matched.is_none() {
                            self.note_terminal_input_without_bell(client, pane);
                        }
                        return Ok(());
                    }
                    self.input_text(client, kind, context, pane, text, matched.is_none())?;
                }
                InputMessage::Key {
                    pane,
                    input,
                    text_follows,
                } => {
                    self.reject_unattached_input(client, pane)?;
                    self.enqueue_pending_committed_text(
                        client,
                        pane,
                        CommittedTextLane::Terminal,
                        &input,
                        text_follows,
                    );
                    self.input_key(client, kind, context, pane, input, text_follows)?;
                }
                InputMessage::BrowserSurfaceText { pane, text } => {
                    let matched = self.match_pending_committed_text(
                        client,
                        pane,
                        CommittedTextLane::BrowserSurface,
                        text.is_empty(),
                    );
                    if matched.is_none() {
                        self.reject_invalid_browser_surface_input(client, pane)?;
                    }
                    if text.is_empty() {
                        return Ok(());
                    }
                    let filtered_text = self.filter_suppressed_text(client, &text, matched);
                    let text = filtered_text.as_ref();
                    if text.is_empty() {
                        return Ok(());
                    }
                    if read_only {
                        return Ok(());
                    }
                    self.input_browser_surface_text(
                        client,
                        kind,
                        context,
                        pane,
                        text,
                        matched.is_none(),
                    )?;
                }
                InputMessage::BrowserSurfaceKey {
                    pane,
                    input,
                    text_follows,
                } => {
                    self.reject_invalid_browser_surface_input(client, pane)?;
                    self.enqueue_pending_committed_text(
                        client,
                        pane,
                        CommittedTextLane::BrowserSurface,
                        &input,
                        text_follows,
                    );
                    if read_only {
                        return Ok(());
                    }
                    self.input_browser_surface_key(
                        client,
                        kind,
                        context,
                        pane,
                        input,
                        text_follows,
                    )?;
                }
                InputMessage::ResizeTerminal {
                    pane,
                    columns,
                    rows,
                    cell_width_px,
                    cell_height_px,
                } => {
                    resize_report_applied = self.apply_terminal_size_report(
                        client,
                        pane,
                        TerminalGeometry {
                            columns,
                            rows,
                            cell_width_px,
                            cell_height_px,
                        },
                        layout_generation,
                    )?;
                }
                InputMessage::TerminalView {
                    pane,
                    action: zz_terminal::TerminalViewAction::Paste(text),
                } => {
                    self.dismiss_client_message(client);
                    let modal_active = chooser_holds_input(&self.inner.lock(), client);
                    let mode_owns_paste = self.read_client(client, |c| {
                        c.and_then(|c| c.copy_session.as_ref())
                            .is_some_and(|session| session.pane == pane)
                    });
                    if !modal_active && !mode_owns_paste {
                        self.note_terminal_input(client, pane);
                        self.input_paste(client, pane, &text)?;
                    }
                }
                InputMessage::TerminalView { pane, action } => {
                    let terminal = {
                        let inner = self.inner.lock();
                        if !read_only && chooser_holds_input(&inner, client) {
                            None
                        } else {
                            if !client_is_attached_to_pane(&inner, client, pane) {
                                return Err(ServerError::PaneNotAttached(pane).into());
                            }
                            Some(
                                inner
                                    .terminals
                                    .get(&pane)
                                    .cloned()
                                    .ok_or(ServerError::PaneExited(pane))?,
                            )
                        }
                    };
                    if let Some(terminal) = terminal {
                        if terminal_view_action_is_input(&action) {
                            if read_only {
                                self.note_terminal_input_without_bell(client, pane);
                            } else {
                                self.note_terminal_input(client, pane);
                            }
                        }
                        let refresh_changed = {
                            let mut inner = self.inner.lock();
                            let before = copy_mode_refresh_needed(&inner);
                            sync_copy_session_for_view_action(&mut inner, client, pane, &action)?;
                            before != copy_mode_refresh_needed(&inner)
                        };
                        if refresh_changed {
                            self.nudge_client_timers();
                        }
                        terminal.view_action(TerminalViewId(client.0), action);
                    }
                }
                InputMessage::ClientFocus { focused } => {
                    self.input_client_focus(client, kind, context, focused)?;
                }
                InputMessage::MouseKey {
                    key,
                    pane,
                    window,
                    column,
                    row,
                    border,
                    view_action,
                    press_action,
                    status_range_start,
                    press,
                } => {
                    self.input_mouse_key(
                        client,
                        kind,
                        context,
                        &key,
                        &MouseEventTarget {
                            pane,
                            window,
                            column,
                            row,
                            border,
                            view_action,
                            press_action,
                            status_range_start,
                            press,
                            drag: None,
                        },
                    )?;
                }
                InputMessage::DismissClientMessage | InputMessage::RetiredPopup { .. } => {}
                InputMessage::ClipboardReply { data } => self.store_clipboard_reply(client, data),
                InputMessage::ResizeCommandOutput {
                    columns,
                    rows,
                    cell_width_px,
                    cell_height_px,
                } => {
                    let terminal = self.read_client(client, |c| {
                        c.and_then(|c| c.command_output.as_ref())
                            .map(|output| Arc::clone(&output.terminal))
                    });
                    if let Some(terminal) = terminal {
                        terminal.resize(columns, rows, cell_width_px, cell_height_px);
                    }
                }
                InputMessage::CommandOutputView { action } => {
                    let terminal = self.read_client(client, |c| {
                        c.and_then(|c| c.command_output.as_ref())
                            .map(|output| Arc::clone(&output.terminal))
                    });
                    if let Some(terminal) = terminal {
                        terminal.view_action(TerminalViewId(client.0), action);
                    }
                }
                InputMessage::ChooseTree { action } => {
                    self.input_choose_tree(client, kind, context, action)?;
                }
                InputMessage::ChooseBuffer { action } => {
                    self.input_choose_buffer(client, kind, context, action)?;
                }
                InputMessage::DisplayPanes { action } => {
                    self.input_display_panes(client, kind, context, action, false)?;
                }
                InputMessage::CommandPrompt { action } => {
                    self.input_command_prompt_action(client, kind, context, action)?;
                }
                InputMessage::ResizeSplit {
                    window,
                    split,
                    ratio_basis_points,
                } => {
                    self.input_resize_split(client, kind, window, split, ratio_basis_points)?;
                }
                InputMessage::CancelPrefix { request_id } => {
                    self.cancel_prefix(client, request_id);
                }
                InputMessage::Menu { action } => {
                    self.input_menu(client, context, action);
                }
                InputMessage::Confirm { action } => {
                    self.input_confirm(client, context, action);
                }
                InputMessage::ClientSuspendState { suspended } => {
                    {
                        let mut inner = self.inner.lock();
                        if let Some(registered) = inner.client_mut(client) {
                            registered.suspended = suspended;
                        }
                    }
                    self.publish_mux_snapshots();
                }
                InputMessage::ClientTerminalSize { columns, rows } => {
                    if columns > 0 && rows > 0 {
                        resize_report_applied = self.apply_client_size_report(
                            client,
                            kind,
                            context,
                            columns,
                            rows,
                            layout_generation,
                        );
                    }
                }
            }
            Ok(())
        })();
        if !resize_report_applied {
            return result;
        }
        let publish_snapshot = {
            let mut inner = self.inner.lock();
            let current = inner.engine.state.generation();
            if resize_split && result.is_ok() {
                inner
                    .client_mut(client)
                    .and_then(|c| c.published_snapshot.take());
            }
            resize_split && result.is_ok()
                || current != generation
                    && inner.publication.last_published_mux_generation != current
        };
        if publish_snapshot {
            self.publish_snapshot();
        }
        if result.is_ok()
            && let Some((hook_scope, copy_modes_before)) = hook_state_before
        {
            let events = {
                let mut inner = self.inner.lock();
                let transitions = std::mem::take(&mut inner.modes.pane_mode_transitions);
                let mut diff = hook_scope.finish(&inner.engine, "");
                diff.events.extend(hook_events::server_mode_hook_events(
                    &inner.engine,
                    transitions,
                ));
                if let Some(copy_modes_before) = copy_modes_before {
                    let copy_modes_after = active_copy_mode_panes(&inner);
                    diff.events.extend(hook_events::pane_mode_hook_events(
                        &inner.engine,
                        &diff.before,
                        &copy_modes_before,
                        &copy_modes_after,
                    ));
                }
                diff.events
            };
            self.run_event_hooks(events);
        }
        let (events, reap) = {
            let mut inner = self.inner.lock();
            (
                take_pane_mode_hook_events(&mut inner),
                !inner.modes.pane_mode_kill_panes.is_empty(),
            )
        };
        if !context.no_hooks && !events.is_empty() {
            self.run_event_hooks(events);
        }
        if reap {
            self.reap_pane_mode_kill_panes(client, kind, context);
        }
        if let Some(pane_focus_before) = pane_focus_before {
            let events = {
                let mut inner = self.inner.lock();
                pane_focus_before
                    .close(&inner)
                    .map(|probe| pane_focus_settle(&mut inner, &probe))
                    .unwrap_or_default()
            };
            self.run_event_hooks(events);
        }
        log::trace!(
            target: "zz_daemon::diagnostics::input",
            "dispatch end client={client} success={} elapsed_us={} context={context:#?}",
            result.is_ok(),
            diagnostic_elapsed_us(started),
        );
        result
    }

    pub(super) fn apply_terminal_size_report(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
        geometry: TerminalGeometry,
        layout_generation: Option<u64>,
    ) -> Result<bool, DaemonError> {
        let resize = {
            let mut inner = self.inner.lock();
            if let Some(layout_generation) = layout_generation
                && ctrl::normalize_resize(
                    &mut inner,
                    client,
                    InputMessage::ResizeTerminalV2 {
                        pane,
                        columns: geometry.columns,
                        rows: geometry.rows,
                        cell_width_px: geometry.cell_width_px,
                        cell_height_px: geometry.cell_height_px,
                        layout_generation,
                    },
                )
                .is_none()
            {
                return Ok(false);
            }
            if inner.engine.state.window_for_pane(pane).is_none() {
                return Err(ServerError::PaneExited(pane).into());
            }
            if !client_is_attached_to_pane(&inner, client, pane) {
                return Err(ServerError::PaneNotAttached(pane).into());
            }
            if inner
                .client(client)
                .and_then(|c| c.streamed_terminals.as_ref())
                .and_then(|streamed| streamed.get(&pane))
                == Some(&TerminalStreamKind::Preview)
            {
                return Ok(true);
            }
            inner
                .panes
                .terminal_geometries
                .entry(pane)
                .or_default()
                .insert(client, geometry);
            sync_client_view_areas(&inner, client);
            if let Some(cell) = inner
                .client_mut(client)
                .and_then(|c| c.cell_pixels.as_mut())
            {
                *cell = (geometry.cell_width_px, geometry.cell_height_px);
            }
            let extent =
                inner
                    .engine
                    .window_extent_for_pane_geometry(pane, geometry.columns, geometry.rows);
            record_report_place(&mut inner, client, pane, extent);
            if let Some(reported) = pane_geometry_from(&inner, pane, GeometrySource::ClientReport) {
                inner
                    .engine
                    .set_pane_geometry(pane, reported.columns, reported.rows);
            }
            terminal_resize_for_pane(&inner, pane)
        };
        if let Some((terminal, geometry)) = resize {
            terminal.resize(
                geometry.columns,
                geometry.rows,
                geometry.cell_width_px,
                geometry.cell_height_px,
            );
        }
        self.refit_client_overlays(client);
        Ok(true)
    }

    pub(super) fn apply_client_size_report(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        columns: u16,
        rows: u16,
        layout_generation: Option<u64>,
    ) -> bool {
        let hook_events = {
            let mut inner = self.inner.lock();
            if let Some(layout_generation) = layout_generation
                && ctrl::normalize_resize(
                    &mut inner,
                    client,
                    InputMessage::ClientTerminalSizeV2 {
                        columns,
                        rows,
                        layout_generation,
                    },
                )
                .is_none()
            {
                return false;
            }
            if let Some(registered) = inner.client_mut(client) {
                registered.size = Some((columns, rows));
            }
            sync_client_view_areas(&inner, client);
            let mut hook_events = Vec::new();
            if kind == ClientKind::Interactive {
                if let Some(event) =
                    set_current_window_latest_client(&mut inner, client, !context.no_hooks)
                {
                    hook_events.push(event);
                }
                if !context.no_hooks
                    && let Some(event) = client_hook_event(&inner, "client-resized", client)
                {
                    hook_events.push(event);
                }
            }
            hook_events
        };
        self.run_event_hooks(hook_events);
        true
    }

    fn reject_unattached_input(&self, client: ClientId, pane: PaneId) -> Result<(), ServerError> {
        if client_attached_session(&self.inner.lock(), client).is_some() {
            Ok(())
        } else {
            Err(ServerError::PaneNotAttached(pane))
        }
    }

    fn reject_invalid_browser_surface_input(
        &self,
        client: ClientId,
        pane: PaneId,
    ) -> Result<(), ServerError> {
        let inner = self.inner.lock();
        if !client_is_attached_to_pane(&inner, client, pane) {
            return Err(ServerError::PaneNotAttached(pane));
        }
        match inner.engine.state.pane(pane).map(|pane| &pane.kind) {
            Some(PaneKind::Browser(_)) => Ok(()),
            Some(_) => Err(ServerError::InvalidTarget(format!(
                "{pane} is not a browser pane"
            ))),
            None => Err(ServerError::PaneExited(pane)),
        }
    }

    pub(super) fn note_terminal_input(self: &Arc<Self>, client: ClientId, pane: PaneId) {
        self.note_terminal_input_accounting(client, pane, true);
    }

    pub(super) fn note_terminal_input_without_bell(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
    ) {
        self.note_terminal_input_accounting(client, pane, false);
    }

    fn note_terminal_input_accounting(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
        clear_bell: bool,
    ) {
        let (resizes, layout_changed, refresh_activity_choosers, client_active) = {
            let mut inner = self.inner.lock();
            let refresh_activity_choosers = note_terminal_activity_locked(&mut inner, client, pane);
            let client_active = (inner.client(client).and_then(|c| c.kind)
                == Some(ClientKind::Interactive))
            .then(|| inner.engine.state.window_for_pane(pane))
            .flatten()
            .and_then(|window| set_window_latest_client(&mut inner, client, window, true));
            let (resizes, layout_changed) =
                terminal_resizes_after_client_input(&mut inner, client, pane);
            (
                resizes,
                layout_changed,
                refresh_activity_choosers,
                client_active,
            )
        };
        apply_terminal_resizes(resizes);
        let bell_cleared = clear_bell && self.clear_pane_alerts(pane);
        if layout_changed || bell_cleared {
            self.publish_snapshot_state();
        } else if refresh_activity_choosers {
            self.refresh_choose_trees();
        }
        if let Some(event) = client_active {
            self.run_event_hooks(vec![event]);
        }
    }

    fn client_focus_report(
        &self,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        focused: bool,
        update_latest: bool,
    ) -> Option<(bool, bool, Vec<PendingHookEvent>)> {
        let mut inner = self.inner.lock();
        client_attached_session(&inner, client)?;
        let pane_focus_before = (!context.no_hooks).then(|| capture_client_focus_probe(&inner));
        if let Some(registered) = inner.client_mut(client) {
            registered.focused = Some(focused);
        }
        let seen = focused && pane_summary::mark_focused_window_seen(&mut inner, client);
        let mut hook_events = Vec::new();
        let pane_focus = pane_focus_before
            .as_ref()
            .map(|before| pane_focus_hook_events(&mut inner, before));
        let pane_focus = pane_focus.unwrap_or_default();
        if focused {
            hook_events.extend(client_focus_hook(&inner, kind, context, client, focused));
            let anchor = hook_events.len();
            splice_pane_focus_events(&mut hook_events, anchor, pane_focus);
        } else {
            splice_pane_focus_events(&mut hook_events, 0, pane_focus);
            hook_events.extend(client_focus_hook(&inner, kind, context, client, focused));
        }
        if kind == ClientKind::Interactive
            && focused
            && update_latest
            && let Some(event) =
                set_current_window_latest_client(&mut inner, client, !context.no_hooks)
        {
            hook_events.push(event);
        }
        let report = (
            inner.attachment.client_flags.contains(client),
            inner.engine.focus_events(),
            hook_events,
        );
        drop(inner);
        if seen {
            self.publish_snapshot_state();
        }
        Some(report)
    }

    fn input_client_focus(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        focused: bool,
    ) -> Result<(), DaemonError> {
        let Some((read_only, focus_events, hook_events)) =
            self.client_focus_report(client, kind, context, focused, true)
        else {
            return Ok(());
        };
        self.run_event_hooks(hook_events);
        if !focus_events {
            return Ok(());
        }
        if !read_only {
            self.dismiss_client_message(client);
        }
        if !read_only {
            self.end_display_panes(client, context);
        }
        if !read_only && self.input_command_prompt_focus(client, kind, context, focused) {
            self.sync_key_table(client, false);
            return Ok(());
        }
        self.note_enabled_client_focus(client, focused);

        let (pane, decision, repeat_binding) = {
            let mut inner = self.inner.lock();
            let Some(session) = client_attached_session(&inner, client) else {
                drop(inner);
                self.sync_key_table(client, false);
                return Ok(());
            };
            let Some(session_state) = inner.engine.state.sessions.get(&session) else {
                drop(inner);
                self.sync_key_table(client, false);
                return Ok(());
            };
            let window = client_focused_window(&inner, client, session_state);
            let Some(pane) = inner
                .engine
                .state
                .windows
                .get(&window)
                .map(|window| window.active_pane)
            else {
                drop(inner);
                self.sync_key_table(client, false);
                return Ok(());
            };
            context.retarget(&ExecutionContext::new(
                Some(session),
                Some(window),
                Some(pane),
            ));

            let root_table = inner.engine.key_table_for_session(session).clone();
            let mut key_engine = inner
                .client_mut(client)
                .and_then(|c| c.key_engine.take())
                .unwrap_or_default();
            let transient_table = if inner
                .client(client)
                .is_some_and(|c| c.choose_tree.is_some())
            {
                Some("choose-tree".to_owned())
            } else if inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
            {
                Some("choose-buffer".to_owned())
            } else {
                let copy_mode_active = inner
                    .client(client)
                    .and_then(|c| c.copy_session.as_ref())
                    .is_some_and(|copy| copy.pane == pane && !copy.exiting)
                    || command_output_owns_pane(&inner, client, pane);
                if copy_mode_active {
                    key_engine
                        .active_table()
                        .filter(|table| matches!(*table, "copy-mode" | "copy-mode-vi"))
                        .map(str::to_owned)
                        .or_else(|| {
                            inner
                                .engine
                                .copy_mode_table_for_pane(pane)
                                .ok()
                                .map(str::to_owned)
                        })
                } else {
                    None
                }
            };
            let repeat_time_ms = inner.engine.repeat_time_for_session(session);
            let initial_repeat_time_ms = inner.engine.initial_repeat_time_for_session(session);
            let (decision, repeat_binding) = if let Some(table) = transient_table {
                key_engine.handle_transient_mode_synthetic_any(
                    &inner.engine.keys,
                    &table,
                    &root_table,
                )
            } else {
                key_engine.handle_synthetic_any_with_repeat_metadata(
                    &inner.engine.keys,
                    Instant::now(),
                    Duration::from_millis(u64::from(repeat_time_ms)),
                    Duration::from_millis(u64::from(initial_repeat_time_ms)),
                    &root_table,
                )
            };
            inner.client_entry(client).key_engine.replace(key_engine);
            (pane, decision, repeat_binding)
        };

        let result = match decision {
            KeyDecision::Commands(commands) => {
                self.execute_key_commands(client, kind, context, pane, &commands, repeat_binding)
            }
            KeyDecision::Pass | KeyDecision::Prefix | KeyDecision::Ignore => Ok(()),
        };
        self.sync_key_table(client, false);
        result
    }

    fn note_enabled_client_focus(&self, client: ClientId, focused: bool) {
        let (resizes, layout_changed, refresh_activity_choosers) = {
            let mut inner = self.inner.lock();
            let Some(session) = client_attached_session(&inner, client) else {
                return;
            };
            let refresh_activity_choosers =
                note_client_activity_locked(&mut inner, client, session);
            let (resizes, layout_changed) = if focused {
                terminal_resizes_after_client_focus(&mut inner, client)
            } else {
                (Vec::new(), false)
            };
            (resizes, layout_changed, refresh_activity_choosers)
        };
        apply_terminal_resizes(resizes);
        if layout_changed {
            self.publish_snapshot_state();
        }
        if refresh_activity_choosers {
            self.refresh_choose_trees();
        }
    }

    fn input_resize_split(
        &self,
        client: ClientId,
        kind: ClientKind,
        window: WindowId,
        split: SplitId,
        ratio_basis_points: u16,
    ) -> Result<(), DaemonError> {
        if kind != ClientKind::Interactive {
            return Err(ServerError::InvalidCommand(
                "split dragging requires an interactive client".to_owned(),
            )
            .into());
        }
        if ratio_basis_points > SPLIT_RATIO_BASIS {
            return Err(ServerError::InvalidCommand(format!(
                "split ratio must be between 0 and {SPLIT_RATIO_BASIS}"
            ))
            .into());
        }
        {
            let mut inner = self.inner.lock();
            if inner.client(client).is_none_or(|c| c.subscriber.is_none()) {
                return Err(ServerError::InvalidCommand(
                    "split dragging requires a subscribed client".to_owned(),
                )
                .into());
            }
            let attached = client_attached_session(&inner, client).ok_or_else(|| {
                ServerError::InvalidTarget("client is not attached to a session".to_owned())
            })?;
            let active_window = inner
                .engine
                .state
                .sessions
                .get(&attached)
                .map(|session| client_focused_window(&inner, client, session))
                .ok_or_else(|| {
                    ServerError::InvalidTarget("attached session no longer exists".to_owned())
                })?;
            let target = inner
                .engine
                .state
                .windows
                .get(&window)
                .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
            if target.session != attached || active_window != window {
                return Err(ServerError::InvalidTarget(format!(
                    "window {window} is not active for the attached client"
                ))
                .into());
            }
            if target.zoomed_pane.is_some() {
                return Err(ServerError::InvalidCommand(
                    "cannot drag a split while its window is zoomed".to_owned(),
                )
                .into());
            }
            if !target.layout.project().contains_split(split) {
                return Err(ServerError::MissingTarget(split.to_string()).into());
            }
            inner.engine.state.resize_split(
                window,
                split,
                f32::from(ratio_basis_points) / f32::from(SPLIT_RATIO_BASIS),
            )?;
        }
        Ok(())
    }

    fn input_text(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane: PaneId,
        text: &str,
        account_activity: bool,
    ) -> Result<(), DaemonError> {
        self.dismiss_client_message(client);
        {
            let inner = self.inner.lock();
            if chooser_holds_input(&inner, client) {
                return Ok(());
            }
        }
        if self.input_command_prompt_text(client, kind, context, text) {
            return Ok(());
        }
        if account_activity {
            self.note_terminal_input(client, pane);
        }
        let (command_output_active, followed) = {
            let mut inner = self.inner.lock();
            let followed = follow_command_output_focus(&mut inner, client, pane);
            (command_output_owns_pane(&inner, client, pane), followed)
        };
        if followed {
            self.sync_key_table(client, false);
        }
        let mut decided = false;
        let result = (|| -> Result<(), DaemonError> {
            let mut sinks = None;
            let mut pass_start = None;
            for (offset, character) in text.char_indices() {
                let mut encoded = [0_u8; 4];
                let (decision, repeat_binding) = self.key_decision_with_repeat(
                    client,
                    character.encode_utf8(&mut encoded),
                    false,
                );
                match decision {
                    KeyDecision::Pass => {
                        if command_output_active
                            || command_output_owns_pane(&self.inner.lock(), client, pane)
                        {
                            if let Some(start) = pass_start.take() {
                                self.dispatch_input_text(
                                    client,
                                    pane,
                                    &mut sinks,
                                    &text[start..offset],
                                )?;
                            }
                            continue;
                        }
                        if pass_start.is_none() {
                            pass_start = Some(offset);
                        }
                    }
                    KeyDecision::Prefix | KeyDecision::Ignore => {
                        decided = true;
                        if let Some(start) = pass_start.take() {
                            self.dispatch_input_text(
                                client,
                                pane,
                                &mut sinks,
                                &text[start..offset],
                            )?;
                        }
                    }
                    KeyDecision::Commands(commands) => {
                        decided = true;
                        if let Some(start) = pass_start.take() {
                            self.dispatch_input_text(
                                client,
                                pane,
                                &mut sinks,
                                &text[start..offset],
                            )?;
                        }
                        let previous = context.invoking_key().map(str::to_owned);
                        context.set_invoking_key(Some(character.to_string()));
                        let dispatched = self.execute_key_commands(
                            client,
                            kind,
                            context,
                            pane,
                            &commands,
                            repeat_binding,
                        );
                        context.set_invoking_key(previous);
                        dispatched?;
                        sinks = None;
                        if self
                            .read_client(client, |c| c.is_some_and(|c| c.command_prompt.is_some()))
                        {
                            let remaining = &text[offset + character.len_utf8()..];
                            if !remaining.is_empty() {
                                self.input_command_prompt_text(client, kind, context, remaining);
                            }
                            break;
                        }
                    }
                }
            }
            if let Some(start) = pass_start {
                self.dispatch_input_text(client, pane, &mut sinks, &text[start..])?;
            }
            Ok(())
        })();
        self.sync_key_table(client, decided);
        result
    }

    fn input_browser_surface_text(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane: PaneId,
        text: &str,
        account_activity: bool,
    ) -> Result<(), DaemonError> {
        let blocked = {
            let inner = self.inner.lock();
            inner
                .client(client)
                .is_some_and(|c| c.choose_tree.is_some())
                || inner
                    .client(client)
                    .is_some_and(|c| c.choose_buffer.is_some())
                || inner
                    .client(client)
                    .is_some_and(|c| c.display_panes.is_some())
                || command_output_owns_pane(&inner, client, pane)
        };
        if blocked || self.input_command_prompt_text(client, kind, context, text) {
            return Ok(());
        }
        if account_activity {
            self.note_terminal_input(client, pane);
        }
        self.dispatch_input_text(client, pane, &mut None, text)
            .map_err(Into::into)
    }

    fn dispatch_input_text(
        &self,
        client: ClientId,
        pane: PaneId,
        sinks: &mut Option<Vec<PaneSink>>,
        text: &str,
    ) -> Result<(), ServerError> {
        if text.is_empty() {
            return Ok(());
        }
        let sinks = if let Some(sinks) = sinks.as_ref() {
            sinks
        } else {
            sinks.insert(self.input_sinks(client, pane)?)
        };
        let terminal_text = sinks
            .iter()
            .any(|sink| matches!(sink, PaneSink::Terminal(_)))
            .then(|| Arc::<str>::from(text));
        for sink in sinks {
            match sink {
                PaneSink::Terminal(terminal) => terminal.send_text_for_view(
                    TerminalViewId(client.0),
                    Arc::clone(
                        terminal_text
                            .as_ref()
                            .expect("terminal text is allocated when a terminal sink exists"),
                    ),
                ),
                PaneSink::Browser(target) => self.publish_for_pane(
                    *target,
                    &EventPayload::BrowserCommand {
                        pane: *target,
                        command: BrowserCommand::SendKeys(vec![zz_protocol::KeyToken::Literal(
                            text.to_owned(),
                        )]),
                    },
                ),
            }
        }
        Ok(())
    }

    pub(super) fn input_sinks(
        &self,
        client: ClientId,
        source: PaneId,
    ) -> Result<Vec<PaneSink>, ServerError> {
        let inner = self.inner.lock();
        if !client_is_attached_to_pane(&inner, client, source) {
            return Err(ServerError::PaneNotAttached(source));
        }
        resolve_input_sinks(&inner, source)
    }

    pub(super) fn filter_suppressed_text<'a>(
        &self,
        client: ClientId,
        text: &'a str,
        matched: Option<PendingCommittedText>,
    ) -> Cow<'a, str> {
        let Some(character) = matched.and_then(|matched| matched.suppressed_character) else {
            return Cow::Borrowed(text);
        };
        let mut inner = self.inner.lock();
        let remove_client = if let Some(characters) = inner
            .client_mut(client)
            .and_then(|c| c.suppressed_text.as_mut())
        {
            if let Some(count) = characters.get_mut(&character) {
                *count = count.saturating_sub(1);
            }
            characters.retain(|_, count| *count != 0);
            characters.is_empty()
        } else {
            false
        };
        if remove_client {
            inner
                .client_mut(client)
                .and_then(|c| c.suppressed_text.take());
        }
        let Some(offset) = text.find(character) else {
            return Cow::Borrowed(text);
        };
        let mut filtered = String::with_capacity(text.len() - character.len_utf8());
        filtered.push_str(&text[..offset]);
        filtered.push_str(&text[offset + character.len_utf8()..]);
        Cow::Owned(filtered)
    }

    pub(super) fn enqueue_pending_committed_text(
        &self,
        client: ClientId,
        pane: PaneId,
        lane: CommittedTextLane,
        input: &zz_terminal::KeyInput,
        text_follows: bool,
    ) {
        if !text_follows
            || !matches!(
                input.action,
                zz_terminal::KeyAction::Press | zz_terminal::KeyAction::Repeat
            )
        {
            return;
        }
        let mut inner = self.inner.lock();
        enqueue_pending_committed_text_locked(&mut inner, client, pane, lane, None);
    }

    pub(super) fn match_pending_committed_text(
        &self,
        client: ClientId,
        pane: PaneId,
        lane: CommittedTextLane,
        empty: bool,
    ) -> Option<PendingCommittedText> {
        let mut inner = self.inner.lock();
        let mut pending = inner
            .client_mut(client)
            .and_then(|c| c.pending_committed_text.take())?;
        let Some(index) = pending
            .iter()
            .position(|entry| entry.pane == pane && entry.lane == lane)
        else {
            inner
                .client_entry(client)
                .pending_committed_text
                .replace(pending);
            return None;
        };
        for _ in 0..index {
            let stale = pending.pop_front().expect("committed-text prefix");
            retire_pending_committed_text_locked(&mut inner, client, stale);
        }
        let matched = pending.pop_front().expect("matched committed text");
        if !pending.is_empty() {
            inner
                .client_entry(client)
                .pending_committed_text
                .replace(pending);
        }
        if empty {
            retire_pending_committed_text_locked(&mut inner, client, matched);
        }
        Some(matched)
    }

    pub(super) fn suppress_committed_character(
        &self,
        client: ClientId,
        pane: PaneId,
        lane: CommittedTextLane,
        input: &zz_terminal::KeyInput,
        text_follows: bool,
    ) {
        let Some(character) = committed_character(input, text_follows) else {
            return;
        };
        let mut inner = self.inner.lock();
        suppress_committed_character_locked(&mut inner, client, pane, lane, character);
    }

    pub(super) fn clear_pending_committed_text(&self, client: ClientId) {
        let mut inner = self.inner.lock();
        let Some(pending) = inner
            .client_mut(client)
            .and_then(|c| c.pending_committed_text.take())
        else {
            return;
        };
        for stale in pending {
            retire_pending_committed_text_locked(&mut inner, client, stale);
        }
    }
}

pub(super) const MAX_PENDING_COMMITTED_TEXT: usize = 32;

fn chooser_holds_input(inner: &ServerState, client: ClientId) -> bool {
    let Some(state) = inner.client(client) else {
        return false;
    };
    if state.display_panes.is_some() {
        return true;
    }
    let Some(source) = state
        .choose_tree
        .as_ref()
        .map(|chooser| chooser.source_pane)
        .or_else(|| {
            state
                .choose_buffer
                .as_ref()
                .map(|chooser| chooser.source_pane)
        })
    else {
        return false;
    };
    client_focused_window_for_attachment(inner, client)
        .and_then(|window| inner.engine.state.windows.get(&window))
        .is_none_or(|window| window.active_pane == source)
}

fn sync_copy_session_for_view_action(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
    action: &zz_terminal::TerminalViewAction,
) -> Result<(), ServerError> {
    if terminal_view_action_enters_copy_mode(action) {
        enter_copy_session(inner, client, pane)?;
    } else if terminal_view_action_exits_copy_mode(action) {
        begin_exit_copy_session(inner, client);
    } else if terminal_view_action_arms_scroll_exit(action)
        && let Some(session) = inner
            .client_mut(client)
            .and_then(|c| c.copy_session.as_mut())
        && session.pane == pane
    {
        session.scroll_exit = true;
    }
    Ok(())
}

fn client_focus_hook(
    inner: &ServerState,
    kind: ClientKind,
    context: &ExecutionContext,
    client: ClientId,
    focused: bool,
) -> Option<PendingHookEvent> {
    if kind != ClientKind::Interactive || context.no_hooks {
        return None;
    }
    let name = if focused {
        "client-focus-in"
    } else {
        "client-focus-out"
    };
    client_hook_event(inner, name, client)
}

fn pane_focus_settle(inner: &mut ServerState, before: &PaneFocusProbe) -> Vec<PendingHookEvent> {
    let (leading, removals) = pane_focus_candidates(inner, before);
    if leading.is_empty() && removals.is_empty() {
        return Vec::new();
    }
    let events = pane_focus_hook_events(inner, before);
    let mut flattened = Vec::new();
    splice_pane_focus_events(&mut flattened, 0, events);
    flattened
}

fn set_window_latest_client(
    inner: &mut ServerState,
    client: ClientId,
    window: WindowId,
    event_hooks_enabled: bool,
) -> Option<PendingHookEvent> {
    let session = client_attached_session(inner, client)?;
    if inner.engine.state.windows.get(&window)?.session != session {
        return None;
    }
    if inner
        .attachment
        .window_latest_clients
        .insert(window, client)
        == Some(client)
    {
        return None;
    }
    event_hooks_enabled
        .then(|| client_hook_event_for_window(inner, "client-active", client, window))
        .flatten()
}

fn set_current_window_latest_client(
    inner: &mut ServerState,
    client: ClientId,
    event_hooks_enabled: bool,
) -> Option<PendingHookEvent> {
    let window = client_focused_window_for_attachment(inner, client)?;
    set_window_latest_client(inner, client, window, event_hooks_enabled)
}

fn note_terminal_activity_locked(inner: &mut ServerState, client: ClientId, pane: PaneId) -> bool {
    let Some(session) = client_is_attached_to_pane(inner, client, pane)
        .then(|| inner.engine.state.window_for_pane(pane))
        .flatten()
        .and_then(|window| inner.engine.state.windows.get(&window))
        .map(|window| window.session)
    else {
        return false;
    };
    note_client_activity_locked(inner, client, session)
}

fn note_client_activity_locked(
    inner: &mut ServerState,
    client: ClientId,
    session: SessionId,
) -> bool {
    let now = unix_timestamp();
    inner.engine.mark_session_active_at(session, now);
    inner.attachment.activity_sequence = inner.attachment.activity_sequence.saturating_add(1);
    let activity = inner.attachment.activity_sequence;
    if let Some(registered) = inner.client_mut(client) {
        registered.activity = Some(activity);
        registered.activity_time = Some(now);
    }
    inner
        .clients
        .values()
        .filter_map(|c| c.choose_tree.as_ref())
        .any(|chooser| chooser.sort.order() == Some(TmuxSortOrder::Activity))
}

fn terminal_resizes_after_client_focus(
    inner: &mut ServerState,
    client: ClientId,
) -> (Vec<(Arc<TerminalSession>, TerminalGeometry)>, bool) {
    let visible_panes = inner
        .client(client)
        .and_then(|c| c.visible_terminals.as_ref())
        .cloned()
        .unwrap_or_default();
    terminal_resizes_after_client_sequence(inner, client, &visible_panes)
}

fn terminal_resizes_after_client_input(
    inner: &mut ServerState,
    client: ClientId,
    source_pane: PaneId,
) -> (Vec<(Arc<TerminalSession>, TerminalGeometry)>, bool) {
    let Some(visible_panes) = inner
        .client(client)
        .and_then(|c| c.visible_terminals.as_ref())
        .cloned()
    else {
        return (Vec::new(), false);
    };
    if !visible_panes.contains(&source_pane) || !inner.terminals.contains_key(&source_pane) {
        return (Vec::new(), false);
    }
    terminal_resizes_after_client_sequence(inner, client, &visible_panes)
}

fn terminal_resizes_after_client_sequence(
    inner: &mut ServerState,
    client: ClientId,
    visible_panes: &BTreeSet<PaneId>,
) -> (Vec<(Arc<TerminalSession>, TerminalGeometry)>, bool) {
    let previous_owners = visible_panes
        .iter()
        .map(|pane| (*pane, terminal_geometry_owner(inner, *pane)))
        .collect::<Vec<_>>();
    let previous_geometries = visible_panes
        .iter()
        .map(|pane| {
            (
                *pane,
                terminal_resize_for_pane(inner, *pane).map(|(_, geometry)| geometry),
            )
        })
        .collect::<BTreeMap<_, _>>();

    mark_client_terminal_latest(inner, client);

    let owner_changed_panes = previous_owners
        .into_iter()
        .filter_map(|(pane, previous)| {
            (terminal_geometry_owner(inner, pane) != previous).then_some(pane)
        })
        .collect::<BTreeSet<_>>();
    if owner_changed_panes.is_empty() {
        return (Vec::new(), false);
    }
    let layout_changed = write_back_terminal_geometries(inner, &owner_changed_panes);
    let resizes = visible_panes
        .iter()
        .filter_map(|pane| {
            let resize = terminal_resize_for_pane(inner, *pane)?;
            (previous_geometries.get(pane).copied().flatten() != Some(resize.1)).then_some(resize)
        })
        .collect();
    (resizes, layout_changed)
}

fn terminal_view_action_is_input(action: &zz_terminal::TerminalViewAction) -> bool {
    use zz_terminal::TerminalViewAction as Action;

    match action {
        Action::Focus(_) | Action::ClearLinkHover => false,
        Action::Mouse(input) => !terminal_mouse_is_bare_hover(*input),
        Action::ScrollLines(_)
        | Action::ScrollPages(_)
        | Action::ScrollTop
        | Action::ScrollBottom
        | Action::ScrollToFraction(_)
        | Action::ScrollToOffset(_)
        | Action::ScrollWheel { .. }
        | Action::SelectionPress(_)
        | Action::SelectionDrag(_)
        | Action::SelectionAutoscroll { .. }
        | Action::SelectionRelease(_)
        | Action::SelectAll
        | Action::ClearSelection
        | Action::ClearHistory
        | Action::EnterCopyMode
        | Action::EnterCopyModeScrollExit
        | Action::EnterCopyModeWith { .. }
        | Action::CopyMode(_)
        | Action::CopyModeCounted { .. }
        | Action::CopySelection { .. }
        | Action::SearchBegin(_)
        | Action::SearchUpdate(_)
        | Action::SearchNext
        | Action::SearchPrevious
        | Action::SearchClose
        | Action::Paste(_) => true,
    }
}

fn terminal_view_action_dismisses_display_panes(action: &zz_terminal::TerminalViewAction) -> bool {
    match action {
        zz_terminal::TerminalViewAction::Mouse(input) => !terminal_mouse_is_bare_hover(*input),
        zz_terminal::TerminalViewAction::ScrollWheel { .. } => true,
        _ => false,
    }
}

fn terminal_mouse_is_bare_hover(input: zz_terminal::TerminalMouseInput) -> bool {
    input.phase() == zz_terminal::TerminalMousePhase::Motion && input.button().is_none()
}

fn terminal_view_action_is_modal_input(action: &zz_terminal::TerminalViewAction) -> bool {
    !matches!(
        action,
        zz_terminal::TerminalViewAction::Focus(_) | zz_terminal::TerminalViewAction::ClearLinkHover
    )
}

#[derive(Clone, Copy)]
enum EarlyInputActivity {
    WithoutBell(PaneId),
    Full(PaneId),
}

fn input_dismisses_client_message(input: &InputMessage) -> bool {
    match input {
        InputMessage::Key { input, .. } => input.action != zz_terminal::KeyAction::Release,
        InputMessage::DismissClientMessage => true,
        InputMessage::TerminalView { action, .. } => {
            terminal_view_action_dismisses_display_panes(action)
        }
        _ => false,
    }
}

fn input_is_ignored_by_client_message(input: &InputMessage) -> bool {
    match input {
        InputMessage::Key { .. }
        | InputMessage::Text { .. }
        | InputMessage::TerminalView {
            action: zz_terminal::TerminalViewAction::Paste(_),
            ..
        }
        | InputMessage::ClientFocus { .. }
        | InputMessage::DismissClientMessage => true,
        InputMessage::TerminalView { action, .. } => {
            terminal_view_action_dismisses_display_panes(action)
        }
        _ => false,
    }
}

fn enqueue_pending_committed_text_locked(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
    lane: CommittedTextLane,
    suppressed_character: Option<char>,
) {
    let stale = {
        let pending = inner
            .client_entry(client)
            .pending_committed_text
            .get_or_insert_default();
        let stale = (pending.len() >= MAX_PENDING_COMMITTED_TEXT)
            .then(|| pending.pop_front())
            .flatten();
        pending.push_back(PendingCommittedText {
            pane,
            lane,
            suppressed_character,
        });
        stale
    };
    if let Some(character) = suppressed_character {
        *inner
            .client_entry(client)
            .suppressed_text
            .get_or_insert_default()
            .entry(character)
            .or_default() += 1;
    }
    if let Some(stale) = stale {
        retire_pending_committed_text_locked(inner, client, stale);
    }
}

fn retire_pending_committed_text_locked(
    inner: &mut ServerState,
    client: ClientId,
    pending: PendingCommittedText,
) {
    let Some(character) = pending.suppressed_character else {
        return;
    };
    let remove_client = if let Some(characters) = inner
        .client_mut(client)
        .and_then(|c| c.suppressed_text.as_mut())
    {
        if let Some(count) = characters.get_mut(&character) {
            *count = count.saturating_sub(1);
        }
        characters.retain(|_, count| *count != 0);
        characters.is_empty()
    } else {
        false
    };
    if remove_client {
        inner
            .client_mut(client)
            .and_then(|c| c.suppressed_text.take());
    }
}

fn read_only_blocks_input(input: &InputMessage) -> bool {
    match input {
        InputMessage::ResizeSplit { .. }
        | InputMessage::RetiredPopup { .. }
        | InputMessage::Menu { .. }
        | InputMessage::Confirm { .. }
        | InputMessage::MouseKey { .. }
        | InputMessage::DismissClientMessage => true,
        InputMessage::TerminalView { action, .. } => {
            !terminal_view_action_is_read_only_safe(action)
        }
        InputMessage::Text { .. }
        | InputMessage::Key { .. }
        | InputMessage::BrowserSurfaceText { .. }
        | InputMessage::BrowserSurfaceKey { .. }
        | InputMessage::ChooseTree { .. }
        | InputMessage::ChooseBuffer { .. }
        | InputMessage::DisplayPanes { .. }
        | InputMessage::CommandPrompt { .. }
        | InputMessage::ResizeTerminal { .. }
        | InputMessage::ResizeTerminalV2 { .. }
        | InputMessage::ClientTerminalSizeV2 { .. }
        | InputMessage::ResizeCommandOutput { .. }
        | InputMessage::CommandOutputView { .. }
        | InputMessage::CancelPrefix { .. }
        | InputMessage::ClientSuspendState { .. }
        | InputMessage::ClientTerminalSize { .. }
        | InputMessage::ClientFocus { .. }
        | InputMessage::ClipboardReply { .. } => false,
    }
}

pub(super) fn terminal_view_action_is_read_only_safe(
    action: &zz_terminal::TerminalViewAction,
) -> bool {
    use zz_terminal::TerminalViewAction as Action;

    match action {
        Action::ScrollLines(_)
        | Action::ScrollPages(_)
        | Action::ScrollTop
        | Action::ScrollBottom
        | Action::ScrollToFraction(_)
        | Action::ScrollToOffset(_)
        | Action::ClearLinkHover
        | Action::EnterCopyMode
        | Action::EnterCopyModeScrollExit
        | Action::EnterCopyModeWith { .. } => true,
        Action::CopyMode(action) | Action::CopyModeCounted { action, .. } => {
            copy_mode_action_is_read_only_safe(action)
        }
        Action::SelectionPress(_)
        | Action::SelectionDrag(_)
        | Action::SelectionAutoscroll { .. }
        | Action::SelectionRelease(_)
        | Action::Mouse(_)
        | Action::ScrollWheel { .. }
        | Action::SelectAll
        | Action::ClearHistory
        | Action::ClearSelection
        | Action::SearchBegin(_)
        | Action::SearchUpdate(_)
        | Action::SearchNext
        | Action::SearchPrevious
        | Action::SearchClose
        | Action::CopySelection { .. }
        | Action::Focus(_)
        | Action::Paste(_) => false,
    }
}

fn terminal_view_action_is_mouse(action: &zz_terminal::TerminalViewAction) -> bool {
    matches!(
        action,
        zz_terminal::TerminalViewAction::Mouse(_)
            | zz_terminal::TerminalViewAction::ScrollWheel { .. }
    )
}

pub(super) fn terminal_mouse_rejected(
    inner: &ServerState,
    client: ClientId,
    input: &InputMessage,
) -> bool {
    let (terminal, action) = match input {
        InputMessage::TerminalView { pane, action } => (inner.terminals.get(pane), action),
        _ => return false,
    };
    if !terminal_view_action_is_mouse(action)
        || inner.client(client).is_none_or(|c| c.size.is_none())
        || inner
            .engine
            .effective_mouse(client_attached_session(inner, client))
    {
        return false;
    }
    !terminal.is_some_and(|terminal| {
        terminal
            .latest_viewport_for(TerminalViewId(client.0))
            .unwrap_or_else(|| terminal.latest_viewport())
            .mouse_tracking
    })
}
