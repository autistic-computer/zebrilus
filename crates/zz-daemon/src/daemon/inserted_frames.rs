use super::*;

impl Shared {
    pub(super) fn execute_foreground_inserted_commands(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        source: &InsertedCommandSource,
        label: &str,
        parent_queue: Option<&CommandQueueExecution>,
    ) -> Result<InsertedCommandResult, DaemonError> {
        if let Some(parent) = parent_queue.filter(|queue| queue.frame_active.get()) {
            parent.insert_foreground_child(InsertedQueueChild {
                context: context.clone(),
                source: source.clone(),
                label: label.to_owned(),
                control_target: context.control_command_target(),
                mux_source: MuxOptionSource::RuntimeCommand,
                kind: InsertedQueueChildKind::Foreground(None),
                leaf: None,
                guard: None,
                leaf_name: None,
            });
            return Ok(InsertedCommandResult::default());
        }
        let control_target = context.control_command_target();
        let callback_failures_start =
            parent_queue.map(|execution| execution.callback_parse_failures.borrow().len());
        let result = self.execute_inserted_commands_with_control_target_in_queue(
            client,
            kind,
            context,
            source,
            label,
            control_target,
            parent_queue,
            false,
            None,
        );
        if let (Some(queue_execution), Some(start)) = (parent_queue, callback_failures_start) {
            queue_execution.nest_callback_parse_failures_since(start);
        }
        result.map_err(post_admission_callback_error)
    }

    pub(super) fn execute_command_alias_group(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
        client_terminal: ClientTerminal,
        queue_execution: &CommandQueueExecution,
    ) -> Result<Execution, DaemonError> {
        let body = MuxEngine::command_alias_group_body(command).ok_or_else(|| {
            ServerError::CommandParse(format!("unknown command: {}", command.name))
        })?;
        let source = InsertedCommandSource::Block(body.into());
        let control_target = context.control_command_target();
        let result = self.execute_inserted_commands_with_control_target_and_mux_source(
            client,
            kind,
            context,
            &source,
            "<command-alias>",
            control_target,
            mux_source,
            InsertedCommandMode::CommandAlias {
                client_terminal,
                queue_execution,
                stdin: command.stdin(),
            },
        )?;
        inserted_execution(client, kind, result)
    }

    pub(super) fn execute_inserted_commands(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        source: &InsertedCommandSource,
        label: &str,
    ) -> Result<InsertedCommandResult, DaemonError> {
        self.execute_inserted_commands_with_control_target(
            client, kind, context, source, label, None,
        )
    }

    pub(super) fn execute_inserted_commands_with_control_target(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        source: &InsertedCommandSource,
        label: &str,
        control_target: Option<(ClientId, u8)>,
    ) -> Result<InsertedCommandResult, DaemonError> {
        self.execute_inserted_commands_with_control_target_in_queue(
            client,
            kind,
            context,
            source,
            label,
            control_target,
            None,
            false,
            None,
        )
    }

    pub(super) fn execute_detached_inserted_commands_with_control_target(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        source: &InsertedCommandSource,
        label: &str,
        control_target: Option<(ClientId, u8)>,
        shutdown_blocker: Option<ShutdownBlocker>,
    ) -> Result<InsertedCommandResult, DaemonError> {
        self.execute_inserted_commands_with_control_target_in_queue(
            client,
            kind,
            context,
            source,
            label,
            control_target,
            None,
            true,
            shutdown_blocker,
        )
    }

    fn execute_inserted_commands_with_control_target_in_queue(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        source: &InsertedCommandSource,
        label: &str,
        control_target: Option<(ClientId, u8)>,
        parent_queue: Option<&CommandQueueExecution>,
        detached: bool,
        shutdown_blocker: Option<ShutdownBlocker>,
    ) -> Result<InsertedCommandResult, DaemonError> {
        self.execution_item()
            .execute_inserted_commands_with_control_target_in_queue_in_item(
                client,
                kind,
                context,
                source,
                label,
                control_target,
                parent_queue,
                detached,
                shutdown_blocker,
            )
    }

