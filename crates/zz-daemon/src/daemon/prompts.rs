use super::*;

impl Shared {
    pub(super) fn command_prompt_route(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        command: &CommandInvocation,
    ) -> Result<Option<CommandPromptRoute>, DaemonError> {
        let Ok(parsed) = parse_buffer_command_args(
            "command-prompt",
            &command.args,
            &['I', 'p', 'T', 't'],
            &['1', 'C', 'b', 'e', 'F', 'i', 'k', 'l', 'N', 'P'],
        ) else {
            return Ok(None);
        };
        if parsed.has('P') {
            return Ok(None);
        }
        let inner = self.inner.lock();
        let target = resolve_client_target(&inner, client, kind, parsed.value('t'))?;
        let target_kind = inner
            .client(target)
            .and_then(|c| c.kind)
            .unwrap_or(ClientKind::Command);
        let terminal = client_terminal(&inner, target, target_kind);
        let mut target_context = context.clone();
        retarget_context_to_attachment(&inner, target, &mut target_context);
        let positional_start = command.args.len().saturating_sub(parsed.positional.len());
        let routed = if parsed.has('F')
            && !parsed.positional.is_empty()
            && !command.argument_is_command_block(positional_start)
        {
            let expanded = expand_command_format(
                &inner,
                &target_context,
                client_attached_session(&inner, target),
                Some("command-prompt"),
                &command.args[positional_start],
                Some(target),
                false,
            );
            let mut routed = command.clone();
            routed.args[positional_start] = expanded.into();
            routed
        } else {
            command.clone()
        };
        let wait = matches!(kind, ClientKind::Command | ClientKind::Control)
            && !context.has_no_client()
            && !parsed.has('b')
            && !parsed.has('i')
            && inner
                .client(target)
                .is_none_or(|c| c.command_prompt.is_none());
        drop(inner);
        Ok(Some(CommandPromptRoute {
            client: target,
            kind: target_kind,
            terminal,
            context: target_context,
            command: routed,
            wait,
        }))
    }

