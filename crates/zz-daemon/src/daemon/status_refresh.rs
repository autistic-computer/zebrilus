use super::*;

impl Shared {
    pub(super) fn refresh_status(&self) {
        self.refresh_status_for_sessions(None);
    }

    pub(super) fn status_job_needs(&self, client: ClientId) -> FormatNeeds {
        self.status_job_needs
            .lock()
            .get(&client)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn refresh_status_for_sessions(&self, sessions: Option<&BTreeSet<SessionId>>) {
        self.refresh_status_filtered(sessions, None);
    }

    pub(super) fn refresh_status_filtered(
        &self,
        sessions: Option<&BTreeSet<SessionId>>,
        clients: Option<&BTreeSet<ClientId>>,
    ) {
        let startup_ready = *self.startup_ready.lock();
        let requests = {
            let mut inner = self.inner.lock();
            let mut targets = status_targets(&inner, sessions, clients);
            if clients.is_none() {
                targets
                    .retain(|client| !inner.client(*client).is_some_and(|c| c.ctrl_initializing));
            }
            if targets.is_empty() {
                return;
            }
            inner.engine.set_format_now(unix_timestamp());
            status_requests_with_selected_facts(
                &inner,
                targets,
                startup_ready,
                &self.status_job_needs,
            )
        };
        self.publish_status_requests(&requests);
    }

    pub(super) fn publish_status_requests(&self, requests: &[StatusRequest]) {
        if requests.is_empty() {
            return;
        }
        let (changed, animation_started) = {
            let _round_trips = zz_terminal::forbid_actor_round_trips();
            let mut status = self.status.lock();
            (
                status.render_changed(requests),
                status.take_animation_started(),
            )
        };
        if animation_started {
            self.nudge_client_timers();
        }
        if !changed.is_empty() {
            let mut inner = self.inner.lock();
            for (client, status) in &changed {
                inner
                    .client_entry(*client)
                    .status_rows
                    .replace((status.rows.clone(), status.base_style.clone()));
            }
        }
        for (client, status) in changed {
            self.publish_to_client(client, EventPayload::StatusChanged { status });
        }
    }

    pub(super) fn refresh_control_subscriptions(&self) {
        let events = {
            let mut inner = self.inner.lock();
            inner.engine.set_format_now(unix_timestamp());
            let clients = inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.control_output.as_ref().map(|value| (id, value)))
                .filter(|(_, output)| !output.subscriptions.is_empty())
                .filter_map(|(client, _)| {
                    Some((*client, client_attached_session(&inner, *client)?))
                })
                .collect::<Vec<_>>();
            if clients.is_empty() {
                return;
            }
            let mut events = Vec::new();
            for (client, session) in clients {
                let client_facts = client_format_facts(&inner, client, session);
                let mut subscriptions = inner
                    .client_mut(client)
                    .and_then(|c| c.control_output.as_mut())
                    .map(|output| std::mem::take(&mut output.subscriptions))
                    .unwrap_or_default();
                let mut facts = borrowed_format_hook_facts(&inner);
                facts.seed.client = Some(client_facts);
                let contexts = inner
                    .engine
                    .format_context_snapshot(FormatClient::Attached(session));
                for (name, subscription) in &mut subscriptions {
                    let mut current = BTreeMap::new();
                    for target in control_subscription_targets(&inner, session, subscription.scope)
                    {
                        let mut context = contexts.status_context(
                            Some(target.session),
                            target.window,
                            target.pane,
                        );
                        context.set_format_value("config_files", inner.config.config_files.clone());
                        let mut hooks =
                            DaemonFormatHooks::command(&facts).with_option_engine(&inner.engine);
                        let value =
                            expand_format_values(&subscription.format, &context, &mut hooks);
                        if subscription.previous.get(&target) != Some(&value) {
                            events.push((
                                client,
                                EventPayload::SubscriptionChanged {
                                    name: name.clone(),
                                    session: target.session,
                                    window: target.window,
                                    window_index: target.window_index,
                                    pane: target.pane,
                                    value: value.clone(),
                                },
                            ));
                        }
                        current.insert(target, value);
                    }
                    subscription.previous = current;
                }
                if let Some(output) = inner
                    .client_mut(client)
                    .and_then(|c| c.control_output.as_mut())
                {
                    output.subscriptions = subscriptions;
                }
            }
            events
        };
        for (client, payload) in events {
            self.publish_to_client(client, payload);
        }
    }

