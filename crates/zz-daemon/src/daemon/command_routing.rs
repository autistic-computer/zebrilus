use super::*;

impl Shared {
    pub(super) fn execute(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
    ) -> Result<Execution, DaemonError> {
        self.execute_with_mux_source(
            client,
            kind,
            context,
            command,
            MuxOptionSource::RuntimeCommand,
        )
    }

    pub(super) fn execute_with_mux_source(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
    ) -> Result<Execution, DaemonError> {
        self.execute_with_mux_source_in_queue(client, kind, context, command, mux_source, None)
    }

    pub(super) fn execute_with_mux_source_in_queue(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let routed = resolve_and_prepare_command(&self.inner.lock().engine, command)?.0;
        self.execute_with_mux_source_routed_in_queue(
            client,
            kind,
            context,
            &routed,
            mux_source,
            queue_execution,
        )
    }

    pub(super) fn execute_with_mux_source_routed(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
    ) -> Result<Execution, DaemonError> {
        self.execute_with_mux_source_routed_in_queue(
            client, kind, context, command, mux_source, None,
        )
    }

    pub(super) fn execute_with_mux_source_routed_in_queue(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let client_terminal = if context.has_no_client() {
            ClientTerminal::NoClient
        } else {
            let inner = self.inner.lock();
            format_provenance_client(context, client).map_or(ClientTerminal::Absent, |client| {
                let kind = inner.client(client).and_then(|c| c.kind).unwrap_or(kind);
                client_terminal(&inner, client, kind)
            })
        };
        self.execute_with_mux_source_routed_for_terminal_in_queue(
            client,
            kind,
            context,
            command,
            mux_source,
            client_terminal,
            queue_execution,
        )
    }

    pub(super) fn execute_with_mux_source_routed_for_terminal_in_queue(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
        client_terminal: ClientTerminal,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let legacy = !prints_stored_bytes(&command.name)
            && control_reads_legacy_layouts(
                &self.inner.lock(),
                format_provenance_client(context, client),
            );
        let previous_legacy = context.legacy_layouts();
        context.set_legacy_layouts(legacy);
        let result = self
            .execution_item()
            .execute_with_mux_source_routed_for_terminal_in_queue_in_item(
                client,
                kind,
                context,
                command,
                mux_source,
                client_terminal,
                queue_execution,
            );
        context.set_legacy_layouts(previous_legacy);
        result.map(|mut execution| {
            if legacy {
                legacy_command_output(&mut execution);
            }
            execution
        })
    }

