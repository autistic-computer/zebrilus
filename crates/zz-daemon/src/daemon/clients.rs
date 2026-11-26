use super::*;

impl Shared {
    #[inline]
    pub(super) fn read_client<T>(
        &self,
        id: ClientId,
        read: impl FnOnce(Option<&Client>) -> T,
    ) -> T {
        read(self.inner.lock().client(id))
    }

    pub(super) fn register_identity(
        &self,
        kind: ClientKind,
        client_instance_id: ClientInstanceId,
        device_name: Option<String>,
        color_scheme: Option<TerminalColorScheme>,
        client_has_terminal: bool,
        startup_reentry: bool,
    ) -> Option<ClientId> {
        let mut inner = self.inner.lock();
        if self.stopping.load(Ordering::Acquire) || self.shutdown_pending.load(Ordering::Acquire) {
            return None;
        }
        inner.engine.set_format_now(unix_timestamp());
        let client = ClientId(inner.attachment.next_client_id);
        inner.attachment.next_client_id = inner.attachment.next_client_id.saturating_add(1);
        inner
            .attachment
            .cold_bootstrap
            .register(client, startup_reentry);
        let now = unix_timestamp();
        inner.attachment.activity_sequence = inner.attachment.activity_sequence.saturating_add(1);
        let activity = inner.attachment.activity_sequence;
        let color_scheme = (kind == ClientKind::Interactive)
            .then_some(color_scheme)
            .flatten();
        if let Some(color_scheme) = color_scheme {
            inner.config.active_color_scheme = color_scheme;
        }
        inner.clients.insert(
            client,
            Box::new(Client {
                instance_id: Some(client_instance_id),
                kind: Some(kind),
                name: device_name,
                color_scheme,
                has_terminal: kind == ClientKind::Interactive && client_has_terminal,
                activity: Some(activity),
                activity_time: Some(now),
                created_time: Some(now),
                focused: Some(true),
                ..Client::default()
            }),
        );
        Some(client)
    }

    pub(super) fn register(
        &self,
        kind: ClientKind,
        client_instance_id: ClientInstanceId,
        device_name: Option<String>,
        color_scheme: Option<TerminalColorScheme>,
        client_has_terminal: bool,
        startup_reentry: bool,
    ) -> Option<(ClientId, ServerHello)> {
        let startup_ready = *self.startup_ready.lock();
        let client = self.register_identity(
            kind,
            client_instance_id,
            device_name,
            color_scheme,
            client_has_terminal,
            startup_reentry,
        )?;
        let mut inner = self.inner.lock();
        let mut capabilities = zz_protocol::CAPABILITY_NAMES
            .iter()
            .filter(|name| {
                ![
                    zz_protocol::CONTROL_CAPABILITY,
                    zz_protocol::CONTROL_STDIO_CAPABILITY,
                    zz_protocol::TTY_INPUT_CAPABILITY,
                ]
                .contains(name)
            })
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        if kind == ClientKind::Interactive && client_has_terminal {
            capabilities.extend(attach::terminal_option_capabilities(&inner.engine));
        }
        let hello_mux_options = inner.config.mux_options.clone();
        inner
            .client_entry(client)
            .published_mux_options
            .replace(hello_mux_options.clone());
        let hello = ServerHello {
            protocol_version: PROTOCOL_VERSION,
            server_id: self.server_id,
            client_id: client,
            client_instance_id,
            capabilities,
            appearance: (*published_appearance(&inner)).clone(),
            appearance_provenance: inner.config.appearance_provenance.clone(),
            mux_options: hello_mux_options,
            status: StatusLine::default(),
            key_tables: inner.engine.keys.snapshot(),
            clock: unix_timestamp(),
            platform: host_platform(),
            revision: zz_protocol::REVISION,
        };
        if kind == ClientKind::Interactive && client_has_terminal {
            return Some((client, hello));
        }
        let option_snapshot = Arc::new(inner.engine.format_option_snapshot());
        let request = status_request(
            &inner,
            client,
            &inner.engine.state.snapshot(),
            option_snapshot,
            format_hook_facts(&inner),
            startup_ready,
            self.status_job_needs(client),
        );
        drop(inner);
        let mut hello = hello;
        let animation_started = {
            let mut status = self.status.lock();
            hello.status = status.render_initial(&request);
            status.take_animation_started()
        };
        if animation_started {
            self.nudge_client_timers();
        }
        self.inner
            .lock()
            .client_entry(client)
            .status_rows
            .replace((hello.status.rows.clone(), hello.status.base_style.clone()));
        Some((client, hello))
    }