    pub(super) fn inserted_child_execution(
        &self,
        parent_queue: Option<&CommandQueueExecution>,
        detached: bool,
        shutdown_blocker: Option<ShutdownBlocker>,
    ) -> CommandQueueExecution {
        let detached = parent_queue.map_or(detached, |execution| execution.detached);
        let shutdown_blocker = shutdown_blocker.or_else(|| {
            parent_queue.and_then(|execution| execution.fork_shutdown_blocker(detached))
        });
        self.command_queue_execution(CommandExecutionState {
            draining: parent_queue.is_some_and(CommandQueueExecution::is_draining),
            wait_yields: parent_queue.is_some_and(|execution| execution.wait_yields),
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
                parent_queue.is_some_and(|execution| execution.suppress_after_hooks.get()),
            ),
            suppress_output: Cell::new(
                parent_queue.is_some_and(|execution| execution.suppress_output.get()),
            ),
        })
    }

    fn execute_inserted_commands_with_control_target_in_queue_in_item(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        source: &InsertedCommandSource,
        label: &str,
        control_target: Option<(ClientId, u8)>,
        parent_queue: Option<&CommandQueueExecution>,
        detached: bool,
        shutdown_blocker: Option<ShutdownBlocker>,
    ) -> Result<InsertedCommandResult, DaemonError> {
        let queue_execution =
            self.inserted_child_execution(parent_queue, detached, shutdown_blocker);
        let mut result = self.execute_inserted_commands_with_control_target_and_mux_source(
            client,
            kind,
            context,
            source,
            label,
            control_target,
            MuxOptionSource::RuntimeCommand,
            InsertedCommandMode::Standard(&queue_execution),
        );
        if let Ok(result) = &mut result {
            result.yielded = queue_execution.has_yielded();
        }
        let reported_failure = queue_execution.reported_failures.get();
        self.finish_command_queue_execution(&queue_execution, parent_queue);
        if parent_queue.is_none()
            && !detached
            && reported_failure
            && queue_execution.deferred_shutdown.get() != DeferredShutdown::Force
            && matches!(kind, ClientKind::Command | ClientKind::Control)
            && let Ok(result) = &result
        {
            return Err(DaemonError::ReportedCommandExit {
                output: result.output.clone(),
                exit_code: 1,
            });
        }
        result
    }

    pub(super) fn execute_inserted_commands_with_control_target_and_mux_source(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        source: &InsertedCommandSource,
        label: &str,
        control_target: Option<(ClientId, u8)>,
        mux_source: MuxOptionSource,
        mode: InsertedCommandMode,
    ) -> Result<InsertedCommandResult, DaemonError> {
        let alias_terminal = match mode {
            InsertedCommandMode::Standard(_) => None,
            InsertedCommandMode::CommandAlias {
                client_terminal, ..
            } => Some(client_terminal),
        };
        let stdin = match mode {
            InsertedCommandMode::CommandAlias { stdin, .. } => stdin.cloned(),
            InsertedCommandMode::Standard(_) => None,
        };
        let root = self.prepare_inserted_queue_frame(
            context.clone(),
            source,
            label,
            control_target,
            mux_source,
            InsertedFrameExecution::Borrowed(mode.queue_execution()),
            alias_terminal,
            stdin,
        )?;
        let mut frames = vec![root];
        let (finished, result) = self
            .advance_inserted_frames(
                client,
                kind,
                &mut frames,
                usize::MAX,
                false,
                false,
                |frame, command, target| {
                    Some(self.execute_inserted_frame_command(client, kind, frame, command, target))
                },
            )
            .expect("synchronous inserted queue finished");
        *context = finished.context;
        result
    }

    pub(super) fn run_inserted_queue_frames<E: FrameExecution>(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        frames: &mut Vec<InsertedQueueFrame<E>>,
        inline: bool,
        step_limit: bool,
    ) -> Option<(
        InsertedQueueFrame<E>,
        Result<InsertedCommandResult, DaemonError>,
    )> {
        self.advance_inserted_frames(
            client,
            kind,
            frames,
            usize::MAX,
            inline,
            step_limit,
            |frame, command, target| {
                Some(if frame.request_root {
                    (
                        if let Some(terminal) = frame.alias_terminal {
                            self.execute_with_mux_source_routed_for_terminal_in_queue(
                                client,
                                kind,
                                &mut frame.context,
                                command,
                                frame.mux_source,
                                terminal,
                                Some(&frame.execution),
                            )
                        } else {
                            self.execute_with_mux_source_routed_in_queue(
                                client,
                                kind,
                                &mut frame.context,
                                command,
                                frame.mux_source,
                                Some(&frame.execution),
                            )
                        },
                        None,
                        false,
                        None,
                        false,
                    )
                } else {
                    self.execute_inserted_frame_command(client, kind, frame, command, target)
                })
            },
        )
    }

    pub(super) fn advance_inserted_frames<E: FrameExecution>(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        frames: &mut Vec<InsertedQueueFrame<E>>,
        budget: usize,
        inline: bool,
        step_limit: bool,
        mut execute: impl FnMut(
            &mut InsertedQueueFrame<E>,
            &CommandInvocation,
            Option<(ClientId, u8)>,
        ) -> Option<InsertedCommandStep>,
    ) -> Option<(
        InsertedQueueFrame<E>,
        Result<InsertedCommandResult, DaemonError>,
    )> {
        for _ in 0..budget {
            let frame = frames.last_mut().expect("inserted queue frame");
            if frame.wait_boundary.is_some() {
                let wait = {
                    let mut item = self.command_item.as_ref().expect("command item").lock();
                    let wait = item.pending_wait.as_ref().expect("registered wait");
                    if !wait.continuation.ready() {
                        return None;
                    }
                    item.pending_wait.take().unwrap()
                };
                remove_wait_item(
                    &mut self.inner.lock(),
                    &wait.name,
                    wait.continuation.token,
                    false,
                );
                let (boundary, mut step) = frame.wait_boundary.take().unwrap();
                if let Some(terminal) = &wait.terminal {
                    terminal.apply(&mut step.0);
                }
                if let Some(resume) = wait.file.filter(|_| step.0.is_ok()) {
                    step.0 = resume(self, &mut frame.context, &frame.execution);
                    if let Some(next) = self
                        .command_item
                        .as_ref()
                        .unwrap()
                        .lock()
                        .pending_wait
                        .as_mut()
                    {
                        if next.leaf.is_none() {
                            next.leaf = wait.leaf;
                        }
                        if next.guard.is_none() {
                            next.guard = wait.guard;
                        }
                        frame.wait_boundary = Some((boundary, step));
                        return None;
                    }
                }
                #[cfg(unix)]
                if let Some(shell) = wait.shell {
                    step.0 = shell.finish(self, &mut frame.context, Some(&frame.execution));
                }
                if let Some(overlay) = wait.overlay {
                    step.0 = if overlay.accepted.load(Ordering::Acquire) {
                        self.run_confirm_commands(
                            client,
                            kind,
                            &mut frame.context,
                            &overlay.commands,
                            Some(&frame.execution),
                        )
                        .map_err(post_admission_callback_error)
                    } else {
                        Err(DaemonError::CommandExit {
                            output: RawText::default(),
                            exit_code: 1,
                        })
                    };
                }
                if let Some(mut leaf) = wait.leaf {
                    let command = leaf.command.take().expect("wait command");
                    step.0 = self.finish_inserted_leaf(
                        client,
                        kind,
                        &mut frame.context,
                        step.0,
                        Some(&frame.execution),
                        &command,
                        leaf,
                    );
                }
                if let Some(guard) = wait.guard {
                    if let Some(child) = frame.execution.child.borrow_mut().as_mut() {
                        child.guard = Some(guard);
                    } else {
                        step = self.finish_inserted_control_guard(
                            step.0,
                            inserted_frame_mode(&frame.execution, frame.alias_terminal),
                            guard,
                        );
                    }
                }
                if let Some(child) =
                    self.settle_inserted_frame_step(client, kind, frame, boundary, step)
                {
                    frames.push(child);
                }
                if !inline && step_limit {
                    return None;
                }
                continue;
            }
            #[cfg(unix)]
            if inline
                && frame.commands.as_slice().first().is_some_and(|command| {
                    !wait_queue::can_run_inline(self, &frame.context, command)
                })
            {
                return None;
            }
            let stream_client = frame.context.replay_client().unwrap_or(client);
            let cancelled = frame.execution.has_yielded()
                || self.command_queue_cancelled(client)
                || self.command_client_exiting(stream_client)
                || self.shutdown_closed()
                    && !frame.execution.is_draining()
                    && !frame.execution.detached;
            let command = (!cancelled).then(|| frame.commands.next()).flatten();
            if let Some(mut command) = command {
                let group = command
                    .source
                    .as_ref()
                    .map_or(InsertedPhysicalGroup::Unlocated, |source| {
                        InsertedPhysicalGroup::Source(source.source.clone(), source.line)
                    });
                if frame.prepared
                    && let Some(failed) = &frame.failed_group
                {
                    if failed == &group {
                        continue;
                    }
                    frame.failed_group = None;
                }
                if frame.carried_a_stream
                    && command_stdin_sink(canonical_command(&command.name), &command.args)
                        .is_some_and(|sink| sink != CommandStdinSink::ConfigReplay)
                {
                    if let Some(stdin) = frame.stdin.take() {
                        command.set_stdin(stdin);
                    } else {
                        command.set_stdin_spent();
                    }
                }
                let callback_failures_start =
                    frame.execution.callback_parse_failures.borrow().len();
                let command_control_target = frame.control_target.filter(|(client, _)| {
                    frame.hook.is_some()
                        || frame.alias_terminal.is_some()
                        || frame.execution.detached
                        || self.is_capturing_control_command_events(*client)
                        || frame.execution.deferred_shutdown.get() != DeferredShutdown::Force
                });
                let stdout_sequence = self.command_stdout_sequence(stream_client);
                let step = execute(frame, &command, command_control_target);
                let boundary = InsertedCommandBoundary {
                    command,
                    group,
                    callback_failures_start,
                    command_control_target,
                    stdout_sequence,
                };
                let Some(step) = step else {
                    frame.parked_boundary = Some(boundary);
                    return None;
                };
                if self
                    .command_item
                    .as_ref()
                    .is_some_and(|item| item.lock().pending_wait.is_some())
                {
                    frame.wait_boundary = Some((boundary, step));
                    return None;
                }
                if let Some(child) =
                    self.settle_inserted_frame_step(client, kind, frame, boundary, step)
                {
                    frames.push(child);
                }
                if !inline && step_limit {
                    return None;
                }
                continue;
            }
            if !frame.execution.has_yielded()
                && let Some(state) = frame.events.as_mut()
                && let Some(event) = state.events.pop_front()
            {
                if let Some(child) = self.next_event_queue_child(state, event) {
                    frame.execution.insert_foreground_child(child);
                    let boundary = InsertedCommandBoundary {
                        command: CommandInvocation::new("<event-hook>", [] as [&str; 0]),
                        group: InsertedPhysicalGroup::Unlocated,
                        callback_failures_start: frame
                            .execution
                            .callback_parse_failures
                            .borrow()
                            .len(),
                        command_control_target: None,
                        stdout_sequence: 0,
                    };
                    let step = (Ok(Execution::default()), None, false, None, false);
                    if let Some(child) =
                        self.settle_inserted_frame_step(client, kind, frame, boundary, step)
                    {
                        frames.push(child);
                    }
                }
                continue;
            }
            if let Some(hook) = frame.hook.as_mut() {
                frame.execution.frame_active.set(false);
                let yielded = frame.execution.has_yielded();
                self.finish_command_queue_execution(&frame.execution, Some(&hook.parent));
                if !yielded && let Some(commands) = hook.groups.pop_front() {
                    frame.execution =
                        E::owned(self.hook_group_execution(&hook.parent, hook.initial_draining));
                    frame.execution.frame_active.set(true);
                    frame.context = hook.context.clone();
                    frame.commands = commands.into_iter();
                    continue;
                }
                let hook = frame.hook.take().expect("finished hook policy");
                frame.execution = hook.parent;
            }
            let mut finished = frames.pop().expect("finished inserted frame");
            finished.execution.frame_active.set(false);
            let result = self.finish_inserted_queue_frame(client, kind, &mut finished);
            if let Some(parent) = frames.last_mut() {
                let (boundary, child, token) = parent.boundary.take().expect("parent continuation");
                assert!(parent.execution.item.resume(token));
                self.finish_command_queue_execution(&finished.execution, Some(&parent.execution));
                if !matches!(
                    child.kind,
                    InsertedQueueChildKind::Hook(_) | InsertedQueueChildKind::Event { .. }
                ) {
                    parent.context = finished.context.clone();
                }
                let step = self.resume_inserted_child(
                    client,
                    kind,
                    parent,
                    child,
                    result,
                    boundary.callback_failures_start,
                );
                if let Some(child) =
                    self.settle_inserted_frame_step(client, kind, parent, boundary, step)
                {
                    frames.push(child);
                }
            } else {
                return Some((finished, result));
            }
        }
        None
    }

    pub(super) fn settle_inserted_frame_step<E: FrameExecution>(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        frame: &mut InsertedQueueFrame<E>,
        mut boundary: InsertedCommandBoundary,
        mut step: InsertedCommandStep,
    ) -> Option<InsertedQueueFrame<E>> {
        loop {
            let child = frame.execution.child.borrow_mut().take();
            if let Some(child) = child {
                let mut execution =
                    self.inserted_child_execution(Some(&frame.execution), false, None);
                if matches!(
                    child.kind,
                    InsertedQueueChildKind::Hook(_) | InsertedQueueChildKind::Event { .. }
                ) {
                    execution.item.yield_boundary = true;
                }
                let (alias_terminal, stdin) = match &child.kind {
                    InsertedQueueChildKind::Alias {
                        client_terminal,
                        stdin,
                    } => (Some(*client_terminal), stdin.clone()),
                    InsertedQueueChildKind::Foreground(_)
                    | InsertedQueueChildKind::Hook(_)
                    | InsertedQueueChildKind::Event { .. } => (None, None),
                };
                let prepared_child = self.prepare_inserted_queue_frame(
                    child.context.clone(),
                    &child.source,
                    &child.label,
                    child.control_target,
                    child.mux_source,
                    E::owned(execution),
                    alias_terminal,
                    stdin,
                );
                let mut child = child;
                child.leaf_name = step.1;
                let token = frame
                    .execution
                    .item
                    .wait()
                    .expect("parent continuation token");
                frame.boundary = Some((boundary, child, token));
                match prepared_child {
                    Ok(child) => {
                        hook_events::release_input_change_window(self);
                        return Some(child);
                    }
                    Err(error) => {
                        let (saved_boundary, child, token) =
                            frame.boundary.take().expect("child continuation");
                        assert!(frame.execution.item.resume(token));
                        boundary = saved_boundary;
                        step = self.resume_inserted_child(
                            client,
                            kind,
                            frame,
                            child,
                            Err(error),
                            boundary.callback_failures_start,
                        );
                        continue;
                    }
                }
            }
            if let InsertedFrameAction::Finish(error) =
                self.finish_inserted_frame_command(client, kind, frame, boundary, step)
            {
                frame.terminal_error = error;
                frame.commands = Vec::new().into_iter();
            }
            return None;
        }
    }

    pub(super) fn prepare_inserted_queue_frame<E: FrameExecution>(
        self: &Arc<Self>,
        context: ExecutionContext,
        source: &InsertedCommandSource,
        label: &str,
        control_target: Option<(ClientId, u8)>,
        mux_source: MuxOptionSource,
        execution: E,
        alias_terminal: Option<ClientTerminal>,
        stdin: Option<RawText>,
    ) -> Result<InsertedQueueFrame<E>, DaemonError> {
        if let InsertedCommandSource::Events(source) = source {
            execution.frame_active.set(true);
            return Ok(InsertedQueueFrame {
                request_root: false,
                request_result: None,
                wait_boundary: None,
                execution,
                context,
                commands: Vec::new().into_iter(),
                prepared: true,
                control_target: None,
                mux_source,
                alias_terminal: None,
                carried_a_stream: false,
                stdin: None,
                label: label.to_owned(),
                result: InsertedCommandResult::default(),
                stdout_claim: StdoutClaim::None,
                first_error: None,
                failed_group: None,
                boundary: None,
                terminal_error: None,
                parked_boundary: None,
                hook: None,
                events: Some(EventQueueFrame {
                    events: std::mem::take(&mut *source.events.borrow_mut()).into(),
                    publish_control: source.publish_control,
                    shutdown_already_blocked: source.shutdown_already_blocked,
                    notifications: Vec::new(),
                }),
            });
        }
        if let InsertedCommandSource::Hooks(source) = source {
            let mut hook_context = context;
            let control_target = hook_context
                .control_command_target()
                .map(|(client, _)| (client, CONTROL_COMMAND_FRAME_FLAGS_NONE));
            if let Some(target) = control_target {
                hook_context.set_control_command_target(Some(target));
                hook_context.set_replay_client(None);
            } else if hook_context.replay_client().is_some_and(|client| {
                self.read_client(client, |c| c.and_then(|c| c.kind)) == Some(ClientKind::Control)
            }) {
                hook_context.set_replay_client(None);
            }
            hook_context.enter_hook(source.variables.clone());
            let group_execution = self.hook_group_execution(&execution, source.initial_draining);
            let mut groups: VecDeque<_> = std::mem::take(&mut *source.commands.borrow_mut()).into();
            let commands = groups.pop_front().unwrap_or_default().into_iter();
            group_execution.frame_active.set(true);
            return Ok(InsertedQueueFrame {
                request_root: false,
                request_result: None,
                wait_boundary: None,
                execution: E::owned(group_execution),
                context: hook_context.clone(),
                commands,
                prepared: true,
                control_target,
                mux_source,
                alias_terminal: None,
                carried_a_stream: false,
                stdin: None,
                label: label.to_owned(),
                result: InsertedCommandResult::default(),
                stdout_claim: StdoutClaim::None,
                first_error: None,
                failed_group: None,
                boundary: None,
                terminal_error: None,
                parked_boundary: None,
                events: None,
                hook: Some(HookQueueFrame {
                    parent: execution,
                    variables: source.variables.clone(),
                    skip_resolution_errors: source.skip_resolution_errors,
                    replaying: source.replaying,
                    initial_draining: source.initial_draining,
                    groups,
                    context: hook_context,
                }),
            });
        }
        let (commands, prepared) = if let InsertedCommandSource::Commands(commands) = source {
            (commands.clone(), true)
        } else {
            let mut parsed = {
                let inner = self.inner.lock();
                match source {
                    InsertedCommandSource::String(input) => inner.engine.parse_config(label, input),
                    InsertedCommandSource::Block(input) => {
                        inner.engine.parse_config_raw(label, input)
                    }
                    InsertedCommandSource::Commands(_)
                    | InsertedCommandSource::Shell(_)
                    | InsertedCommandSource::Hooks(_)
                    | InsertedCommandSource::Events(_) => unreachable!(),
                }
            };
            let prepared = matches!(source, InsertedCommandSource::Block(_));
            if let Some(diagnostic) = parsed.diagnostics.into_iter().next() {
                return Err(DaemonError::InsertedCommandParse(diagnostic.message));
            }
            if prepared {
                let owner = label
                    .strip_prefix('<')
                    .and_then(|label| label.strip_suffix('>'))
                    .unwrap_or(label);
                self.inner
                    .lock()
                    .engine
                    .prepare_frozen_callback_invocations(&mut parsed.commands, owner)?;
            }
            (parsed.commands, prepared)
        };
        execution.frame_active.set(true);
        Ok(InsertedQueueFrame {
            request_root: false,
            request_result: None,
            wait_boundary: None,
            execution,
            context,
            commands: commands.into_iter(),
            prepared,
            control_target,
            mux_source,
            alias_terminal,
            carried_a_stream: stdin.is_some(),
            stdin,
            label: label.to_owned(),
            result: InsertedCommandResult::default(),
            stdout_claim: StdoutClaim::None,
            first_error: None,
            failed_group: None,
            boundary: None,
            terminal_error: None,
            parked_boundary: None,
            hook: None,
            events: None,
        })
    }

    pub(super) fn execute_inserted_frame_command<E: FrameExecution>(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        frame: &mut InsertedQueueFrame<E>,
        command: &CommandInvocation,
        command_control_target: Option<(ClientId, u8)>,
    ) -> InsertedCommandStep {
        if let Some(hook) = &frame.hook {
            frame.context.no_hooks = true;
            frame.context.format_variables.clone_from(&hook.variables);
        }
        let mode = inserted_frame_mode(&frame.execution, frame.alias_terminal);
        let prepared = frame.prepared;
        let mux_source = frame.mux_source;
        let context = &mut frame.context;
        self.execute_inserted_leaf(
            client,
            kind,
            context,
            command,
            command_control_target,
            prepared,
            mux_source,
            mode,
        )
    }

    pub(super) fn execute_inserted_leaf(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        command_control_target: Option<(ClientId, u8)>,
        prepared: bool,
        mux_source: MuxOptionSource,
        mode: InsertedCommandMode,
    ) -> InsertedCommandStep {
        let (
            execution,
            routed_name,
            alias_group,
            direct_callback_failure,
            direct_command_prepare_error,
        ) = match command_control_target {
            Some(target) => self.execute_control_command_with_guard(
                client, kind, context, command, target, prepared, mux_source, mode,
            ),
            None if prepared => {
                let alias_group = MuxEngine::is_command_alias_group(command);
                let execution = match mode {
                    InsertedCommandMode::Standard(queue_execution) => self
                        .execute_with_mux_source_routed_in_queue(
                            client,
                            kind,
                            context,
                            command,
                            mux_source,
                            Some(queue_execution),
                        ),
                    InsertedCommandMode::CommandAlias {
                        client_terminal,
                        queue_execution,
                        ..
                    } => self.execute_with_mux_source_routed_for_terminal_in_queue(
                        client,
                        kind,
                        context,
                        command,
                        mux_source,
                        client_terminal,
                        Some(queue_execution),
                    ),
                };
                (
                    execution,
                    Some(canonical_command(&command.name).to_owned()),
                    alias_group,
                    None,
                    false,
                )
            }
            None => {
                let routed = prepare_config_command(&self.inner.lock().engine, command)
                    .map(|(command, _)| command);
                let direct_command_prepare_error = routed.is_err();
                let alias_group = routed.as_ref().is_ok_and(MuxEngine::is_command_alias_group);
                let routed_name = routed
                    .as_ref()
                    .ok()
                    .map(|command| canonical_command(&command.name).to_owned());
                let execution = match mode {
                    InsertedCommandMode::Standard(queue_execution) => match routed {
                        Ok(command) => self.execute_with_mux_source_routed_in_queue(
                            client,
                            kind,
                            context,
                            &command,
                            MuxOptionSource::RuntimeCommand,
                            Some(queue_execution),
                        ),
                        Err(error) => Err(error.into()),
                    },
                    InsertedCommandMode::CommandAlias { .. } => unreachable!(),
                };
                (
                    execution,
                    routed_name,
                    alias_group,
                    None,
                    direct_command_prepare_error,
                )
            }
        };
        (
            execution,
            routed_name,
            alias_group,
            direct_callback_failure,
            direct_command_prepare_error,
        )
    }

    fn resume_inserted_child<E: FrameExecution>(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        parent: &mut InsertedQueueFrame<E>,
        child: Box<InsertedQueueChild>,
        result: Result<InsertedCommandResult, DaemonError>,
        callback_failures_start: usize,
    ) -> InsertedCommandStep {
        let alias_group = matches!(child.kind, InsertedQueueChildKind::Alias { .. });
        let mut execution = match child.kind {
            InsertedQueueChildKind::Alias { .. } => {
                result.and_then(|result| inserted_execution(client, kind, result))
            }
            InsertedQueueChildKind::Foreground(route) => {
                parent
                    .execution
                    .nest_callback_parse_failures_since(callback_failures_start);
                match result.map_err(post_admission_callback_error) {
                    Ok(result) => match route {
                        Some(route) => {
                            self.finish_inserted_run_shell(route, "run-shell".to_owned(), &result)
                        }
                        None => inserted_execution(client, kind, result),
                    },
                    Err(error) => Err(error),
                }
            }
            InsertedQueueChildKind::Hook(continuation) => {
                let output = result.map_or_else(
                    |error| {
                        daemon_error_output(&error).map_or_else(String::new, ToString::to_string)
                    },
                    |result| result.output.to_string(),
                );
                if let Some(continuation) = continuation {
                    self.finish_hook_leaf_result(
                        client,
                        Some(&parent.execution),
                        continuation.result,
                        continuation.pane_exit_code,
                        &output,
                    )
                } else {
                    Ok(Execution {
                        output: output.into(),
                        effects: Vec::new(),
                    })
                }
            }
            InsertedQueueChildKind::Event { draining } => {
                if draining && result.as_ref().is_ok_and(|result| result.yielded) {
                    parent.execution.yield_locally();
                }
                Ok(Execution::default())
            }
        };
        if let Some(mut leaf) = child.leaf {
            let command = leaf.command.take().expect("suspended command");
            execution = self.finish_inserted_leaf(
                client,
                kind,
                &mut parent.context,
                execution,
                Some(&parent.execution),
                &command,
                leaf,
            );
        }
        if let Some(next) = parent.execution.child.borrow_mut().as_mut() {
            next.guard = child.guard;
            return (execution, child.leaf_name, alias_group, None, false);
        }
        if let Some(guard) = child.guard {
            let mode = inserted_frame_mode(&parent.execution, parent.alias_terminal);
            self.finish_inserted_control_guard(execution, mode, guard)
        } else {
            (execution, child.leaf_name, alias_group, None, false)
        }
    }

    fn finish_inserted_frame_command<E: FrameExecution>(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        frame: &mut InsertedQueueFrame<E>,
        boundary: InsertedCommandBoundary,
        step: InsertedCommandStep,
    ) -> InsertedFrameAction {
        if frame.request_root {
            frame.request_result = Some(step.0);
            return InsertedFrameAction::Finish(None);
        }
        if frame.events.is_some() {
            return InsertedFrameAction::Continue;
        }
        if frame.hook.is_some() {
            return self.finish_hook_frame_command(client, kind, frame, boundary, step);
        }
        let InsertedCommandBoundary {
            command,
            group,
            callback_failures_start,
            command_control_target,
            stdout_sequence,
        } = boundary;
        let (
            execution,
            routed_name,
            alias_group,
            direct_callback_failure,
            direct_command_prepare_error,
        ) = step;
        let mode = inserted_frame_mode(&frame.execution, frame.alias_terminal);
        let prepared = frame.prepared;
        let label = &frame.label;
        let stream_client = frame.context.replay_client().unwrap_or(client);
        let alias_stdout = frame.alias_terminal.is_some()
            && kind == ClientKind::Command
            && stream_client != ClientId(u64::MAX);
        let result = &mut frame.result;
        let stdout_claim = &mut frame.stdout_claim;
        let first_error = &mut frame.first_error;
        let failed_group = &mut frame.failed_group;
        let routed_name =
            routed_name.unwrap_or_else(|| canonical_command(&command.name).to_owned());
        if let Some(failure) = direct_callback_failure {
            mode.queue_execution()
                .callback_parse_failures
                .borrow_mut()
                .push(failure);
        }
        if command_control_target.is_none()
            && matches!(mode, InsertedCommandMode::Standard(_))
            && !direct_command_prepare_error
            && execution.as_ref().err().is_some_and(command_parse_error)
        {
            mode.queue_execution().note_reported_failure();
            mode.queue_execution()
                .callback_parse_failures
                .borrow_mut()
                .push(CallbackParseFailure::runtime_command(
                    execution
                        .as_ref()
                        .err()
                        .map_or_else(String::new, daemon_error_text),
                    label,
                ));
        }
        let callback_parse_depth = execution
            .as_ref()
            .err()
            .map_or(0, post_admission_callback_parse_depth);
        if callback_parse_depth != 0
            && mode
                .queue_execution()
                .callback_parse_failures
                .borrow()
                .len()
                == callback_failures_start
            && let Some(failure) = CallbackParseFailure::for_command(
                execution
                    .as_ref()
                    .err()
                    .map_or_else(String::new, daemon_error_text),
                &routed_name,
                None,
                callback_parse_depth,
            )
        {
            mode.queue_execution()
                .callback_parse_failures
                .borrow_mut()
                .push(failure);
        }
        let callback_group_action = mode.queue_execution().callback_group_action_since(
            callback_failures_start,
            &routed_name,
            callback_parse_depth,
        );
        let nested_callback_only = callback_parse_depth > 1 && callback_group_action.is_none();
        let preserve_callback_group_action =
            alias_group || matches!(mode, InsertedCommandMode::CommandAlias { .. });
        if let Some((guard_client, flags)) = command_control_target {
            match execution {
                Ok(_) => {}
                Err(DaemonError::CommandExit { exit_code, .. }) => {
                    if flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL {
                        self.inner
                            .lock()
                            .client_entry(guard_client)
                            .command_streams
                            .get_or_insert_default()
                            .exit_code = exit_code;
                    }
                    result.exit_code = exit_code;
                    if matches!(mode, InsertedCommandMode::CommandAlias { .. }) {
                        if routed_name == "source-file" {
                            return InsertedFrameAction::Finish(None);
                        }
                        return InsertedFrameAction::Continue;
                    }
                    if prepared {
                        *failed_group = Some(group);
                        return InsertedFrameAction::Continue;
                    }
                    return InsertedFrameAction::Finish(None);
                }
                Err(error) => {
                    if direct_command_prepare_error {
                        return InsertedFrameAction::Finish(Some(error));
                    }
                    if callback_parse_depth != 0 {
                        if prepared
                            || callback_group_action == Some(CallbackGroupAction::Continue)
                            || nested_callback_only
                        {
                            if first_error.is_none() {
                                *first_error = Some(discard_all_command_output(error));
                            }
                            if prepared && callback_group_action == Some(CallbackGroupAction::Fail)
                            {
                                *failed_group = Some(group);
                            }
                            if callback_group_action.is_some() && !preserve_callback_group_action {
                                mode.queue_execution()
                                    .handle_callback_group_actions_since(callback_failures_start);
                            }
                            return InsertedFrameAction::Continue;
                        }
                        if callback_group_action.is_some() && !preserve_callback_group_action {
                            mode.queue_execution()
                                .handle_callback_group_actions_since(callback_failures_start);
                        }
                        return InsertedFrameAction::Finish(Some(error));
                    }
                    let (guard_error, sticky_failure, _) = control_command_guard_error(&error);
                    if !guard_error {
                        return InsertedFrameAction::Continue;
                    }
                    if sticky_failure && flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL {
                        self.inner
                            .lock()
                            .client_entry(guard_client)
                            .command_streams
                            .get_or_insert_default()
                            .exit_code = 1;
                        result.exit_code = 1;
                    }
                    if prepared {
                        if matches!(mode, InsertedCommandMode::CommandAlias { .. }) {
                            if first_error.is_none() {
                                *first_error = Some(discard_all_command_output(error));
                            }
                            return InsertedFrameAction::Finish(None);
                        }
                        *failed_group = Some(group);
                        return InsertedFrameAction::Continue;
                    }
                    return InsertedFrameAction::Finish(None);
                }
            }
            return InsertedFrameAction::Continue;
        }
        match execution {
            Ok(execution) if !mode.queue_execution().suppress_output.get() => {
                self.append_alias_output(
                    result,
                    &execution.output,
                    &routed_name,
                    stdout_claim,
                    alias_stdout.then_some((stream_client, stdout_sequence)),
                );
            }
            Ok(_) => {}
            Err(DaemonError::CommandExit { output, exit_code }) => {
                if !mode.queue_execution().suppress_output.get() {
                    self.append_alias_output(
                        result,
                        &output,
                        &routed_name,
                        stdout_claim,
                        alias_stdout.then_some((stream_client, stdout_sequence)),
                    );
                }
                result.exit_code = exit_code;
                if matches!(mode, InsertedCommandMode::CommandAlias { .. }) {
                    if routed_name == "source-file" {
                        return InsertedFrameAction::Finish(None);
                    }
                    return InsertedFrameAction::Continue;
                }
                if prepared {
                    *failed_group = Some(group);
                    return InsertedFrameAction::Continue;
                }
                return InsertedFrameAction::Finish(None);
            }
            Err(error) => {
                if callback_parse_depth != 0
                    && (prepared
                        || callback_group_action == Some(CallbackGroupAction::Continue)
                        || nested_callback_only)
                {
                    if first_error.is_none() {
                        *first_error = Some(discard_all_command_output(error));
                    }
                    if prepared && callback_group_action == Some(CallbackGroupAction::Fail) {
                        *failed_group = Some(group);
                    }
                    if callback_group_action.is_some() && !preserve_callback_group_action {
                        mode.queue_execution()
                            .handle_callback_group_actions_since(callback_failures_start);
                    }
                    return InsertedFrameAction::Continue;
                }
                if callback_parse_depth != 0
                    && callback_group_action.is_some()
                    && !preserve_callback_group_action
                {
                    mode.queue_execution()
                        .handle_callback_group_actions_since(callback_failures_start);
                }
                if prepared {
                    if !mode.queue_execution().suppress_output.get()
                        && mode.queue_execution().deferred_shutdown.get() == DeferredShutdown::Force
                        && kind == ClientKind::Command
                        && client != ClientId(u64::MAX)
                    {
                        self.record_command_stderr(client, &daemon_error_text(&error));
                    }
                    if !mode.queue_execution().suppress_output.get()
                        && let Some(error_output) = daemon_error_output(&error)
                    {
                        append_inserted_output(&mut result.output, error_output);
                    }
                    if first_error.is_none() {
                        *first_error = Some(discard_all_command_output(error));
                    }
                    if matches!(mode, InsertedCommandMode::CommandAlias { .. }) {
                        return InsertedFrameAction::Finish(None);
                    }
                    *failed_group = Some(group);
                    return InsertedFrameAction::Continue;
                }
                return InsertedFrameAction::Finish(Some(prepend_command_output(
                    std::mem::take(&mut result.output),
                    error,
                )));
            }
        }
        InsertedFrameAction::Continue
    }

    fn finish_inserted_queue_frame<E: FrameExecution>(
        &self,
        client: ClientId,
        kind: ClientKind,
        frame: &mut InsertedQueueFrame<E>,
    ) -> Result<InsertedCommandResult, DaemonError> {
        if let Some(events) = frame.events.take() {
            self.finish_event_queue_notifications(events.notifications);
        }
        let stream_client = frame.context.replay_client().unwrap_or(client);
        if !frame.request_root
            && frame.alias_terminal.is_some()
            && kind == ClientKind::Command
            && stream_client != ClientId(u64::MAX)
        {
            self.record_command_stdout_claim(stream_client, frame.stdout_claim);
        }
        let mut result = std::mem::take(&mut frame.result);
        result.yielded = frame.execution.has_yielded();
        if frame.execution.deferred_shutdown.get() == DeferredShutdown::Force {
            result.exit_code = 0;
            if client != ClientId(u64::MAX)
                && let Some(streams) = self
                    .inner
                    .lock()
                    .client_mut(client)
                    .and_then(|c| c.command_streams.as_mut())
            {
                streams.exit_code = 0;
            }
            return Ok(result);
        }
        if let Some(error) = frame.terminal_error.take() {
            return Err(error);
        }
        if let Some(error) = frame.first_error.take() {
            return Err(prepend_command_output(result.output, error));
        }
        Ok(result)
    }

    pub(super) fn execute_control_command_with_guard(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        target: (ClientId, u8),
        prepared: bool,
        mux_source: MuxOptionSource,
        mode: InsertedCommandMode,
    ) -> (
        Result<Execution, DaemonError>,
        Option<String>,
        bool,
        Option<CallbackParseFailure>,
        bool,
    ) {
        let aggregate_forced_shutdown = matches!(mode, InsertedCommandMode::Standard(_))
            && !mode.queue_execution().detached
            && !self.is_capturing_control_command_events(target.0);
        let deferred_shutdown_before = mode.queue_execution().deferred_shutdown.get();
        let reported_failure_before = mode.queue_execution().reported_failures.get();
        let routed = if prepared {
            Ok(command.clone())
        } else {
            prepare_config_command(&self.inner.lock().engine, command).map(|(command, _)| command)
        };
        let early_shell_guard = routed.as_ref().is_ok_and(|command| {
            canonical_command(&command.name) == "run-shell"
                && parse_run_shell_args(&command.args).is_ok_and(|args| !args.command_mode)
        });
        if early_shell_guard {
            self.publish_control_command_guard(Some(target), RawText::default(), false, false);
        }
        let capture = self.begin_control_command_event_capture(target.0);
        let direct_command_prepare_error = !prepared && routed.is_err();
        let source_command = routed
            .as_ref()
            .is_ok_and(|command| canonical_command(&command.name) == "source-file");
        let alias_group = routed.as_ref().is_ok_and(MuxEngine::is_command_alias_group);
        let routed_name = routed
            .as_ref()
            .ok()
            .map(|command| canonical_command(&command.name).to_owned());
        let execution = match routed {
            Ok(command) => match mode {
                InsertedCommandMode::Standard(queue_execution) => self
                    .execute_with_mux_source_routed_in_queue(
                        client,
                        kind,
                        context,
                        &command,
                        mux_source,
                        Some(queue_execution),
                    ),
                InsertedCommandMode::CommandAlias {
                    client_terminal,
                    queue_execution,
                    ..
                } => self.execute_with_mux_source_routed_for_terminal_in_queue(
                    client,
                    kind,
                    context,
                    &command,
                    mux_source,
                    client_terminal,
                    Some(queue_execution),
                ),
            },
            Err(error) => Err(error.into()),
        };
        let guard = InsertedControlGuard {
            target,
            aggregate_forced_shutdown,
            deferred_shutdown_before,
            reported_failure_before,
            early_shell_guard,
            direct_command_prepare_error,
            source_command,
            alias_group,
            routed_name,
        };
        if let Some(child) = mode.queue_execution().child.borrow_mut().as_mut() {
            let mut capture = capture;
            capture.active = false;
            child.guard = Some(guard);
            return (
                execution,
                None,
                alias_group,
                None,
                direct_command_prepare_error,
            );
        }
        let mut capture = capture;
        capture.active = false;
        {
            let mut item = self.command_item.as_ref().expect("command item").lock();
            if let Some(wait) = item.pending_wait.as_mut() {
                wait.guard = Some(guard);
                return (
                    execution,
                    None,
                    alias_group,
                    None,
                    direct_command_prepare_error,
                );
            }
        }
        self.finish_inserted_control_guard(execution, mode, guard)
    }

    fn finish_inserted_control_guard(
        self: &Arc<Self>,
        execution: Result<Execution, DaemonError>,
        mode: InsertedCommandMode,
        guard: InsertedControlGuard,
    ) -> (
        Result<Execution, DaemonError>,
        Option<String>,
        bool,
        Option<CallbackParseFailure>,
        bool,
    ) {
        let InsertedControlGuard {
            target,
            aggregate_forced_shutdown,
            deferred_shutdown_before,
            reported_failure_before,
            early_shell_guard,
            direct_command_prepare_error,
            source_command,
            alias_group,
            routed_name,
        } = guard;
        let item_id = self.command_item.as_ref().expect("command item").lock().id;
        let mut captured_events = self
            .pop_control_command_event_capture(target.0, item_id)
            .expect("control command event capture frame");
        let forced_shutdown_transition = aggregate_forced_shutdown
            && deferred_shutdown_before != DeferredShutdown::Force
            && mode.queue_execution().deferred_shutdown.get() == DeferredShutdown::Force;
        let source_guard = source_command
            .then(|| {
                captured_events.events.iter().position(|event| {
                    matches!(
                        event,
                        EventPayload::ControlCommandGuard { .. }
                            | EventPayload::ControlCommandGuardRaw { .. }
                    )
                })
            })
            .flatten();
        if source_command
            && target.1 == CONTROL_COMMAND_FRAME_FLAGS_CONTROL
            && !reported_failure_before
            && mode.queue_execution().reported_failures.get()
            && let Some(index) = source_guard
            && let EventPayload::ControlCommandGuard { sticky_failure, .. }
            | EventPayload::ControlCommandGuardRaw { sticky_failure, .. } =
                &mut captured_events.events[index]
        {
            *sticky_failure = true;
        }
        let source_error = source_guard.and_then(|index| match &captured_events.events[index] {
            EventPayload::ControlCommandGuard {
                output,
                error: true,
                ..
            }
            | EventPayload::ControlCommandGuardRaw {
                output,
                error: true,
                ..
            } => Some(output.to_string()),
            _ => None,
        });
        let callback_parse_depth = execution
            .as_ref()
            .err()
            .map_or(0, post_admission_callback_parse_depth);
        let runtime_command_parse_error = !direct_command_prepare_error
            && execution.as_ref().err().is_some_and(command_parse_error);
        if runtime_command_parse_error {
            mode.queue_execution().note_reported_failure();
            self.inner
                .lock()
                .client_entry(target.0)
                .command_streams
                .get_or_insert_default()
                .exit_code = 1;
        }
        let callback_parse_failure = (callback_parse_depth == 1)
            .then(|| {
                let event = Arc::new(());
                CallbackParseFailure::for_command(
                    execution
                        .as_ref()
                        .err()
                        .map_or_else(String::new, daemon_error_text),
                    routed_name.as_deref()?,
                    Some(event),
                    callback_parse_depth,
                )
            })
            .flatten();
        if forced_shutdown_transition || early_shell_guard {
            self.publish_captured_control_command_events(target.0, captured_events);
        } else if !direct_command_prepare_error && callback_parse_depth > 1 {
            self.publish_control_command_guard_tree(
                Some(target),
                RawText::default(),
                false,
                false,
                captured_events,
            );
        } else if !direct_command_prepare_error
            && callback_parse_failure
                .as_ref()
                .is_some_and(|failure| failure.group_action == CallbackGroupAction::Fail)
        {
            let failure = callback_parse_failure
                .as_ref()
                .expect("callback parse failure exists");
            self.publish_callback_parse_control_command_guard(
                Some(target),
                failure.message.clone().into(),
                true,
                false,
                Arc::clone(
                    failure
                        .control_event
                        .as_ref()
                        .expect("Control callback parse event"),
                ),
            );
            self.publish_captured_control_command_events(target.0, captured_events);
        } else if !direct_command_prepare_error
            && callback_parse_failure
                .as_ref()
                .is_some_and(|failure| failure.group_action == CallbackGroupAction::Continue)
        {
            let failure = callback_parse_failure
                .as_ref()
                .expect("callback parse failure exists");
            self.publish_control_command_guard_tree(
                Some(target),
                RawText::default(),
                false,
                false,
                captured_events,
            );
            self.publish_callback_parse_event(
                target.0,
                EventPayload::ControlCommandOutput {
                    output: failure.message.clone(),
                },
                Arc::clone(
                    failure
                        .control_event
                        .as_ref()
                        .expect("Control callback parse event"),
                ),
            );
        } else if !direct_command_prepare_error
            && (alias_group || source_command && !captured_events.is_empty())
        {
            self.publish_captured_control_command_events(target.0, captured_events);
        } else if !direct_command_prepare_error {
            let (output, error, sticky_failure) =
                control_command_guard_for_execution(&execution, mode);
            self.publish_control_command_guard_tree(
                Some(target),
                output,
                error,
                sticky_failure,
                captured_events,
            );
        }
        let execution = if let Some(source_error) = source_error {
            Err(ServerError::InvalidCommand(source_error).into())
        } else {
            execution
        };
        (
            execution,
            routed_name,
            alias_group,
            callback_parse_failure,
            direct_command_prepare_error,
        )
    }

    pub(super) fn publish_control_command_guard_tree(
        &self,
        target: Option<(ClientId, u8)>,
        output: RawText,
        error: bool,
        sticky_failure: bool,
        captured: CapturedControlCommandEvents,
    ) {
        self.publish_control_command_guard(target, output, error, sticky_failure);
        if let Some((client, _)) = target {
            self.publish_captured_control_command_events(client, captured);
        }
    }

    pub(super) fn publish_control_command_guard(
        &self,
        target: Option<(ClientId, u8)>,
        output: RawText,
        error: bool,
        sticky_failure: bool,
    ) {
        self.publish_control_command_guard_with_callback_parse_event(
            target,
            output,
            error,
            sticky_failure,
            None,
        );
    }

    fn publish_callback_parse_control_command_guard(
        &self,
        target: Option<(ClientId, u8)>,
        output: RawText,
        error: bool,
        sticky_failure: bool,
        callback_parse_event: Arc<()>,
    ) {
        self.publish_control_command_guard_with_callback_parse_event(
            target,
            output,
            error,
            sticky_failure,
            Some(callback_parse_event),
        );
    }

    fn publish_control_command_guard_with_callback_parse_event(
        &self,
        target: Option<(ClientId, u8)>,
        output: RawText,
        error: bool,
        sticky_failure: bool,
        callback_parse_event: Option<Arc<()>>,
    ) {
        if let Some((client, flags)) = target {
            let payload = if self.read_client(client, |c| {
                c.is_some_and(|c| c.ctrl_subscriptions.is_some())
            }) {
                EventPayload::ControlCommandGuardRaw {
                    output,
                    error,
                    sticky_failure,
                    flags,
                }
            } else {
                EventPayload::ControlCommandGuard {
                    output,
                    error,
                    sticky_failure,
                    flags,
                }
            };
            self.publish_to_client_with_callback_parse_event(client, payload, callback_parse_event);
        }
    }
}