    pub(super) fn execute_with_mux_source_routed_for_terminal_in_queue_in_item(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
        client_terminal: ClientTerminal,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let _round_trips = zz_terminal::forbid_actor_round_trips();
        {
            let item = self.command_item.as_ref().expect("command item").lock();
            if let cmdq::State::Waiting(token) = item.state() {
                item.resume(token);
            }
        }
        let canonical = canonical_command(&command.name);
        #[cfg(unix)]
        if let Some(handed_off) = self.hand_off_from_loop(
            client,
            kind,
            context,
            command,
            canonical,
            mux_source,
            client_terminal,
            queue_execution,
        ) {
            return handed_off;
        }
        hook_events::release_input_change_window(self);
        let split_input = matches!(canonical, "split-window" | "new-pane")
            && command_stdin_sink(canonical, &command.args) == Some(CommandStdinSink::PaneInput);
        let streamed_command = if split_input {
            let mut command = command.clone();
            command.set_stdin_spent();
            Some(command)
        } else {
            self.command_with_caller_stdin(client, context, command)?
        };
        if streamed_command.is_none()
            && command_stdin_sink(canonical, &command.args).is_some()
            && let Some(wait) = self
                .command_item
                .as_ref()
                .unwrap()
                .lock()
                .pending_wait
                .as_mut()
            && wait.file.is_some()
        {
            let command = command.clone();
            wait.file = Some(Box::new(move |shared, context, queue| {
                shared.execute_with_mux_source_routed_for_terminal_in_queue(
                    client,
                    kind,
                    context,
                    &command,
                    mux_source,
                    client_terminal,
                    Some(queue),
                )
            }));
            return Ok(Execution::default());
        }
        let command = streamed_command.as_ref().unwrap_or(command);
        if matches!(
            canonical,
            "command-prompt" | "show-prompt-history" | "clear-prompt-history"
        ) {
            self.ensure_prompt_history();
            if let Some(wait) = self
                .command_item
                .as_ref()
                .unwrap()
                .lock()
                .pending_wait
                .as_mut()
            {
                let command = command.clone();
                wait.file = Some(Box::new(move |shared, context, queue| {
                    shared.execute_with_mux_source_routed_for_terminal_in_queue(
                        client,
                        kind,
                        context,
                        &command,
                        mux_source,
                        client_terminal,
                        Some(queue),
                    )
                }));
                return Ok(Execution::default());
            }
        }
        if MuxEngine::is_command_alias_group(command) {
            if let Some(parent) = queue_execution.filter(|queue| queue.frame_active.get()) {
                let body = MuxEngine::command_alias_group_body(command).expect("alias body");
                parent.insert_foreground_child(InsertedQueueChild {
                    context: context.clone(),
                    source: InsertedCommandSource::Block(body.into()),
                    label: "<command-alias>".to_owned(),
                    control_target: context.control_command_target(),
                    mux_source,
                    kind: InsertedQueueChildKind::Alias {
                        client_terminal,
                        stdin: command.stdin().cloned(),
                    },
                    leaf: None,
                    guard: None,
                    leaf_name: None,
                });
                return Ok(Execution::default());
            }
            let detached = queue_execution.is_some_and(|execution| execution.detached);
            let shutdown_blocker =
                queue_execution.and_then(|execution| execution.fork_shutdown_blocker(detached));
            let alias_execution = self.command_queue_execution(CommandExecutionState {
                draining: queue_execution.is_some_and(CommandQueueExecution::is_draining),
                wait_yields: queue_execution.is_some_and(|execution| execution.wait_yields),
                detached,
                deferred_shutdown: Cell::new(DeferredShutdown::None),
                deferred_control_exit: Cell::new(None),
                yielded: Cell::new(CommandQueueYield::None),
                yield_boundary: false,
                shutdown_blocker: RefCell::new(shutdown_blocker),
                pending_event_hooks: RefCell::new(Vec::new()),
                deferred_shell_jobs: RefCell::new(Vec::new()),
                callback_parse_failures: RefCell::new(Vec::new()),
                deferred_config_replay_issues: RefCell::new(Vec::new()),
                reported_failures: Cell::new(false),
                frame_active: Cell::new(false),
                child: RefCell::new(None),
                suppress_after_hooks: Cell::new(
                    queue_execution.is_some_and(|execution| execution.suppress_after_hooks.get()),
                ),
                suppress_output: Cell::new(
                    queue_execution.is_some_and(|execution| execution.suppress_output.get()),
                ),
            });
            let result = self.execute_command_alias_group(
                client,
                kind,
                context,
                command,
                mux_source,
                client_terminal,
                &alias_execution,
            );
            let reported_failure = alias_execution.reported_failures.get();
            self.finish_command_queue_execution(&alias_execution, queue_execution);
            if queue_execution.is_none()
                && reported_failure
                && alias_execution.deferred_shutdown.get() != DeferredShutdown::Force
                && matches!(kind, ClientKind::Command | ClientKind::Control)
                && let Ok(execution) = &result
            {
                return Err(DaemonError::ReportedCommandExit {
                    output: execution.output.clone(),
                    exit_code: 1,
                });
            }
            return result;
        }
        let no_hooks = context.no_hooks
            || queue_execution.is_some_and(|execution| execution.suppress_after_hooks.get());
        let original_context = context.clone();
        let name = canonical.to_owned();
        let previous_client_terminal = context_client_terminal(context);
        let format_facts_unread = (kind == ClientKind::Command
            && client_terminal == ClientTerminal::Absent
            && matches!(
                name.as_str(),
                "bind-key" | "set-option" | "set-window-option" | "list-keys"
            ))
        .then(|| hook_events::format_facts_unread(&name, &command.args));
        if kind != ClientKind::Command || name != "refresh-client" {
            let provenance_client = format_provenance_client(context, client);
            let client_attached_context = if client_terminal == ClientTerminal::Present {
                let inner = self.inner.lock();
                provenance_client.and_then(|client| {
                    let session = client_attached_session(&inner, client)?;
                    let session_state = inner.engine.state.sessions.get(&session)?;
                    let window = client_focused_window(&inner, client, session_state);
                    let pane = inner.engine.state.windows.get(&window)?.active_pane;
                    Some((session, window, pane))
                })
            } else {
                None
            };
            set_context_client_terminal(context, client_terminal);
            context.set_attached_client_context(client_attached_context);
            if format_facts_unread != Some(true) {
                let target_format_client = {
                    let inner = self.inner.lock();
                    if context.has_no_client() {
                        FormatClient::NoClient
                    } else {
                        provenance_client
                            .and_then(|client| current_format_client_with_session(&inner, client))
                            .map_or(FormatClient::NoClient, |(_, session)| {
                                FormatClient::Attached(session)
                            })
                    }
                };
                context.set_format_client(target_format_client);
            }
        }
        let result = self.execute_with_mux_source_raw(
            client,
            kind,
            context,
            command,
            mux_source,
            client_terminal,
            queue_execution,
            format_facts_unread,
        );
        let (result, pane_exit_status) = self.wait_for_pane_command(client, kind, result);
        let mut leaf = InsertedLeafContinuation {
            original_context,
            previous_client_terminal,
            no_hooks,
            name,
            command: None,
            pane_exit_status,
        };
        if let Some(queue) = queue_execution
            && let Some(child) = queue.child.borrow_mut().as_mut()
        {
            leaf.command = Some(command.clone());
            child.leaf = Some(leaf);
            return result;
        }
        if let Some(item) = &self.command_item {
            let mut item = item.lock();
            if let Some(wait) = item.pending_wait.as_mut() {
                leaf.command = Some(command.clone());
                wait.leaf = Some(leaf);
                return result;
            }
        }
        self.finish_inserted_leaf(
            client,
            kind,
            context,
            result,
            queue_execution,
            command,
            leaf,
        )
    }