    pub(super) fn subscribe(&self, client: ClientId, outbound: Arc<OutboundMailbox>) {
        let attached = {
            let mut inner = self.inner.lock();
            inner.client_entry(client).subscriber.replace(outbound);
            sync_terminal_freeze(&inner, client);
            let streamed = inner
                .client(client)
                .and_then(|c| c.streamed_terminals.as_ref())
                .map(|streamed| streamed.keys().copied().collect::<Vec<_>>())
                .unwrap_or_default();
            for pane in streamed {
                sync_client_pane_sink(&inner, client, pane);
            }
            if inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Control) {
                inner
                    .client_entry(client)
                    .control_output
                    .get_or_insert_default();
            }
            client_attached_session(&inner, client).is_some()
        };
        if attached {
            self.nudge_client_timers();
        }
    }

    pub(super) fn send_attached(
        self: &Arc<Self>,
        client: ClientId,
        outbound: &Arc<OutboundMailbox>,
        session: SessionId,
        snapshot: MuxSnapshot,
    ) -> bool {
        if self.read_client(client, |c| {
            c.is_some_and(|c| c.ctrl_subscriptions.is_some())
        }) {
            return self.send_compact_attached(client, outbound, session);
        }
        let (read_only, client_flags) = {
            let flags = self.inner.lock().attachment.client_flags.get(client);
            (flags.read_only, flags.reconnect_flags())
        };
        let target = self.startup_config_delivery_target(client, false);
        let mut pending = target.map(|_| self.startup_config_causes.lock());
        if pending.as_ref().is_some_and(|causes| causes.is_none()) {
            pending = None;
        }
        outbound.reset_kitty_images();
        outbound.reset_pasted_images();
        let sent = (snapshot.content_digest(), snapshot.generation);
        {
            let _order = self.snapshot_order.lock();
            send_agent_parents(&self.inner.lock(), outbound);
            if !outbound.enqueue_attached(&ProtocolMessage::Attached {
                session,
                snapshot,
                read_only,
                client_flags,
            }) {
                return false;
            }
            let mut inner = self.inner.lock();
            inner
                .client_mut(client)
                .and_then(|c| c.published_key_table.take());
            inner.client_entry(client).published_snapshot.replace(sent);
        }
        self.sync_key_table(client, false);
        let startup_delivery = match (target, pending.as_ref().and_then(|causes| causes.as_ref())) {
            (Some((kind, pane)), Some(causes)) => {
                self.deliver_startup_config_causes(client, kind, pane, causes, outbound)
            }
            (Some(_), None) => StartupConfigDelivery::NoPending,
            (None, _) => StartupConfigDelivery::Ineligible,
        };
        if startup_delivery == StartupConfigDelivery::AdmissionFailed {
            return false;
        }
        self.send_resync_as(
            client,
            outbound,
            startup_delivery.command_output(),
            attach::ResyncScope::Attach,
        );
        if !outbound.is_open() {
            if let Some(output_id) = startup_delivery.command_output() {
                self.retire_command_output_if_exact(client, output_id);
            }
            return false;
        }
        if !self.publish_effective_mux_options_to(client, outbound)
            || !self.publish_terminal_negotiation_to(client, outbound)
            || !outbound.is_open()
        {
            if let Some(output_id) = startup_delivery.command_output() {
                self.retire_command_output_if_exact(client, output_id);
            }
            return false;
        }
        if startup_delivery.delivered()
            && let Some(pending) = pending.as_mut()
        {
            pending.take();
        }
        true
    }

    #[cfg(feature = "agent")]
    pub(super) fn client_instance_id(&self, client: ClientId) -> Option<ClientInstanceId> {
        self.read_client(client, |client| client.and_then(|c| c.instance_id))
    }

    #[cfg(test)]
    pub(super) fn register_subscribed(
        &self,
        kind: ClientKind,
        device_name: Option<String>,
        color_scheme: Option<TerminalColorScheme>,
        outbound: Arc<OutboundMailbox>,
    ) -> (ClientId, ServerHello) {
        let (client, hello) = self
            .register(
                kind,
                ClientInstanceId::default(),
                device_name,
                color_scheme,
                kind == ClientKind::Interactive,
                false,
            )
            .expect("server accepts test client");
        if matches!(kind, ClientKind::Interactive | ClientKind::Control) {
            self.subscribe(client, outbound);
        }
        (client, hello)
    }

    pub(super) fn unregister(self: &Arc<Self>, client: ClientId) {
        let closed = client_lifecycle_event(&self.inner.lock(), "client-closed", client);
        let (detached, _) = self.detach_client_state(client, false);
        if detached {
            self.publish_snapshot_after_detach(client);
        }
        self.enforce_destroy_unattached();
        self.fail_gui_requests_for(client);
        self.status.lock().forget(client);
        let (terminals, command_output, confirm_waiters, shutdown, wait_wakes) = {
            let mut inner = self.inner.lock();
            let mut removed_client = inner.clients.remove(&client);
            let files = inner
                .transfers
                .client_file_waiters
                .extract_if(.., |_, waiter| waiter.client == client)
                .map(|(_, waiter)| waiter)
                .collect::<Vec<_>>();
            drop(inner);
            for waiter in files {
                (waiter.complete)(
                    self,
                    Err(client_file_failure("the invoking client is gone", &waiter.path).into()),
                );
            }
            let mut inner = self.inner.lock();
            if inner
                .panes
                .pane_read_observations
                .values()
                .any(|observation| observation.strong_count() != 0)
            {
                (self.terminal_requests.notifier())();
            }
            if let Some(route) = inner.attachment.control_routes.remove(&client) {
                for pane in &route.panes {
                    if let Some(sink) = inner
                        .terminals
                        .get(pane)
                        .and_then(|terminal| shard_sink::PaneSink::of(terminal))
                    {
                        sink.set_control(&route.mailbox, false);
                    }
                }
            }
            inner.attachment.client_flags.clear(client);
            inner
                .transfers
                .paste_uploads
                .retain(|(uploader, _), _| *uploader != client);
            inner
                .transfers
                .file_writes
                .retain(|(writer, _), _| *writer != client);
            let mut wait_wakes = remove_client_wait_items(&mut inner.waits.wait_channels, client);
            wait_wakes.extend(hook_events::take_event_waiter_wakes(
                &mut inner.waits.event_waiters,
                Some(client),
            ));
            pane_exit::cancel_client(&mut inner, client);
            let confirm_waiters = inner
                .clients
                .values()
                .flat_map(|client| {
                    let confirm =
                        client
                            .confirm
                            .as_ref()
                            .and_then(|confirm| match &confirm.execution {
                                ConfirmExecution::Blocking { waiter } => Some(waiter),
                                _ => None,
                            });
                    [
                        confirm,
                        client
                            .display_panes
                            .as_ref()
                            .and_then(|overlay| overlay.waiter.as_ref()),
                        client
                            .command_prompt
                            .as_ref()
                            .and_then(|prompt| prompt.waiter.as_ref()),
                    ]
                    .into_iter()
                    .flatten()
                })
                .filter(|waiter| waiter.0.client == client)
                .cloned()
                .collect::<Vec<_>>();
            let cold_shutdown = inner.attachment.cold_bootstrap.unregister(client);
            if cold_shutdown {
                self.stopping.store(true, Ordering::Release);
                self.accept_wake.wake();
            }
            let shutdown = cold_shutdown || self.should_shutdown_if_empty(&inner);
            (
                inner.terminals.values().cloned().collect::<Vec<_>>(),
                removed_client
                    .as_mut()
                    .and_then(|c| c.command_output.take()),
                confirm_waiters,
                shutdown,
                wait_wakes,
            )
        };
        self.wake_wait_items(wait_wakes);
        let view = TerminalViewId(client.0);
        if let Some(command_output) = command_output {
            command_output.terminal.view_action(
                view,
                zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
            );
        }
        for terminal in terminals {
            terminal.release_view(view);
        }
        for waiter in confirm_waiters {
            waiter.complete(false);
        }
        if shutdown {
            self.request_shutdown_without_hooks();
        } else if let Some(event) = closed {
            self.run_event_hooks(vec![event]);
        }
    }

    pub(super) fn list_clients(
        &self,
        context: &ExecutionContext,
        name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let spec = zz_protocol::catalog_command_spec("list-clients").expect("list-clients spec");
        let options = zz_protocol::parse_tmux_options(spec, args)?;
        spec.validate_positional_maximum(options.positionals.len())?;
        let mut parsed = ParsedBufferCommandArgs::default();
        let mut json = false;
        for option in options.options {
            match option {
                zz_protocol::TmuxOption::Flag("--json") => json = true,
                zz_protocol::TmuxOption::Flag("-r") => {
                    parsed.flags.insert('r');
                }
                zz_protocol::TmuxOption::Value(name, value) => {
                    if let Some(option) = name.chars().nth(1) {
                        parsed.values.insert(option, value.to_owned());
                    }
                }
                zz_protocol::TmuxOption::Flag(_) => {}
            }
        }
        let mut inner = self.inner.lock();
        let target = parsed
            .value('t')
            .map(|target| inner.engine.resolve_session(Some(target), context.session))
            .transpose()?;
        let sort = TmuxSort::parse(parsed.value('O'), parsed.has('r'), None)?;
        inner.engine.set_format_now(unix_timestamp());
        let mut clients = inner
            .attachment
            .attached
            .iter()
            .flat_map(|(session, clients)| {
                clients
                    .iter()
                    .copied()
                    .filter(|client| !inner.client(*client).is_some_and(|c| c.suspended))
                    .map(|client| (client, *session))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        clients.sort_by_key(|(client, _)| client.0);
        sort.apply(
            &mut clients,
            |(left, left_session), (right, right_session)| {
                let left_facts = client_format_facts(&inner, *left, *left_session);
                let right_facts = client_format_facts(&inner, *right, *right_session);
                let ordering = match sort.order() {
                    Some(TmuxSortOrder::Activity) => inner
                        .client(*right)
                        .and_then(|c| c.activity)
                        .unwrap_or_default()
                        .cmp(
                            &inner
                                .client(*left)
                                .and_then(|c| c.activity)
                                .unwrap_or_default(),
                        ),
                    Some(TmuxSortOrder::Creation) => left.0.cmp(&right.0),
                    Some(TmuxSortOrder::Name) => left_facts.name.cmp(&right_facts.name),
                    Some(TmuxSortOrder::Size) => (
                        left_facts.width.parse::<u16>().unwrap_or_default(),
                        left_facts.height.parse::<u16>().unwrap_or_default(),
                    )
                        .cmp(&(
                            right_facts.width.parse::<u16>().unwrap_or_default(),
                            right_facts.height.parse::<u16>().unwrap_or_default(),
                        )),
                    _ => std::cmp::Ordering::Equal,
                };
                ordering.then_with(|| left_facts.name.cmp(&right_facts.name))
            },
        );
        let format = parsed.value('F').unwrap_or(
            "#{client_name}: #{session_name} [#{client_width}x#{client_height} #{client_termname}] #{?#{!=:#{client_uid},#{uid}},[user #{?client_user,#{client_user},#{client_uid},}] ,}#{?client_flags,(,}#{client_flags}#{?client_flags,),}",
        );
        let mut output = Vec::new();
        for (line, (client, session_id)) in clients.into_iter().enumerate() {
            if target.is_some_and(|target| target != session_id) {
                continue;
            }
            let session = inner
                .engine
                .state
                .sessions
                .get(&session_id)
                .ok_or_else(|| ServerError::MissingTarget(session_id.to_string()))?;
            let focused = client_focused_window(&inner, client, session);
            let mut format_context = inner.engine.format_status_context_for_client(
                Some(session_id),
                Some(focused),
                None,
                session_id,
            );
            format_context.set_format_value("config_files", inner.config.config_files.clone());
            let mut client_facts = client_format_facts(&inner, client, session_id);
            client_facts.line = line;
            let mut facts = borrowed_format_hook_facts(&inner);
            facts.seed.client = Some(client_facts);
            let variables = context.format_variables().cloned().unwrap_or_default();
            if let Some(filter) = parsed.value('f') {
                let mut hooks =
                    DaemonFormatHooks::command_with_optional_variables(&facts, Some(&variables))
                        .with_option_engine(&inner.engine)
                        .with_command_item(name);
                if !format_true(&expand_format_values(filter, &format_context, &mut hooks)) {
                    continue;
                }
            }
            let mut hooks =
                DaemonFormatHooks::command_with_optional_variables(&facts, Some(&variables))
                    .with_option_engine(&inner.engine)
                    .with_command_item(name);
            output.push(if json {
                format_context
                    .scoped_format_values("client", &mut hooks)
                    .to_string()
                    .into()
            } else {
                expand_format_bytes(format, &format_context, &mut hooks)
            });
        }
        Ok(Execution {
            output: RawText::join(&output, b"\n"),
            effects: Vec::new(),
        })
    }

    pub(super) fn switch_client(
        self: &Arc<Self>,
        invoking_client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_buffer_command_args(
            "switch-client",
            args,
            &['c', 'O', 't', 'T'],
            &['E', 'F', 'l', 'n', 'p', 'r', 'Z'],
        )?;
        require_no_positionals("switch-client", &parsed)?;
        let target_client = {
            let inner = self.inner.lock();
            resolve_client_target(&inner, invoking_client, kind, parsed.value('c'))?
        };
        let (walk_session, mut target_session, target_window, target_pane) = {
            let inner = self.inner.lock();
            let current_session = client_attached_session(&inner, target_client)
                .ok_or_else(|| ServerError::InvalidCommand("no current client".to_owned()))?;
            let current_session_state = inner
                .engine
                .state
                .sessions
                .get(&current_session)
                .ok_or_else(|| ServerError::MissingTarget(current_session.to_string()))?;
            let current_window =
                client_focused_window(&inner, target_client, current_session_state);
            let current_pane = inner.engine.state.windows[&current_window].active_pane;
            match parsed.value('t') {
                Some(target)
                    if target == "="
                        || target.contains(':')
                        || target.contains('.')
                        || target.contains('%') =>
                {
                    let pane = inner.engine.resolve_pane(
                        Some(target),
                        Some(current_window),
                        Some(current_pane),
                    )?;
                    let window = inner
                        .engine
                        .state
                        .window_for_pane(pane)
                        .ok_or_else(|| ServerError::MissingTarget(target.to_owned()))?;
                    let session = inner.engine.state.windows[&window].session;
                    (current_session, session, Some(window), Some(pane))
                }
                Some(target) => {
                    let session = inner
                        .engine
                        .resolve_session(Some(target), Some(current_session))?;
                    (current_session, session, None, None)
                }
                None => (current_session, current_session, None, None),
            }
        };
        if parsed.has('r') {
            let mut inner = self.inner.lock();
            if !inner.attachment.client_flags.remove(target_client) {
                inner.attachment.client_flags.insert(target_client);
            }
        }
        if let Some(table) = parsed.value('T') {
            let mut inner = self.inner.lock();
            if inner.engine.keys.table_names().all(|name| name != table) {
                return Err(
                    ServerError::InvalidCommand(format!("table {table} doesn't exist")).into(),
                );
            }
            inner
                .client_entry(target_client)
                .key_engine
                .get_or_insert_default()
                .switch_client_table(Some(table.to_owned()));
            drop(inner);
            self.sync_key_table(target_client, false);
            return Ok(Execution::default());
        }
        let sort = TmuxSort::parse(parsed.value('O'), parsed.has('r'), None)?;
        if parsed.has('n') || parsed.has('p') {
            let inner = self.inner.lock();
            let mut sessions = inner
                .engine
                .state
                .sessions_by_name()
                .into_iter()
                .map(|session| session.id)
                .collect::<Vec<_>>();
            sort.apply(&mut sessions, |left, right| {
                let left = &inner.engine.state.sessions[left];
                let right = &inner.engine.state.sessions[right];
                let ordering = match sort.order() {
                    Some(TmuxSortOrder::Activity) => right.sort_activity.cmp(&left.sort_activity),
                    Some(TmuxSortOrder::Creation) => left.sort_created.cmp(&right.sort_created),
                    Some(TmuxSortOrder::Index) => left.id.cmp(&right.id),
                    Some(TmuxSortOrder::Name) => left.name.cmp(&right.name),
                    _ => std::cmp::Ordering::Equal,
                };
                ordering.then_with(|| left.name.cmp(&right.name))
            });
            let current = sessions
                .iter()
                .position(|session| *session == walk_session)
                .ok_or_else(|| ServerError::MissingTarget(walk_session.to_string()))?;
            let selected = if parsed.has('n') {
                (current + 1) % sessions.len()
            } else {
                (current + sessions.len() - 1) % sessions.len()
            };
            target_session = sessions[selected];
        } else if parsed.has('l') {
            let inner = self.inner.lock();
            target_session = inner
                .client(target_client)
                .and_then(|c| c.last_session)
                .filter(|session| inner.engine.state.sessions.contains_key(session))
                .ok_or_else(|| ServerError::InvalidCommand("can't find last session".to_owned()))?;
        }
        let selection_events = if !parsed.has('n') && !parsed.has('p') && !parsed.has('l') {
            let mut inner = self.inner.lock();
            let mut events = Vec::new();
            if let Some(pane) = target_pane.filter(|pane| {
                target_window
                    .is_some_and(|window| inner.engine.state.windows[&window].active_pane != *pane)
            }) {
                let scope = hook_events::HookScope::open(&mut inner.engine);
                inner
                    .engine
                    .state
                    .select_pane_with_zoom(pane, parsed.has('Z'))?;
                events.extend(scope.finish(&inner.engine, "switch-client").events);
            }
            if let Some(window) = target_window {
                let scope = hook_events::HookScope::open(&mut inner.engine);
                inner.engine.set_format_now(unix_timestamp());
                inner.engine.state.select_window(target_session, window)?;
                events.extend(scope.finish(&inner.engine, "switch-client").events);
            }
            events
        } else {
            Vec::new()
        };
        self.run_event_hooks(selection_events);
        let preserve_repeat = context.repeat_binding();
        let same_session =
            client_attached_session(&self.inner.lock(), target_client) == Some(target_session);
        if !parsed.has('E') {
            let mut inner = self.inner.lock();
            if let Some(environment) = inner
                .client(target_client)
                .and_then(Client::local_environment)
                .cloned()
            {
                inner
                    .engine
                    .update_session_environment_from_client(target_session, environment.map())?;
            }
        }
        let mut held = self.hold_attach_terminals(target_client);
        let (snapshot, mut attach_events) =
            self.attach_collect_event_hooks(target_client, target_session, !context.no_hooks)?;
        if same_session && !context.no_hooks {
            let inner = self.inner.lock();
            let hook_context =
                hook_events::live_session_context(&inner.engine.state, target_session);
            let client_name = client_format_name(&inner, target_client);
            attach_events.push(
                PendingHookEvent::client(
                    "client-session-changed",
                    hook_context,
                    target_client,
                    Some(client_name.as_str()),
                )
                .with_session_change(
                    &inner.engine.state,
                    Some(target_session),
                    target_session,
                ),
            );
        }
        if !preserve_repeat {
            self.inner
                .lock()
                .client_entry(target_client)
                .key_engine
                .get_or_insert_default()
                .switch_table(None);
            self.sync_key_table(target_client, false);
        }
        let (outbound, control_target) = {
            let inner = self.inner.lock();
            (
                inner
                    .client(target_client)
                    .and_then(|c| c.subscriber.as_ref())
                    .cloned(),
                inner.client(target_client).and_then(|c| c.kind) == Some(ClientKind::Control),
            )
        };
        if control_target {
            if let Some(outbound) = outbound.as_ref() {
                self.send_attached(target_client, outbound, target_session, snapshot.clone());
            }
            self.publish_mux_snapshots();
            held.release();
            attach_events.sort_by_key(|event| event.name != "client-session-changed");
        }
        self.run_event_hooks(attach_events);
        if target_client == invoking_client {
            retarget_context_to_attachment(&self.inner.lock(), target_client, context);
        }
        if !control_target && let Some(outbound) = outbound {
            self.send_attached(target_client, &outbound, target_session, snapshot);
        }
        self.publish_snapshot();
        Ok(Execution::default())
    }

    fn set_control_client_flags(&self, client: ClientId, flags: &str) {
        let event = {
            let mut inner = self.inner.lock();
            inner.attachment.client_flags.apply(client, flags);
            if inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control) {
                return;
            }
            let (before, after) = {
                let output = inner
                    .client_entry(client)
                    .control_output
                    .get_or_insert_default();
                let before = control_flag_state(output);
                apply_control_client_flags(output, flags);
                let after = control_flag_state(output);
                (before, after)
            };
            sync_control_feed(&inner, client, &self.control_wake);
            (before != after).then(|| {
                (
                    inner
                        .client(client)
                        .and_then(|c| c.subscriber.as_ref())
                        .cloned(),
                    EventPayload::ControlFlags {
                        wait_exit: after.0,
                        pause_after_ms: after.1,
                        no_output: after.2,
                        new_layouts: after.3,
                    },
                )
            })
        };
        if let Some((Some(subscriber), payload)) = event {
            Self::send_event(&subscriber, payload);
        }
    }

    pub(super) fn set_requested_client_flags(
        &self,
        client: ClientId,
        requested: Option<&str>,
        read_only: bool,
    ) {
        let event = {
            let mut inner = self.inner.lock();
            if let Some(requested) = requested {
                inner.attachment.client_flags.apply(client, requested);
            }
            if read_only {
                inner.attachment.client_flags.insert(client);
            }
            if inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control) {
                return;
            }
            let output = inner
                .client_entry(client)
                .control_output
                .get_or_insert_default();
            let before = control_flag_state(output);
            if let Some(requested) = requested {
                apply_control_client_flags(output, requested);
            }
            let after = control_flag_state(output);
            sync_control_feed(&inner, client, &self.control_wake);
            (before != after).then(|| {
                (
                    inner
                        .client(client)
                        .and_then(|c| c.subscriber.as_ref())
                        .cloned(),
                    EventPayload::ControlFlags {
                        wait_exit: after.0,
                        pause_after_ms: after.1,
                        no_output: after.2,
                        new_layouts: after.3,
                    },
                )
            })
        };
        if let Some((Some(subscriber), payload)) = event {
            Self::send_event(&subscriber, payload);
        }
    }

    fn set_control_pane_state(&self, client: ClientId, value: &str) {
        let Some((pane, state)) = parse_control_pane_state(value) else {
            return;
        };
        let event = {
            let mut inner = self.inner.lock();
            if inner.engine.state.pane(pane).is_none() {
                return;
            }
            let output = inner
                .client_entry(client)
                .control_output
                .get_or_insert_default();
            let pane_output = output.panes.entry(pane).or_default();
            let (paused, cleared) = match state {
                "on" if pane_output.mode == ControlPaneOutputMode::Off => {
                    pane_output.mode = ControlPaneOutputMode::On;
                    (None, true)
                }
                "off" => {
                    pane_output.mode = ControlPaneOutputMode::Off;
                    (None, true)
                }
                "pause" if pane_output.mode != ControlPaneOutputMode::Paused => {
                    pane_output.mode = ControlPaneOutputMode::Paused;
                    (Some(true), true)
                }
                "continue" if pane_output.mode == ControlPaneOutputMode::Paused => {
                    pane_output.mode = ControlPaneOutputMode::On;
                    (Some(false), true)
                }
                "on" | "pause" | "continue" => (None, false),
                _ => return,
            };
            if cleared {
                if let Some(feed) = inner
                    .client(client)
                    .and_then(|c| c.subscriber.as_ref())
                    .and_then(|subscriber| subscriber.control_feed())
                {
                    feed.clear_pane(pane);
                }
                sync_control_feed(&inner, client, &self.control_wake);
            }
            paused.map(|paused| EventPayload::PaneOutputState { pane, paused })
        };
        if let Some(payload) = event {
            self.publish_to_client(client, payload);
        }
    }

    pub(super) fn set_control_client_size(
        self: &Arc<Self>,
        client: ClientId,
        value: &str,
    ) -> Result<(), DaemonError> {
        let update = parse_control_client_size(value)?;
        let (changed, resizes, mut events) = {
            let mut inner = self.inner.lock();
            let scope = hook_events::HookScope::open(&mut inner.engine);
            let unzoomed_layouts = zoomed_window_layouts(&inner);
            let mut affected = control_client_sized_panes(&inner, client);
            let output = inner
                .client_entry(client)
                .control_output
                .get_or_insert_default();
            match update {
                ControlClientSizeUpdate::Client(geometry) => output.geometry = Some(geometry),
                ControlClientSizeUpdate::Window(window, Some(geometry)) => {
                    output.window_geometries.insert(window, geometry);
                }
                ControlClientSizeUpdate::Window(window, None) => {
                    output.window_geometries.remove(&window);
                }
            }
            affected.extend(control_client_sized_panes(&inner, client));
            let changed = write_back_terminal_geometries(&mut inner, &affected);
            let resizes = terminal_resizes_for_panes(&inner, &affected);
            let sized = client_sized_windows(&inner);
            let mut events = scope.finish(&inner.engine, "refresh-client").events;
            hook_events::replace_window_resize_events(
                &mut events,
                &inner.engine,
                &sized,
                &unzoomed_layouts,
            );
            (changed, resizes, events)
        };
        apply_terminal_resizes(resizes);
        if changed {
            self.publish_snapshot_state();
        }
        for event in events
            .iter_mut()
            .filter(|event| event.name == "window-layout-changed")
        {
            self.publish_to_control_clients(
                EventPayload::HookEvent {
                    name: event.name.to_owned(),
                    variables: event.variables.clone(),
                },
                None,
                true,
            );
            event
                .variables
                .remove(hook_events::UNZOOMED_LAYOUT_VARIABLE);
            event.control_notified = true;
        }
        self.run_event_hooks(events);
        Ok(())
    }

    pub(super) fn update_control_subscription(&self, client: ClientId, value: &str) {
        let mut inner = self.inner.lock();
        let output = inner
            .client_entry(client)
            .control_output
            .get_or_insert_default();
        match parse_control_subscription(value) {
            ControlSubscriptionUpdate::Remove(name) => {
                output.subscriptions.remove(name);
            }
            ControlSubscriptionUpdate::Set {
                name,
                scope,
                format,
            } => {
                output.subscriptions.insert(
                    name.to_owned(),
                    ControlSubscription {
                        scope,
                        format: format.to_owned(),
                        previous: BTreeMap::new(),
                    },
                );
            }
        }
        drop(inner);
        self.nudge_client_timers();
    }

    pub(super) fn refresh_client(
        self: &Arc<Self>,
        client: ClientId,
        _kind: ClientKind,
        name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_buffer_command_args(
            name,
            args,
            &['A', 'B', 'C', 'F', 'f', 'r', 't'],
            &['c', 'D', 'l', 'L', 'R', 'S', 'U'],
        )?;
        let target = {
            let inner = self.inner.lock();
            resolve_attached_client(&inner, client, parsed.value('t'))?
        };
        if parsed.has('c')
            || parsed.has('D')
            || parsed.has('L')
            || parsed.has('R')
            || parsed.has('U')
            || !parsed.positional.is_empty()
        {
            return Err(ServerError::UnsupportedCommand(
                "refresh-client interactive behavior".to_owned(),
            )
            .into());
        }
        if parsed.has('l') {
            self.query_client_clipboard(target);
            return Ok(Execution::default());
        }
        let mut handled_flags = false;
        for flags in parsed.values('F') {
            self.set_control_client_flags(target, flags);
            handled_flags = true;
        }
        for flags in parsed.values('f') {
            self.set_control_client_flags(target, flags);
            handled_flags = true;
        }
        if parsed.value('r').is_some() {
            return Err(ServerError::UnsupportedCommand(
                "refresh-client interactive behavior".to_owned(),
            )
            .into());
        }
        if parsed.value('A').is_some() {
            if self.read_client(target, |c| c.and_then(|c| c.kind)) != Some(ClientKind::Control) {
                return Err(ServerError::InvalidCommand("not a control client".to_owned()).into());
            }
            for value in parsed.values('A') {
                self.set_control_pane_state(target, value);
            }
            return Ok(Execution::default());
        }
        if parsed.value('B').is_some() {
            if self.read_client(target, |c| c.and_then(|c| c.kind)) != Some(ClientKind::Control) {
                return Err(ServerError::InvalidCommand("not a control client".to_owned()).into());
            }
            for value in parsed.values('B') {
                self.update_control_subscription(target, value);
            }
            return Ok(Execution::default());
        }
        if parsed.value('C').is_some() {
            if self.read_client(target, |c| c.and_then(|c| c.kind)) != Some(ClientKind::Control) {
                return Err(ServerError::InvalidCommand("not a control client".to_owned()).into());
            }
            for value in parsed.values('C') {
                self.set_control_client_size(target, value)?;
            }
            return Ok(Execution::default());
        }
        if handled_flags && !parsed.has('S') {
            return Ok(Execution::default());
        }
        let startup_ready = *self.startup_ready.lock();
        let request = {
            let mut inner = self.inner.lock();
            inner.engine.set_format_now(unix_timestamp());
            status_request_with_selected_options(
                &inner,
                target,
                None,
                startup_ready,
                self.status_job_needs(target),
            )
        };
        let (status, animation_started) = {
            let mut renderer = self.status.lock();
            (
                renderer.render_forced_shared(&request),
                renderer.take_animation_started(),
            )
        };
        if animation_started {
            self.nudge_client_timers();
        }
        self.publish_to_client(target, EventPayload::StatusChanged { status });
        Ok(Execution::default())
    }

    pub(super) fn lock_command(
        &self,
        client: ClientId,
        context: &ExecutionContext,
        name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let value_options = if name == "lock-server" {
            &[][..]
        } else {
            &['t'][..]
        };
        let parsed = parse_buffer_command_args(name, args, value_options, &[])?;
        let target = parsed.value('t').map(str::to_owned);
        let inner = self.inner.lock();
        match name {
            "lock-server" => {}
            "lock-session" => {
                inner
                    .engine
                    .resolve_session(target.as_deref(), context.session)?;
            }
            "lock-client" => {
                resolve_attached_client(&inner, client, target.as_deref())?;
            }
            _ => unreachable!("lock command is catalogued"),
        }
        Ok(Execution::default())
    }

    pub(super) fn add_client_terminal_features(
        &self,
        client: ClientId,
        kind: ClientKind,
        features: &[String],
    ) {
        if kind != ClientKind::Interactive {
            return;
        }
        let learned = terminal_feature_mask(features.iter().map(String::as_str));
        if learned == 0 {
            return;
        }
        let changed = {
            let mut inner = self.inner.lock();
            let Some(registered) = inner.client_mut(client).filter(|c| c.has_terminal) else {
                return;
            };
            let carried = registered.features.get_or_insert(0);
            let before = *carried;
            *carried |= learned;
            *carried != before
        };
        if changed {
            self.refresh_status();
        }
    }

    pub(super) fn set_client_terminal_type(
        &self,
        client: ClientId,
        kind: ClientKind,
        term_type: &str,
    ) {
        if kind != ClientKind::Interactive || term_type.is_empty() {
            return;
        }
        let changed = {
            let mut inner = self.inner.lock();
            let Some(registered) = inner.client_mut(client).filter(|c| c.has_terminal) else {
                return;
            };
            registered
                .terminal_type
                .replace(term_type.to_owned())
                .as_deref()
                != Some(term_type)
        };
        if changed {
            self.refresh_status();
        }
    }
}

