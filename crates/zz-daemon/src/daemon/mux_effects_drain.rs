use super::*;

impl Shared {
    #[expect(clippy::option_option, reason = "the effect carries an optional path")]
    pub(super) fn drain_server_effects(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        queue_execution: Option<&CommandQueueExecution>,
        execution: &mut Execution,
        import_tmux_config: Option<Option<String>>,
        reload_config: &mut bool,
        force_shutdown_requested: bool,
        immediate_hooks: Vec<(String, Vec<Vec<CommandInvocation>>, ExecutionContext)>,
        recheck_shutdown_requested: bool,
    ) -> Result<(), DaemonError> {
        if let Some(path) = import_tmux_config {
            let output = self.import_tmux_configuration(path.as_deref(), context)?;
            append_inserted_output(&mut execution.output, &output);
            *reload_config = true;
        }

        if force_shutdown_requested {
            if let Some((control, _)) = context.control_command_target() {
                if let Some(queue_execution) = queue_execution
                    && queue_execution.detached
                {
                    queue_execution.deferred_control_exit.set(Some(control));
                } else {
                    self.publish_to_client(
                        control,
                        EventPayload::ControlExit {
                            reason: String::new(),
                        },
                    );
                }
            }
            if let Some(queue_execution) = queue_execution
                && !queue_execution.ensure_shutdown_blocker(self)
            {
                return Err(
                    ServerError::InvalidCommand("server is shutting down".to_owned()).into(),
                );
            }
            self.response_admissions.lock().frozen = true;
            if queue_execution.is_some() {
                self.begin_queue_shutdown();
            }
        }

        for (name, commands, context) in immediate_hooks {
            let mut variables = context.format_variables.clone();
            if name.starts_with('@')
                && variables.get("hook_event") == Some(&name)
                && client != ClientId(u64::MAX)
            {
                let client_name = client_format_name(&self.inner.lock(), client);
                variables.insert("hook_client".to_owned(), client_name);
            }
            if variables.get("hook_event") == Some(&name) {
                self.feed_event_waiters(&name, &variables);
            }
            variables.insert("hook".to_owned(), name);
            let hook_output = self.run_hook_commands(
                client,
                kind,
                &context,
                commands,
                &variables,
                queue_execution,
            );
            append_inserted_output(&mut execution.output, &hook_output);
        }

        let deferred_shutdown_request = if force_shutdown_requested {
            DeferredShutdown::Force
        } else if recheck_shutdown_requested {
            DeferredShutdown::Recheck
        } else {
            DeferredShutdown::None
        };
        if deferred_shutdown_request != DeferredShutdown::None {
            if let Some(queue_execution) = queue_execution {
                queue_execution.deferred_shutdown.set(
                    queue_execution
                        .deferred_shutdown
                        .get()
                        .max(deferred_shutdown_request),
                );
            } else if deferred_shutdown_request == DeferredShutdown::Force {
                self.request_shutdown();
            } else {
                self.request_shutdown_without_hooks();
            }
        }

        Ok(())
    }

    pub(super) fn drain_terminal_effects(
        self: &Arc<Self>,
        client: ClientId,
        format_variables: BTreeMap<String, String>,
        pipes_to_close: Vec<PanePipe>,
        key_listing: Option<(zz_mux::KeyListing, FormatHookFacts)>,
        deferred_terminal_commands: Vec<DeferredTerminalCommand>,
        terminal_wait: &mut Option<terminal_requests::CommandWait>,
        respawned_terminals: bool,
        snapshot_changed: bool,
    ) {
        for pipe in pipes_to_close {
            stop_pane_pipe(pipe);
        }

        if let Some((listing, facts)) = key_listing {
            let wait =
                terminal_wait.get_or_insert_with(|| terminal_requests::CommandWait::new(self));
            cmdq::render_key_listing(self, client, listing, facts, format_variables, wait.start());
        }
        if deferred_terminal_commands.iter().any(|command| {
            matches!(command, DeferredTerminalCommand::ArmCopySource { .. })
                || command.copy_mode_terminal().is_some()
        }) {
            let wait =
                terminal_wait.get_or_insert_with(|| terminal_requests::CommandWait::new(self));
            wait.effects(self, deferred_terminal_commands);
        } else {
            for command in deferred_terminal_commands {
                #[cfg(test)]
                let wrap_search = command.wrap_search();
                command.run();
                #[cfg(test)]
                if let Some(enabled) = wrap_search {
                    self.delivered_wrap_search_commands.lock().push(enabled);
                }
            }
        }
        if respawned_terminals && !snapshot_changed {
            self.refresh_terminal_visibility();
        }
    }