    pub(super) fn finish_inserted_leaf(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        result: Result<Execution, DaemonError>,
        queue_execution: Option<&CommandQueueExecution>,
        command: &CommandInvocation,
        leaf: InsertedLeafContinuation,
    ) -> Result<Execution, DaemonError> {
        let InsertedLeafContinuation {
            original_context,
            previous_client_terminal,
            no_hooks,
            name,
            pane_exit_status,
            ..
        } = leaf;
        let pane_exit_code = pane_exit_status.map_or(0, |status| status.load(Ordering::Acquire));
        set_context_client_terminal(context, previous_client_terminal);
        context.copy_client_attachment(&original_context);
        let suppress_after_hook = matches!(
            &result,
            Ok(execution) if execution.effects.contains(&MuxEffect::SuppressAfterHook)
        );
        let suppress_callback_parse_hook = result.as_ref().err().is_some_and(|error| {
            post_admission_callback_parse_depth(error) != 0
                && CallbackGroupAction::for_command(&name) == Some(CallbackGroupAction::Continue)
        });
        if !no_hooks
            && !suppress_after_hook
            && !suppress_callback_parse_hook
            && !matches!(&result, Err(DaemonError::ReportedCommandExit { .. }))
            && let Some(hook) = match &result {
                Ok(_) => MuxEngine::after_command_hook(&name),
                Err(_) => Some("command-error"),
            }
            && self.event_waited(hook)
        {
            let mut variables = hook_format_variables(command, hook);
            variables.insert(HOOK_CONTEXT_FORMAT.to_owned(), hook.to_owned());
            self.feed_event_waiters(hook, &variables);
        }
        let hook_output = if no_hooks
            || suppress_after_hook
            || suppress_callback_parse_hook
            || matches!(&result, Err(DaemonError::ReportedCommandExit { .. }))
        {
            String::new()
        } else if let Some(hook) = match &result {
            Ok(_) => MuxEngine::after_command_hook(&name),
            Err(_) => Some("command-error"),
        } {
            if self.command_hook_set(hook) {
                let mut hook_context = match &result {
                    Ok(execution) => self.command_hook_context(&name, execution, context),
                    Err(_) => original_context,
                };
                {
                    let inner = self.inner.lock();
                    inner.engine.repair_context(&mut hook_context);
                }
                self.run_command_hook(client, kind, &hook_context, command, hook, queue_execution)
            } else {
                self.inner
                    .lock()
                    .engine
                    .note_hook_fired(context.session, hook, unix_timestamp());
                String::new()
            }
        } else {
            String::new()
        };
        if let Some(queue) = queue_execution
            && let Some(child) = queue.child.borrow_mut().as_mut()
            && let InsertedQueueChildKind::Hook(continuation) = &mut child.kind
        {
            *continuation = Some(HookLeafContinuation {
                result,
                pane_exit_code,
            });
            return Ok(Execution::default());
        }
        self.finish_hook_leaf_result(
            client,
            queue_execution,
            result,
            pane_exit_code,
            &hook_output,
        )
    }

    pub(super) fn finish_command_queue_execution(
        self: &Arc<Self>,
        execution: &CommandQueueExecution,
        parent: Option<&CommandQueueExecution>,
    ) {
        if !execution.item.finish() {
            return;
        }
        if let Some(parent) = parent {
            if let Some(blocker) = execution.shutdown_blocker.borrow_mut().take() {
                let mut parent_blocker = parent.shutdown_blocker.borrow_mut();
                if parent_blocker.is_none() {
                    *parent_blocker = Some(blocker);
                }
            }
            parent.deferred_shutdown.set(
                parent
                    .deferred_shutdown
                    .get()
                    .max(execution.deferred_shutdown.get()),
            );
            if parent.deferred_control_exit.get().is_none() {
                parent
                    .deferred_control_exit
                    .set(execution.deferred_control_exit.take());
            }
            if execution.has_yielded() {
                if execution.yield_boundary {
                    parent.yield_locally();
                } else if execution.yielded.get() == CommandQueueYield::Propagate {
                    parent.yield_queue();
                }
            }
            if !execution.yield_boundary && execution.yielded.get() == CommandQueueYield::Local {
                parent.suppress_after_hooks.set(true);
            }
            if execution.suppress_after_hooks.get() {
                parent.suppress_after_hooks.set(true);
            }
            if execution.suppress_output.get() && !execution.yield_boundary {
                parent.suppress_output.set(true);
            }
            parent
                .pending_event_hooks
                .borrow_mut()
                .extend(execution.pending_event_hooks.borrow_mut().drain(..));
            parent
                .deferred_shell_jobs
                .borrow_mut()
                .extend(execution.deferred_shell_jobs.borrow_mut().drain(..));
            parent
                .callback_parse_failures
                .borrow_mut()
                .extend(execution.callback_parse_failures.borrow_mut().drain(..));
            parent.deferred_config_replay_issues.borrow_mut().extend(
                execution
                    .deferred_config_replay_issues
                    .borrow_mut()
                    .drain(..),
            );
            parent
                .reported_failures
                .set(parent.reported_failures.get() || execution.reported_failures.get());
            return;
        }
        let deferred_shutdown = execution.deferred_shutdown.get();
        let deferred_control_exit = execution.deferred_control_exit.take();
        let pending_event_hooks = execution
            .pending_event_hooks
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        let deferred_shell_jobs = execution
            .deferred_shell_jobs
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        if deferred_shutdown != DeferredShutdown::Force {
            self.run_event_hooks(pending_event_hooks);
            for launch in deferred_shell_jobs {
                launch(self);
            }
        }
        let shutdown_requested = match deferred_shutdown {
            DeferredShutdown::None => false,
            DeferredShutdown::Recheck => self.should_shutdown_if_empty(&self.inner.lock()),
            DeferredShutdown::Force => true,
        };
        if let Some(control) = deferred_control_exit {
            self.publish_to_client(
                control,
                EventPayload::ControlExit {
                    reason: String::new(),
                },
            );
        }
        if shutdown_requested {
            if deferred_shutdown == DeferredShutdown::Force {
                self.enter_queue_shutdown_phase();
            }
            self.request_shutdown_after_blockers();
        }
        execution.shutdown_blocker.borrow_mut().take();
    }