    pub(super) fn run_format_monitors(self: &Arc<Self>) {
        let fires = {
            let mut inner = self.inner.lock();
            let monitors = inner.engine.format_monitors();
            if monitors.is_empty() {
                return;
            }
            inner.engine.set_format_now(unix_timestamp());
            let base_facts = borrowed_format_hook_facts(&inner);
            let mut fires = Vec::new();
            let mut samples_by_monitor = Vec::new();
            let contexts = inner.engine.format_context_snapshot(FormatClient::NoClient);
            for monitor in monitors {
                let Some(session) = monitor
                    .session
                    .filter(|session| inner.engine.state.sessions.contains_key(session))
                    .or_else(|| inner.engine.state.sessions.keys().next().copied())
                else {
                    continue;
                };
                let scope = match monitor.scope {
                    FormatMonitorScope::Session => ControlSubscriptionScope::Session,
                    FormatMonitorScope::Pane(pane) => ControlSubscriptionScope::Pane(pane),
                    FormatMonitorScope::AllPanes => ControlSubscriptionScope::AllPanes,
                    FormatMonitorScope::Window(window) => ControlSubscriptionScope::Window(window),
                    FormatMonitorScope::AllWindows => ControlSubscriptionScope::AllWindows,
                };
                let mut seen = BTreeSet::new();
                let mut samples = Vec::new();
                for target in control_subscription_targets(&inner, session, scope) {
                    let mut context =
                        contexts.status_context(Some(target.session), target.window, target.pane);
                    context.set_format_value("config_files", inner.config.config_files.clone());
                    let mut hooks =
                        DaemonFormatHooks::command(&base_facts).with_option_engine(&inner.engine);
                    let value = expand_format_values(&monitor.format, &context, &mut hooks);
                    let key = FormatMonitorTarget {
                        session: Some(target.session),
                        window: target.window,
                        pane: target.pane,
                    };
                    seen.insert(key);
                    samples.push((key, value));
                }
                samples_by_monitor.push((monitor, seen, samples));
            }
            for (monitor, seen, samples) in samples_by_monitor {
                for (key, value) in samples {
                    if let Some(last) = inner
                        .engine
                        .record_format_monitor_sample(monitor.id, key, &value)
                    {
                        fires.push((monitor.id, monitor.name.clone(), value, last, key));
                    }
                }
                inner.engine.sweep_format_monitor_targets(monitor.id, &seen);
            }
            fires
        };
        for (id, name, value, last, key) in fires {
            let variables = self
                .inner
                .lock()
                .engine
                .format_monitor_hook_variables(&name, &value, &last, key);
            self.feed_event_waiters(&name, &variables);
            let prepared = {
                let inner = self.inner.lock();
                inner
                    .engine
                    .format_monitor_hook_body(id)
                    .and_then(|body| {
                        let facts = borrowed_format_hook_facts(&inner);
                        let mut format_context =
                            inner
                                .engine
                                .format_status_context(key.session, key.window, key.pane);
                        format_context
                            .set_format_value("config_files", inner.config.config_files.clone());
                        let expanded = {
                            let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                                &facts,
                                Some(&variables),
                            )
                            .with_option_engine(&inner.engine);
                            expand_format_values(&body, &format_context, &mut hooks)
                        };
                        inner.engine.parse_monitor_hook_body(&expanded)
                    })
                    .map(|commands| {
                        let mut context = monitor_hook_context(&inner, key);
                        context.set_no_client();
                        context.set_replay_client(None);
                        context.set_control_command_target(None);
                        (context, commands, variables)
                    })
            };
            let Some((context, commands, variables)) = prepared else {
                continue;
            };
            #[cfg(unix)]
            if self.timer_rx.lock().is_none() {
                let _ = self.timer_tx.send(timers::TimerInput::MonitorHook {
                    context: Box::new(context),
                    commands,
                    variables,
                });
                continue;
            }
            self.run_hook_commands_with_policy(
                ClientId(u64::MAX),
                ClientKind::Command,
                &context,
                commands,
                &variables,
                true,
                None,
                false,
            );
        }
    }

    pub(super) fn log_diagnostic_snapshot(&self, reason: &str) {
        if !log::log_enabled!(target: "zz_daemon::diagnostics::state", log::Level::Trace) {
            return;
        }
        let DiagnosticSample {
            mux,
            terminals,
            command_outputs,
            subscribers,
            attached,
            visible_terminals,
            key_clients,
            copy_sessions,
            swallowed_keys,
            suppressed_text,
            pending_committed_text,
            command_prompts,
            choose_trees,
            choose_buffers,
            display_panes,
            command_history,
            paste_buffers,
            active_copy_pipes,
        } = DiagnosticSample::capture(&self.inner.lock());
        let attachment_count = attached.values().map(BTreeSet::len).sum::<usize>();
        log::info!(
            target: "zz_daemon::diagnostics::state",
            "snapshot reason={reason} server_id={} stopping={} mux_generation={} sessions={} terminals={} command_outputs={} subscribers={} attachments={} visible_terminal_clients={} key_clients={} copy_sessions={} swallowed_key_clients={} suppressed_text_clients={} pending_committed_text_clients={} command_prompts={} choose_trees={} choose_buffers={} display_panes={} command_history={} paste_buffers={} active_copy_pipes={}",
            self.server_id,
            self.stopping.load(Ordering::Acquire),
            mux.generation,
            mux.sessions.len(),
            terminals.len(),
            command_outputs.len(),
            subscribers.len(),
            attachment_count,
            visible_terminals.len(),
            key_clients.len(),
            copy_sessions.len(),
            swallowed_keys.len(),
            suppressed_text.len(),
            pending_committed_text.len(),
            command_prompts.len(),
            choose_trees.len(),
            choose_buffers.len(),
            display_panes.len(),
            command_history.len(),
            paste_buffers.len(),
            active_copy_pipes,
        );
        log::trace!(
            target: "zz_daemon::diagnostics::state",
            "snapshot reason={reason} mux={mux:#?} attached={attached:#?} visible_terminals={visible_terminals:#?} key_clients={key_clients:#?} copy_sessions={copy_sessions:#?} swallowed_keys={swallowed_keys:#?} suppressed_text={suppressed_text:#?} pending_committed_text={pending_committed_text:#?} command_prompt_clients={command_prompts:#?} choose_tree_clients={choose_trees:#?} choose_buffer_clients={choose_buffers:#?} display_panes_clients={display_panes:#?} command_history={command_history:#?} paste_buffers={paste_buffers:#?}",
        );
        for (pane, terminal) in terminals {
            let viewport = terminal.latest_viewport();
            log::info!(
                target: "zz_daemon::diagnostics::terminal_state",
                "snapshot reason={reason} pane={pane} terminal_strong_count={} diagnostics={:?}",
                Arc::strong_count(&terminal),
                terminal.diagnostics(),
            );
            log::trace!(
                target: "zz_daemon::diagnostics::terminal_state",
                "snapshot reason={reason} pane={pane} viewport={viewport:#?}"
            );
        }
        for (client, pane, terminal) in command_outputs {
            let viewport = terminal.latest_viewport();
            log::info!(
                target: "zz_daemon::diagnostics::terminal_state",
                "snapshot reason={reason} command_output_client={client} pane={pane} terminal_strong_count={} diagnostics={:?}",
                Arc::strong_count(&terminal),
                terminal.diagnostics(),
            );
            log::trace!(
                target: "zz_daemon::diagnostics::terminal_state",
                "snapshot reason={reason} command_output_client={client} pane={pane} viewport={viewport:#?}"
            );
        }
        for (client, subscriber) in subscribers {
            subscriber.log_diagnostic_snapshot(client, reason);
        }
    }
}