    pub(super) fn confirm_before(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        command_name: &str,
        invocation: &CommandInvocation,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_confirm_before_args(&invocation.args)?;
        let command = parsed
            .command
            .as_deref()
            .expect("confirm command was required");
        let typed = invocation.argument_is_command_block(parsed.positional_start);
        let typed_commands = if typed {
            let command = command_block_body(command).unwrap_or(command);
            Some(self.parse_confirm_commands(command, true)?)
        } else {
            None
        };
        let (target_client, target, expanded_command) = {
            let inner = self.inner.lock();
            let target_client =
                resolve_popup_client(&inner, client, context, parsed.target_client.as_deref())?;
            let pane = client_context_pane(&inner, target_client)
                .ok_or_else(|| ServerError::InvalidCommand("no current client".to_owned()))?;
            let mut target = ExecutionContext::for_pane(&inner.engine.state, pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            target.set_format_client_session(client_attached_session(&inner, target_client));
            target
                .format_variables
                .clone_from(&context.format_variables);
            let expanded_command = (!typed).then(|| {
                expand_popup_value(
                    &inner,
                    &target,
                    context.session,
                    Some(command_name),
                    command,
                    Some(target_client),
                )
            });
            (target_client, target, expanded_command)
        };
        let commands = match typed_commands {
            Some(commands) => commands,
            None => self.parse_confirm_commands(
                expanded_command
                    .as_deref()
                    .expect("string confirm command was expanded"),
                false,
            )?,
        };
        let prompt = if let Some(prompt) = parsed.prompt.as_deref() {
            format!("{prompt} ")
        } else {
            let first = commands
                .first()
                .ok_or_else(|| ServerError::CommandParse("empty command".to_owned()))?;
            format!(
                "Confirm '{}'? ({{confirm-key}}/n) ",
                canonical_command(&first.name)
            )
        };
        let confirm_key = parsed.confirm_key.as_deref().unwrap_or("y");
        let bytes = confirm_key.as_bytes();
        if bytes.len() != 1 || !(32..=126).contains(&bytes[0]) {
            return Err(ServerError::InvalidCommand("invalid confirm key".to_owned()).into());
        }
        let prompt = if parsed.prompt.is_some() {
            prompt
        } else {
            prompt.replace("{confirm-key}", confirm_key)
        };
        let prompt = expand_popup_value(
            &self.inner.lock(),
            &target,
            context.session,
            None,
            &prompt,
            None,
        );
        let prompt_cursors = {
            let inner = self.inner.lock();
            prompt_cursors(&inner, client_attached_session(&inner, target_client))
        };
        let state = ConfirmState {
            prompt,
            confirm_key: bytes[0],
            default_yes: parsed.default_yes,
            prompt_cursor: prompt_cursors[0],
        };
        let blocking_commands = commands.clone();
        let waiter = (!parsed.background
            && matches!(kind, ClientKind::Command | ClientKind::Control))
        .then(|| self.register_overlay_wait(client, Some(commands.clone()), queue_execution));
        let execution = if parsed.background {
            ConfirmExecution::Background { commands }
        } else if matches!(kind, ClientKind::Command | ClientKind::Control) {
            ConfirmExecution::Blocking {
                waiter: self
                    .command_item
                    .as_ref()
                    .unwrap()
                    .lock()
                    .raising_overlay
                    .take()
                    .unwrap(),
            }
        } else {
            ConfirmExecution::Deferred {
                commands,
                context: Box::new(context.clone()),
            }
        };
        let mut events = Vec::new();
        let mut outputs = Vec::new();
        let mut resume_terminals = false;
        {
            let mut inner = self.inner.lock();
            dismiss_overlays(
                &mut inner,
                target_client,
                None,
                &mut events,
                &mut outputs,
                &mut resume_terminals,
            );
            inner
                .client_entry(target_client)
                .confirm
                .replace(ConfirmSession {
                    state: state.clone(),
                    execution,
                });
        }
        for event in events {
            self.publish_to_client(target_client, event);
        }
        for (owner, output) in outputs {
            self.retire_command_output(owner, output);
        }
        if resume_terminals {
            self.resume_client_terminals(target_client);
        }
        self.publish_to_client(target_client, EventPayload::Confirm { state: Some(state) });
        if parsed.background || !matches!(kind, ClientKind::Command | ClientKind::Control) {
            return Ok(Execution::default());
        }
        let waiter = waiter.unwrap();
        self.finish_overlay_wait(&waiter, true);
        if self
            .command_item
            .as_ref()
            .is_some_and(|item| item.lock().pending_wait.is_some())
        {
            return Ok(Execution::default());
        }
        if !waiter.accepted.load(Ordering::Acquire) {
            return Err(DaemonError::CommandExit {
                output: RawText::default(),
                exit_code: 1,
            });
        }
        let mut command_context = context.clone();
        self.run_confirm_commands(
            client,
            kind,
            &mut command_context,
            &blocking_commands,
            queue_execution,
        )
        .map_err(post_admission_callback_error)
    }

    pub(super) fn parse_confirm_commands(
        &self,
        command: &str,
        typed: bool,
    ) -> Result<Vec<CommandInvocation>, DaemonError> {
        let inner = self.inner.lock();
        let result = if typed {
            inner
                .engine
                .prepare_frozen_callback_commands(command, true, "confirm-before")
        } else {
            inner
                .engine
                .prepare_callback_commands(command, false, "confirm-before")
        };
        result.map_err(|error| match error {
            ServerError::CommandParse(message) if !typed => {
                ServerError::InvalidCommand(message).into()
            }
            ServerError::NativeCommandParse(message) if !typed => {
                ServerError::NativeInvalidCommand(message).into()
            }
            error => error.into(),
        })
    }

    pub(super) fn input_confirm(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        action: ConfirmAction,
    ) {
        let Some(session) = self
            .inner
            .lock()
            .client_mut(client)
            .and_then(|c| c.confirm.take())
        else {
            return;
        };
        self.publish_to_client(client, EventPayload::Confirm { state: None });
        let ConfirmAction::Reply(accepted) = action;
        match session.execution {
            ConfirmExecution::Blocking { waiter } => {
                waiter.complete(accepted);
            }
            ConfirmExecution::Deferred {
                commands,
                mut context,
            } if accepted => {
                self.inner.lock().engine.repair_context(&mut context);
                self.execute_confirm_commands(
                    client,
                    ClientKind::Interactive,
                    &mut context,
                    &commands,
                    "confirm-before",
                );
            }
            ConfirmExecution::Background { commands } if accepted => {
                let mut target = {
                    let inner = self.inner.lock();
                    client_context_pane(&inner, client)
                        .and_then(|pane| ExecutionContext::for_pane(&inner.engine.state, pane))
                        .unwrap_or_else(|| context.clone())
                };
                self.execute_confirm_commands(
                    client,
                    ClientKind::Interactive,
                    &mut target,
                    &commands,
                    "confirm-before",
                );
            }
            ConfirmExecution::Deferred { .. } | ConfirmExecution::Background { .. } => {}
        }
    }

    fn execute_confirm_commands(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        commands: &[CommandInvocation],
        title: &str,
    ) {
        #[cfg(unix)]
        if self.loop_active.load(Ordering::Acquire) {
            let title = title.to_owned();
            self.enqueue_inserted_task(
                client,
                kind,
                context,
                &InsertedCommandSource::Commands(commands.to_vec()),
                "<confirm-before>",
                context.control_command_target(),
                false,
                None,
                Box::new(move |shared, context, result| match result {
                    Ok(result) => shared.route_background_inserted_output(
                        client,
                        kind,
                        context,
                        title,
                        &result.output,
                    ),
                    Err(error) => {
                        shared.publish_background_command_error(client, context, &error, false);
                    }
                }),
            );
            return;
        }
        match self.run_confirm_commands(client, kind, context, commands, None) {
            Ok(execution) => self.route_background_inserted_output(
                client,
                kind,
                context,
                title.to_owned(),
                &execution.output,
            ),
            Err(error) => self.publish_background_command_error(client, context, &error, false),
        }
    }

    pub(super) fn run_confirm_commands(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        commands: &[CommandInvocation],
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        if let Some(parent) = queue_execution.filter(|queue| queue.frame_active.get()) {
            parent.insert_foreground_child(InsertedQueueChild {
                context: context.clone(),
                source: InsertedCommandSource::Commands(commands.to_vec()),
                label: "<confirm-before>".to_owned(),
                control_target: context.control_command_target(),
                mux_source: MuxOptionSource::RuntimeCommand,
                kind: InsertedQueueChildKind::Foreground(None),
                leaf: None,
                guard: None,
                leaf_name: None,
            });
            return Ok(Execution::default());
        }
        if queue_execution.is_none() && context.control_command_target().is_some() {
            let queue_execution = self.command_queue_execution(CommandExecutionState {
                draining: false,
                wait_yields: false,
                detached: false,
                deferred_shutdown: Cell::new(DeferredShutdown::None),
                deferred_control_exit: Cell::new(None),
                yielded: Cell::new(CommandQueueYield::None),
                yield_boundary: false,
                shutdown_blocker: RefCell::new(None),
                pending_event_hooks: RefCell::new(Vec::new()),
                deferred_shell_jobs: RefCell::new(Vec::new()),
                callback_parse_failures: RefCell::new(Vec::new()),
                deferred_config_replay_issues: RefCell::new(Vec::new()),
                reported_failures: Cell::new(false),
                frame_active: Cell::new(false),
                child: RefCell::new(None),
                suppress_after_hooks: Cell::new(false),
                suppress_output: Cell::new(false),
            });
            let result =
                self.run_confirm_commands(client, kind, context, commands, Some(&queue_execution));
            let reported_failure = queue_execution.reported_failures.get();
            self.finish_command_queue_execution(&queue_execution, None);
            if reported_failure
                && queue_execution.deferred_shutdown.get() != DeferredShutdown::Force
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
        if self.shutdown_closed()
            && !queue_execution.is_some_and(|queue| queue.is_draining() || queue.detached)
        {
            return Ok(Execution::default());
        }
        let mut output = RawText::default();
        let mut first_error = None;
        let mut reported_error = None;
        let mut failed_group = None;
        for command in commands {
            let routed_name = canonical_command(&command.name);
            let group = command
                .source
                .as_ref()
                .map(|source| (source.source.clone(), source.line));
            if let Some(failed) = &failed_group {
                if failed == &group {
                    continue;
                }
                failed_group = None;
            }
            let callback_failures_start =
                queue_execution.map(|execution| execution.callback_parse_failures.borrow().len());
            let (execution, guarded) = match (context.control_command_target(), queue_execution) {
                (Some(target), Some(queue_execution)) => {
                    let (execution, _, _, callback_failure, _) = self
                        .execute_control_command_with_guard(
                            client,
                            kind,
                            context,
                            command,
                            target,
                            true,
                            MuxOptionSource::RuntimeCommand,
                            InsertedCommandMode::Standard(queue_execution),
                        );
                    if let Some(failure) = callback_failure {
                        queue_execution
                            .callback_parse_failures
                            .borrow_mut()
                            .push(failure);
                    }
                    (execution, true)
                }
                _ => (
                    self.execute_with_mux_source_routed_in_queue(
                        client,
                        kind,
                        context,
                        command,
                        MuxOptionSource::RuntimeCommand,
                        queue_execution,
                    ),
                    false,
                ),
            };
            let callback_parse_depth = execution
                .as_ref()
                .err()
                .map_or(0, post_admission_callback_parse_depth);
            if let (Some(queue_execution), Some(start)) = (queue_execution, callback_failures_start)
                && callback_parse_depth != 0
                && queue_execution.callback_parse_failures.borrow().len() == start
                && let Some(failure) = CallbackParseFailure::for_command(
                    execution
                        .as_ref()
                        .err()
                        .map_or_else(String::new, daemon_error_text),
                    routed_name,
                    None,
                    callback_parse_depth,
                )
            {
                queue_execution
                    .callback_parse_failures
                    .borrow_mut()
                    .push(failure);
            }
            let callback_group_action = match (queue_execution, callback_failures_start) {
                (Some(queue_execution), Some(start)) => queue_execution
                    .callback_group_action_since(start, routed_name, callback_parse_depth),
                _ if callback_parse_depth == 1 => CallbackGroupAction::for_command(routed_name),
                _ => None,
            };
            let nested_callback_only = callback_parse_depth > 1 && callback_group_action.is_none();
            match execution {
                Ok(_) if guarded => {}
                Ok(execution) => append_inserted_output(&mut output, &execution.output),
                Err(error @ DaemonError::ReportedCommandExit { .. }) => {
                    if !guarded && let Some(error_output) = daemon_error_output(&error) {
                        append_inserted_output(&mut output, error_output);
                    }
                    if reported_error.is_none() {
                        reported_error = Some(discard_all_command_output(error));
                    }
                }
                Err(error) => {
                    if !guarded && let Some(error_output) = daemon_error_output(&error) {
                        append_inserted_output(&mut output, error_output);
                    }
                    if first_error.is_none() {
                        first_error = Some(discard_all_command_output(error));
                    }
                    if callback_parse_depth == 0
                        || callback_group_action == Some(CallbackGroupAction::Fail)
                    {
                        failed_group = Some(group);
                    }
                    if callback_parse_depth != 0
                        && (callback_group_action.is_some() || nested_callback_only)
                        && let (Some(queue_execution), Some(start)) =
                            (queue_execution, callback_failures_start)
                    {
                        queue_execution.handle_callback_group_actions_since(start);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(prepend_command_output(output, error)),
            None if reported_error.is_some() => Err(prepend_command_output(
                output,
                reported_error.expect("reported confirm error"),
            )),
            None => Ok(Execution {
                output,
                effects: Vec::new(),
            }),
        }
    }

    pub(super) fn input_command_prompt_text(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        text: &str,
    ) -> bool {
        let result = {
            let mut inner = self.inner.lock();
            let Some(prompt) = inner.client(client).and_then(|c| c.command_prompt.as_ref()) else {
                return false;
            };
            if prompt.reads_raw_keys() {
                return true;
            }
            let history =
                command_prompt_history_snapshot(prompt_history(&inner, prompt.prompt_type));
            let prompt = inner
                .client_mut(client)
                .and_then(|c| c.command_prompt.as_mut())
                .expect("prompt checked above");
            if prompt.insert(text) {
                let incremental = prompt.mode == CommandPromptMode::Incremental;
                Ok((prompt.state(&history), incremental))
            } else {
                Err(())
            }
        };
        match result {
            Ok((state, incremental)) => {
                let fired = incremental.then(|| format!("={}", state.input));
                self.publish_to_client(client, EventPayload::CommandPrompt { state: Some(state) });
                if let Some(input) = fired {
                    self.run_command_prompt_template(client, kind, context, input);
                }
            }
            Err(()) => {
                self.publish_to_client(
                    client,
                    EventPayload::ClientMessage {
                        pane: None,
                        kind: ClientMessageKind::Warning,
                        text: format!(
                            "command prompt input exceeds {} bytes",
                            zz_protocol::MAX_COMMAND_PROMPT_BYTES
                        ),
                    },
                );
            }
        }
        true
    }

    pub(super) fn input_command_prompt_key(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane: PaneId,
        lane: CommittedTextLane,
        input: &zz_terminal::KeyInput,
        text_follows: bool,
    ) -> bool {
        let outcome = {
            let mut inner = self.inner.lock();
            let Some(mut prompt) = inner
                .client_mut(client)
                .and_then(|c| c.command_prompt.take())
            else {
                return false;
            };
            let raw_mode = prompt.reads_raw_keys();
            let prompt_type = prompt.prompt_type;
            let (action, state) = {
                let history = prompt_history(&inner, prompt_type);
                let action = command_prompt_key(&mut prompt, input, text_follows, history);
                let state = matches!(
                    action,
                    PromptKeyAction::Updated | PromptKeyAction::Incremental(_)
                )
                .then(|| prompt.state(history));
                (action, state)
            };
            let consumed_character = (raw_mode && action != PromptKeyAction::SubmitAndPass)
                .then(|| committed_character(input, text_follows))
                .flatten();
            let outcome = command_prompt_outcome(&mut inner, client, prompt, action, state);
            if let Some(character) = consumed_character {
                suppress_committed_character_locked(&mut inner, client, pane, lane, character);
            }
            outcome
        };
        self.finish_command_prompt_outcome(client, kind, context, outcome)
    }

    pub(super) fn input_command_prompt_focus(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        focused: bool,
    ) -> bool {
        let outcome = {
            let mut inner = self.inner.lock();
            let Some(prompt) = inner
                .client_mut(client)
                .and_then(|c| c.command_prompt.take())
            else {
                return false;
            };
            let action = command_prompt_focus_action(prompt.mode, focused);
            command_prompt_outcome(&mut inner, client, prompt, action, None)
        };
        self.finish_command_prompt_outcome(client, kind, context, outcome)
    }

    fn finish_command_prompt_outcome(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        outcome: PromptKeyOutcome,
    ) -> bool {
        match outcome.event {
            PromptWireUpdate::Silent => {}
            PromptWireUpdate::Republish(state) => self.publish_to_client(
                client,
                EventPayload::CommandPrompt {
                    state: Some(*state),
                },
            ),
            PromptWireUpdate::Retire => {
                self.publish_to_client(client, EventPayload::CommandPrompt { state: None });
                self.settle_pane_prompts();
            }
        }
        if outcome.retired {
            self.resume_client_terminals(client);
        }
        if let Some((prompt_type, input)) = &outcome.remember {
            self.record_prompt_history(*prompt_type, input);
        }
        if outcome.limit_exceeded {
            self.publish_to_client(
                client,
                EventPayload::ClientMessage {
                    pane: context.pane,
                    kind: ClientMessageKind::Warning,
                    text: format!(
                        "command prompt input exceeds {} bytes",
                        zz_protocol::MAX_COMMAND_PROMPT_BYTES
                    ),
                },
            );
        }
        if let Some(submission) = outcome.submission {
            self.submit_command_prompt(client, kind, context, &submission);
        }
        drop(outcome.waiter);
        !outcome.pass
    }

    fn settle_pane_prompts(self: &Arc<Self>) {
        let events = hook_events::pane_prompt_hook_events(&mut self.inner.lock());
        self.run_event_hooks(events);
    }

    fn run_command_prompt_template(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        input: String,
    ) {
        let (template, source) = self
            .read_client(client, |c| {
                c.and_then(|c| c.command_prompt.as_ref())
                    .map(|prompt| (prompt.template.clone(), prompt.source.clone()))
            })
            .unwrap_or_default();
        self.submit_command_prompt(
            client,
            kind,
            context,
            &CommandPromptSubmission {
                inputs: vec![input],
                template,
                source,
            },
        );
    }

    pub(super) fn input_command_prompt_action(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        action: CommandPromptAction,
    ) -> Result<(), DaemonError> {
        if kind != ClientKind::Interactive {
            return Err(ServerError::InvalidCommand(
                "command prompt actions require an interactive client".to_owned(),
            )
            .into());
        }
        let (read_only, read_only_pane) = {
            let inner = self.inner.lock();
            let read_only = inner.attachment.client_flags.contains(client);
            (
                read_only,
                read_only
                    .then(|| client_input_pane(&inner, client, context.pane))
                    .flatten(),
            )
        };
        if let Some(pane) = read_only_pane {
            self.note_terminal_input_without_bell(client, pane);
        }
        if read_only {
            return Ok(());
        }

        match action {
            CommandPromptAction::Update { input, cursor } => {
                let fired = {
                    let mut inner = self.inner.lock();
                    match inner
                        .client_mut(client)
                        .and_then(|c| c.command_prompt.as_mut())
                    {
                        Some(prompt) => {
                            prompt.replace_input(input, cursor)?;
                            (prompt.mode == CommandPromptMode::Incremental)
                                .then(|| format!("={}", prompt.input))
                        }
                        None => None,
                    }
                };
                if let Some(input) = fired {
                    self.run_command_prompt_template(client, kind, context, input);
                }
            }
            CommandPromptAction::Submit { input } => {
                if input.len() > zz_protocol::MAX_COMMAND_PROMPT_BYTES {
                    return Err(ServerError::InvalidCommand(format!(
                        "command prompt input exceeds {} bytes",
                        zz_protocol::MAX_COMMAND_PROMPT_BYTES
                    ))
                    .into());
                }
                let (submission, remembered, prompt_type, retired, chained, waiter) = {
                    let mut inner = self.inner.lock();
                    let Some(mut prompt) = inner
                        .client_mut(client)
                        .and_then(|c| c.command_prompt.take())
                    else {
                        return Ok(());
                    };
                    let cursor = u32::try_from(input.chars().count()).unwrap_or(u32::MAX);
                    prompt.replace_input(input, cursor)?;
                    let retired = prompt.freezes();
                    let prompt_type = prompt.prompt_type;
                    let remembered = (!prompt.input.is_empty()).then(|| prompt.input.clone());
                    if prompt.mode != CommandPromptMode::Incremental && prompt.advance() {
                        let state = prompt.state(prompt_history(&inner, prompt_type));
                        inner.client_entry(client).command_prompt.replace(prompt);
                        (None, remembered, prompt_type, false, Some(state), None)
                    } else {
                        let waiter = prompt.waiter.take();
                        let submission = (prompt.mode != CommandPromptMode::Incremental
                            && (prompt.answers.iter().any(|answer| !answer.is_empty())
                                || prompt.template.is_some()))
                        .then_some(CommandPromptSubmission {
                            inputs: prompt.answers,
                            template: prompt.template,
                            source: prompt.source,
                        });
                        (submission, remembered, prompt_type, retired, None, waiter)
                    }
                };
                if let Some(remembered) = &remembered {
                    self.record_prompt_history(prompt_type, remembered);
                }
                if let Some(state) = chained {
                    self.publish_to_client(
                        client,
                        EventPayload::CommandPrompt { state: Some(state) },
                    );
                    return Ok(());
                }
                self.publish_to_client(client, EventPayload::CommandPrompt { state: None });
                self.settle_pane_prompts();
                if retired {
                    self.resume_client_terminals(client);
                }
                if let Some(submission) = submission {
                    self.submit_command_prompt(client, kind, context, &submission);
                }
                drop(waiter);
            }
            CommandPromptAction::Close => {
                let retired = self
                    .inner
                    .lock()
                    .client_mut(client)
                    .and_then(|c| c.command_prompt.take());
                if let Some(retired) = retired {
                    self.publish_to_client(client, EventPayload::CommandPrompt { state: None });
                    self.settle_pane_prompts();
                    if retired.freezes() {
                        self.resume_client_terminals(client);
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn submit_command_prompt(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        submission: &CommandPromptSubmission,
    ) {
        self.submit_template(client, kind, context, submission, "command-prompt");
    }

    pub(super) fn submit_template(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        submission: &CommandPromptSubmission,
        origin: &str,
    ) {
        if self.shutdown_closed() {
            return;
        }
        let typed = matches!(
            &submission.template,
            Some(CommandPromptTemplate::Commands(_))
        );
        let mut commands = match &submission.template {
            Some(CommandPromptTemplate::Commands(commands)) => commands.clone(),
            template => {
                let template = match template {
                    Some(CommandPromptTemplate::String(template)) => template.as_str(),
                    Some(CommandPromptTemplate::Commands(_)) => unreachable!(),
                    None => "%1",
                };
                let command =
                    MuxEngine::substitute_command_prompt_template(template, &submission.inputs);
                let prepared = {
                    let inner = self.inner.lock();
                    let origin_source = format!("<{origin}>");
                    let source = submission
                        .source
                        .as_ref()
                        .map_or(origin_source.as_str(), |source| source.source.as_str());
                    let mut parsed = inner.engine.parse_config(source, &command);
                    if let Some(source) = &submission.source {
                        shift_command_prompt_source(&mut parsed, source);
                    }
                    if let Some(diagnostic) = parsed.diagnostics.first() {
                        Err(submission.source.as_ref().map_or_else(
                            || diagnostic.message.clone(),
                            |_| {
                                format!(
                                    "{}:{}: {}",
                                    diagnostic.source, diagnostic.line, diagnostic.message
                                )
                            },
                        ))
                    } else {
                        let mut commands = parsed.commands;
                        let mut error = None;
                        for command in &mut commands {
                            if let Err(cause) = inner
                                .engine
                                .prepare_callback_invocations(std::slice::from_mut(command), origin)
                            {
                                error = Some(submission.source.as_ref().map_or_else(
                                    || cause.tmux_message(),
                                    |_| config_command_error(command, &cause.tmux_message()),
                                ));
                                break;
                            }
                        }
                        error.map_or_else(|| Ok(commands), Err)
                    }
                };
                match prepared {
                    Ok(commands) => commands,
                    Err(text) => {
                        self.publish_to_client(
                            client,
                            EventPayload::ClientMessage {
                                pane: context.pane,
                                kind: ClientMessageKind::Error,
                                text,
                            },
                        );
                        return;
                    }
                }
            }
        };
        if typed {
            let substitution = self
                .inner
                .lock()
                .engine
                .substitute_command_prompt_commands(&mut commands, &submission.inputs);
            if let Err(error) = substitution {
                self.publish_to_client(
                    client,
                    EventPayload::ClientMessage {
                        pane: context.pane,
                        kind: ClientMessageKind::Error,
                        text: error.to_string(),
                    },
                );
                return;
            }
        }

        let mut output = String::new();
        let mut output_truncated = false;
        let mut failed_group = None;
        for command in commands {
            let group = command
                .source
                .as_ref()
                .map(|source| (source.source.clone(), source.line));
            if typed && let Some(failed) = &failed_group {
                if failed == &group {
                    continue;
                }
                failed_group = None;
            }
            let execution = self.execute_with_mux_source_routed(
                client,
                kind,
                context,
                &command,
                MuxOptionSource::RuntimeCommand,
            );
            match execution {
                Ok(execution) if !execution.output.is_empty() && !output_truncated => {
                    output_truncated = append_command_prompt_output(&mut output, &execution.output);
                }
                Ok(_) => {}
                Err(error @ DaemonError::ReportedCommandExit { .. }) => {
                    if let Some(error_output) = daemon_error_output(&error)
                        && !output_truncated
                    {
                        output_truncated = append_command_prompt_output(&mut output, error_output);
                    }
                }
                Err(error) => {
                    if let Some(error_output) = daemon_error_output(&error)
                        && !output_truncated
                    {
                        output_truncated = append_command_prompt_output(&mut output, error_output);
                    }
                    if typed {
                        self.publish_to_client(
                            client,
                            EventPayload::ClientMessage {
                                pane: context.pane,
                                kind: ClientMessageKind::Error,
                                text: error.to_string(),
                            },
                        );
                        failed_group = Some(group);
                        continue;
                    }
                    if output_truncated {
                        if !output.is_empty() {
                            output.push('\n');
                        }
                        output.push_str(COMMAND_PROMPT_OUTPUT_TRUNCATED);
                    }
                    if !output.is_empty()
                        && let Err(output_error) = self.open_command_output(
                            client,
                            context.pane,
                            "command output".to_owned(),
                            &output,
                        )
                    {
                        self.publish_to_client(
                            client,
                            EventPayload::ClientMessage {
                                pane: context.pane,
                                kind: ClientMessageKind::Error,
                                text: output_error.to_string(),
                            },
                        );
                    }
                    self.publish_to_client(
                        client,
                        EventPayload::ClientMessage {
                            pane: context.pane,
                            kind: ClientMessageKind::Error,
                            text: error.to_string(),
                        },
                    );
                    return;
                }
            }
        }
        if output_truncated {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(COMMAND_PROMPT_OUTPUT_TRUNCATED);
        }
        if !output.is_empty()
            && let Err(error) =
                self.open_command_output(client, context.pane, "command output".to_owned(), &output)
        {
            self.publish_to_client(
                client,
                EventPayload::ClientMessage {
                    pane: context.pane,
                    kind: ClientMessageKind::Error,
                    text: error.to_string(),
                },
            );
        }
    }
}

const MAX_COMMAND_PROMPT_HISTORY_SNAPSHOT_ITEMS: usize = 20;

const MAX_COMMAND_PROMPT_HISTORY_SNAPSHOT_BYTES: usize = 32 * 1024;

fn shift_command_prompt_source(parsed: &mut ParsedConfig, source: &SourceSpan) {
    let offset = source.line.saturating_sub(1);
    for diagnostic in &mut parsed.diagnostics {
        diagnostic.source.clone_from(&source.source);
        diagnostic.line = diagnostic.line.saturating_add(offset);
    }
    for command in &mut parsed.commands {
        if let Some(command_source) = &mut command.source {
            command_source.source.clone_from(&source.source);
            command_source.line = command_source.line.saturating_add(offset);
        }
    }
}

pub(super) struct CommandPromptRoute {
    pub(super) client: ClientId,
    pub(super) kind: ClientKind,
    pub(super) terminal: ClientTerminal,
    pub(super) context: ExecutionContext,
    pub(super) command: CommandInvocation,
    pub(super) wait: bool,
}

#[derive(Clone, Debug)]
pub(super) struct CommandPrompt {
    prompt: String,
    pub(super) input: String,
    pub(super) cursor: usize,
    steps: Vec<CommandPromptStep>,
    current: usize,
    answers: Vec<String>,
    pub(super) template: Option<CommandPromptTemplate>,
    pub(super) source: Option<SourceSpan>,
    history_index: Option<usize>,
    history_draft: String,
    pub(super) prompt_type: CommandPromptType,
    pub(super) mode: CommandPromptMode,
    no_freeze: bool,
    pub(super) pane: Option<PaneId>,
    last: String,
    pub(super) waiter: Option<OverlayWait>,
    vi_keys: bool,
    command_mode: bool,
    word_separators: String,
    pub(super) serial: u64,
    cursors: [PromptCursor; 2],
}

static NEXT_COMMAND_PROMPT: AtomicU64 = AtomicU64::new(1);

impl CommandPrompt {
    pub(super) fn new(
        steps: Vec<CommandPromptStep>,
        template: Option<CommandPromptTemplate>,
        source: Option<SourceSpan>,
        prompt_type: CommandPromptType,
        mode: CommandPromptMode,
        no_freeze: bool,
        pane: Option<PaneId>,
    ) -> Self {
        let first = steps.first();
        let prompt = first.map(|step| step.label.clone()).unwrap_or_default();
        let input = first.map(|step| step.input.clone()).unwrap_or_default();
        let incremental = mode == CommandPromptMode::Incremental;
        let (input, last) = if incremental {
            (String::new(), input)
        } else {
            (input, String::new())
        };
        let cursor = input.len();
        Self {
            prompt,
            history_draft: input.clone(),
            input,
            cursor,
            steps,
            current: 0,
            answers: Vec::new(),
            template,
            source,
            history_index: None,
            prompt_type,
            mode,
            no_freeze,
            pane,
            last,
            waiter: None,
            vi_keys: false,
            command_mode: false,
            word_separators: String::new(),
            serial: NEXT_COMMAND_PROMPT.fetch_add(1, Ordering::Relaxed),
            cursors: [PromptCursor::default(); 2],
        }
    }

    pub(super) const fn with_cursors(mut self, cursors: [PromptCursor; 2]) -> Self {
        self.cursors = cursors;
        self
    }

    pub(super) fn with_key_options(mut self, vi_keys: bool, word_separators: String) -> Self {
        self.vi_keys = vi_keys;
        self.word_separators = word_separators;
        self
    }

    pub(super) fn advance(&mut self) -> bool {
        self.answers.push(std::mem::take(&mut self.input));
        let Some(step) = self.steps.get(self.current.saturating_add(1)) else {
            self.cursor = 0;
            return false;
        };
        self.current = self.current.saturating_add(1);
        self.prompt.clone_from(&step.label);
        self.input.clone_from(&step.input);
        self.cursor = self.input.len();
        self.history_index = None;
        self.history_draft.clone_from(&self.input);
        self.last = String::new();
        true
    }

    fn answers_with(&self, answer: String) -> Vec<String> {
        let mut answers = self.answers.clone();
        answers.push(answer);
        answers
    }

    pub(super) fn kind(&self) -> CommandPromptKind {
        if self.template.is_some() {
            CommandPromptKind::Value
        } else {
            CommandPromptKind::Command
        }
    }

    pub(super) const fn freezes(&self) -> bool {
        !self.no_freeze && !matches!(self.mode, CommandPromptMode::Incremental)
    }

    const fn reads_raw_keys(&self) -> bool {
        matches!(
            self.mode,
            CommandPromptMode::Single | CommandPromptMode::Numeric | CommandPromptMode::Key
        )
    }

    pub(super) fn state(&self, history: &[String]) -> CommandPromptState {
        CommandPromptState {
            prompt: self.prompt.clone(),
            input: self.input.clone(),
            cursor: u32::try_from(self.input[..self.cursor].chars().count()).unwrap_or(u32::MAX),
            kind: self.kind(),
            history: if self.kind() == CommandPromptKind::Command {
                command_prompt_history_snapshot(history)
            } else {
                Vec::new()
            },
            prompt_type: self.prompt_type,
            mode: self.mode,
            no_freeze: self.no_freeze,
            pane: self.pane,
            command_mode: self.command_mode,
            prompt_cursor: self.cursors[usize::from(self.command_mode)],
        }
    }

    fn replace_input(&mut self, input: String, cursor: u32) -> Result<(), ServerError> {
        if input.len() > zz_protocol::MAX_COMMAND_PROMPT_BYTES {
            return Err(ServerError::InvalidCommand(format!(
                "command prompt input exceeds {} bytes",
                zz_protocol::MAX_COMMAND_PROMPT_BYTES
            )));
        }
        let cursor = usize::try_from(cursor).unwrap_or(usize::MAX);
        let cursor = char_index_to_byte(&input, cursor).ok_or_else(|| {
            ServerError::InvalidCommand("command prompt cursor is out of bounds".to_owned())
        })?;
        self.input = input;
        self.cursor = cursor;
        self.finish_edit();
        Ok(())
    }

    pub(super) fn insert(&mut self, text: &str) -> bool {
        if text.is_empty()
            || self.input.len().saturating_add(text.len()) > zz_protocol::MAX_COMMAND_PROMPT_BYTES
        {
            return false;
        }
        self.input.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.finish_edit();
        true
    }

    fn incremental_prefix(&mut self, backward: bool) -> char {
        if self.input.is_empty() {
            self.input.clone_from(&self.last);
            self.cursor = self.input.len();
            self.finish_edit();
            '='
        } else if backward {
            '-'
        } else {
            '+'
        }
    }

    pub(super) fn move_left(&mut self) -> bool {
        let next = previous_char_boundary(&self.input, self.cursor);
        let changed = next != self.cursor;
        self.cursor = next;
        changed
    }

    fn move_right(&mut self) -> bool {
        let next = next_char_boundary(&self.input, self.cursor);
        let changed = next != self.cursor;
        self.cursor = next;
        changed
    }

    fn cursor_index(&self) -> usize {
        self.input[..self.cursor].chars().count()
    }

    fn set_cursor_index(&mut self, index: usize) -> bool {
        let cursor = char_index_to_byte(&self.input, index).unwrap_or(self.input.len());
        let changed = cursor != self.cursor;
        self.cursor = cursor;
        changed
    }

    fn backward_word_index(&self, separators: &str) -> usize {
        let buffer = self.input.chars().collect::<Vec<_>>();
        let mut index = self.cursor_index();
        while index != 0 {
            index -= 1;
            if !prompt_space(buffer.get(index)) {
                break;
            }
        }
        let word_is_separators = prompt_in_list(separators, buffer.get(index));
        while index != 0 {
            index -= 1;
            if prompt_space(buffer.get(index))
                || word_is_separators != prompt_in_list(separators, buffer.get(index))
            {
                index += 1;
                break;
            }
        }
        index
    }

    fn move_word_left(&mut self, separators: &str) -> bool {
        let index = self.backward_word_index(separators);
        self.set_cursor_index(index)
    }

    fn move_word_right(&mut self, vi: bool, separators: &str) -> bool {
        let buffer = self.input.chars().collect::<Vec<_>>();
        let size = buffer.len();
        let mut index = self.cursor_index();
        if !vi {
            while index != size && prompt_space(buffer.get(index)) {
                index += 1;
            }
        }
        if index == size {
            return self.set_cursor_index(index);
        }
        let word_is_separators =
            prompt_in_list(separators, buffer.get(index)) && !prompt_space(buffer.get(index));
        loop {
            index += 1;
            if prompt_space(buffer.get(index)) {
                if vi {
                    while index != size && prompt_space(buffer.get(index)) {
                        index += 1;
                    }
                }
                break;
            }
            if index == size || word_is_separators != prompt_in_list(separators, buffer.get(index))
            {
                break;
            }
        }
        self.set_cursor_index(index)
    }

    fn move_end_word(&mut self, separators: &str) -> bool {
        let buffer = self.input.chars().collect::<Vec<_>>();
        let size = buffer.len();
        let mut index = self.cursor_index();
        if index == size {
            return false;
        }
        loop {
            index += 1;
            if index == size {
                return self.set_cursor_index(index);
            }
            if !prompt_space(buffer.get(index)) {
                break;
            }
        }
        let word_is_separators = prompt_in_list(separators, buffer.get(index));
        loop {
            index += 1;
            if index == size
                || prompt_space(buffer.get(index))
                || word_is_separators != prompt_in_list(separators, buffer.get(index))
            {
                break;
            }
        }
        self.set_cursor_index(index - 1)
    }

    fn delete_backward(&mut self) -> bool {
        let previous = previous_char_boundary(&self.input, self.cursor);
        if previous == self.cursor {
            return false;
        }
        self.input.replace_range(previous..self.cursor, "");
        self.cursor = previous;
        self.finish_edit();
        true
    }

    pub(super) fn delete_forward(&mut self) -> bool {
        let next = next_char_boundary(&self.input, self.cursor);
        if next == self.cursor {
            return false;
        }
        self.input.replace_range(self.cursor..next, "");
        self.finish_edit();
        true
    }

    pub(super) fn delete_previous_word(&mut self, separators: &str) -> bool {
        let end = self.cursor;
        if !self.move_word_left(separators) {
            return false;
        }
        self.input.replace_range(self.cursor..end, "");
        self.finish_edit();
        true
    }

    pub(super) fn clear(&mut self) -> bool {
        if self.input.is_empty() {
            return false;
        }
        self.input.clear();
        self.cursor = 0;
        self.finish_edit();
        true
    }

    fn delete_to_end(&mut self) -> bool {
        if self.cursor == self.input.len() {
            return false;
        }
        self.input.truncate(self.cursor);
        self.finish_edit();
        true
    }

    pub(super) fn history_up(&mut self, history: &[String]) -> bool {
        if history.is_empty() {
            return false;
        }
        let index = self.history_index.map_or_else(
            || {
                self.history_draft.clone_from(&self.input);
                history.len() - 1
            },
            |index| index.saturating_sub(1),
        );
        if self.history_index == Some(index) {
            return false;
        }
        self.history_index = Some(index);
        self.input.clone_from(&history[index]);
        self.cursor = self.input.len();
        true
    }

    pub(super) fn history_down(&mut self, history: &[String]) -> bool {
        let Some(index) = self.history_index else {
            return false;
        };
        if index + 1 < history.len() {
            self.history_index = Some(index + 1);
            self.input.clone_from(&history[index + 1]);
        } else {
            self.history_index = None;
            self.input.clone_from(&self.history_draft);
        }
        self.cursor = self.input.len();
        true
    }

    fn finish_edit(&mut self) {
        self.history_index = None;
        self.history_draft.clone_from(&self.input);
    }
}

fn command_prompt_history_snapshot(history: &[String]) -> Vec<String> {
    let mut bytes: usize = 0;
    let mut snapshot = history
        .iter()
        .rev()
        .take(MAX_COMMAND_PROMPT_HISTORY_SNAPSHOT_ITEMS)
        .take_while(|entry| {
            let next = bytes.saturating_add(entry.len());
            if next > MAX_COMMAND_PROMPT_HISTORY_SNAPSHOT_BYTES {
                false
            } else {
                bytes = next;
                true
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    snapshot.reverse();
    snapshot
}

fn char_index_to_byte(input: &str, index: usize) -> Option<usize> {
    input
        .char_indices()
        .nth(index)
        .map(|(offset, _)| offset)
        .or_else(|| (index == input.chars().count()).then_some(input.len()))
}

fn previous_char_boundary(text: &str, index: usize) -> usize {
    text[..index]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

fn next_char_boundary(text: &str, index: usize) -> usize {
    text[index..]
        .char_indices()
        .nth(1)
        .map_or(text.len(), |(offset, _)| index + offset)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PromptKeyAction {
    Handled,
    Updated,
    Close,
    Submit,
    SubmitIncremental,
    SubmitAnswer(String),
    SubmitAndPass,
    Incremental(char),
    LimitExceeded,
}

#[derive(Default)]
enum PromptWireUpdate {
    #[default]
    Silent,
    Republish(Box<CommandPromptState>),
    Retire,
}

#[derive(Default)]
struct PromptKeyOutcome {
    event: PromptWireUpdate,
    retired: bool,
    remember: Option<(CommandPromptType, String)>,
    submission: Option<CommandPromptSubmission>,
    pass: bool,
    limit_exceeded: bool,
    waiter: Option<OverlayWait>,
}

pub(super) fn command_prompt_focus_action(
    mode: CommandPromptMode,
    focused: bool,
) -> PromptKeyAction {
    match mode {
        CommandPromptMode::Key => {
            PromptKeyAction::SubmitAnswer(if focused { "FocusIn" } else { "FocusOut" }.to_owned())
        }
        CommandPromptMode::Numeric => PromptKeyAction::SubmitAndPass,
        CommandPromptMode::Text
        | CommandPromptMode::Single
        | CommandPromptMode::Incremental
        | CommandPromptMode::BackspaceExit => PromptKeyAction::Handled,
    }
}

fn command_prompt_outcome(
    inner: &mut ServerState,
    client: ClientId,
    prompt: CommandPrompt,
    action: PromptKeyAction,
    state: Option<CommandPromptState>,
) -> PromptKeyOutcome {
    let mut prompt = prompt;
    let prompt_type = prompt.prompt_type;
    match action {
        PromptKeyAction::Handled => {
            inner.client_entry(client).command_prompt.replace(prompt);
            PromptKeyOutcome::default()
        }
        PromptKeyAction::Updated => {
            inner.client_entry(client).command_prompt.replace(prompt);
            PromptKeyOutcome {
                event: PromptWireUpdate::Republish(Box::new(
                    state.expect("an updated prompt built its state"),
                )),
                ..PromptKeyOutcome::default()
            }
        }
        PromptKeyAction::LimitExceeded => {
            inner.client_entry(client).command_prompt.replace(prompt);
            PromptKeyOutcome {
                limit_exceeded: true,
                ..PromptKeyOutcome::default()
            }
        }
        PromptKeyAction::Incremental(prefix) => {
            let fired = CommandPromptSubmission {
                inputs: vec![format!("{prefix}{}", prompt.input)],
                template: prompt.template.clone(),
                source: prompt.source.clone(),
            };
            inner.client_entry(client).command_prompt.replace(prompt);
            PromptKeyOutcome {
                event: PromptWireUpdate::Republish(Box::new(
                    state.expect("an incremental edit built its state"),
                )),
                submission: Some(fired),
                ..PromptKeyOutcome::default()
            }
        }
        PromptKeyAction::Close => PromptKeyOutcome {
            event: PromptWireUpdate::Retire,
            retired: prompt.freezes(),
            waiter: prompt.waiter.take(),
            ..PromptKeyOutcome::default()
        },
        PromptKeyAction::SubmitIncremental => PromptKeyOutcome {
            event: PromptWireUpdate::Retire,
            retired: prompt.freezes(),
            waiter: prompt.waiter.take(),
            remember: (!prompt.input.is_empty()).then_some((prompt_type, prompt.input)),
            ..PromptKeyOutcome::default()
        },
        PromptKeyAction::SubmitAnswer(answer) => PromptKeyOutcome {
            event: PromptWireUpdate::Retire,
            retired: prompt.freezes(),
            waiter: prompt.waiter.take(),
            submission: Some(CommandPromptSubmission {
                inputs: prompt.answers_with(answer),
                template: prompt.template,
                source: prompt.source,
            }),
            ..PromptKeyOutcome::default()
        },
        PromptKeyAction::Submit => {
            let retired = prompt.freezes();
            let remember =
                (!prompt.input.is_empty()).then_some((prompt_type, prompt.input.clone()));
            if prompt.advance() {
                let state = prompt.state(prompt_history(inner, prompt_type));
                inner.client_entry(client).command_prompt.replace(prompt);
                return PromptKeyOutcome {
                    event: PromptWireUpdate::Republish(Box::new(state)),
                    remember,
                    ..PromptKeyOutcome::default()
                };
            }
            let submission = (prompt.answers.iter().any(|answer| !answer.is_empty())
                || prompt.template.is_some())
            .then(|| CommandPromptSubmission {
                inputs: prompt.answers.clone(),
                template: prompt.template.clone(),
                source: prompt.source.clone(),
            });
            PromptKeyOutcome {
                event: PromptWireUpdate::Retire,
                retired,
                remember,
                submission,
                pass: false,
                limit_exceeded: false,
                waiter: prompt.waiter.take(),
            }
        }
        PromptKeyAction::SubmitAndPass => {
            let retired = prompt.freezes();
            let waiter = prompt.waiter.take();
            let inputs = prompt.answers_with(prompt.input.clone());
            let submission = (inputs.iter().any(|input| !input.is_empty())
                || prompt.template.is_some())
            .then_some({
                CommandPromptSubmission {
                    inputs,
                    template: prompt.template,
                    source: prompt.source,
                }
            });
            PromptKeyOutcome {
                event: PromptWireUpdate::Retire,
                retired,
                remember: None,
                submission,
                pass: true,
                limit_exceeded: false,
                waiter,
            }
        }
    }
}

fn client_input_pane(
    inner: &ServerState,
    client: ClientId,
    preferred: Option<PaneId>,
) -> Option<PaneId> {
    preferred
        .filter(|pane| client_is_attached_to_pane(inner, client, *pane))
        .or_else(|| {
            client_focused_window_for_attachment(inner, client)
                .and_then(|window| inner.engine.state.windows.get(&window))
                .map(|window| window.active_pane)
        })
}

fn command_prompt_raw_key(
    prompt: &mut CommandPrompt,
    input: &zz_terminal::KeyInput,
) -> PromptKeyAction {
    if input.modifiers.platform() {
        return PromptKeyAction::Handled;
    }
    match prompt.mode {
        CommandPromptMode::Key => PromptKeyAction::SubmitAnswer(prompt_key_spelling(input)),
        CommandPromptMode::Numeric => match input.key {
            zz_terminal::KeyCode::Character(character)
                if character.is_ascii_digit()
                    && !input.modifiers.control()
                    && !input.modifiers.alt() =>
            {
                let mut encoded = [0_u8; 4];
                if prompt.insert(character.encode_utf8(&mut encoded)) {
                    PromptKeyAction::Updated
                } else {
                    PromptKeyAction::LimitExceeded
                }
            }
            _ => PromptKeyAction::SubmitAndPass,
        },
        _ => {
            let Some(character) = single_key_character(input) else {
                return PromptKeyAction::Handled;
            };
            let mut encoded = [0_u8; 4];
            if !prompt.insert(character.encode_utf8(&mut encoded)) {
                return PromptKeyAction::LimitExceeded;
            }
            if prompt.input.chars().count() == 1 {
                PromptKeyAction::SubmitAnswer(prompt.input.clone())
            } else {
                PromptKeyAction::Close
            }
        }
    }
}

fn single_key_character(input: &zz_terminal::KeyInput) -> Option<char> {
    match input.key {
        zz_terminal::KeyCode::Backspace => Some('\u{7f}'),
        zz_terminal::KeyCode::Escape => Some('\u{1b}'),
        zz_terminal::KeyCode::Enter => Some('\r'),
        zz_terminal::KeyCode::Tab => Some('\t'),
        zz_terminal::KeyCode::Character(character) if input.modifiers.control() => character
            .is_ascii()
            .then(|| char::from(character.to_ascii_uppercase() as u8 & 0x1f)),
        zz_terminal::KeyCode::Character(character) => Some(
            if input.modifiers.shift() && character.is_ascii_lowercase() {
                character.to_ascii_uppercase()
            } else {
                character
            },
        ),
        _ => None,
    }
}

fn prompt_space(character: Option<&char>) -> bool {
    character == Some(&' ')
}

fn prompt_in_list(separators: &str, character: Option<&char>) -> bool {
    character.is_some_and(|character| character.is_ascii() && separators.contains(*character))
}

enum PromptViKey {
    Edit(zz_terminal::KeyInput),
    Motion(PromptWordMotion),
    Append,
    Handled,
}

#[derive(Clone, Copy)]
enum PromptWordMotion {
    Forward { vi: bool, separators: bool },
    End { separators: bool },
    Backward { separators: bool },
}

impl PromptWordMotion {
    pub(super) fn apply(self, prompt: &mut CommandPrompt) -> bool {
        let separators = std::mem::take(&mut prompt.word_separators);
        let changed = match self {
            Self::Forward {
                vi,
                separators: use_separators,
            } => prompt.move_word_right(vi, if use_separators { &separators } else { "" }),
            Self::End {
                separators: use_separators,
            } => prompt.move_end_word(if use_separators { &separators } else { "" }),
            Self::Backward {
                separators: use_separators,
            } => prompt.move_word_left(if use_separators { &separators } else { "" }),
        };
        prompt.word_separators = separators;
        changed
    }
}

fn prompt_key_input(
    input: &zz_terminal::KeyInput,
    key: zz_terminal::KeyCode,
    control: bool,
    alt: bool,
) -> zz_terminal::KeyInput {
    zz_terminal::KeyInput {
        action: input.action,
        key,
        modifiers: zz_terminal::Modifiers::new(false, control, alt, false),
        text: None,
        unshifted_codepoint: None,
    }
}

fn prompt_translate_key(prompt: &mut CommandPrompt, input: &zz_terminal::KeyInput) -> PromptViKey {
    use zz_terminal::KeyCode;
    let control = input.modifiers.control();
    let alt = input.modifiers.alt();
    let character = match input.key {
        KeyCode::Character(character) => Some(
            if input.modifiers.shift() && character.is_ascii_lowercase() {
                character.to_ascii_uppercase()
            } else {
                character
            },
        ),
        _ => None,
    };
    let plain = |key| prompt_key_input(input, key, false, false);
    let control_letter = |letter| prompt_key_input(input, KeyCode::Character(letter), true, false);
    if !prompt.command_mode {
        if control
            && let Some(letter) = character
            && matches!(
                letter.to_ascii_lowercase(),
                'a' | 'c' | 'e' | 'g' | 'h' | 'k' | 'n' | 'p' | 't' | 'u' | 'v' | 'w' | 'y'
            )
        {
            return PromptViKey::Edit(input.clone());
        }
        if control && character == Some('[') {
            prompt.command_mode = true;
            prompt.move_left();
            return PromptViKey::Handled;
        }
        if !control && !alt {
            match input.key {
                KeyCode::Escape => {
                    prompt.command_mode = true;
                    prompt.move_left();
                    return PromptViKey::Handled;
                }
                KeyCode::Tab
                | KeyCode::Enter
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::ArrowDown
                | KeyCode::End
                | KeyCode::Home
                | KeyCode::ArrowLeft
                | KeyCode::ArrowRight
                | KeyCode::ArrowUp => return PromptViKey::Edit(input.clone()),
                _ => {}
            }
        }
        if (control || alt) && matches!(input.key, KeyCode::ArrowLeft | KeyCode::ArrowRight) {
            return PromptViKey::Edit(input.clone());
        }
        return PromptViKey::Append;
    }

    if input.key == KeyCode::Backspace {
        return PromptViKey::Edit(plain(KeyCode::ArrowLeft));
    }
    if control && character == Some('[') {
        return PromptViKey::Handled;
    }
    if input.key == KeyCode::Escape && !control && !alt {
        return PromptViKey::Handled;
    }
    if !control && !alt {
        match character {
            Some('A' | 'I' | 'C' | 's' | 'a') => prompt.command_mode = false,
            Some('S') => {
                prompt.command_mode = false;
                return PromptViKey::Edit(control_letter('u'));
            }
            Some('i') => {
                prompt.command_mode = false;
                return PromptViKey::Handled;
            }
            _ => {}
        }
    }
    if control && let Some(letter) = character {
        return match letter.to_ascii_lowercase() {
            'h' | 'c' => PromptViKey::Edit(input.clone()),
            _ => PromptViKey::Handled,
        };
    }
    if alt {
        return PromptViKey::Handled;
    }
    match input.key {
        KeyCode::Enter => return PromptViKey::Edit(plain(KeyCode::Enter)),
        KeyCode::Delete => return PromptViKey::Edit(plain(KeyCode::Delete)),
        KeyCode::ArrowDown => return PromptViKey::Edit(plain(KeyCode::ArrowDown)),
        KeyCode::ArrowLeft => return PromptViKey::Edit(plain(KeyCode::ArrowLeft)),
        KeyCode::ArrowRight => return PromptViKey::Edit(plain(KeyCode::ArrowRight)),
        KeyCode::ArrowUp => return PromptViKey::Edit(plain(KeyCode::ArrowUp)),
        _ => {}
    }
    match character {
        Some('A' | '$') => PromptViKey::Edit(plain(KeyCode::End)),
        Some('I' | '0' | '^') => PromptViKey::Edit(plain(KeyCode::Home)),
        Some('C' | 'D') => PromptViKey::Edit(control_letter('k')),
        Some('X') => PromptViKey::Edit(plain(KeyCode::Backspace)),
        Some('b') => PromptViKey::Motion(PromptWordMotion::Backward { separators: true }),
        Some('B') => PromptViKey::Motion(PromptWordMotion::Backward { separators: false }),
        Some('d') => PromptViKey::Edit(control_letter('u')),
        Some('e') => PromptViKey::Motion(PromptWordMotion::End { separators: true }),
        Some('E') => PromptViKey::Motion(PromptWordMotion::End { separators: false }),
        Some('w') => PromptViKey::Motion(PromptWordMotion::Forward {
            vi: true,
            separators: true,
        }),
        Some('W') => PromptViKey::Motion(PromptWordMotion::Forward {
            vi: true,
            separators: false,
        }),
        Some('q') => PromptViKey::Edit(control_letter('c')),
        Some('s' | 'x') => PromptViKey::Edit(plain(KeyCode::Delete)),
        Some('j') => PromptViKey::Edit(plain(KeyCode::ArrowDown)),
        Some('h') => PromptViKey::Edit(plain(KeyCode::ArrowLeft)),
        Some('a' | 'l') => PromptViKey::Edit(plain(KeyCode::ArrowRight)),
        Some('k') => PromptViKey::Edit(plain(KeyCode::ArrowUp)),
        _ => PromptViKey::Handled,
    }
}

pub(super) fn command_prompt_key(
    prompt: &mut CommandPrompt,
    input: &zz_terminal::KeyInput,
    text_follows: bool,
    history: &[String],
) -> PromptKeyAction {
    if input.action == zz_terminal::KeyAction::Release {
        return PromptKeyAction::Handled;
    }
    if prompt.reads_raw_keys() {
        return command_prompt_raw_key(prompt, input);
    }
    let incremental = prompt.mode == CommandPromptMode::Incremental;
    if incremental
        && matches!(
            input.key,
            zz_terminal::KeyCode::ArrowUp
                | zz_terminal::KeyCode::ArrowDown
                | zz_terminal::KeyCode::PageUp
                | zz_terminal::KeyCode::PageDown
        )
    {
        return PromptKeyAction::Close;
    }
    let before = (prompt.command_mode, prompt.cursor);
    let translated = prompt.vi_keys.then(|| prompt_translate_key(prompt, input));
    let action = match translated {
        None => command_prompt_edit_key(prompt, input, text_follows, history),
        Some(PromptViKey::Handled) => PromptKeyAction::Handled,
        Some(PromptViKey::Motion(motion)) => {
            if motion.apply(prompt) {
                PromptKeyAction::Updated
            } else {
                PromptKeyAction::Handled
            }
        }
        Some(PromptViKey::Edit(key)) => {
            command_prompt_edit_key(prompt, &key, text_follows, history)
        }
        Some(PromptViKey::Append) => match input.key {
            zz_terminal::KeyCode::Character(character)
                if !input.modifiers.control()
                    && !input.modifiers.alt()
                    && !input.modifiers.platform() =>
            {
                if text_follows {
                    PromptKeyAction::Handled
                } else {
                    let text = input
                        .text
                        .as_deref()
                        .map_or_else(|| character.to_string(), str::to_owned);
                    if prompt.insert(&text) {
                        PromptKeyAction::Updated
                    } else {
                        PromptKeyAction::LimitExceeded
                    }
                }
            }
            _ => PromptKeyAction::Handled,
        },
    };
    match action {
        PromptKeyAction::Handled if (prompt.command_mode, prompt.cursor) != before => {
            PromptKeyAction::Updated
        }
        PromptKeyAction::Updated if incremental => PromptKeyAction::Incremental('='),
        PromptKeyAction::Submit if incremental => PromptKeyAction::SubmitIncremental,
        other => other,
    }
}

fn command_prompt_edit_key(
    prompt: &mut CommandPrompt,
    input: &zz_terminal::KeyInput,
    text_follows: bool,
    history: &[String],
) -> PromptKeyAction {
    let changed = |changed| {
        if changed {
            PromptKeyAction::Updated
        } else {
            PromptKeyAction::Handled
        }
    };
    let control = input.modifiers.control();
    let alt = input.modifiers.alt();
    let platform = input.modifiers.platform();

    let backspace = |prompt: &mut CommandPrompt| {
        if prompt.mode == CommandPromptMode::BackspaceExit && prompt.input.is_empty() {
            PromptKeyAction::Close
        } else {
            changed(prompt.delete_backward())
        }
    };

    match input.key {
        zz_terminal::KeyCode::Enter => PromptKeyAction::Submit,
        zz_terminal::KeyCode::Escape => PromptKeyAction::Close,
        zz_terminal::KeyCode::Backspace => backspace(prompt),
        zz_terminal::KeyCode::Delete => changed(prompt.delete_forward()),
        zz_terminal::KeyCode::Home => changed({
            let changed = prompt.cursor != 0;
            prompt.cursor = 0;
            changed
        }),
        zz_terminal::KeyCode::End => changed({
            let changed = prompt.cursor != prompt.input.len();
            prompt.cursor = prompt.input.len();
            changed
        }),
        zz_terminal::KeyCode::ArrowLeft if control || alt => {
            changed(PromptWordMotion::Backward { separators: true }.apply(prompt))
        }
        zz_terminal::KeyCode::ArrowRight if control || alt => changed(
            PromptWordMotion::Forward {
                vi: false,
                separators: true,
            }
            .apply(prompt),
        ),
        zz_terminal::KeyCode::ArrowLeft if platform => changed({
            let changed = prompt.cursor != 0;
            prompt.cursor = 0;
            changed
        }),
        zz_terminal::KeyCode::ArrowRight if platform => changed({
            let changed = prompt.cursor != prompt.input.len();
            prompt.cursor = prompt.input.len();
            changed
        }),
        zz_terminal::KeyCode::ArrowLeft => changed(prompt.move_left()),
        zz_terminal::KeyCode::ArrowRight => changed(prompt.move_right()),
        zz_terminal::KeyCode::ArrowUp => changed(prompt.history_up(history)),
        zz_terminal::KeyCode::ArrowDown => changed(prompt.history_down(history)),
        zz_terminal::KeyCode::Character(character) if control => {
            match character.to_ascii_lowercase() {
                'a' => changed({
                    let changed = prompt.cursor != 0;
                    prompt.cursor = 0;
                    changed
                }),
                'e' => changed({
                    let changed = prompt.cursor != prompt.input.len();
                    prompt.cursor = prompt.input.len();
                    changed
                }),
                'b' => changed(prompt.move_left()),
                'f' => changed(prompt.move_right()),
                'p' => changed(prompt.history_up(history)),
                'n' => changed(prompt.history_down(history)),
                'h' => backspace(prompt),
                'd' => changed(prompt.delete_forward()),
                'u' => changed(prompt.clear()),
                'k' => changed(prompt.delete_to_end()),
                'w' => changed({
                    let separators = std::mem::take(&mut prompt.word_separators);
                    let deleted = prompt.delete_previous_word(&separators);
                    prompt.word_separators = separators;
                    deleted
                }),
                'c' | 'g' | '[' => PromptKeyAction::Close,
                letter @ ('r' | 's') if prompt.mode == CommandPromptMode::Incremental => {
                    PromptKeyAction::Incremental(prompt.incremental_prefix(letter == 'r'))
                }
                _ => PromptKeyAction::Handled,
            }
        }
        zz_terminal::KeyCode::Character(character) if alt => match character.to_ascii_lowercase() {
            'b' => changed(PromptWordMotion::Backward { separators: true }.apply(prompt)),
            'f' => changed(
                PromptWordMotion::Forward {
                    vi: false,
                    separators: true,
                }
                .apply(prompt),
            ),
            _ => PromptKeyAction::Handled,
        },
        zz_terminal::KeyCode::Character(_) if platform => PromptKeyAction::Handled,
        zz_terminal::KeyCode::Character(character) => {
            if text_follows {
                PromptKeyAction::Handled
            } else {
                let text = input
                    .text
                    .as_deref()
                    .map_or_else(|| character.to_string(), str::to_owned);
                if prompt.insert(&text) {
                    PromptKeyAction::Updated
                } else {
                    PromptKeyAction::LimitExceeded
                }
            }
        }
        zz_terminal::KeyCode::Tab
        | zz_terminal::KeyCode::Insert
        | zz_terminal::KeyCode::PageUp
        | zz_terminal::KeyCode::PageDown
        | zz_terminal::KeyCode::Function(_)
        | zz_terminal::KeyCode::Unidentified
        | zz_terminal::KeyCode::User(_) => PromptKeyAction::Handled,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedConfirmBefore {
    background: bool,
    default_yes: bool,
    confirm_key: Option<String>,
    prompt: Option<String>,
    target_client: Option<String>,
    command: Option<String>,
    positional_start: usize,
}

fn parse_confirm_before_args(args: &[RawText]) -> Result<ParsedConfirmBefore, ServerError> {
    let parsed = parse_buffer_command_args("confirm-before", args, &['c', 'p', 't'], &['b', 'y'])?;
    let positional_start = args.len().saturating_sub(parsed.positional.len());
    let [command] = parsed.positional.as_slice() else {
        unreachable!("confirm-before positional bounds were validated");
    };
    Ok(ParsedConfirmBefore {
        background: parsed.has('b'),
        default_yes: parsed.has('y'),
        confirm_key: parsed.value('c').map(str::to_owned),
        prompt: parsed.value('p').map(str::to_owned),
        target_client: parsed.value('t').map(str::to_owned),
        command: Some(command.clone()),
        positional_start,
    })
}
