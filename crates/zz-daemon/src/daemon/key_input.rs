use super::*;

impl Shared {
    pub(super) fn input_key(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane: PaneId,
        input: zz_terminal::KeyInput,
        text_follows: bool,
    ) -> Result<(), DaemonError> {
        let (choose_tree, choose_buffer, display_panes, read_only) = {
            let inner = self.inner.lock();
            let chooser = chooser_takes_keys(&inner, client);
            (
                chooser
                    && inner
                        .client(client)
                        .is_some_and(|c| c.choose_tree.is_some()),
                chooser
                    && inner
                        .client(client)
                        .is_some_and(|c| c.choose_buffer.is_some()),
                inner
                    .client(client)
                    .is_some_and(|c| c.display_panes.is_some()),
                inner.attachment.client_flags.contains(client),
            )
        };
        if choose_tree || choose_buffer || display_panes {
            if input.action == zz_terminal::KeyAction::Release {
                let _ = self.key_decision(client, &input_key_name(&input), true);
                return Ok(());
            }
            if choose_tree && !read_only {
                self.suppress_committed_character(
                    client,
                    pane,
                    CommittedTextLane::Terminal,
                    &input,
                    text_follows,
                );
                return self.input_choose_tree(client, kind, context, ChooseTreeAction::Key(input));
            }
            if choose_buffer && !read_only {
                self.suppress_committed_character(
                    client,
                    pane,
                    CommittedTextLane::Terminal,
                    &input,
                    text_follows,
                );
                return self.input_choose_buffer(
                    client,
                    kind,
                    context,
                    ChooseBufferAction::Key(input),
                );
            }
            if !read_only {
                return self.input_display_panes(
                    client,
                    kind,
                    context,
                    DisplayPanesAction::Key(input),
                    text_follows,
                );
            }
        }
        if !read_only
            && self.input_command_prompt_key(
                client,
                kind,
                context,
                pane,
                CommittedTextLane::Terminal,
                &input,
                text_follows,
            )
        {
            return Ok(());
        }
        if !read_only
            && input.action != zz_terminal::KeyAction::Release
            && self.inner.lock().engine.dead_pane_dismisses_on_key(pane)
        {
            let target = pane.to_string();
            self.execute_gesture(
                client,
                kind,
                context,
                "dead_pane_dismiss",
                &CommandInvocation::new("kill-pane", ["-t", target.as_str()]),
            )?;
            return Ok(());
        }
        let modal = (!read_only)
            .then(|| active_modal(&self.inner.lock(), pane))
            .flatten();
        if let Some(modal) = modal {
            let name = input_key_name(&input);
            if modal.close_on_cancel
                && input.action != zz_terminal::KeyAction::Release
                && matches!(name.as_str(), "Escape" | "C-c")
            {
                let target = pane.to_string();
                self.execute_gesture(
                    client,
                    kind,
                    context,
                    "modal_cancel",
                    &CommandInvocation::new("kill-pane", ["-t", target.as_str()]),
                )?;
                return Ok(());
            }
            if modal.capture_keys {
                self.note_terminal_input(client, pane);
                if input.action != zz_terminal::KeyAction::Release
                    && self.pane_mode_key(client, context, pane, &input, text_follows)
                {
                    return Ok(());
                }
                return self
                    .dispatch_input_key(client, pane, input)
                    .map_err(Into::into);
            }
        }
        if read_only {
            self.note_terminal_input_without_bell(client, pane);
        } else {
            self.note_terminal_input(client, pane);
        }
        if follow_command_output_focus(&mut self.inner.lock(), client, pane) {
            self.sync_key_table(client, false);
        }
        let key = input_key_name(&input);
        let release = input.action == zz_terminal::KeyAction::Release;
        let (decision, repeat_binding) = self.key_decision_with_repeat(client, &key, release);
        if decision != KeyDecision::Pass && input.modifiers == zz_terminal::Modifiers::default() {
            self.suppress_committed_character(
                client,
                pane,
                CommittedTextLane::Terminal,
                &input,
                text_follows,
            );
        }
        let result = match decision {
            KeyDecision::Pass => {
                if read_only || command_output_owns_pane(&self.inner.lock(), client, pane) {
                    if !read_only {
                        self.suppress_committed_character(
                            client,
                            pane,
                            CommittedTextLane::Terminal,
                            &input,
                            text_follows,
                        );
                    }
                    return Ok(());
                }
                if input.action != zz_terminal::KeyAction::Release
                    && self.pane_mode_key(client, context, pane, &input, text_follows)
                {
                    self.sync_key_table(client, false);
                    return Ok(());
                }
                self.dispatch_input_key(client, pane, input)
                    .map_err(Into::into)
            }
            KeyDecision::Prefix | KeyDecision::Ignore => Ok(()),
            KeyDecision::Commands(commands) => {
                let previous = context.invoking_key().map(str::to_owned);
                context.set_invoking_key(Some(key.as_str().to_owned()));
                let dispatched = self.execute_key_commands(
                    client,
                    kind,
                    context,
                    pane,
                    &commands,
                    repeat_binding,
                );
                context.set_invoking_key(previous);
                dispatched
            }
        };
        self.sync_key_table(client, !release);
        result
    }