    pub(super) fn execute_with_mux_source_raw(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
        invoking_client_terminal: ClientTerminal,
        queue_execution: Option<&CommandQueueExecution>,
        format_facts_unread: Option<bool>,
    ) -> Result<Execution, DaemonError> {
        let canonical = canonical_command(&command.name);
        let argument_sink = (command.stdin().is_some() || command.stdin_was_spent())
            && command_stdin_sink(canonical, &command.args)
                .is_some_and(CommandStdinSink::is_argument);
        if argument_sink && command.stdin_was_spent() {
            let source_client = context.replay_client().unwrap_or(client);
            let source_kind = self
                .read_client(source_client, |client| client.and_then(|c| c.kind))
                .unwrap_or(kind);
            let error = self.take_caller_stdin_error(source_client);
            self.route_source_error(source_client, source_kind, context.pane, &error);
            return Err(if canonical == "load-buffer" {
                DaemonError::ReportedCommandExit {
                    output: RawText::default(),
                    exit_code: 1,
                }
            } else {
                DaemonError::CommandExit {
                    output: RawText::default(),
                    exit_code: 1,
                }
            });
        }
        let streamed_command = command.stdin().filter(|_| argument_sink).map(|stdin| {
            let mut command = command.clone();
            append_stdin_payload(canonical, &mut command.args, stdin.clone());
            command
        });
        let command = streamed_command.as_ref().unwrap_or(command);
        if (daemon_command_dispatch(canonical).is_some() || canonical == "display-panes")
            && let Some(spec) = zz_protocol::catalog_command_spec(canonical)
            && spec.uses_tmux_option_grammar()
        {
            let parsed = zz_protocol::parse_tmux_command_options(spec, command)?;
            let transported_stdin = kind == ClientKind::Command
                && canonical == "load-buffer"
                && load_buffer_stdin_payload(&command.args).is_some();
            let positionals = parsed.positionals.len() - usize::from(transported_stdin);
            spec.validate_positional_minimum(positionals)?;
            spec.validate_positional_maximum(positionals)?;
        }
        if canonical == "new-pane"
            && let Some(name) = gone_path_name(command)
        {
            self.publish_to_client(
                client,
                EventPayload::ClientMessage {
                    pane: context.pane,
                    kind: ClientMessageKind::Warning,
                    text: format!("{name} no longer exists"),
                },
            );
            return Ok(Execution::default());
        }
        let mouse_resolved = context
            .invoking_mouse()
            .and_then(|mouse| zz_mux::resolve_invoking_mouse_targets(command, mouse));
        let command = mouse_resolved.as_ref().unwrap_or(command);
        let format_facts_unread =
            format_facts_unread.filter(|_| streamed_command.is_none() && mouse_resolved.is_none());
        let unattached_watch = {
            let inner = self.inner.lock();
            inner
                .engine
                .destroy_unattached_explicit_anywhere()
                .then(|| inner.attachment.attached.clone())
        };
        let preempted = zz_mux::CommandSpec::DAEMON_COMMAND_NAMES
            .contains(&canonical)
            .then(|| {
                match daemon_command_dispatch(canonical)
                    .expect("daemon command catalog and dispatch must agree")
                {
                    DaemonCommandDispatch::Events => Err(ServerError::InvalidCommand(
                        "events streams to the zz command-line client; run it as `zz events`"
                            .into(),
                    )
                    .into()),
                    DaemonCommandDispatch::AgentCatalog => {
                        self.agent_catalog(client, context, &command.args)
                    }
                    DaemonCommandDispatch::Providers => self.providers(client, &command.args),
                    DaemonCommandDispatch::CapturePane => {
                        self.capture_pane(client, context, canonical, &command.args)
                    }
                    DaemonCommandDispatch::RunShell => {
                        self.run_shell(client, kind, context, canonical, command, queue_execution)
                    }
                    DaemonCommandDispatch::IfShell => {
                        self.if_shell(client, kind, context, canonical, command, queue_execution)
                    }
                    DaemonCommandDispatch::AgentSend => {
                        self.agent_send(client, kind, context, &command.args)
                    }
                    DaemonCommandDispatch::AgentRespond => {
                        self.agent_respond(kind, context, &command.args)
                    }
                    DaemonCommandDispatch::SelectChildChat => {
                        #[cfg(feature = "agent")]
                        let selected = self.select_child_chat(client, kind, context, &command.args);
                        #[cfg(not(feature = "agent"))]
                        let selected = Err(ServerError::InvalidCommand(
                            "select-child-chat needs a zz built with agent support".to_owned(),
                        )
                        .into());
                        selected
                    }
                    DaemonCommandDispatch::FlipAgent => self.flip_agent(context, &command.args),
                    DaemonCommandDispatch::AgentAccount => {
                        self.agent_account(context, &command.args)
                    }
                    DaemonCommandDispatch::AgentAccounts => {
                        self.agent_accounts(client, context, &command.args)
                    }
                    DaemonCommandDispatch::SendLastOutput => {
                        self.send_last_output(client, context, &command.args)
                    }
                    DaemonCommandDispatch::ShowLastOutput => {
                        self.show_last_output(client, context, &command.args)
                    }
                    DaemonCommandDispatch::Inspect => self.inspect(client, context, &command.args),
                    DaemonCommandDispatch::SendText => {
                        self.send_text(client, context, &command.args)
                    }
                    DaemonCommandDispatch::WaitPane => {
                        self.wait_pane(client, kind, context, &command.args)
                    }
                    DaemonCommandDispatch::RunPane => {
                        self.run_pane(client, kind, context, &command.args)
                    }
                    DaemonCommandDispatch::CaptureBrowser => {
                        self.capture_browser(context, &command.args)
                    }
                    DaemonCommandDispatch::LoadBrowserUrl => {
                        self.load_browser_url(context, &command.args)
                    }
                    DaemonCommandDispatch::ReadFile => Self::read_file(&command.args),
                    DaemonCommandDispatch::WriteFile => Self::write_file(&command.args),
                    DaemonCommandDispatch::Simulator => {
                        self.simulator_command(context, canonical, &command.args)
                    }
                    DaemonCommandDispatch::Html => self.html(
                        client,
                        kind,
                        context,
                        &command.args,
                        mux_source,
                        invoking_client_terminal,
                        queue_execution,
                        format_facts_unread,
                    ),
                    DaemonCommandDispatch::DebugMarker => {
                        Ok(debug_marker(client, context, &command.args))
                    }
                    DaemonCommandDispatch::Tools => {
                        workspace_tools_catalog(&command.args).map_err(Into::into)
                    }
                    DaemonCommandDispatch::Tailnet => crate::tailnet::command(
                        &command
                            .args
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>(),
                    )
                    .map(|output| Execution {
                        output: output.into(),
                        effects: Vec::new(),
                    })
                    .map_err(|error| ServerError::InvalidCommand(error).into()),
                    DaemonCommandDispatch::LocalCli => Err(ServerError::InvalidCommand(format!(
                        "{canonical} runs in the zz CLI of the machine it acts for; run it from a shell as `zz {canonical}`"
                    ))
                    .into()),
                    DaemonCommandDispatch::RestartServer => self.restart_server(
                        client,
                        kind,
                        context,
                        &command.args,
                        mux_source,
                        invoking_client_terminal,
                        queue_execution,
                    ),
                    DaemonCommandDispatch::UpdateServer => {
                        self.update_server(client, &command.args)
                    }
                    DaemonCommandDispatch::Buffer => self.buffer_command_for_client(
                        Some(client),
                        kind,
                        context,
                        canonical,
                        &command.args,
                    ),
                    DaemonCommandDispatch::ListClients => {
                        self.list_clients(context, canonical, &command.args)
                    }
                    DaemonCommandDispatch::ShowMessages => {
                        self.show_messages(canonical, &command.args)
                    }
                    DaemonCommandDispatch::PromptHistory => {
                        self.prompt_history_command(canonical, &command.args)
                    }
                    DaemonCommandDispatch::RefreshClient => {
                        self.refresh_client(client, kind, canonical, &command.args)
                    }
                    DaemonCommandDispatch::WaitFor
                        if queue_execution.is_some_and(CommandQueueExecution::is_draining) =>
                    {
                        if let Some(queue_execution) = queue_execution
                            && queue_execution.draining
                            && queue_execution.wait_yields
                        {
                            queue_execution.yield_queue();
                        }
                        Ok(Execution::default())
                    }
                    DaemonCommandDispatch::WaitFor => {
                        self.wait_for_in(client, kind, context, &command.args)
                    }
                    DaemonCommandDispatch::PipePane => {
                        self.pipe_pane(context, canonical, &command.args)
                    }
                    DaemonCommandDispatch::DisplayPopup => self.display_popup(
                        client,
                        kind,
                        context,
                        canonical,
                        &command.args,
                        mux_source,
                        invoking_client_terminal,
                        queue_execution,
                        format_facts_unread,
                    ),
                    DaemonCommandDispatch::DisplayMenu => {
                        self.display_menu(client, kind, context, canonical, command)
                    }
                    DaemonCommandDispatch::ConfirmBefore => self.confirm_before(
                        client,
                        kind,
                        context,
                        canonical,
                        command,
                        queue_execution,
                    ),
                    DaemonCommandDispatch::Lock => {
                        self.lock_command(client, context, canonical, &command.args)
                    }
                    DaemonCommandDispatch::SwitchClient => {
                        self.switch_client(client, kind, context, &command.args)
                    }
                }
            });
        if let Some(result) = preempted {
            if result.is_ok() {
                self.inner.lock().engine.repair_context(context);
            }
            self.enforce_destroy_unattached_if_changed(unattached_watch);
            return result.map_err(|error| match error {
                DaemonError::Server(error) => {
                    let spec = zz_protocol::catalog_command_spec(canonical)
                        .expect("daemon command has catalog metadata");
                    spec.classify_usage_error(error).into()
                }
                error => error,
            });
        }
        let generation = self.inner.lock().engine.state.generation();
        let prompt_route = if canonical == "command-prompt" {
            self.command_prompt_route(client, kind, context, command)?
        } else {
            None
        };
        let client_alias_route = (canonical == "display-message")
            .then(|| self.display_message_client_alias(client, command))
            .flatten();
        let result = if let Some(mut route) = prompt_route {
            let waiter = route
                .wait
                .then(|| self.register_overlay_wait(client, None, queue_execution));
            let result = self.execute_with_mux_source_inner(
                route.client,
                route.kind,
                &mut route.context,
                &route.command,
                mux_source,
                route.terminal,
                queue_execution,
                None,
            );
            if let Some(waiter) = waiter {
                self.finish_overlay_wait(&waiter, result.is_ok());
            }
            result
        } else if let Some(route) = client_alias_route {
            let result = self.execute_with_mux_source_inner(
                client,
                kind,
                context,
                route.command.as_ref().unwrap_or(command),
                mux_source,
                invoking_client_terminal,
                queue_execution,
                None,
            );
            if result.is_ok() && route.command.is_none() {
                self.route_source_error(client, kind, context.pane, "no current client");
            }
            result
        } else {
            self.execute_with_mux_source_inner(
                client,
                kind,
                context,
                command,
                mux_source,
                invoking_client_terminal,
                queue_execution,
                format_facts_unread,
            )
        };
        let publish_snapshot = {
            let inner = self.inner.lock();
            let current = inner.engine.state.generation();
            current != generation && inner.publication.last_published_mux_generation != current
        };
        if publish_snapshot {
            self.publish_snapshot();
        }
        self.enforce_destroy_unattached_if_changed(unattached_watch);
        result
    }