struct DiagnosticSample {
    mux: MuxSnapshot,
    terminals: Vec<(PaneId, Arc<TerminalSession>)>,
    command_outputs: Vec<(ClientId, PaneId, Arc<TerminalSession>)>,
    subscribers: Vec<(ClientId, Arc<OutboundMailbox>)>,
    attached: BTreeMap<SessionId, BTreeSet<ClientId>>,
    visible_terminals: BTreeMap<ClientId, BTreeSet<PaneId>>,
    key_clients: Vec<ClientId>,
    copy_sessions: BTreeMap<ClientId, CopySession>,
    swallowed_keys: BTreeMap<ClientId, BTreeSet<String>>,
    suppressed_text: BTreeMap<ClientId, BTreeMap<char, u32>>,
    pending_committed_text: BTreeMap<ClientId, VecDeque<PendingCommittedText>>,
    command_prompts: Vec<ClientId>,
    choose_trees: Vec<ClientId>,
    choose_buffers: Vec<ClientId>,
    display_panes: Vec<ClientId>,
    command_history: Vec<String>,
    paste_buffers: Vec<PasteBuffer>,
    active_copy_pipes: usize,
}

impl DiagnosticSample {
    pub(super) fn capture(inner: &ServerState) -> Self {
        Self {
            mux: inner.engine.state.snapshot(),
            terminals: inner
                .terminals
                .iter()
                .map(|(pane, terminal)| (*pane, Arc::clone(terminal)))
                .collect(),
            command_outputs: inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.command_output.as_ref().map(|value| (id, value)))
                .map(|(client, output)| (*client, output.pane, Arc::clone(&output.terminal)))
                .collect(),
            subscribers: inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.subscriber.as_ref().map(|value| (id, value)))
                .map(|(client, subscriber)| (*client, Arc::clone(subscriber)))
                .collect(),
            attached: inner.attachment.attached.clone(),
            visible_terminals: inner
                .clients
                .iter()
                .filter_map(|(id, client)| {
                    client
                        .visible_terminals
                        .as_ref()
                        .map(|value| (*id, value.clone()))
                })
                .collect(),
            key_clients: inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.key_engine.as_ref().map(|_| id))
                .copied()
                .collect(),
            copy_sessions: inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.copy_session.as_ref().map(|value| (*id, *value)))
                .collect(),
            swallowed_keys: inner
                .clients
                .iter()
                .filter_map(|(id, client)| {
                    client
                        .swallowed_keys
                        .as_ref()
                        .map(|value| (*id, value.clone()))
                })
                .collect(),
            suppressed_text: inner
                .clients
                .iter()
                .filter_map(|(id, client)| {
                    client
                        .suppressed_text
                        .as_ref()
                        .map(|value| (*id, value.clone()))
                })
                .collect(),
            pending_committed_text: inner
                .clients
                .iter()
                .filter_map(|(id, client)| {
                    client
                        .pending_committed_text
                        .as_ref()
                        .map(|value| (*id, value.clone()))
                })
                .collect(),
            command_prompts: inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.command_prompt.as_ref().map(|_| id))
                .copied()
                .collect(),
            choose_trees: inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.choose_tree.as_ref().map(|_| id))
                .copied()
                .collect(),
            choose_buffers: inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.choose_buffer.as_ref().map(|_| id))
                .copied()
                .collect(),
            display_panes: inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.display_panes.as_ref().map(|_| id))
                .copied()
                .collect(),
            command_history: inner.history.command_history.clone(),
            paste_buffers: inner.buffers.paste_buffers.clone(),
            active_copy_pipes: inner.jobs.active_copy_pipes,
        }
    }
}