    pub(super) fn input_mouse_key(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        key: &str,
        mouse: &MouseEventTarget,
    ) -> Result<(), DaemonError> {
        let base = key.rsplit('-').next().unwrap_or(key);
        let armed = self
            .inner
            .lock()
            .client_mut(client)
            .and_then(|registered| registered.mouse_drag.take());
        if let Some(drag) = armed {
            if base.starts_with("MouseDrag") && !base.starts_with("MouseDragEnd") {
                return self.update_mouse_drag(client, kind, context, drag, mouse);
            }
            if base.starts_with("Wheel")
                && let Some(registered) = self.inner.lock().client_mut(client)
            {
                registered.mouse_drag = Some(drag);
            }
        }
        let outside_modal = (!key.contains("Status"))
            .then(|| {
                let inner = self.inner.lock();
                let window = mouse
                    .window
                    .or_else(|| client_focused_window_for_attachment(&inner, client))?;
                inner.engine.state.windows.get(&window)?.floats.modal
            })
            .flatten()
            .filter(|modal| mouse.pane != Some(modal.pane));
        if let Some(modal) = outside_modal {
            if modal.close_on_click
                && ["MouseDown", "SecondClick", "TripleClick"]
                    .iter()
                    .any(|prefix| base.starts_with(prefix))
            {
                let target = modal.pane.to_string();
                self.execute_gesture(
                    client,
                    kind,
                    context,
                    "modal_click",
                    &CommandInvocation::new("kill-pane", ["-t", target.as_str()]),
                )?;
            }
            return Ok(());
        }
        let (pane, window) = (mouse.pane, mouse.window);
        let root_was_first;
        let Some((commands, repeat_binding, session, window, pane)) = ({
            let inner = self.inner.lock();
            let Some(session) = client_attached_session(&inner, client) else {
                return Ok(());
            };
            let root_table = inner.engine.key_table_for_session(session);
            let active_table = inner
                .client(client)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table)
                .map(str::to_owned);
            let mode_table = active_table
                .as_deref()
                .is_none_or(|table| table == root_table)
                .then(|| mouse_mode_key_table(&inner, client, pane))
                .flatten();
            root_was_first = mode_table.is_none()
                && active_table
                    .as_deref()
                    .is_none_or(|table| table == root_table);
            let binding = active_table
                .as_deref()
                .filter(|table| *table != root_table)
                .and_then(|table| inner.engine.keys.get(table, key))
                .or_else(|| {
                    mode_table
                        .as_deref()
                        .and_then(|table| inner.engine.keys.get(table, key))
                })
                .or_else(|| inner.engine.keys.get(&root_table, key));
            binding.map(|binding| {
                let window =
                    window.or_else(|| client_focused_window_for_attachment(&inner, client));
                let pane = pane.or_else(|| {
                    window
                        .and_then(|window| inner.engine.state.windows.get(&window))
                        .map(|window| window.active_pane)
                });
                (
                    binding.commands.clone(),
                    binding.repeat,
                    session,
                    window,
                    pane,
                )
            })
        }) else {
            if root_was_first {
                self.forward_mouse_key_to_pane(client, key, mouse);
            }
            return Ok(());
        };
        let Some(pane) = pane else {
            return Ok(());
        };
        context.retarget(&ExecutionContext::new(Some(session), window, Some(pane)));
        let previous = context.invoking_key().map(str::to_owned);
        let previous_mouse = context.invoking_mouse().cloned();
        let previous_variables = context.format_variables.clone();
        let mouse = MouseEventTarget {
            pane: Some(pane),
            window,
            ..mouse.clone()
        };
        let (variables, probe) =
            mouse_format_variables(&self.inner.lock(), &mouse, TerminalViewId(client.0));
        context.format_variables.extend(variables);
        context.set_invoking_key(Some(key.to_owned()));
        context.set_invoking_mouse(Some(mouse));
        let continuation = cmdq::WaitContinuation::new(None, None);
        #[cfg(unix)]
        if self.loop_active.load(Ordering::Acquire)
            && let Some(item) = &self.command_item
        {
            item.lock().pending_wait = Some(Box::new(RegisteredWait {
                name: String::new(),
                continuation: continuation.clone(),
                #[cfg(unix)]
                shell: None,
                overlay: None,
                leaf: None,
                guard: None,
                terminal: None,
                file: None,
            }));
        }
        let result = if let Some(probe) = probe {
            let request = probe.terminal.pointer_context_request(
                probe.view,
                probe.column,
                probe.row,
                self.terminal_requests.notifier(),
            );
            let done = continuation.clone();
            let mut captured_context = context.clone();
            let terminal = probe.terminal;
            self.terminal_requests
                .submit(request, move |shared, result| {
                    let valid = {
                        let inner = shared.inner.lock();
                        inner.client(client).is_some()
                            && client_is_attached_to_pane(&inner, client, pane)
                            && inner
                                .terminals
                                .get(&pane)
                                .is_some_and(|current| Arc::ptr_eq(current, &terminal))
                    };
                    if valid {
                        captured_context
                            .format_variables
                            .extend(pointer_format_variables(result.unwrap_or_default()));
                        #[cfg(unix)]
                        if shared.loop_active.load(Ordering::Acquire) {
                            shared.resume_mouse_binding(terminal_reads::MouseBinding {
                                client,
                                kind,
                                pane,
                                terminal: Some(terminal),
                                context: captured_context,
                                commands,
                                repeat_binding,
                                done,
                            });
                            return;
                        }
                        if let Err(error) = shared.execute_key_commands(
                            client,
                            kind,
                            &mut captured_context,
                            pane,
                            &commands,
                            repeat_binding,
                        ) {
                            log::warn!("mouse binding failed: {error}");
                        }
                        shared.sync_key_table(client, false);
                    }
                    done.complete();
                });
            self.terminal_requests.finish_off_loop(self, &continuation);
            Ok(())
        } else {
            #[cfg(unix)]
            if self.loop_active.load(Ordering::Acquire) {
                self.resume_mouse_binding(terminal_reads::MouseBinding {
                    client,
                    kind,
                    pane,
                    terminal: None,
                    context: context.clone(),
                    commands,
                    repeat_binding,
                    done: continuation,
                });
                Ok(())
            } else {
                self.execute_key_commands(client, kind, context, pane, &commands, repeat_binding)
            }
            #[cfg(not(unix))]
            self.execute_key_commands(client, kind, context, pane, &commands, repeat_binding)
        };
        context.set_invoking_key(previous);
        context.set_invoking_mouse(previous_mouse);
        context.format_variables = previous_variables;
        self.sync_key_table(client, false);
        result
    }

    fn update_mouse_drag(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        drag: zz_mux::MouseDrag,
        mouse: &MouseEventTarget,
    ) -> Result<(), DaemonError> {
        let (session, window) = {
            let inner = self.inner.lock();
            let Some(session) = client_attached_session(&inner, client) else {
                return Ok(());
            };
            let Some(window) = inner.engine.state.window_for_pane(drag.pane) else {
                return Ok(());
            };
            (session, window)
        };
        let mut context = context.clone();
        context.retarget(&ExecutionContext::new(
            Some(session),
            Some(window),
            Some(drag.pane),
        ));
        context.no_hooks = true;
        context.set_invoking_mouse(Some(MouseEventTarget {
            pane: Some(drag.pane),
            window: Some(window),
            drag: Some(drag),
            ..mouse.clone()
        }));
        let target = drag.pane.to_string();
        self.execute(
            client,
            kind,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-M", "-t", target.as_str()]),
        )?;
        Ok(())
    }

    fn forward_mouse_key_to_pane(
        self: &Arc<Self>,
        client: ClientId,
        key: &str,
        mouse: &MouseEventTarget,
    ) {
        let Some(pane) = mouse.pane else {
            return;
        };
        let terminal = {
            let inner = self.inner.lock();
            if inner.attachment.client_flags.contains(client)
                || !client_is_attached_to_pane(&inner, client, pane)
            {
                None
            } else {
                inner.terminals.get(&pane).cloned()
            }
        };
        let Some(terminal) = terminal else {
            return;
        };
        let consumed = {
            let mut inner = self.inner.lock();
            consume_pane_mode_mouse(&mut inner, pane, mouse.view_action.as_ref()).then(|| {
                (
                    mouse_pane_cell(&inner, pane, mouse),
                    client_attached_session(&inner, client),
                )
            })
        };
        if let Some((cell, session)) = consumed {
            if let Some((x, y)) = cell {
                let mut context = ExecutionContext::new(session, mouse.window, Some(pane));
                self.pane_mode_mouse(client, &mut context, pane, key, x, y);
            } else {
                let events = take_pane_mode_hook_events(&mut self.inner.lock());
                self.run_event_hooks(events);
            }
            self.publish_mux_snapshots();
            return;
        }
        let Some(action @ zz_terminal::TerminalViewAction::Mouse(_)) = mouse.view_action.clone()
        else {
            return;
        };
        self.note_terminal_input(client, pane);
        terminal.view_action(TerminalViewId(client.0), action);
    }

    pub(super) fn input_browser_surface_key(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane: PaneId,
        input: zz_terminal::KeyInput,
        text_follows: bool,
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
        };
        if blocked {
            self.suppress_committed_character(
                client,
                pane,
                CommittedTextLane::BrowserSurface,
                &input,
                text_follows,
            );
            return Ok(());
        }
        if self.input_command_prompt_key(
            client,
            kind,
            context,
            pane,
            CommittedTextLane::BrowserSurface,
            &input,
            text_follows,
        ) {
            return Ok(());
        }
        if command_output_owns_pane(&self.inner.lock(), client, pane) {
            self.suppress_committed_character(
                client,
                pane,
                CommittedTextLane::BrowserSurface,
                &input,
                text_follows,
            );
            return Ok(());
        }
        self.note_terminal_input(client, pane);
        self.dispatch_input_key(client, pane, input)
            .map_err(Into::into)
    }

    pub(super) fn plain_key_generation(
        &self,
        client: ClientId,
        pane: PaneId,
        input: &zz_terminal::KeyInput,
    ) -> Option<u64> {
        if !matches!(
            input.action,
            zz_terminal::KeyAction::Press | zz_terminal::KeyAction::Repeat
        ) {
            return None;
        }
        plain_key_generation_locked(&self.inner.lock(), client, pane, input)
    }

    pub(super) fn tty_key_generation(
        &self,
        client: ClientId,
        pane: PaneId,
        input: &zz_terminal::KeyInput,
        bytes: &[u8],
    ) -> Option<u64> {
        let inner = self.inner.lock();
        if client_active_pane(&inner, client) != Some(pane) || inner.engine.user_key_claims(bytes) {
            return None;
        }
        plain_key_generation_locked(&inner, client, pane, input)
    }

    pub(super) fn tty_input_left(&self, client: ClientId) -> bool {
        self.inner
            .lock()
            .client(client)
            .is_some_and(|c| c.exits_on_detach && c.detached)
    }

    pub(super) fn input_plain_key(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
        input: zz_terminal::KeyInput,
        generation: u64,
    ) -> Result<(), DaemonError> {
        self.note_terminal_input(client, pane);
        let result = self.dispatch_input_key(client, pane, input);
        let publish = {
            let inner = self.inner.lock();
            let current = inner.engine.state.generation();
            current != generation && inner.publication.last_published_mux_generation != current
        };
        if publish {
            self.publish_snapshot();
        }
        result.map_err(Into::into)
    }

    fn dispatch_input_key(
        &self,
        client: ClientId,
        pane: PaneId,
        input: zz_terminal::KeyInput,
    ) -> Result<(), ServerError> {
        let mut sinks = self.input_sinks(client, pane)?.into_iter().peekable();
        let mut owned_input = Some(input);
        while let Some(sink) = sinks.next() {
            let sink_input = if sinks.peek().is_none() {
                owned_input.take().expect("last sink owns key input")
            } else {
                owned_input
                    .as_ref()
                    .expect("key input is retained until the last sink")
                    .clone()
            };
            match sink {
                PaneSink::Terminal(terminal) => {
                    terminal.send_key_for_view(TerminalViewId(client.0), sink_input);
                }
                PaneSink::Browser(target) => self.publish_for_pane(
                    target,
                    &EventPayload::BrowserCommand {
                        pane: target,
                        command: BrowserCommand::Key(sink_input),
                    },
                ),
            }
        }
        Ok(())
    }

    pub(super) fn execute_gesture(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        gesture: &str,
        command: &CommandInvocation,
    ) -> Result<(), DaemonError> {
        let error = match self.execute(client, kind, context, command) {
            Ok(execution) => {
                if kind == ClientKind::Interactive && !execution.output.is_empty() {
                    self.open_command_output(
                        client,
                        context.pane,
                        command.name.clone(),
                        &execution.output,
                    )?;
                }
                return Ok(());
            }
            Err(error) => error,
        };
        if kind == ClientKind::Interactive
            && let Some(output) = daemon_error_output(&error)
        {
            self.open_command_output(client, context.pane, command.name.clone(), output)?;
        }
        log::warn!(
            target: "zz_daemon::diagnostics::input",
            "gesture_command_failed client={client} gesture={gesture} command={} args={:?} error={error}",
            command.name,
            command.args,
        );
        self.publish_to_client(
            client,
            EventPayload::ClientMessage {
                pane: context.pane,
                kind: ClientMessageKind::Error,
                text: error.to_string(),
            },
        );
        Err(error)
    }

    pub(super) fn inject_client_keys(
        self: &Arc<Self>,
        target: ClientId,
        keys: &[zz_protocol::KeyToken],
        repeat: u32,
    ) {
        let inputs = keys.iter().flat_map(client_key_inputs).collect::<Vec<_>>();
        if inputs.is_empty() {
            return;
        }
        let entered = {
            let mut item = self.command_item.as_ref().expect("command item").lock();
            let entered = item.client_key_injection_depth;
            if entered < MAX_CLIENT_KEY_INJECTION_DEPTH {
                item.client_key_injection_depth += 1;
            }
            entered
        };
        if entered >= MAX_CLIENT_KEY_INJECTION_DEPTH {
            log::warn!(
                target: "zz_daemon::diagnostics::input",
                "send-keys -K stopped at injection depth {entered} for client={target}"
            );
            return;
        }
        for _ in 0..repeat {
            for input in &inputs {
                let Some((kind, pane, mut context)) = ({
                    let inner = self.inner.lock();
                    client_context_pane(&inner, target).map(|pane| {
                        let mut context = ExecutionContext::default();
                        retarget_context_to_attachment(&inner, target, &mut context);
                        if let Some(registered) = inner.client(target) {
                            registered.seed_context_environment(&mut context);
                        }
                        let kind = inner
                            .client(target)
                            .and_then(|c| c.kind)
                            .unwrap_or(ClientKind::Interactive);
                        (kind, pane, context)
                    })
                }) else {
                    break;
                };
                if let Err(error) =
                    self.input_key(target, kind, &mut context, pane, input.clone(), false)
                {
                    log::warn!(
                        target: "zz_daemon::diagnostics::input",
                        "send-keys -K injection failed client={target} error={error}"
                    );
                }
            }
        }
        let mut item = self.command_item.as_ref().expect("command item").lock();
        item.client_key_injection_depth = item.client_key_injection_depth.saturating_sub(1);
    }

    pub(super) fn execute_key_commands(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane: PaneId,
        commands: &[CommandInvocation],
        repeat_binding: bool,
    ) -> Result<(), DaemonError> {
        self.execution_item().execute_key_commands_in_item(
            client,
            kind,
            context,
            pane,
            commands,
            repeat_binding,
        )
    }

    fn execute_key_commands_in_item(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane: PaneId,
        commands: &[CommandInvocation],
        repeat_binding: bool,
    ) -> Result<(), DaemonError> {
        let mut output = String::new();
        let mut output_truncated = false;
        let (read_only_commands, blocked) = {
            let mut inner = self.inner.lock();
            if inner.attachment.client_flags.contains(client) {
                let commands = commands.to_vec();
                let blocked = commands
                    .iter()
                    .any(|command| {
                        read_only_guard_client(&inner, client, command)
                            .is_some_and(|guarded| inner.attachment.client_flags.contains(guarded))
                            && !command_is_read_only_safe(command)
                    })
                    .then(|| {
                        let duration_ms = client_attached_session(&inner, client)
                            .map_or(750, |session| {
                                inner.engine.display_time_for_session(session)
                            });
                        (duration_ms, next_timed_message_id(&mut inner))
                    });
                (Some(commands), blocked)
            } else {
                (None, None)
            }
        };
        if let Some((duration_ms, message_id)) = blocked {
            self.publish_to_client(
                client,
                EventPayload::TimedClientMessage {
                    pane: Some(pane),
                    kind: ClientMessageKind::Error,
                    text: "client is read-only".to_owned(),
                    duration_ms,
                    message_id,
                },
            );
            return Ok(());
        }
        #[cfg(unix)]
        if self.hand_off_key_commands(client, kind, context, commands, repeat_binding) {
            return Ok(());
        }
        let mut first_error = None;
        let mut failed_group = None;
        for (index, original) in commands.iter().enumerate() {
            let group = original
                .source
                .as_ref()
                .map(|source| (source.source.clone(), source.line));
            if let Some(failed) = &failed_group {
                if failed == &group {
                    continue;
                }
                failed_group = None;
            }
            let command = if let Some(commands) = &read_only_commands {
                commands[index].clone()
            } else {
                original.clone()
            };
            let previous_repeat_binding = context.repeat_binding();
            context.set_repeat_binding(repeat_binding);
            let execution = self.execute_with_mux_source_routed(
                client,
                kind,
                context,
                &command,
                MuxOptionSource::RuntimeCommand,
            );
            context.set_repeat_binding(previous_repeat_binding);
            let execution = match execution {
                Ok(execution) => execution,
                Err(error) => {
                    let reported_exit = matches!(&error, DaemonError::ReportedCommandExit { .. });
                    let command_exit =
                        reported_exit || matches!(&error, DaemonError::CommandExit { .. });
                    if let Some(error_output) = daemon_error_output(&error)
                        && !output_truncated
                    {
                        output_truncated = append_command_prompt_output(&mut output, error_output);
                    }
                    log::warn!(
                        target: "zz_daemon::diagnostics::input",
                        "key_command_failed client={client} command={} args={:?} error={error}",
                        command.name,
                        command.args,
                    );
                    if !reported_exit {
                        failed_group = Some(group);
                    }
                    if !command_exit && first_error.is_none() {
                        first_error = Some(error);
                    }
                    continue;
                }
            };
            if !execution.output.is_empty() && !output_truncated {
                output_truncated = append_command_prompt_output(&mut output, &execution.output);
            }
        }
        if output_truncated {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(COMMAND_PROMPT_OUTPUT_TRUNCATED);
        }
        if kind == ClientKind::Interactive && !output.is_empty() {
            let title = commands.first().map_or_else(
                || "command output".to_owned(),
                |command| {
                    if commands.len() == 1 {
                        command.name.clone()
                    } else {
                        "command output".to_owned()
                    }
                },
            );
            self.open_command_output(client, Some(pane), title, &output)?;
        }
        first_error.map_or(Ok(()), Err)
    }

    pub(super) fn sync_key_table(&self, client: ClientId, force: bool) {
        let now = Instant::now();
        let mut inner = self.inner.lock();
        let session_table = client_attached_session(&inner, client)
            .map(|session| inner.engine.key_table_for_session(session));
        let engine = inner.client(client).and_then(|c| c.key_engine.as_ref());
        let shown = engine
            .and_then(|engine| engine.shown_table(now))
            .filter(|(table, _)| session_table.as_deref() != Some(*table))
            .map(|(table, repeat)| (Some(table.to_owned()), repeat))
            .unwrap_or_default();
        let deadline = engine
            .and_then(KeyEngine::next_deadline)
            .filter(|deadline| *deadline > now);
        let rescheduled = match deadline {
            Some(deadline) => {
                inner
                    .client_entry(client)
                    .key_table_deadline
                    .replace(deadline)
                    != Some(deadline)
            }
            None => inner
                .client_mut(client)
                .and_then(|c| c.key_table_deadline.take())
                .is_some(),
        };
        if rescheduled {
            let _ = self.timer_tx.send(timers::TimerInput::KeyTable(
                KeyTableDeadlineCommand::Schedule(client, deadline),
            ));
        }
        let previous = if shown.0.is_some() {
            inner
                .client_entry(client)
                .published_key_table
                .replace(shown.clone())
        } else {
            inner
                .client_mut(client)
                .and_then(|c| c.published_key_table.take())
        }
        .unwrap_or_default();
        let is_prefix = |table: &Option<String>| table.as_deref() == Some("prefix");
        let armed_changed = is_prefix(&previous.0) != is_prefix(&shown.0);
        let table_changed = previous != shown || (force && shown.0.is_some());
        let Some(subscriber) = inner.client(client).and_then(|c| c.subscriber.as_ref()) else {
            return;
        };
        if armed_changed {
            let armed = is_prefix(&shown.0);
            log::debug!(
                target: "zz_daemon::diagnostics::input",
                "prefix_armed_published client={client} armed={armed}"
            );
            Self::send_event(subscriber, EventPayload::PrefixArmed { armed });
        }
        if table_changed {
            let (table, repeat) = shown;
            Self::send_event(subscriber, EventPayload::KeyTableActive { table, repeat });
        }
    }

    pub(super) fn cancel_prefix(&self, client: ClientId, request_id: u64) {
        {
            let mut inner = self.inner.lock();
            if inner
                .client(client)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table)
                == Some("prefix")
            {
                inner
                    .client_entry(client)
                    .key_engine
                    .get_or_insert_default()
                    .cancel_prefix();
            }
        }
        self.sync_key_table(client, false);
        self.publish_to_client(client, EventPayload::PrefixCancelled { request_id });
    }

    pub(super) fn key_decision(&self, client: ClientId, key: &str, release: bool) -> KeyDecision {
        self.key_decision_with_repeat(client, key, release).0
    }

    pub(super) fn key_decision_with_repeat(
        &self,
        client: ClientId,
        key: &str,
        release: bool,
    ) -> (KeyDecision, bool) {
        self.resolve_key_decision(client, key, release, KeyEngine::handle_with_repeat_metadata)
    }

    pub(super) fn overlay_key_decision(&self, client: ClientId, key: &str) -> (KeyDecision, bool) {
        self.resolve_key_decision(
            client,
            key,
            false,
            KeyEngine::handle_overlay_with_repeat_metadata,
        )
    }

    fn resolve_key_decision(
        &self,
        client: ClientId,
        key: &str,
        release: bool,
        handle: KeyEngineHandler,
    ) -> (KeyDecision, bool) {
        let mut inner = self.inner.lock();
        if release {
            return if inner
                .client_mut(client)
                .and_then(|c| c.swallowed_keys.as_mut())
                .is_some_and(|keys| keys.remove(bare_key_name(key)))
            {
                (KeyDecision::Prefix, false)
            } else {
                (KeyDecision::Pass, false)
            };
        }
        let mut key_engine = inner
            .client_mut(client)
            .and_then(|c| c.key_engine.take())
            .unwrap_or_default();
        let table = key_engine.active_table().map(str::to_owned);
        let prefix_timeout_ms = inner.engine.prefix_timeout_ms();
        let (initial_repeat_time_ms, repeat_time_ms, root_table) = client_attached_session(
            &inner, client,
        )
        .map_or((0, 500, "root".to_owned()), |session| {
            (
                inner.engine.initial_repeat_time_for_session(session),
                inner.engine.repeat_time_for_session(session),
                inner.engine.key_table_for_session(session),
            )
        });
        let (decision, repeat_binding) = handle(
            &mut key_engine,
            &inner.engine.keys,
            key,
            Instant::now(),
            Duration::from_millis(u64::from(repeat_time_ms)),
            Duration::from_millis(u64::from(initial_repeat_time_ms)),
            Duration::from_millis(u64::from(prefix_timeout_ms)),
            &root_table,
        );
        inner.client_entry(client).key_engine.replace(key_engine);
        if decision != KeyDecision::Pass {
            inner
                .client_entry(client)
                .swallowed_keys
                .get_or_insert_default()
                .insert(bare_key_name(key).to_owned());
        }
        drop(inner);
        if decision != KeyDecision::Pass {
            log::debug!(
                target: "zz_daemon::diagnostics::input",
                "key_decision client={client} key={key} table={} decision={decision:?}",
                table.as_deref().unwrap_or(&root_table)
            );
        }
        (decision, repeat_binding)
    }
}