    pub(super) fn display_message_client_alias(
        self: &Arc<Self>,
        client: ClientId,
        command: &CommandInvocation,
    ) -> Option<ClientAliasRoute> {
        if !command.args.iter().any(|argument| {
            argument
                .as_bytes()
                .iter()
                .any(|byte| matches!(byte, b'@' | b'{'))
        }) {
            return None;
        }
        let parsed = parse_buffer_command_args(
            "display-message",
            &command.args,
            &['c', 'd', 'F', 't'],
            &['a', 'C', 'I', 'l', 'N', 'p', 'v'],
        )
        .ok()?;
        let target = parsed.value('t')?;
        if !matches!(target, "@" | "{active}" | "{current}") {
            return None;
        }
        let inner = self.inner.lock();
        let pane = client_attached_session(&inner, client)
            .and_then(|session| inner.engine.state.sessions.get(&session))
            .map(|session| client_focused_window(&inner, client, session))
            .and_then(|window| inner.engine.state.windows.get(&window))
            .map(|window| window.active_pane);
        drop(inner);
        let command = pane.and_then(|pane| {
            let args = replace_short_option_value(
                &command.args,
                &['c', 'd', 'F', 't'],
                't',
                &pane.to_string(),
            )?;
            let mut routed = command.clone();
            routed.args = args;
            Some(routed)
        });
        Some(ClientAliasRoute { command })
    }