    pub(super) fn drain_mode_effects(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane_mode_keys: Vec<(ClientId, PaneId, Vec<zz_protocol::KeyToken>, u32)>,
        pane_mode_pointers: Vec<(PaneId, String, usize, usize)>,
        injected_client_keys: Vec<(ClientId, Vec<zz_protocol::KeyToken>, u32)>,
        mode_table_keys: Vec<(Vec<ClientId>, PaneId, Vec<zz_protocol::KeyToken>, u32)>,
        refresh_armed: bool,
        unfocused_copy_mode_exits: Vec<(ClientId, Arc<TerminalSession>)>,
        retired_command_outputs: Vec<(ClientId, RetiredCommandOutput)>,
        relocated_terminal_views: Vec<(
            Arc<TerminalSession>,
            BTreeSet<ClientId>,
            BTreeSet<ClientId>,
        )>,
    ) -> Result<(), DaemonError> {
        for (selected, pane, keys, repeat) in pane_mode_keys {
            self.inject_pane_mode_keys(selected, context, pane, &keys, repeat)?;
        }
        for (pane, key, x, y) in pane_mode_pointers {
            self.pane_mode_mouse(client, context, pane, &key, x, y);
        }
        self.reap_pane_mode_kill_panes(client, kind, context);
        for (target, keys, repeat) in injected_client_keys {
            self.inject_client_keys(target, &keys, repeat);
        }
        for (owners, pane, keys, repeat) in mode_table_keys {
            self.inject_mode_table_keys(&owners, pane, &keys, repeat);
        }
        if refresh_armed {
            self.arm_copy_mode_refresh();
        }
        if clock_modes_are_open(&self.inner.lock()) {
            self.arm_pane_mode_clock();
        }
        for (client, terminal) in unfocused_copy_mode_exits {
            terminal.view_action(
                TerminalViewId(client.0),
                zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
            );
        }
        for (client, output) in retired_command_outputs {
            self.retire_command_output(client, output);
        }
        for (terminal, previous, next) in relocated_terminal_views {
            for client in previous.difference(&next) {
                terminal.detach_view(TerminalViewId(client.0));
            }
            for client in next.difference(&previous) {
                terminal.attach_view(TerminalViewId(client.0));
            }
        }
        Ok(())
    }

    pub(super) fn drain_detach_effect(
        self: &Arc<Self>,
        detach: Option<(ResolvedDetach, DetachExit)>,
        event_hooks_enabled: bool,
    ) {
        match detach {
            None => {}
            Some((_, exit)) if exit.exec.as_deref() == Some("") => {}
            Some((scope, exit)) => {
                let victims = requested_detach_victims(&self.inner.lock(), scope);
                for (session, victim) in victims {
                    if exit.exec.is_some()
                        && self.read_client(victim, |c| c.and_then(|c| c.kind))
                            == Some(ClientKind::Control)
                    {
                        continue;
                    }
                    let action = self.client_exit_action(session, &exit);
                    self.publish_to_client(
                        victim,
                        EventPayload::detached_requested_with_action(session, None, action),
                    );
                    self.detach_with_event_hooks(victim, event_hooks_enabled);
                }
            }
        }
    }

    pub(super) fn drain_attach_effect(
        self: &Arc<Self>,
        client: ClientId,
        invoking_client_terminal: ClientTerminal,
        event_hooks_enabled: bool,
        attach: Option<(SessionId, bool, bool, bool, Option<String>, bool)>,
        pending_hook_events: &mut Vec<PendingHookEvent>,
    ) -> Result<(), DaemonError> {
        if let Some((
            session,
            detach_others,
            detach_others_hangup,
            read_only,
            flags,
            update_environment,
        )) = attach
        {
            let eviction = if detach_others_hangup {
                ClientExitAction::ParentHangup
            } else {
                ClientExitAction::Exit
            };
            self.set_requested_client_flags(client, flags.as_deref(), read_only);
            if invoking_client_terminal == ClientTerminal::Absent {
                return Err(ServerError::InvalidCommand(
                    "open terminal failed: not a terminal".to_owned(),
                )
                .into());
            }
            if update_environment {
                let mut inner = self.inner.lock();
                if let Some(environment) = inner
                    .client(client)
                    .and_then(Client::local_environment)
                    .cloned()
                {
                    inner
                        .engine
                        .update_session_environment_from_client(session, environment.map())?;
                }
            }
            if invoking_client_terminal == ClientTerminal::Present {
                let _held = self.hold_attach_terminals(client);
                let (mut snapshot, attach_hook_events) =
                    self.attach_collect_event_hooks(client, session, event_hooks_enabled)?;
                pending_hook_events.extend(attach_hook_events);
                if detach_others {
                    self.evict_clients_with_event_hooks(
                        Some(session),
                        client,
                        event_hooks_enabled,
                        &eviction,
                    );
                    let inner = self.inner.lock();
                    snapshot = inner.engine.state.snapshot();
                    let presence = snapshot_presence(&inner);
                    stamp_snapshot_for_client(&inner, client, &mut snapshot, &presence);
                }
                let outbound =
                    self.read_client(client, |c| c.and_then(|c| c.subscriber.as_ref()).cloned());
                if let Some(outbound) = outbound {
                    self.send_attached(client, &outbound, session, snapshot);
                }
                self.publish_snapshot();
            } else if detach_others {
                self.evict_clients_with_event_hooks(
                    Some(session),
                    client,
                    event_hooks_enabled,
                    &eviction,
                );
                self.publish_snapshot();
            }
        }
        Ok(())
    }