const CONTROL_SIZE_MINIMUM: u16 = 1;

fn zoomed_window_layouts(inner: &ServerState) -> BTreeMap<WindowId, String> {
    inner
        .engine
        .state
        .windows
        .iter()
        .filter(|(_, window)| window.zoomed_pane.is_some())
        .map(|(id, window)| {
            (
                *id,
                window.layout_string(
                    zz_mux::LayoutFormat::V2,
                    inner.engine.state.pane_base_index(*id),
                ),
            )
        })
        .collect()
}

fn client_sized_windows(inner: &ServerState) -> Vec<WindowId> {
    inner
        .engine
        .state
        .windows
        .iter()
        .filter(|(id, window)| {
            inner.engine.window_size(**id) == WindowSize::Manual
                || window.panes.keys().any(|pane| {
                    pane_geometry_from(inner, *pane, GeometrySource::ClientReport).is_some()
                })
        })
        .map(|(window, _)| *window)
        .collect()
}

fn resolve_attached_client(
    inner: &ServerState,
    invoking_client: ClientId,
    target: Option<&str>,
) -> Result<ClientId, ServerError> {
    let attached = inner
        .attachment
        .attached
        .values()
        .flat_map(BTreeSet::iter)
        .copied()
        .collect::<BTreeSet<_>>();
    if let Some(target) = target {
        return find_attached_client_with_aliases(inner, target, true)
            .ok_or_else(|| client_target_error(target));
    }
    if attached.contains(&invoking_client) {
        return Ok(invoking_client);
    }
    current_client(inner, invoking_client)
        .ok_or_else(|| ServerError::InvalidCommand("no current client".to_owned()))
}