    pub(super) fn route_background_inserted_output(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        title: String,
        output: &str,
    ) {
        if kind == ClientKind::Interactive && !output.is_empty() {
            let _ = self.open_command_output(client, context.pane, title, output);
        }
    }

    pub(super) fn publish_background_command_error(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        error: &DaemonError,
        uppercase: bool,
    ) {
        let mut message = daemon_error_text(error);
        if uppercase
            && self.read_client(client, |c| c.and_then(|c| c.kind)) == Some(ClientKind::Interactive)
        {
            uppercase_first(&mut message);
            if client_attached_session(&self.inner.lock(), client).is_some() {
                self.publish_status_message(client, context, message);
                return;
            }
        }
        push_server_message(&mut self.inner.lock(), message.clone());
        self.publish_to_client(
            client,
            EventPayload::ClientMessage {
                pane: context.pane,
                kind: ClientMessageKind::Error,
                text: message,
            },
        );
    }

    pub(super) fn background_insertion_ticket(&self) -> u64 {
        self.next_background_insertion
            .fetch_add(1, Ordering::AcqRel)
    }

    pub(super) fn apply_background_insertion(
        &self,
        ticket: u64,
        work: impl FnOnce() + Send + 'static,
    ) {
        #[cfg(unix)]
        if self.loop_active.load(Ordering::Acquire) {
            self.background_insertions
                .lock()
                .pending
                .insert(ticket, Box::new(work));
            self.accept_wake.wake();
            return;
        }
        self.background_insertions
            .lock()
            .pending
            .insert(ticket, Box::new(work));
        self.drain_background_insertions();
    }

    pub(super) fn drain_background_insertions(&self) {
        if self.next_background_insertion.load(Ordering::Acquire) == 0 {
            return;
        }
        {
            let mut insertions = self.background_insertions.lock();
            if insertions.draining || insertions.pending.is_empty() {
                return;
            }
            insertions.draining = true;
        }
        loop {
            let Some((_, work)) = ({
                let mut insertions = self.background_insertions.lock();
                let next = insertions.pending.pop_first();
                if next.is_none() {
                    insertions.draining = false;
                }
                next
            }) else {
                return;
            };
            work();
        }
    }