    pub(super) fn drain_watched_terminals(
        self: &Arc<Self>,
        terminals_to_watch: Vec<(PaneId, Arc<TerminalSession>)>,
        snapshot_changed: &mut bool,
    ) -> Result<(), DaemonError> {
        for (pane, terminal) in terminals_to_watch {
            self.watch_terminal(pane, &terminal)?;
            *snapshot_changed |= self.locate_moved_pane(pane);
        }
        Ok(())
    }

    pub(super) fn drain_client_events(
        self: &Arc<Self>,
        client: ClientId,
        client_events: Vec<EventPayload>,
        mut direct_events: Vec<EventPayload>,
        mut deferred_direct_events: Vec<DeferredClientEvent>,
        targeted_events: Vec<DeferredClientEvent>,
        client_message_retires: Vec<(ClientId, ActiveClientMessage)>,
        prompt_cleared_message: Option<ActiveClientMessage>,
        resume_client_terminals: bool,
        client_message_schedule: Option<ClientMessageDeadline>,
        panes_mode_overlays: Vec<(ClientId, Option<DisplayPanesState>)>,
        panes_mode_deadline: Option<(PaneId, u64, Instant)>,
    ) {
        for event in client_events {
            let pane = match &event {
                EventPayload::BrowserCommand { pane, .. }
                | EventPayload::TerminalUiCommand { pane, .. } => *pane,
                _ => continue,
            };
            self.publish_for_pane(pane, &event);
        }
        defer_unowned_client_events(client, &mut direct_events, &mut deferred_direct_events);
        for event in deferred_direct_events {
            self.publish_deferred_client_event(event);
        }
        for event in targeted_events {
            self.publish_deferred_client_event(event);
        }
        for (target, retired) in client_message_retires {
            self.retire_client_message(target, retired, true);
        }
        if let Some(retired) = prompt_cleared_message {
            self.retire_client_message(client, retired, true);
        }
        if resume_client_terminals {
            self.resume_client_terminals(client);
        }
        if let Some(deadline) = client_message_schedule {
            let _ = self.timer_tx.send(timers::TimerInput::ClientMessage(
                ClientMessageDeadlineCommand::Schedule(deadline),
            ));
        }
        for (viewer, state) in panes_mode_overlays {
            self.publish_to_client(viewer, EventPayload::DisplayPanes { state });
        }
        if let Some((pane, token, deadline)) = panes_mode_deadline {
            let shared = Arc::downgrade(&self.server_owner());
            let _ = self.timer_tx.send(timers::TimerInput::Callback {
                deadline: Some(deadline),
                callback: Box::new(move || {
                    if let Some(shared) = shared.upgrade() {
                        shared.expire_panes_mode(pane, token);
                    }
                }),
            });
        }
    }

    pub(super) fn drain_alert_effects(
        self: &Arc<Self>,
        monitor_silence_changed: bool,
        silence_schedules: Vec<SilenceDeadline>,
        activity_requeues: Vec<(WindowId, PaneId)>,
    ) {
        if monitor_silence_changed {
            self.reset_all_silence_timers();
        }
        for deadline in silence_schedules {
            let _ = self.timer_tx.send(timers::TimerInput::Silence(
                SilenceDeadlineCommand::Schedule(deadline),
            ));
        }
        for (window, pane) in activity_requeues {
            self.raise_window_activity(window, pane);
        }
    }