fn bare_key_name(key: &str) -> &str {
    let mut bare = key;
    while let Some(rest) = bare.strip_prefix("C-").or_else(|| bare.strip_prefix("M-")) {
        bare = rest;
    }
    bare
}

type KeyEngineHandler = fn(
    &mut KeyEngine,
    &KeyTables,
    &str,
    Instant,
    Duration,
    Duration,
    Duration,
    &str,
) -> (KeyDecision, bool);

struct PointerProbe {
    terminal: Arc<TerminalSession>,
    view: TerminalViewId,
    column: u16,
    row: u16,
}

fn mouse_format_variables(
    inner: &ServerState,
    mouse: &MouseEventTarget,
    view: TerminalViewId,
) -> (BTreeMap<String, String>, Option<PointerProbe>) {
    let mut variables = BTreeMap::new();
    let Some(pane) = mouse.pane else {
        return (variables, None);
    };
    variables.insert("mouse_pane".to_owned(), pane.to_string());
    let Some(window) = inner.engine.state.window_for_pane(pane) else {
        return (variables, None);
    };
    let geometry = inner
        .engine
        .pane_format_geometry(None, Some(window), Some(pane));
    let mut probe = None;
    if let (Some(left), Some(top)) = (geometry.pane_left, geometry.pane_top)
        && let (Some(x), Some(y)) = (mouse.column.checked_sub(left), mouse.row.checked_sub(top))
    {
        variables.insert("mouse_x".to_owned(), x.to_string());
        variables.insert("mouse_y".to_owned(), y.to_string());
        if geometry
            .pane_right
            .is_some_and(|right| mouse.column <= right)
            && geometry
                .pane_bottom
                .is_some_and(|bottom| mouse.row <= bottom)
        {
            probe = inner
                .terminals
                .get(&pane)
                .cloned()
                .map(|terminal| PointerProbe {
                    terminal,
                    view,
                    column: x,
                    row: y,
                });
        }
    }
    (variables, probe)
}