    #[cfg(unix)]
    pub(super) fn loop_leaf_enabled(&self) -> bool {
        self.command_item
            .as_ref()
            .is_some_and(|item| item.lock().loop_leaf)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DaemonCommandDispatch {
    Tailnet,
    Events,
    AgentCatalog,
    Providers,
    CapturePane,
    RunShell,
    IfShell,
    AgentSend,
    AgentRespond,
    SelectChildChat,
    FlipAgent,
    AgentAccount,
    AgentAccounts,
    SendLastOutput,
    ShowLastOutput,
    Inspect,
    SendText,
    WaitPane,
    RunPane,
    CaptureBrowser,
    LoadBrowserUrl,
    ReadFile,
    WriteFile,
    Simulator,
    Html,
    DebugMarker,
    Tools,
    LocalCli,
    RestartServer,
    UpdateServer,
    Buffer,
    ListClients,
    ShowMessages,
    PromptHistory,
    RefreshClient,
    WaitFor,
    PipePane,
    DisplayPopup,
    DisplayMenu,
    ConfirmBefore,
    Lock,
    SwitchClient,
}

pub(super) const DAEMON_COMMAND_DISPATCHES: &[(&str, DaemonCommandDispatch)] = &[
    ("events", DaemonCommandDispatch::Events),
    ("agent-catalog", DaemonCommandDispatch::AgentCatalog),
    ("providers", DaemonCommandDispatch::Providers),
    ("capture-pane", DaemonCommandDispatch::CapturePane),
    ("capturep", DaemonCommandDispatch::CapturePane),
    ("run-shell", DaemonCommandDispatch::RunShell),
    ("run", DaemonCommandDispatch::RunShell),
    ("if-shell", DaemonCommandDispatch::IfShell),
    ("if", DaemonCommandDispatch::IfShell),
    ("agent-send", DaemonCommandDispatch::AgentSend),
    ("agent-respond", DaemonCommandDispatch::AgentRespond),
    ("select-child-chat", DaemonCommandDispatch::SelectChildChat),
    ("flip-agent", DaemonCommandDispatch::FlipAgent),
    ("agent-account", DaemonCommandDispatch::AgentAccount),
    ("agent-accounts", DaemonCommandDispatch::AgentAccounts),
    ("send-last-output", DaemonCommandDispatch::SendLastOutput),
    ("show-last-output", DaemonCommandDispatch::ShowLastOutput),
    ("inspect", DaemonCommandDispatch::Inspect),
    ("send-text", DaemonCommandDispatch::SendText),
    ("wait-pane", DaemonCommandDispatch::WaitPane),
    ("run-pane", DaemonCommandDispatch::RunPane),
    ("capture-browser", DaemonCommandDispatch::CaptureBrowser),
    ("load-browser-url", DaemonCommandDispatch::LoadBrowserUrl),
    ("capture-simulator", DaemonCommandDispatch::Simulator),
    ("tap-simulator", DaemonCommandDispatch::Simulator),
    ("swipe-simulator", DaemonCommandDispatch::Simulator),
    ("type-simulator", DaemonCommandDispatch::Simulator),
    ("press-simulator", DaemonCommandDispatch::Simulator),
    ("html", DaemonCommandDispatch::Html),
    ("debug-marker", DaemonCommandDispatch::DebugMarker),
    ("tools", DaemonCommandDispatch::Tools),
    ("fleet", DaemonCommandDispatch::LocalCli),
    ("copy-file", DaemonCommandDispatch::LocalCli),
    ("read-file", DaemonCommandDispatch::ReadFile),
    ("write-file", DaemonCommandDispatch::WriteFile),
    ("list-hosts", DaemonCommandDispatch::LocalCli),
    ("update", DaemonCommandDispatch::LocalCli),
    ("build-info", DaemonCommandDispatch::LocalCli),
    ("restart-server", DaemonCommandDispatch::RestartServer),
    ("update-server", DaemonCommandDispatch::UpdateServer),
    ("tailnet", DaemonCommandDispatch::Tailnet),
    ("set-buffer", DaemonCommandDispatch::Buffer),
    ("setb", DaemonCommandDispatch::Buffer),
    ("show-buffer", DaemonCommandDispatch::Buffer),
    ("showb", DaemonCommandDispatch::Buffer),
    ("list-buffers", DaemonCommandDispatch::Buffer),
    ("lsb", DaemonCommandDispatch::Buffer),
    ("load-buffer", DaemonCommandDispatch::Buffer),
    ("loadb", DaemonCommandDispatch::Buffer),
    ("save-buffer", DaemonCommandDispatch::Buffer),
    ("saveb", DaemonCommandDispatch::Buffer),
    ("delete-buffer", DaemonCommandDispatch::Buffer),
    ("deleteb", DaemonCommandDispatch::Buffer),
    ("paste-buffer", DaemonCommandDispatch::Buffer),
    ("pasteb", DaemonCommandDispatch::Buffer),
    ("list-clients", DaemonCommandDispatch::ListClients),
    ("lsc", DaemonCommandDispatch::ListClients),
    ("show-messages", DaemonCommandDispatch::ShowMessages),
    ("showmsgs", DaemonCommandDispatch::ShowMessages),
    ("clear-prompt-history", DaemonCommandDispatch::PromptHistory),
    ("clearphist", DaemonCommandDispatch::PromptHistory),
    ("show-prompt-history", DaemonCommandDispatch::PromptHistory),
    ("showphist", DaemonCommandDispatch::PromptHistory),
    ("refresh-client", DaemonCommandDispatch::RefreshClient),
    ("refresh", DaemonCommandDispatch::RefreshClient),
    ("wait-for", DaemonCommandDispatch::WaitFor),
    ("wait", DaemonCommandDispatch::WaitFor),
    ("pipe-pane", DaemonCommandDispatch::PipePane),
    ("pipep", DaemonCommandDispatch::PipePane),
    ("display-popup", DaemonCommandDispatch::DisplayPopup),
    ("popup", DaemonCommandDispatch::DisplayPopup),
    ("display-menu", DaemonCommandDispatch::DisplayMenu),
    ("menu", DaemonCommandDispatch::DisplayMenu),
    ("confirm-before", DaemonCommandDispatch::ConfirmBefore),
    ("confirm", DaemonCommandDispatch::ConfirmBefore),
    ("lock-client", DaemonCommandDispatch::Lock),
    ("lockc", DaemonCommandDispatch::Lock),
    ("lock-server", DaemonCommandDispatch::Lock),
    ("lock", DaemonCommandDispatch::Lock),
    ("lock-session", DaemonCommandDispatch::Lock),
    ("locks", DaemonCommandDispatch::Lock),
    ("switch-client", DaemonCommandDispatch::SwitchClient),
    ("switchc", DaemonCommandDispatch::SwitchClient),
];

pub(super) fn daemon_command_dispatch(name: &str) -> Option<DaemonCommandDispatch> {
    let name = canonical_command(name);
    DAEMON_COMMAND_DISPATCHES
        .iter()
        .find_map(|(candidate, dispatch)| (*candidate == name).then_some(*dispatch))
}

pub(super) struct ClientAliasRoute {
    pub(super) command: Option<CommandInvocation>,
}

fn replace_short_option_value(
    args: &[RawText],
    value_options: &[char],
    option: char,
    replacement: &str,
) -> Option<Vec<RawText>> {
    let mut rewritten = args.to_vec();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" || !argument.starts_with('-') || argument == "-" {
            return None;
        }
        let mut consumed_next = false;
        for (offset, candidate) in argument[1..].char_indices() {
            if !value_options.contains(&candidate) {
                continue;
            }
            let value_start = 1 + offset + candidate.len_utf8();
            let attached = value_start < argument.len();
            if candidate == option {
                if attached {
                    rewritten[index] = format!("{}{replacement}", &argument[..value_start]).into();
                } else {
                    *rewritten.get_mut(index + 1)? = replacement.into();
                }
                return Some(rewritten);
            }
            consumed_next = !attached;
            break;
        }
        index += usize::from(consumed_next) + 1;
    }
    None
}