    #[cfg(feature = "agent")]
    pub(super) fn drain_agent_effects(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command_name: &str,
        execution: &Execution,
        agent_options_changed: bool,
        removed_panes: &[PaneId],
        agent_panes_closed: &[PaneId],
        agent_panes_opened: Vec<(PaneId, Option<Arc<TerminalSession>>)>,
        agent_children: Vec<PaneId>,
        agent_briefs: Vec<(PaneId, String)>,
    ) {
        if agent_options_changed {
            self.reconfigure_agents();
        }
        self.close_agent_panes(removed_panes);
        self.close_agent_panes(agent_panes_closed);
        for (pane, replaced) in agent_panes_opened {
            self.open_agent_pane(pane, replaced);
            self.request_pane_location(pane);
        }
        for pane in agent_children {
            self.approve_child_tools(pane);
        }
        if matches!(command_name, "new-pane" | "split-window" | "new-window") {
            let created = execution.effects.iter().find_map(|effect| match effect {
                MuxEffect::PaneCreated {
                    pane,
                    kind: PaneKindSnapshot::Agent(_),
                    ..
                } => Some(*pane),
                _ => None,
            });
            self.open_created_chat(client, kind, context, command_name, created, agent_briefs);
        }
        self.close_orphaned_chats(removed_panes);
    }

    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "each flag is one effect accumulator"
    )]
    pub(super) fn drain_publication_effects(
        self: &Arc<Self>,
        removed_panes: Vec<PaneId>,
        client_terminal_changed: bool,
        mux_options_event: bool,
        mux_option_refresh_sessions: &BTreeSet<SessionId>,
        mode_styles_changed: bool,
        option_signals: Vec<String>,
        mut snapshot_changed: bool,
        status_formats_changed: bool,
        status_refresh_sessions: &BTreeSet<SessionId>,
    ) {
        for pane in removed_panes {
            self.cancel_pane_files(pane);
            self.publish(EventPayload::PaneRemoved(pane));
        }
        if client_terminal_changed {
            self.publish_terminal_negotiation();
        }
        if mux_options_event {
            self.publish_effective_mux_options(None);
        } else if !mux_option_refresh_sessions.is_empty() {
            self.publish_effective_mux_options(Some(mux_option_refresh_sessions));
        }
        if mode_styles_changed {
            self.refresh_published_appearance();
        }
        self.restyle_client_overlays();
        for channel in option_signals {
            if let Some(pane) = channel
                .strip_prefix("@agent_state@")
                .filter(|target| target.starts_with('%'))
                .and_then(|target| target.parse::<PaneId>().ok())
            {
                snapshot_changed |= pane_summary::mirror_agent_state(
                    &mut self.inner.lock(),
                    pane,
                    unix_timestamp(),
                );
                agent_waits::state_changed(self, pane);
                let event = {
                    let inner = self.inner.lock();
                    PendingHookEvent::live_pane("agent-state-changed", pane, &inner.engine).map(
                        |mut event| {
                            let value = inner
                                .engine
                                .format_facts()
                                .user_option(
                                    &pane.to_string(),
                                    &event
                                        .context
                                        .window
                                        .map(|window| window.to_string())
                                        .unwrap_or_default(),
                                    &event
                                        .context
                                        .session
                                        .map(|session| session.to_string())
                                        .unwrap_or_default(),
                                    "@agent_state",
                                )
                                .unwrap_or_default()
                                .to_owned();
                            event.variables.insert("agent_state".to_owned(), value);
                            event
                                .variables
                                .insert("agent_pending_permission".to_owned(), String::new());
                            event
                        },
                    )
                };
                if let Some(event) = event {
                    self.run_event_hooks(vec![event]);
                }
            }
            if let Some(pane) = channel
                .strip_prefix("@name@")
                .and_then(|pane| pane.parse().ok())
            {
                snapshot_changed |= pane_summary::mirror_pane_name(&mut self.inner.lock(), pane);
                #[cfg(all(feature = "agent", unix))]
                self.update_terminal_peer(pane);
            }
            self.signal_wait_channel(&channel);
        }
        if snapshot_changed {
            self.publish_snapshot();
        } else if status_formats_changed {
            self.refresh_status();
        } else if !status_refresh_sessions.is_empty() {
            self.refresh_status_for_sessions(Some(status_refresh_sessions));
        }
    }

    pub(super) fn drain_stream_inputs(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        execution: &mut Execution,
        split_caller_stream: bool,
        source_client: ClientId,
        pane_stream_inputs: Vec<(PaneId, RawText)>,
    ) -> Result<(), DaemonError> {
        for (pane, bytes) in pane_stream_inputs {
            self.feed_pane_stream_input(pane, bytes.as_bytes());
        }
        if split_caller_stream
            && let Some(pane) = execution.effects.iter().find_map(|effect| match effect {
                MuxEffect::PaneCreated { pane, .. } => Some(*pane),
                _ => None,
            })
        {
            self.release_command_stdout(source_client, &mut execution.output);
            let input = CommandInvocation::new("display-message", ["-I", "-t", &pane.to_string()]);
            if let Some(input) = self.command_with_caller_stdin(client, context, &input)?
                && let Some(bytes) = input.stdin()
            {
                self.feed_pane_stream_input(pane, bytes.as_bytes());
            }
        }
        Ok(())
    }
}