fn status_requests_with_selected_facts(
    inner: &ServerState,
    targets: Vec<ClientId>,
    startup_ready: bool,
    job_needs: &crate::status::StatusJobNeeds,
) -> Vec<StatusRequest> {
    let snapshot = targets
        .iter()
        .any(|client| inner.client(*client).and_then(|c| c.kind) == Some(ClientKind::Control))
        .then(|| inner.engine.state.snapshot());
    let option_snapshot = inner.engine.cached_format_option_snapshot();
    let job_needs = job_needs.lock();
    let mut line_needs = BTreeMap::new();
    targets
        .into_iter()
        .map(|client| {
            if inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control) {
                return status_request_with_selected_facts(
                    inner,
                    client,
                    Arc::clone(&option_snapshot),
                    startup_ready,
                    job_needs.get(&client).copied().unwrap_or_default(),
                )
                .as_ref()
                .clone();
            }
            let facts = (inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Control))
                .then(|| format_hook_facts(inner));
            status_request_with_facts(
                inner,
                client,
                facts.as_ref().and(snapshot.as_ref()),
                option_snapshot.clone(),
                facts,
                startup_ready,
                job_needs.get(&client).copied().unwrap_or_default(),
                &mut line_needs,
            )
        })
        .collect()
}

pub(super) fn status_request_with_selected_facts(
    inner: &ServerState,
    client: ClientId,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    startup_ready: bool,
    job_needs: FormatNeeds,
) -> Arc<StatusRequest> {
    status_request_with_selected_options(
        inner,
        client,
        Some(option_snapshot),
        startup_ready,
        job_needs,
    )
}

