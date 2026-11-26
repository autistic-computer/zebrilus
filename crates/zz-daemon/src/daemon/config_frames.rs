use super::*;

impl Shared {
    pub(super) fn replay_config_file(
        self: &Arc<Self>,
        path: &Path,
        parsed: PreparedConfig,
        context: &mut ExecutionContext,
        depth: usize,
        report: &mut ConfigLoadReport,
        client_terminal: ClientTerminal,
        source_client_base: Option<&Path>,
        source_invocations: &mut SourceInvocationAccounting,
        options: SourceFileLoadOptions,
        deferred_control_config_warnings: &mut Vec<DeferredControlConfigWarning>,
    ) -> Result<(), DaemonError> {
        self.replay_config_file_with_parent_queue(
            path,
            parsed,
            context,
            depth,
            report,
            client_terminal,
            source_client_base,
            source_invocations,
            options,
            deferred_control_config_warnings,
            None,
        )
    }

    pub(super) fn replay_config_file_with_parent_queue(
        self: &Arc<Self>,
        path: &Path,
        parsed: PreparedConfig,
        context: &mut ExecutionContext,
        depth: usize,
        report: &mut ConfigLoadReport,
        client_terminal: ClientTerminal,
        source_client_base: Option<&Path>,
        source_invocations: &mut SourceInvocationAccounting,
        options: SourceFileLoadOptions,
        deferred_control_config_warnings: &mut Vec<DeferredControlConfigWarning>,
        parent_queue: Option<&CommandQueueExecution>,
    ) -> Result<(), DaemonError> {
        let queue_execution = self.config_child_execution(parent_queue);
        let result = self.replay_config_file_in_queue(
            path,
            parsed,
            context,
            depth,
            report,
            client_terminal,
            source_client_base,
            source_invocations,
            options,
            deferred_control_config_warnings,
            &queue_execution,
        );
        let reported_failure = queue_execution.reported_failures.get();
        report.reported_failure |= reported_failure;
        self.finish_command_queue_execution(&queue_execution, parent_queue);
        result
    }