fn remove_client_wait_items(
    channels: &mut BTreeMap<String, WaitChannel>,
    client: ClientId,
) -> Vec<cmdq::WaitContinuation> {
    let mut wakes = Vec::new();
    for channel in channels.values_mut() {
        for items in [&mut channel.waiters, &mut channel.lockers] {
            items.retain(|item| {
                if item.client == client {
                    wakes.push(item.continuation.clone());
                    false
                } else {
                    true
                }
            });
        }
        if channel
            .granted
            .as_ref()
            .is_some_and(|item| item.client == client)
        {
            wakes.extend(channel.granted.take().map(|item| item.continuation));
            channel.granted = channel.lockers.pop_front();
            channel.locked = channel.granted.is_some();
            wakes.extend(
                channel
                    .granted
                    .as_ref()
                    .map(|item| item.continuation.clone()),
            );
        }
    }
    wakes
}

fn parse_control_pane_state(value: &str) -> Option<(PaneId, &str)> {
    let (pane, state) = value.split_once(':')?;
    Some((pane.parse().ok()?, state))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ControlClientSizeUpdate {
    Client(TerminalGeometry),
    Window(WindowId, Option<TerminalGeometry>),
}

fn parse_control_dimensions(value: &str, separator: char) -> Option<(u64, u64)> {
    let (columns, rows) = value.split_once(separator)?;
    if columns.is_empty() || rows.is_empty() || rows.contains(separator) {
        return None;
    }
    Some((columns.parse().ok()?, rows.parse().ok()?))
}

fn checked_control_geometry(columns: u64, rows: u64) -> Result<TerminalGeometry, ServerError> {
    let in_range =
        |value| (u64::from(CONTROL_SIZE_MINIMUM)..=u64::from(WINDOW_SIZE_MAXIMUM)).contains(&value);
    if !in_range(columns) || !in_range(rows) {
        return Err(ServerError::InvalidCommand(
            "size too small or too big".to_owned(),
        ));
    }
    Ok(TerminalGeometry {
        columns: u16::try_from(columns).expect("validated control columns fit u16"),
        rows: u16::try_from(rows).expect("validated control rows fit u16"),
        cell_width_px: CONTROL_CELL_WIDTH_PX,
        cell_height_px: CONTROL_CELL_HEIGHT_PX,
    })
}

pub(super) fn parse_control_client_size(
    value: &str,
) -> Result<ControlClientSizeUpdate, ServerError> {
    if let Some((window, size)) = value.split_once(':')
        && value.matches(':').count() == 1
        && let Ok(window) = window.parse::<WindowId>()
    {
        if size.is_empty() {
            return Ok(ControlClientSizeUpdate::Window(window, None));
        }
        let (columns, rows) = parse_control_dimensions(size, 'x')
            .ok_or_else(|| ServerError::InvalidCommand("bad size argument".to_owned()))?;
        return checked_control_geometry(columns, rows)
            .map(|geometry| ControlClientSizeUpdate::Window(window, Some(geometry)));
    }
    let dimensions = parse_control_dimensions(value, ',')
        .or_else(|| parse_control_dimensions(value, 'x'))
        .ok_or_else(|| ServerError::InvalidCommand("bad size argument".to_owned()))?;
    checked_control_geometry(dimensions.0, dimensions.1).map(ControlClientSizeUpdate::Client)
}

pub(super) enum ControlSubscriptionUpdate<'a> {
    Remove(&'a str),
    Set {
        name: &'a str,
        scope: ControlSubscriptionScope,
        format: &'a str,
    },
}

pub(super) fn parse_control_subscription(value: &str) -> ControlSubscriptionUpdate<'_> {
    let mut fields = value.splitn(3, ':');
    let name = fields.next().unwrap_or_default();
    let Some(what) = fields.next() else {
        return ControlSubscriptionUpdate::Remove(value);
    };
    let Some(format) = fields.next() else {
        return ControlSubscriptionUpdate::Remove(value);
    };
    let scope = match what {
        "%*" => ControlSubscriptionScope::AllPanes,
        "@*" => ControlSubscriptionScope::AllWindows,
        what if what.starts_with('%') => what.parse().map_or(
            ControlSubscriptionScope::Session,
            ControlSubscriptionScope::Pane,
        ),
        what if what.starts_with('@') => what.parse().map_or(
            ControlSubscriptionScope::Session,
            ControlSubscriptionScope::Window,
        ),
        _ => ControlSubscriptionScope::Session,
    };
    ControlSubscriptionUpdate::Set {
        name,
        scope,
        format,
    }
}