fn pointer_format_variables(context: zz_terminal::PointerContext) -> BTreeMap<String, String> {
    let mut variables = BTreeMap::new();
    for (name, value) in [
        ("mouse_word", context.word),
        ("mouse_line", context.line),
        ("mouse_hyperlink", context.hyperlink),
    ] {
        if !value.is_empty() {
            variables.insert(name.to_owned(), value);
        }
    }
    variables
}

fn plain_key_generation_locked(
    inner: &ServerState,
    client: ClientId,
    pane: PaneId,
    input: &zz_terminal::KeyInput,
) -> Option<u64> {
    let registered = inner.client(client)?;
    if inner.attachment.client_flags.contains(client)
        || registered.message.is_some()
        || registered.command_prompt.is_some()
        || registered.choose_tree.is_some()
        || registered.choose_buffer.is_some()
        || registered.display_panes.is_some()
        || registered.menu.is_some()
        || registered.confirm.is_some()
        || registered.copy_session.is_some()
        || registered.command_output.is_some()
        || registered.published_key_table.is_some()
        || registered.key_table_deadline.is_some()
        || registered
            .key_engine
            .as_ref()
            .is_some_and(|engine| *engine != KeyEngine::default())
        || inner
            .modes
            .pane_modes
            .get(&pane)
            .is_some_and(|modes| !modes.is_empty())
        || inner.engine.dead_pane_dismisses_on_key(pane)
        || active_modal(inner, pane).is_some_and(|modal| modal.close_on_cancel)
    {
        return None;
    }
    let session = client_attached_session(inner, client)?;
    let (decision, _) = KeyEngine::default().handle_with_repeat_metadata(
        &inner.engine.keys,
        &input_key_name(input),
        Instant::now(),
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        &inner.engine.key_table_for_session(session),
    );
    (decision == KeyDecision::Pass).then(|| inner.engine.state.generation())
}

fn mouse_mode_key_table(
    inner: &ServerState,
    client: ClientId,
    pane: Option<PaneId>,
) -> Option<String> {
    let pane = pane?;
    let in_mode = inner
        .client(client)
        .and_then(|c| c.copy_session.as_ref())
        .is_some_and(|copy| copy.pane == pane && !copy.exiting);
    if !in_mode {
        return None;
    }
    inner
        .engine
        .copy_mode_table_for_pane(pane)
        .ok()
        .map(str::to_owned)
}

fn active_modal(inner: &ServerState, pane: PaneId) -> Option<zz_mux::Modal> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let window = inner.engine.state.windows.get(&window)?;
    let mut modal = window.floats.modal?;
    if modal.pane != pane || window.active_pane != pane {
        return None;
    }
    modal.capture_keys &= inner
        .terminals
        .get(&pane)
        .is_some_and(|terminal| terminal.completion().is_none());
    Some(modal)
}