fn current_format_client_with_session(
    inner: &ServerState,
    invoking_client: ClientId,
) -> Option<(ClientId, SessionId)> {
    if let Some(session) = client_attached_session(inner, invoking_client) {
        return Some((invoking_client, session));
    }
    let client = if let Some(session) = inner
        .client(invoking_client)
        .and_then(|c| c.origin.as_ref())
        .and_then(|pane| inner.engine.state.window_for_pane(*pane))
        .and_then(|window| inner.engine.state.windows.get(&window))
        .map(|window| window.session)
    {
        best_client_on_session(inner, session).or_else(|| best_attached_client(inner))
    } else {
        inner
            .engine
            .state
            .most_recent_context()
            .and_then(|(session, _, _)| best_client_on_session(inner, session))
            .or_else(|| best_attached_client(inner))
    }?;
    client_attached_session(inner, client).map(|session| (client, session))
}

fn gone_path_name(command: &CommandInvocation) -> Option<String> {
    let spec = zz_protocol::catalog_command_spec("new-pane")?;
    let parsed = zz_protocol::parse_tmux_command_options(spec, command).ok()?;
    let path = parsed.options.iter().find_map(|option| match option {
        zz_protocol::TmuxOption::Value("--existing", path) => Some(*path),
        _ => None,
    })?;
    let local = match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            home_directory()?.join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(path),
    };
    if local.try_exists().unwrap_or(true) {
        return None;
    }
    let name = path.trim_end_matches(['/', '\\']);
    Some(
        name.rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(name)
            .to_owned(),
    )
}

fn control_reads_legacy_layouts(inner: &ServerState, client: Option<ClientId>) -> bool {
    client
        .and_then(|client| inner.client(client))
        .is_some_and(|client| {
            client.kind == Some(ClientKind::Control)
                && !client
                    .control_output
                    .as_ref()
                    .is_some_and(|output| output.new_layouts)
        })
}

fn legacy_command_output(execution: &mut Execution) {
    let Ok(text) = std::str::from_utf8(execution.output.as_bytes()) else {
        return;
    };
    if let Cow::Owned(converted) = legacy_layouts_in(text) {
        execution.output = RawText::from(converted);
    }
}

fn debug_marker(client: ClientId, context: &ExecutionContext, args: &[RawText]) -> Execution {
    log::info!(
        target: "zz_daemon::diagnostics::marker",
        "user_marker client={client} session={:?} window={:?} pane={:?} note={:?}",
        context.session,
        context.window,
        context.pane,
        args.join(" "),
    );
    log::logger().flush();
    Execution::default()
}

pub(super) fn workspace_tools_catalog(args: &[RawText]) -> Result<Execution, ServerError> {
    let hooks = match args {
        [flag] if flag == "--install-hooks" => Some(crate::AgentHookChange::Install),
        [flag] if flag == "--remove-hooks" => Some(crate::AgentHookChange::Remove),
        _ => None,
    };
    let output = match hooks {
        Some(change) => crate::change_agent_hooks(change).map_err(ServerError::InvalidCommand)?,
        None => workspace_tools(args)?,
    };
    Ok(Execution {
        output: output.into(),
        effects: Vec::new(),
    })
}