const fn control_flag_state(output: &ControlClientOutput) -> (bool, Option<u64>, bool, bool) {
    (
        output.wait_exit,
        output.pause_after_ms,
        output.no_output,
        output.new_layouts,
    )
}

pub(super) fn apply_control_client_flags(output: &mut ControlClientOutput, flags: &str) {
    for raw in flags.split(',') {
        let (clear, flag) = raw
            .strip_prefix('!')
            .map_or((false, raw), |flag| (true, flag));
        match flag {
            "no-output" => {
                output.no_output = !clear;
                output.panes.clear();
            }
            "wait-exit" => output.wait_exit = !clear,
            "new-layouts" => output.new_layouts = !clear,
            "pause-after" => {
                output.pause_after_ms = (!clear).then_some(0);
            }
            _ => {
                let Some(value) = flag.strip_prefix("pause-after=") else {
                    continue;
                };
                let Some(milliseconds) = parse_pause_after_milliseconds(value) else {
                    continue;
                };
                output.pause_after_ms = (!clear).then_some(milliseconds);
            }
        }
    }
}

fn parse_pause_after_milliseconds(value: &str) -> Option<u64> {
    let mut bytes = value.bytes().skip_while(u8::is_ascii_whitespace).peekable();
    let negative = match bytes.peek() {
        Some(b'+') => {
            bytes.next();
            false
        }
        Some(b'-') => {
            bytes.next();
            true
        }
        _ => false,
    };
    let mut magnitude = 0_u64;
    let mut overflow = false;
    let mut parsed = false;
    while let Some(byte) = bytes.next_if(u8::is_ascii_digit) {
        parsed = true;
        let digit = u64::from(byte - b'0');
        match magnitude
            .checked_mul(10)
            .and_then(|value| value.checked_add(digit))
        {
            Some(value) if !overflow => magnitude = value,
            _ => {
                magnitude = u64::MAX;
                overflow = true;
            }
        }
    }
    if !parsed {
        return None;
    }
    let mut seconds = magnitude as u32;
    if negative && !overflow {
        seconds = seconds.wrapping_neg();
    }
    Some(u64::from(seconds.wrapping_mul(1000)))
}