fn control_command_guard_for_execution(
    result: &Result<Execution, DaemonError>,
    mode: InsertedCommandMode,
) -> (RawText, bool, bool) {
    match result {
        Ok(execution) => (execution.output.clone(), false, false),
        Err(DaemonError::CommandExit { output, .. })
            if matches!(mode, InsertedCommandMode::CommandAlias { .. }) =>
        {
            (output.clone(), false, false)
        }
        Err(error) => {
            let mut output = daemon_error_output(error).cloned().unwrap_or_default();
            let (error, sticky_failure, message) = control_command_guard_error(error);
            append_inserted_output(&mut output, &message);
            (output, error, sticky_failure)
        }
    }
}

pub(super) fn control_command_guard_error(error: &DaemonError) -> (bool, bool, String) {
    match error {
        DaemonError::CommandFailed { error, .. } => control_command_guard_error(error),
        DaemonError::ReportedCommandExit { .. }
        | DaemonError::Server(
            ServerError::UnsupportedCommand(_) | ServerError::NativeUnsupportedCommand(_),
        ) => (false, false, String::new()),
        DaemonError::InsertedCommandParse(message)
        | DaemonError::Server(
            ServerError::CommandParse(message) | ServerError::NativeCommandParse(message),
        ) => (true, false, message.clone()),
        DaemonError::Server(error) => (true, true, error.tmux_message()),
        error => (true, true, daemon_error_text(error)),
    }
}