    pub(super) fn config_child_execution(
        &self,
        parent_queue: Option<&CommandQueueExecution>,
    ) -> CommandQueueExecution {
        let detached = parent_queue.is_some_and(|execution| execution.detached);
        let shutdown_blocker =
            parent_queue.and_then(|execution| execution.fork_shutdown_blocker(detached));
        self.command_queue_execution(CommandExecutionState {
            draining: parent_queue.is_some_and(CommandQueueExecution::is_draining),
            wait_yields: true,
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

    fn replay_config_file_in_queue(
        self: &Arc<Self>,
        path: &Path,
        parsed: PreparedConfig,
        context: &mut ExecutionContext,
        depth: usize,
        report: &mut ConfigLoadReport,
        client_terminal: ClientTerminal,
        source_client_base: Option<&Path>,
        source_invocations: &mut SourceInvocationAccounting,
        options: SourceFileLoadOptions,
        deferred_control_config_warnings: &mut Vec<DeferredControlConfigWarning>,
        queue_execution: &CommandQueueExecution,
    ) -> Result<(), DaemonError> {
        self.execution_item().replay_config_file_in_queue_in_item(
            path,
            parsed,
            context,
            depth,
            report,
            client_terminal,
            source_client_base,
            source_invocations,
            options,
            deferred_control_config_warnings,
            queue_execution,
        )
    }

    fn replay_config_file_in_queue_in_item(
        self: &Arc<Self>,
        path: &Path,
        parsed: PreparedConfig,
        context: &mut ExecutionContext,
        depth: usize,
        report: &mut ConfigLoadReport,
        client_terminal: ClientTerminal,
        source_client_base: Option<&Path>,
        source_invocations: &mut SourceInvocationAccounting,
        options: SourceFileLoadOptions,
        deferred_control_config_warnings: &mut Vec<DeferredControlConfigWarning>,
        queue_execution: &CommandQueueExecution,
    ) -> Result<(), DaemonError> {
        let _key_table_hold = timers::KeyTablePublishHold::enter(self);
        let suppressed_control_capture = self.config_frame_capture(options);
        let (commands, finish_report) = self.prepare_config_queue_frame(
            path,
            parsed,
            context,
            report,
            options,
            deferred_control_config_warnings,
        );
        let root = ConfigQueueFrame {
            execution: InsertedFrameExecution::Borrowed(queue_execution),
            path: path.to_owned(),
            context: context.clone(),
            depth,
            options,
            commands,
            finish_report,
            failed_group: None,
            source: None,
            suppressed_control_capture,
        };
        let mut frames = vec![root];
        loop {
            let frame = frames.last_mut().expect("config queue frame");
            if let Some(mut source) = frame.source.take() {
                if let Some((pending, parsed, diagnostics)) = source.children.pop_front() {
                    if let Some((client, _)) = pending.options.control_target {
                        source.warnings.extend(
                            diagnostics
                                .into_iter()
                                .map(|text| DeferredControlConfigWarning { client, text }),
                        );
                    }
                    let execution = self.config_child_execution(Some(&frame.execution));
                    let mut child_context = pending.context;
                    let suppressed_control_capture = self.config_frame_capture(pending.options);
                    let (commands, finish_report) = self.prepare_config_queue_frame(
                        &pending.path,
                        parsed,
                        &mut child_context,
                        report,
                        pending.options,
                        &mut source.warnings,
                    );
                    let child = ConfigQueueFrame {
                        execution: InsertedFrameExecution::Owned(Box::new(execution)),
                        path: pending.path,
                        context: child_context,
                        depth: frame.depth + 1,
                        options: pending.options,
                        commands,
                        finish_report,
                        failed_group: None,
                        source: None,
                        suppressed_control_capture,
                    };
                    frame.source = Some(source);
                    hook_events::release_input_change_window(self);
                    frames.push(child);
                    continue;
                }
                assert!(frame.execution.item.resume(source.token));
                report.pop_stdout_frame();
                self.publish_deferred_control_config_warnings(source.warnings);
                self.publish_control_source_complete(frame.options.control_target);
                if source.source_command_error && !source.source_has_file {
                    frame.failed_group = source.group;
                }
                if let Some(error) = source.source_error {
                    let mut finished = frames.pop().expect("failed config frame");
                    finished.suppressed_control_capture.take();
                    if let Some(parent) = frames.last_mut() {
                        report.reported_failure |= finished.execution.reported_failures.get();
                        self.finish_command_queue_execution(
                            &finished.execution,
                            Some(&parent.execution),
                        );
                        let boundary = parent.source.as_mut().expect("source boundary");
                        self.record_config_child_error(
                            &finished.path,
                            &finished.context,
                            parent.options,
                            source_invocations,
                            report,
                            boundary,
                            error,
                        );
                        continue;
                    }
                    *context = std::mem::take(&mut finished.context);
                    return Err(error);
                }
            }
            let action =
                frame
                    .commands
                    .pop_front()
                    .map_or(Ok(ConfigFrameAction::Finish), |prepared| {
                        self.execute_config_frame_command(
                            &frame.path,
                            &mut frame.context,
                            frame.depth,
                            report,
                            client_terminal,
                            source_client_base,
                            source_invocations,
                            frame.options,
                            &frame.execution,
                            &mut frame.failed_group,
                            prepared,
                        )
                    });
            match action {
                Ok(ConfigFrameAction::Command(_) | ConfigFrameAction::ReadSource(_)) => {
                    unreachable!("synchronous config replay")
                }
                Ok(ConfigFrameAction::Continue) => continue,
                Ok(ConfigFrameAction::Source(source)) => {
                    frame.source = Some(source);
                    continue;
                }
                Ok(ConfigFrameAction::Finish) | Err(_) => {}
            }
            if action.is_ok() && frame.finish_report {
                self.finish_config_queue_frame(&frame.context, frame.options, report);
            }
            let mut finished = frames.pop().expect("finished config frame");
            finished.suppressed_control_capture.take();
            if let Some(parent) = frames.last_mut() {
                report.reported_failure |= finished.execution.reported_failures.get();
                self.finish_command_queue_execution(&finished.execution, Some(&parent.execution));
                if let Err(error) = action {
                    let boundary = parent.source.as_mut().expect("source boundary");
                    self.record_config_child_error(
                        &finished.path,
                        &finished.context,
                        parent.options,
                        source_invocations,
                        report,
                        boundary,
                        error,
                    );
                }
            } else {
                *context = finished.context;
                return action.map(|_| ());
            }
        }
    }

    pub(super) fn config_frame_capture(
        self: &Arc<Self>,
        options: SourceFileLoadOptions,
    ) -> Option<ControlCommandEventCapture> {
        options
            .suppress_replay_output
            .then(|| {
                options
                    .control_target
                    .map(|(client, _)| self.begin_control_command_event_capture(client))
            })
            .flatten()
    }

    pub(super) fn prepare_config_queue_frame(
        self: &Arc<Self>,
        path: &Path,
        parsed: PreparedConfig,
        context: &mut ExecutionContext,
        report: &mut ConfigLoadReport,
        options: SourceFileLoadOptions,
        deferred_control_config_warnings: &mut Vec<DeferredControlConfigWarning>,
    ) -> (VecDeque<PreparedConfigCommand>, bool) {
        context.format_variables.insert(
            CURRENT_FILE_CONTEXT_FORMAT.to_owned(),
            path.display().to_string(),
        );
        if parsed.empty_command_item && !options.parse_only {
            self.publish_control_source_complete(options.control_target);
        }
        let commands = match parsed.construction {
            Ok(_) if options.parse_only => return (VecDeque::new(), false),
            Ok(commands) => commands,
            Err(failure) => {
                if !options.suppress_replay_output
                    && let Some(replay_client) = options.replay_client
                {
                    let replay_kind = self
                        .read_client(replay_client, |client| client.and_then(|c| c.kind))
                        .unwrap_or(if options.control_target.is_some() {
                            ClientKind::Control
                        } else {
                            ClientKind::Command
                        });
                    self.route_config_replay_errors(
                        replay_client,
                        replay_kind,
                        context.pane,
                        report,
                    );
                }
                log::warn!(
                    "{}: ignoring invalid tmux command: {}",
                    path.display(),
                    failure.message
                );
                report.note_invalid_command(&failure.original, &failure.message);
                if !options.suppress_replay_output
                    && let Some((client, _)) = options.control_target
                {
                    deferred_control_config_warnings.push(DeferredControlConfigWarning {
                        client,
                        text: config_command_error(&failure.original, &failure.message),
                    });
                }
                return (VecDeque::new(), false);
            }
        };
        (commands.into(), true)
    }

    pub(super) fn execute_config_frame_command(
        self: &Arc<Self>,
        path: &Path,
        context: &mut ExecutionContext,
        depth: usize,
        report: &mut ConfigLoadReport,
        client_terminal: ClientTerminal,
        source_client_base: Option<&Path>,
        source_invocations: &mut SourceInvocationAccounting,
        options: SourceFileLoadOptions,
        queue_execution: &CommandQueueExecution,
        failed_group: &mut Option<(String, u32)>,
        prepared: PreparedConfigCommand,
    ) -> Result<ConfigFrameAction, DaemonError> {
        if queue_execution.has_yielded()
            || options
                .replay_client
                .is_some_and(|client| self.command_queue_cancelled(client))
            || self.shutdown_closed() && !queue_execution.is_draining() && !queue_execution.detached
        {
            return Ok(ConfigFrameAction::Finish);
        }
        let command = prepared.original;
        if !options.suppress_replay_output
            && report.delivered_replay_issues < report.replay_issues().len()
            && let Some(replay_client) = options.replay_client
        {
            let replay_kind = self
                .read_client(replay_client, |client| client.and_then(|c| c.kind))
                .unwrap_or(if options.control_target.is_some() {
                    ClientKind::Control
                } else {
                    ClientKind::Command
                });
            self.route_config_replay_errors(replay_client, replay_kind, context.pane, report);
        }
        if failed_group
            .as_ref()
            .is_some_and(|(source, line)| in_source_group(&command, source, *line))
        {
            return Ok(ConfigFrameAction::Continue);
        }
        let routed = prepared.routed;
        let routed_name = canonical_command(&routed.name);
        let show_buffer_waits = routed_name == "show-buffer"
            && client_terminal == ClientTerminal::Absent
            && options.replay_client.is_some_and(|client| {
                self.read_client(client, |c| c.and_then(|c| c.kind)) == Some(ClientKind::Command)
            });
        if queue_execution.draining
            && queue_execution.wait_yields
            && (matches!(routed_name, "load-buffer" | "save-buffer") || show_buffer_waits)
        {
            queue_execution.yield_queue();
            return Ok(ConfigFrameAction::Finish);
        }
        if routed.name == "reload-config" {
            log::warn!(
                "{}: ignoring reload-config while loading configuration",
                path.display()
            );
            self.publish_control_command_guard(
                options.control_target,
                RawText::default(),
                false,
                false,
            );
            return Ok(ConfigFrameAction::Continue);
        }
        let caller_source_stream = routed_name == "source-file"
            && source_file_reads_stdin(&routed.args)
            && (options.control_target.is_some()
                || options.replay_client.is_some_and(|client| {
                    self.read_client(client, |c| {
                        c.and_then(|c| c.command_streams.as_ref())
                            .is_some_and(|streams| {
                                streams.stdin.is_some() || streams.stdin_available
                            })
                    })
                }));
        if routed_name == "source-file" && !caller_source_stream {
            let source_effects = {
                let mut inner = self.inner.lock();
                inner.engine.execute_prepared(context, &routed)
            };
            let source_effects = match source_effects {
                Ok(execution) => execution
                    .effects
                    .into_iter()
                    .filter_map(|effect| match effect {
                        MuxEffect::SourceFile {
                            path,
                            quiet,
                            parse_only,
                            verbose,
                            context,
                            stdin,
                        } => Some(SourceFileRequest {
                            path,
                            quiet,
                            parse_only,
                            verbose,
                            context,
                            stdin,
                        }),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                Err(
                    ServerError::UnsupportedCommand(unsupported)
                    | ServerError::NativeUnsupportedCommand(unsupported),
                ) => {
                    log::warn!(
                        "{}: ignoring unsupported tmux command: {unsupported}",
                        path.display()
                    );
                    report.note_skip_command(&command, &unsupported);
                    self.publish_control_command_guard(
                        options.control_target,
                        RawText::default(),
                        false,
                        false,
                    );
                    return Ok(ConfigFrameAction::Continue);
                }
                Err(
                    ServerError::CommandParse(message) | ServerError::NativeCommandParse(message),
                ) => {
                    log::warn!(
                        "{}: ignoring invalid tmux command: {message}",
                        path.display()
                    );
                    report.note_unlocated_command_error(&message);
                    self.publish_control_command_guard(
                        options.control_target,
                        message.into(),
                        true,
                        false,
                    );
                    *failed_group = source_group(&command);
                    return Ok(ConfigFrameAction::Continue);
                }
                Err(error) => {
                    let message = error.tmux_message();
                    log::warn!("{}: tmux command error: {message}", path.display());
                    report.note_command_error(&command, &message);
                    self.publish_control_command_guard(
                        options.control_target,
                        message.into(),
                        true,
                        true,
                    );
                    *failed_group = source_group(&command);
                    return Ok(ConfigFrameAction::Continue);
                }
            };
            let startup = source_invocations.is_startup();
            if source_invocations.refuses(depth) {
                let error = if startup {
                    config_command_error(&command, NESTED_SOURCE_LIMIT_ERROR)
                } else {
                    NESTED_SOURCE_LIMIT_ERROR.to_owned()
                };
                log::warn!("{error}");
                report.note_source_error(&mut None, &error);
                self.publish_control_command_guard(
                    options.control_target,
                    error.into(),
                    true,
                    false,
                );
                *failed_group = source_group(&command);
                return Ok(ConfigFrameAction::Continue);
            }
            if source_effects.is_empty() {
                log::warn!("{}: ignoring source-file without a path", path.display());
            }
            let mut source_error = None;
            let mut source_error_group = None;
            let mut source_has_file = false;
            let mut source_command_error = false;
            let mut source_parse_error = false;
            let mut source_diagnostics = RawText::default();
            let mut prepared_sources = Vec::new();
            for source_request in source_effects {
                if source_request.path == "-" {
                    source_has_file = true;
                    log::warn!("{STANDARD_INPUT_SOURCE_WARNING}");
                    report.note_invalid_command(&command, STANDARD_INPUT_SOURCE_WARNING);
                    append_inserted_output(
                        &mut source_diagnostics,
                        config_command_error(&command, STANDARD_INPUT_SOURCE_WARNING),
                    );
                    source_parse_error = true;
                    continue;
                }
                let declared_path = PathBuf::from(source_request.path.to_os_string());
                let pattern = source_client_base.map_or_else(
                    || expand_relative(path, &source_request.path),
                    |base| resolve_source_path(&source_request.path, Some(base)),
                );
                let matches = source_glob_matches(&pattern);
                for error in &matches.errors {
                    let warning = source_glob_error_warning(&declared_path, error);
                    log::warn!("{warning}");
                    report.note_located_source_error(&command, &mut source_error_group, &warning);
                    append_inserted_output(&mut source_diagnostics, &warning);
                    source_command_error = true;
                }
                if matches.paths.is_empty() && matches.errors.is_empty() {
                    if !source_request.quiet {
                        let error = missing_source_error(&declared_path);
                        log::warn!("{error}");
                        report.note_located_source_error(&command, &mut source_error_group, &error);
                        append_inserted_output(&mut source_diagnostics, &error);
                        source_command_error = true;
                    }
                    continue;
                }
                source_has_file |= !matches.paths.is_empty();
                prepared_sources.push((source_request, matches.paths));
            }
            self.publish_control_command_guard(
                options.control_target,
                source_diagnostics,
                source_parse_error || source_command_error && !source_has_file,
                false,
            );
            if queue_execution.wait_yields && queue_execution.is_draining() {
                queue_execution.yield_queue();
                self.publish_control_source_complete(options.control_target);
                return Ok(ConfigFrameAction::Finish);
            }
            let mut pending_sources = Vec::new();
            for (source_request, sources) in prepared_sources {
                let nested_options = SourceFileLoadOptions {
                    parse_only: source_request.parse_only,
                    verbose: !options.suppress_verbose
                        && (options.verbose || source_request.verbose),
                    suppress_verbose: options.suppress_verbose,
                    control_target: options.control_target,
                    replay_client: options.replay_client,
                    suppress_replay_output: options.suppress_replay_output,
                    command_client: options.command_client,
                };
                for source in sources {
                    pending_sources.push(PendingConfigFile {
                        path: source,
                        context: source_request.context.clone(),
                        options: nested_options,
                        stdin: None,
                    });
                }
            }
            if queue_execution.frame_active.get() {
                report.push_stdout_frame();
                return Ok(ConfigFrameAction::ReadSource(Box::new(
                    source_queue::SourceRead {
                        pending: pending_sources.into(),
                        parsed: VecDeque::new(),
                        group: source_group(&command),
                        command,
                        source_error_group,
                        source_error,
                        source_command_error,
                        source_has_file,
                    },
                )));
            }
            let mut parsed_sources = Vec::new();
            report.push_stdout_frame();
            for pending in pending_sources {
                let diagnostics_start = report.diagnostics().len();
                match self.parse_config_file(&pending.path, report, pending.options, true) {
                    Ok(Some(parsed)) => {
                        let diagnostics = report.diagnostics()[diagnostics_start..].to_vec();
                        parsed_sources.push((pending, parsed, diagnostics));
                    }
                    Err(DaemonError::Io(error)) => {
                        let warning = if options.control_target.is_some()
                            || source_invocations.is_startup()
                        {
                            source_read_error_warning(&pending.path, &error)
                        } else {
                            client_source_read_error_warning(
                                options.command_client,
                                &pending.path,
                                &error,
                            )
                        };
                        log::warn!("{warning}");
                        report.note_located_source_error(
                            &command,
                            &mut source_error_group,
                            &warning,
                        );
                        if let Some(target) = options.control_target {
                            self.publish_control_source_read_error(
                                target,
                                pending.context.pane,
                                error.kind(),
                                warning,
                            );
                        }
                    }
                    Err(error) if source_error.is_none() => source_error = Some(error),
                    Ok(None) => report
                        .note_startup_command_cause(&command, &missing_source_error(&pending.path)),
                    Err(_) => {}
                }
            }
            let children = parsed_sources.into();
            return Ok(ConfigFrameAction::Source(Box::new(ConfigSourceBoundary {
                token: queue_execution
                    .item
                    .wait()
                    .expect("source continuation token"),
                children,
                group: source_group(&command),
                command,
                source_error_group,
                source_error,
                source_command_error,
                source_has_file,
                warnings: Vec::new(),
            })));
        }
        if options.control_target.is_some() && !MuxEngine::is_command_alias_group(&routed) {
            let name_error = match resolve_command(&routed.name) {
                CommandResolution::Ambiguous(message) => Some(message),
                CommandResolution::Unknown => Some(format!("unknown command: {}", routed.name)),
                CommandResolution::Canonical(_) | CommandResolution::Unimplemented(_) => None,
            };
            if let Some(name_error) = name_error {
                log::warn!(
                    "{}: ignoring invalid tmux command: {name_error}",
                    path.display()
                );
                report.note_invalid_command(&command, &name_error);
                self.publish_sourced_config_warning(
                    options.control_target.map(|(client, _)| client),
                    &command,
                    &name_error,
                );
                *failed_group = source_group(&command);
                return Ok(ConfigFrameAction::Continue);
            }
        }
        let previous_replay_client = context.replay_client();
        let alias_group = MuxEngine::is_command_alias_group(&routed);
        let execution_replay_client = if options.control_target.is_some()
            && !matches!(routed_name, "if-shell" | "run-shell")
        {
            None
        } else {
            options.replay_client
        };
        context.set_replay_client(execution_replay_client);
        let previous_control_target = context.control_command_target();
        context.set_control_command_target(options.control_target);
        let early_shell_guard = routed_name == "run-shell"
            && parse_run_shell_args(&routed.args).is_ok_and(|args| !args.command_mode);
        if early_shell_guard {
            self.publish_control_command_guard(
                options.control_target,
                RawText::default(),
                false,
                false,
            );
        }
        let guard_capture = options
            .control_target
            .map(|(client, _)| client)
            .map(|client| self.begin_control_command_event_capture(client));
        let callback_parse_failures_start = queue_execution.callback_parse_failures.borrow().len();
        let deferred_replay_issues_start =
            queue_execution.deferred_config_replay_issues.borrow().len();
        let stdout_sequence = options
            .replay_client
            .map(|client| self.command_stdout_sequence(client));
        let finish = source_queue::CommandFinish {
            command,
            routed,
            previous_replay_client,
            previous_control_target,
            early_shell_guard,
            guard_capture,
            callback_parse_failures_start,
            deferred_replay_issues_start,
            stdout_sequence,
            alias_group,
            caller_source_stream,
        };
        let result = self.execute_with_mux_source_routed_for_terminal_in_queue(
            ClientId(u64::MAX),
            ClientKind::Command,
            context,
            &finish.routed,
            MuxOptionSource::TmuxConfig,
            client_terminal,
            Some(queue_execution),
        );
        if queue_execution.frame_active.get()
            && (queue_execution.child.borrow().is_some()
                || self
                    .command_item
                    .as_ref()
                    .unwrap()
                    .lock()
                    .pending_wait
                    .is_some())
        {
            return Ok(ConfigFrameAction::Command(Box::new((finish, result))));
        }
        self.finish_config_frame_command(
            path,
            context,
            report,
            options,
            queue_execution,
            failed_group,
            finish,
            result,
        )
    }

    pub(super) fn finish_config_frame_command(
        self: &Arc<Self>,
        path: &Path,
        context: &mut ExecutionContext,
        report: &mut ConfigLoadReport,
        options: SourceFileLoadOptions,
        queue_execution: &CommandQueueExecution,
        failed_group: &mut Option<(String, u32)>,
        finish: source_queue::CommandFinish,
        result: Result<Execution, DaemonError>,
    ) -> Result<ConfigFrameAction, DaemonError> {
        let source_queue::CommandFinish {
            command,
            routed,
            previous_replay_client,
            previous_control_target,
            early_shell_guard,
            guard_capture,
            callback_parse_failures_start,
            deferred_replay_issues_start,
            stdout_sequence,
            alias_group,
            caller_source_stream,
        } = finish;
        let routed_name = canonical_command(&routed.name);
        let mut callback_parse_failures = queue_execution
            .callback_parse_failures
            .borrow_mut()
            .split_off(callback_parse_failures_start);
        let deferred_replay_issues = queue_execution
            .deferred_config_replay_issues
            .borrow_mut()
            .split_off(deferred_replay_issues_start);
        let callback_parse_depth = result
            .as_ref()
            .err()
            .map_or(0, post_admission_callback_parse_depth);
        let had_source_callback_failure = callback_parse_failures
            .iter()
            .any(|failure| failure.owner.is_replay_callback());
        let direct_callback = callback_parse_depth == 1 && !had_source_callback_failure;
        if direct_callback {
            let failure = CallbackParseFailure::for_command(
                result
                    .as_ref()
                    .err()
                    .map_or_else(String::new, daemon_error_text),
                routed_name,
                None,
                callback_parse_depth,
            )
            .unwrap_or(CallbackParseFailure {
                message: result
                    .as_ref()
                    .err()
                    .map_or_else(String::new, daemon_error_text),
                group_action: CallbackGroupAction::Handled,
                owner: CallbackFailureOwner::Source,
                control_event: None,
                depth: callback_parse_depth,
            });
            let index = callback_parse_failures
                .iter()
                .position(|failure| failure.owner == CallbackFailureOwner::CommandErrorHook)
                .unwrap_or(callback_parse_failures.len());
            callback_parse_failures.insert(index, failure);
        }
        let mut callback_failures_before_result = Vec::new();
        let mut callback_failures_after_result = Vec::new();
        for failure in callback_parse_failures {
            if callback_parse_depth == 0 && failure.owner == CallbackFailureOwner::CommandErrorHook
            {
                callback_failures_after_result.push(failure);
            } else {
                callback_failures_before_result.push(failure);
            }
        }
        let source_callback_failures =
            report.note_callback_failures(path, &command, callback_failures_before_result);
        context.set_replay_client(previous_replay_client);
        context.set_control_command_target(previous_control_target);
        let mut captured_events = guard_capture.map_or_else(
            CapturedControlCommandEvents::default,
            ControlCommandEventCapture::finish,
        );
        let mut captured_output = result.as_ref().map_or_else(
            |error| daemon_error_output(error).cloned().unwrap_or_default(),
            |execution| execution.output.clone(),
        );
        if routed_name == "display-message"
            && let Ok(execution) = &result
        {
            report.note_startup_display(&command, &execution.output);
        }
        let raw_stdout =
            matches!(routed_name, "save-buffer" | "show-buffer")
                || alias_group
                    && options.replay_client.zip(stdout_sequence).is_some_and(
                        |(client, sequence)| self.command_raw_stdout_since(client, sequence),
                    );
        if report.note_stdout(&captured_output, raw_stdout) == ReplayStdoutWrite::Denied
            && let Some(replay_client) = options.replay_client
        {
            self.record_command_stderr(replay_client, "Bad file descriptor: -");
            self.record_command_failure(replay_client);
        }
        let publish_guard =
            |output: RawText, error: bool, sticky_failure: bool, captured_events| {
                if alias_group
                    || early_shell_guard
                    || caller_source_stream && routed_name == "source-file"
                {
                    if let Some((client, _)) = options.control_target {
                        self.publish_captured_control_command_events(client, captured_events);
                    }
                } else {
                    self.publish_control_command_guard_tree(
                        options.control_target,
                        output,
                        error,
                        sticky_failure,
                        captured_events,
                    );
                }
            };
        if options.control_target.is_some()
            && !direct_callback
            && !source_callback_failures.is_empty()
        {
            let qualified =
                captured_events.qualify_callback_parse_events(&source_callback_failures);
            debug_assert_eq!(
                qualified,
                source_callback_failures
                    .iter()
                    .filter(|failure| {
                        failure.owner == CallbackFailureOwner::Source
                            && failure.control_event.is_some()
                    })
                    .count()
            );
        }
        match result.map_err(discard_command_output) {
            Ok(_) => publish_guard(captured_output, false, false, captured_events),
            Err(DaemonError::Server(
                ServerError::UnsupportedCommand(unsupported)
                | ServerError::NativeUnsupportedCommand(unsupported),
            )) => {
                log::warn!(
                    "{}: ignoring unsupported tmux command: {unsupported}",
                    path.display()
                );
                report.note_skip_command(&command, &unsupported);
                publish_guard(captured_output, false, false, captured_events);
            }
            Err(DaemonError::InsertedCommandParse(message)) => {
                log::warn!(
                    "{}: invalid inserted tmux command: {message}",
                    path.display()
                );
                let _ = report.note_callback_failures(
                    path,
                    &command,
                    std::mem::take(&mut callback_failures_after_result),
                );
                report.note_deferred_config_replay_issues(path, &command, deferred_replay_issues);
                return Err(DaemonError::InsertedCommandParse(config_command_error(
                    &command, &message,
                )));
            }
            Err(error) if post_admission_callback_parse_depth(&error) != 0 => {
                let failed_source_group = source_callback_failures.iter().any(|failure| {
                    failure.depth == 1 && failure.group_action == CallbackGroupAction::Fail
                });
                let diagnostics = source_callback_failures;
                if let Some((client, _)) = options.control_target {
                    self.inner
                        .lock()
                        .client_entry(client)
                        .command_streams
                        .get_or_insert_default()
                        .exit_code = 1;
                }
                if !direct_callback {
                    publish_guard(captured_output, false, false, captured_events);
                } else if routed_name == "if-shell" {
                    for diagnostic in &diagnostics {
                        append_inserted_output(&mut captured_output, &diagnostic.message);
                    }
                    publish_guard(captured_output, true, false, captured_events);
                } else {
                    publish_guard(captured_output, false, false, captured_events);
                    if let Some((client, _)) = options.control_target {
                        for diagnostic in diagnostics {
                            self.publish_to_client(
                                client,
                                EventPayload::ControlCommandOutput {
                                    output: diagnostic.message,
                                },
                            );
                        }
                    }
                }
                if failed_source_group {
                    *failed_group = source_group(&command);
                }
            }
            Err(DaemonError::Server(
                ServerError::CommandParse(message) | ServerError::NativeCommandParse(message),
            )) => {
                log::warn!(
                    "{}: ignoring invalid tmux command: {message}",
                    path.display()
                );
                report.note_unlocated_command_error(&message);
                append_inserted_output(&mut captured_output, &message);
                publish_guard(captured_output, true, false, captured_events);
                *failed_group = source_group(&command);
            }
            Err(DaemonError::Server(error)) => {
                let message = error.tmux_message();
                log::warn!("{}: tmux command error: {message}", path.display());
                report.note_command_error(&command, &message);
                append_inserted_output(&mut captured_output, &message);
                publish_guard(captured_output, true, true, captured_events);
                *failed_group = source_group(&command);
            }
            Err(error) => {
                report.note_startup_command_cause(&command, &daemon_error_text(&error));
                append_inserted_output(&mut captured_output, daemon_error_text(&error));
                publish_guard(captured_output, true, true, captured_events);
                let _ = report.note_callback_failures(
                    path,
                    &command,
                    std::mem::take(&mut callback_failures_after_result),
                );
                report.note_deferred_config_replay_issues(path, &command, deferred_replay_issues);
                return Err(error);
            }
        }
        let _ = report.note_callback_failures(
            path,
            &command,
            std::mem::take(&mut callback_failures_after_result),
        );
        report.note_deferred_config_replay_issues(path, &command, deferred_replay_issues);
        Ok(ConfigFrameAction::Continue)
    }

    pub(super) fn record_config_child_error(
        &self,
        path: &Path,
        context: &ExecutionContext,
        options: SourceFileLoadOptions,
        source_invocations: &SourceInvocationAccounting,
        report: &mut ConfigLoadReport,
        boundary: &mut ConfigSourceBoundary,
        error: DaemonError,
    ) {
        if let DaemonError::Io(error) = error {
            let warning = if options.control_target.is_some() || source_invocations.is_startup() {
                source_read_error_warning(path, &error)
            } else {
                client_source_read_error_warning(options.command_client, path, &error)
            };
            log::warn!("{warning}");
            report.note_located_source_error(
                &boundary.command,
                &mut boundary.source_error_group,
                &warning,
            );
            if let Some(target) = options.control_target {
                self.publish_control_source_read_error(target, context.pane, error.kind(), warning);
            }
        } else if boundary.source_error.is_none() {
            boundary.source_error = Some(error);
        }
    }

    pub(super) fn finish_config_queue_frame(
        &self,
        context: &ExecutionContext,
        options: SourceFileLoadOptions,
        report: &mut ConfigLoadReport,
    ) {
        let defer_command_error_hook_replay_issues = options.control_target.is_none()
            && context
                .format_variables
                .get(HOOK_CONTEXT_FORMAT)
                .is_some_and(|hook| hook == "command-error");
        if !options.suppress_replay_output
            && !defer_command_error_hook_replay_issues
            && let Some(replay_client) = options.replay_client
        {
            let replay_kind = self
                .read_client(replay_client, |client| client.and_then(|c| c.kind))
                .unwrap_or(if options.control_target.is_some() {
                    ClientKind::Control
                } else {
                    ClientKind::Command
                });
            self.route_config_replay_errors(replay_client, replay_kind, context.pane, report);
        }
    }
}

const CURRENT_FILE_CONTEXT_FORMAT: &str = "current_file";

fn source_group(command: &CommandInvocation) -> Option<(String, u32)> {
    command
        .source
        .as_ref()
        .map(|source| (source.source.clone(), source.line))
}

fn in_source_group(command: &CommandInvocation, source: &str, line: u32) -> bool {
    command
        .source
        .as_ref()
        .is_some_and(|span| span.line == line && span.source == source)
}

struct ConfigQueueFrame<'a> {
    execution: InsertedFrameExecution<'a>,
    path: PathBuf,
    context: ExecutionContext,
    depth: usize,
    options: SourceFileLoadOptions,
    commands: VecDeque<PreparedConfigCommand>,
    finish_report: bool,
    failed_group: Option<(String, u32)>,
    source: Option<Box<ConfigSourceBoundary>>,
    suppressed_control_capture: Option<ControlCommandEventCapture>,
}

fn discard_command_output(error: DaemonError) -> DaemonError {
    match error {
        DaemonError::CommandFailed { error, .. } => discard_command_output(*error),
        error => error,
    }
}

pub(super) const NESTED_SOURCE_LIMIT_ERROR: &str = "too many nested files";

pub(super) fn expand_relative(source: &Path, nested: &RawText) -> PathBuf {
    let nested = PathBuf::from(nested.to_os_string());
    if nested.is_absolute() {
        nested
    } else {
        source_working_directory_glob_prefix(source.parent().unwrap_or_else(|| Path::new(".")))
            .join(nested)
    }
}