pub(super) struct ClientRegistrationGuard {
    pub(super) shared: Arc<Shared>,
    client: ClientId,
    pub(super) armed: bool,
}

pub(super) struct ClientWriterRegistrationGuard {
    shared: Arc<Shared>,
    client: ClientId,
    pub(super) armed: bool,
}

pub(super) struct ResponseAdmissionGuard {
    shared: Arc<Shared>,
    armed: bool,
}

#[derive(Default)]
pub(super) struct ResponseAdmissionState {
    pub(super) active: usize,
    pub(super) frozen: bool,
}

#[derive(Default)]
pub(super) struct ShutdownBlockerState {
    pub(super) active: usize,
    pub(super) detached: usize,
    pub(super) closed: bool,
}

#[cfg(test)]
pub(super) struct ResponseAdmissionHook {
    pub(super) reached: crossbeam_channel::Sender<()>,
    pub(super) release: crossbeam_channel::Receiver<()>,
}

impl ClientRegistrationGuard {
    pub(super) fn new(shared: &Arc<Shared>, client: ClientId) -> Self {
        Self {
            shared: Arc::clone(shared),
            client,
            armed: true,
        }
    }

    pub(super) fn unregister(&mut self) {
        if std::mem::take(&mut self.armed) {
            #[cfg(unix)]
            if self.shared.loop_active.load(Ordering::Acquire) {
                self.shared.lifecycle.release(self.client, false);
                self.shared.accept_wake.wake();
                return;
            }
            self.shared.unregister(self.client);
        }
    }
}