fn control_subscription_target(
    session: SessionId,
    window: Option<&zz_mux::Window>,
    pane: Option<PaneId>,
) -> ControlSubscriptionTarget {
    ControlSubscriptionTarget {
        session,
        window: window.map(|window| window.id),
        window_index: window.map(|window| window.index),
        pane,
    }
}

fn monitor_hook_context(inner: &ServerState, target: FormatMonitorTarget) -> ExecutionContext {
    if let Some(pane) = target.pane
        && let Some(context) = ExecutionContext::for_pane(&inner.engine.state, pane)
    {
        return context;
    }
    if let Some(window) = target.window
        && let Some(state) = inner.engine.state.windows.get(&window)
    {
        return ExecutionContext::new(Some(state.session), Some(window), Some(state.active_pane));
    }
    let Some(session) = target.session else {
        return ExecutionContext::new(None, None, None);
    };
    let Some(state) = inner.engine.state.sessions.get(&session) else {
        return ExecutionContext::new(Some(session), None, None);
    };
    let pane = inner
        .engine
        .state
        .windows
        .get(&state.active_window)
        .map(|window| window.active_pane);
    ExecutionContext::new(Some(session), Some(state.active_window), pane)
}

fn control_subscription_targets(
    inner: &ServerState,
    session: SessionId,
    scope: ControlSubscriptionScope,
) -> Vec<ControlSubscriptionTarget> {
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return Vec::new();
    };
    match scope {
        ControlSubscriptionScope::Session => {
            vec![control_subscription_target(session, None, None)]
        }
        ControlSubscriptionScope::Pane(pane) => {
            let Some(window) = inner
                .engine
                .state
                .window_for_pane(pane)
                .and_then(|window| inner.engine.state.windows.get(&window))
                .filter(|window| window.session == session)
            else {
                return Vec::new();
            };
            vec![control_subscription_target(
                session,
                Some(window),
                Some(pane),
            )]
        }
        ControlSubscriptionScope::Window(window) => inner
            .engine
            .state
            .windows
            .get(&window)
            .filter(|window| window.session == session)
            .map(|window| vec![control_subscription_target(session, Some(window), None)])
            .unwrap_or_default(),
        ControlSubscriptionScope::AllPanes => session_state
            .windows
            .iter()
            .filter_map(|window| inner.engine.state.windows.get(window))
            .flat_map(|window| {
                window
                    .panes
                    .keys()
                    .copied()
                    .map(|pane| control_subscription_target(session, Some(window), Some(pane)))
            })
            .collect(),
        ControlSubscriptionScope::AllWindows => session_state
            .windows
            .iter()
            .filter_map(|window| inner.engine.state.windows.get(window))
            .map(|window| control_subscription_target(session, Some(window), None))
            .collect(),
    }
}