impl Drop for ClientRegistrationGuard {
    fn drop(&mut self) {
        self.unregister();
    }
}

impl ClientWriterRegistrationGuard {
    pub(super) fn new(
        shared: &Arc<Shared>,
        client: ClientId,
        outbound: Arc<OutboundMailbox>,
    ) -> Self {
        shared.client_writers.lock().insert(client, outbound);
        Self {
            shared: Arc::clone(shared),
            client,
            armed: true,
        }
    }

    pub(super) fn unregister(&mut self) {
        if std::mem::take(&mut self.armed) {
            self.shared.client_writers.lock().remove(&self.client);
        }
    }
}

impl Drop for ClientWriterRegistrationGuard {
    fn drop(&mut self) {
        self.unregister();
    }
}

impl ResponseAdmissionGuard {
    pub(super) fn new(shared: &Arc<Shared>) -> Option<Self> {
        let mut state = shared.response_admissions.lock();
        if state.frozen {
            return None;
        }
        state.active = state
            .active
            .checked_add(1)
            .expect("response admission overflow");
        drop(state);
        Some(Self {
            shared: Arc::clone(shared),
            armed: true,
        })
    }

    pub(super) fn finish(&mut self) {
        if !std::mem::take(&mut self.armed) {
            return;
        }
        let mut state = self.shared.response_admissions.lock();
        state.active = state
            .active
            .checked_sub(1)
            .expect("response admission underflow");
        let notify = state.active == 0 && state.frozen;
        drop(state);
        if notify {
            self.shared.response_admissions_changed.notify_all();
            self.shared.accept_wake.wake();
        }
    }
}

impl Drop for ResponseAdmissionGuard {
    fn drop(&mut self) {
        self.finish();
    }
}
