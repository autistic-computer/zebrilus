use super::*;

impl Shared {
    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "each flag is one effect accumulator"
    )]
    pub(super) fn run_source_effects(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        queue_execution: Option<&CommandQueueExecution>,
        invoking_client_terminal: ClientTerminal,
        mut execution: Execution,
        replay_client: Option<ClientId>,
        source_client: ClientId,
        cwd_client: ClientId,
        read_only: bool,
        notifications_only: bool,
        client_timers_changed: bool,
        source_files: Vec<SourceFileRequest>,
        reload_config: bool,
        status_formats_changed: bool,
        status_refresh_sessions: BTreeSet<SessionId>,
        mut pending_hook_events: Vec<PendingHookEvent>,
        incremental_start: Option<CommandPromptSubmission>,
        terminal_wait: Option<terminal_requests::CommandWait>,
    ) -> Result<Execution, DaemonError> {
        let (
            source_kind,
            startup_source_client_working_directory,
            source_session_working_directory,
            source_client_working_directory,
            source_client_registered,
            source_client_terminal,
        ) = {
            let inner = self.inner.lock();
            let registered = inner.client(cwd_client).is_some_and(|c| c.kind.is_some());
            let source_kind = if source_client == client {
                kind
            } else {
                inner
                    .client(source_client)
                    .and_then(|c| c.kind)
                    .unwrap_or(kind)
            };
            let source_session_working_directory = (reload_config || !source_files.is_empty())
                .then(|| {
                    client_attached_session(&inner, cwd_client)
                        .and_then(|session| inner.engine.state.session_working_directory(session))
                        .map(Path::to_owned)
                })
                .flatten();
            let source_client_terminal = if context.has_no_client() {
                ClientTerminal::NoClient
            } else if replay_client.is_some() && registered {
                client_terminal(&inner, source_client, source_kind)
            } else {
                invoking_client_terminal
            };
            (
                source_kind,
                inner.config.startup_source_client_working_directory.clone(),
                source_session_working_directory,
                inner
                    .client(cwd_client)
                    .and_then(|c| c.working_directory.as_ref())
                    .cloned(),
                registered,
                source_client_terminal,
            )
        };
        let source_working_directory = (reload_config || !source_files.is_empty()).then(|| {
            startup_source_client_working_directory
                .clone()
                .or(source_session_working_directory)
                .or(source_client_working_directory)
                .or_else(|| context.client_working_directory().map(Path::to_owned))
                .or_else(home_directory)
                .unwrap_or_else(|| PathBuf::from("/"))
        });
        let source_client_base = startup_source_client_working_directory.or_else(|| {
            if cwd_client != ClientId(u64::MAX) && source_client_registered {
                source_working_directory.clone()
            } else {
                None
            }
        });
        let control_target = context.control_command_target().or_else(|| {
            (source_kind == ClientKind::Control)
                .then_some((source_client, CONTROL_COMMAND_FRAME_FLAGS_CONTROL))
        });
        let control_client = control_target.map(|(client, _)| client);
        let mut source_file_error = None;
        let mut source_path_error = false;
        let mut source_path_matched = false;
        let mut control_source_errors = Vec::new();
        let mut control_source_read_errors = Vec::new();
        let mut control_source_matched = false;
        let mut source_verbose_output = RawText::default();
        let mut source_replay_output = RawText::default();
        let mut source_diagnostics_output = RawText::default();
        let mut source_invocations = SourceInvocationAccounting::default();
        let mut pending_source_files = Vec::new();
        let source_invocation = !source_files.is_empty();
        let mut reported_source_failure = false;
        let mut reported_source_callback_failure = false;
        let control_source_invocation = control_target.is_some() && source_invocation;
        let suppress_source_replay_output = source_kind == ClientKind::Command
            && queue_execution.is_some_and(CommandQueueExecution::is_draining);
        let captured_control_source = control_client.is_some_and(|client| {
            !source_files.is_empty() && self.is_capturing_control_command_events(client)
        });
        for request in source_files {
            let path = request.path;
            if path == "-" {
                if let Some(SourceStream::Bytes(stream)) = request.stdin {
                    source_path_matched = true;
                    control_source_matched |= control_target.is_some();
                    pending_source_files.push(PendingConfigFile {
                        path: PathBuf::from("-"),
                        context: request.context.clone(),
                        stdin: Some(stream),
                        options: SourceFileLoadOptions {
                            parse_only: request.parse_only,
                            verbose: request.verbose && control_target.is_none(),
                            suppress_verbose: control_target.is_some(),
                            control_target,
                            replay_client: (source_client != ClientId(u64::MAX))
                                .then_some(source_client),
                            suppress_replay_output: suppress_source_replay_output,
                            command_client: source_kind == ClientKind::Command,
                        },
                    });
                    continue;
                }
                let read_failure = request.stdin.is_some() || control_target.is_some();
                let text = if read_failure {
                    self.take_caller_stdin_error(source_client)
                } else {
                    STANDARD_INPUT_SOURCE_WARNING.to_owned()
                };
                if read_failure {
                    reported_source_failure = true;
                } else {
                    source_path_error = true;
                }
                if read_failure && control_target.is_some() {
                    control_source_read_errors.push(text);
                    self.record_command_failure(control_client.unwrap_or(source_client));
                    continue;
                }
                if captured_control_source {
                    control_source_errors.push(text.clone());
                } else {
                    self.publish_to_client(
                        control_client.unwrap_or(source_client),
                        EventPayload::ClientMessage {
                            pane: request.context.pane,
                            kind: ClientMessageKind::Warning,
                            text: text.clone(),
                        },
                    );
                }
                if control_target.is_none() && source_kind == ClientKind::Command {
                    self.record_command_stderr(source_client, &text);
                    self.record_command_failure(source_client);
                }
                continue;
            }
            let declared_path = PathBuf::from(path.to_os_string());
            let pattern = resolve_source_path(&path, source_working_directory.as_deref());
            let matches = source_glob_matches(&pattern);
            source_path_error |= !matches.errors.is_empty();
            for error in &matches.errors {
                let text = source_glob_error_warning(&declared_path, error);
                if control_target.is_some() {
                    control_source_errors.push(text);
                } else {
                    self.route_source_error(
                        source_client,
                        source_kind,
                        request.context.pane,
                        &text,
                    );
                }
            }
            if matches.paths.is_empty() && matches.errors.is_empty() {
                if !request.quiet {
                    source_path_error = true;
                    let error = missing_source_error(&declared_path);
                    if control_target.is_some() {
                        control_source_errors.push(error);
                    } else {
                        self.route_source_error(
                            source_client,
                            source_kind,
                            request.context.pane,
                            &error,
                        );
                    }
                }
                continue;
            }
            source_path_matched |= !matches.paths.is_empty();
            control_source_matched |= control_target.is_some() && !matches.paths.is_empty();
            for path in matches.paths {
                pending_source_files.push(PendingConfigFile {
                    path,
                    context: request.context.clone(),
                    stdin: None,
                    options: SourceFileLoadOptions {
                        parse_only: request.parse_only,
                        verbose: request.verbose && control_target.is_none(),
                        suppress_verbose: control_target.is_some(),
                        control_target,
                        replay_client: (source_client != ClientId(u64::MAX))
                            .then_some(source_client),
                        suppress_replay_output: suppress_source_replay_output,
                        command_client: source_kind == ClientKind::Command,
                    },
                });
            }
        }
        if captured_control_source {
            let source_command_error = !control_source_errors.is_empty();
            let frame_flags = control_target
                .map(|(_, flags)| flags)
                .expect("captured Control source target");
            if source_command_error && frame_flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL {
                self.inner
                    .lock()
                    .client_entry(control_client.expect("captured Control source client"))
                    .command_streams
                    .get_or_insert_default()
                    .exit_code = 1;
            }
            self.publish_control_command_guard(
                control_target,
                control_source_errors.join("\n").into(),
                source_command_error && !control_source_matched,
                source_command_error,
            );
            control_source_errors.clear();
        }
        for output in control_source_read_errors {
            self.publish_control_source_read_error(
                control_target.expect("source read errors have a control target"),
                context.pane,
                ErrorKind::Other,
                output,
            );
        }
        if source_invocation && queue_execution.is_some_and(CommandQueueExecution::is_draining) {
            self.enter_queue_shutdown_phase();
        }
        if source_invocation
            && let Some(queue_execution) = queue_execution
            && queue_execution.wait_yields
            && queue_execution.is_draining()
        {
            queue_execution.yield_queue();
            pending_source_files.clear();
        }
        if source_invocation
            && queue_execution.is_some()
            && self
                .command_item
                .as_ref()
                .is_some_and(|item| item.lock().loop_wait)
        {
            return source_queue::SourceExecution {
                client,
                kind,
                execution,
                source_client,
                source_kind,
                source_client_terminal,
                source_client_base,
                control_target,
                control_client,
                source_file_error,
                source_path_error,
                source_path_matched,
                control_source_errors,
                control_source_matched,
                source_verbose_output,
                source_replay_output,
                source_diagnostics_output,
                source_invocations,
                source_invocation,
                reported_source_failure,
                reported_source_callback_failure,
                control_source_invocation,
                suppress_source_replay_output,
                reload_config,
                read_only,
                client_timers_changed,
                status_formats_changed,
                status_refresh_sessions,
                pending_hook_events,
                notifications_only,
                incremental_start,
                terminal_wait,
                pending: pending_source_files.into(),
                parsed: VecDeque::new(),
                read: None,
                replay: None,
                replay_pending: None,
                deferred_control_config_warnings: Vec::new(),
            }
            .run(self, context, queue_execution);
        }
        let mut parsed_source_files = Vec::new();
        let mut deferred_control_config_warnings = Vec::new();
        for pending in pending_source_files {
            let mut report = if control_target.is_some() || source_client == ClientId(u64::MAX) {
                ConfigLoadReport::default()
            } else {
                ConfigLoadReport::with_stdout_transcript()
            };
            let parsed = if let Some(stream) = &pending.stdin {
                self.parse_config_input(
                    &pending.path,
                    ConfigInput::sourced(stream.as_bytes()),
                    &mut report,
                    pending.options,
                    true,
                )
                .map(Some)
                .ok_or_else(|| DaemonError::from(non_utf8_config_error()))
            } else {
                self.parse_config_file(&pending.path, &mut report, pending.options, true)
            };
            match parsed {
                Ok(parsed) => parsed_source_files.push((pending, parsed, report)),
                Err(DaemonError::Io(error)) => {
                    if !pending
                        .context
                        .format_variables
                        .contains_key(HOOK_CONTEXT_FORMAT)
                    {
                        reported_source_failure = true;
                    }
                    let warning = if control_target.is_some() {
                        source_read_error_warning(&pending.path, &error)
                    } else {
                        client_source_read_error_warning(
                            source_kind == ClientKind::Command,
                            &pending.path,
                            &error,
                        )
                    };
                    if let Some(target) = control_target {
                        self.publish_control_source_read_error(
                            target,
                            pending.context.pane,
                            error.kind(),
                            warning,
                        );
                    } else {
                        self.route_source_error(
                            source_client,
                            source_kind,
                            pending.context.pane,
                            &warning,
                        );
                    }
                }
                Err(error) => {
                    if source_file_error.is_none() {
                        source_file_error = Some(error);
                    }
                }
            }
        }
        for (pending, parsed, mut report) in parsed_source_files {
            if let Some((client, _)) = control_target {
                deferred_control_config_warnings.extend(report.diagnostics().iter().map(
                    |diagnostic| DeferredControlConfigWarning {
                        client,
                        text: diagnostic.clone(),
                    },
                ));
            }
            let PendingConfigFile {
                path,
                mut context,
                options,
                stdin: _,
            } = pending;
            let defer_command_error_hook_replay_issues = control_target.is_none()
                && queue_execution.is_some()
                && context.replay_client().is_some()
                && context
                    .format_variables
                    .get(HOOK_CONTEXT_FORMAT)
                    .is_some_and(|hook| hook == "command-error");
            let replayed = parsed.map_or(Ok(()), |parsed| {
                self.replay_config_file_with_parent_queue(
                    &path,
                    parsed,
                    &mut context,
                    0,
                    &mut report,
                    source_client_terminal,
                    source_client_base.as_deref(),
                    &mut source_invocations,
                    options,
                    &mut deferred_control_config_warnings,
                    queue_execution,
                )
            });
            match replayed {
                Ok(()) => {}
                Err(DaemonError::Io(error)) => {
                    if !context.format_variables.contains_key(HOOK_CONTEXT_FORMAT) {
                        reported_source_failure = true;
                    }
                    let warning = if control_target.is_some() {
                        source_read_error_warning(&path, &error)
                    } else {
                        client_source_read_error_warning(
                            source_kind == ClientKind::Command,
                            &path,
                            &error,
                        )
                    };
                    if !options.suppress_replay_output {
                        if let Some(target) = control_target {
                            self.publish_control_source_read_error(
                                target,
                                context.pane,
                                error.kind(),
                                warning,
                            );
                        } else {
                            self.route_source_error(
                                source_client,
                                source_kind,
                                context.pane,
                                &warning,
                            );
                        }
                    }
                }
                Err(error) => {
                    if source_file_error.is_none() {
                        source_file_error = Some(error);
                    }
                }
            }
            let hook_owned_replay = context.format_variables.contains_key(HOOK_CONTEXT_FORMAT);
            reported_source_callback_failure |= report.source_callback_failure;
            if report.reported_failure || !hook_owned_replay && !report.replay_issues().is_empty() {
                reported_source_failure = true;
                if let Some((client, _)) = control_target
                    && let Some(streams) = self
                        .inner
                        .lock()
                        .client_mut(client)
                        .and_then(|c| c.command_streams.as_mut())
                {
                    streams.exit_code = 1;
                }
            }
            if !options.parse_only {
                self.apply_stored_mux_config_overrides("source-file-replay");
            }
            if !options.suppress_replay_output {
                if let Some((verbose, replay)) = report.stdout_transcript() {
                    if control_target.is_none() && source_kind == ClientKind::Command {
                        let claim = if report.stdout_raw_claimed() {
                            Some(StdoutClaim::Raw)
                        } else if verbose.is_empty() && replay.is_empty() {
                            None
                        } else {
                            Some(StdoutClaim::Print)
                        };
                        if let Some(claim) = claim {
                            self.record_command_stdout_claim(source_client, claim);
                        }
                    }
                    append_inserted_output(&mut source_verbose_output, verbose);
                    append_inserted_output(&mut source_replay_output, replay);
                }
                if let Some(diagnostics) = report.stdout_diagnostics() {
                    append_inserted_output(&mut source_diagnostics_output, diagnostics);
                }
                if control_target.is_none() && source_kind == ClientKind::Command {
                    for (index, diagnostic) in report.diagnostics().iter().enumerate() {
                        if !report.replayed_diagnostics.contains(&index) {
                            self.record_command_stdout(source_client, diagnostic);
                        }
                        self.record_command_failure(source_client);
                    }
                }
                if defer_command_error_hook_replay_issues {
                    queue_execution
                        .expect("deferred replay issue queue")
                        .deferred_config_replay_issues
                        .borrow_mut()
                        .extend(
                            report
                                .take_undelivered_replay_issues()
                                .into_iter()
                                .map(DeferredConfigReplayIssue::Replay),
                        );
                } else {
                    if let Some((client, flags)) = control_target {
                        if flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL {
                            self.route_config_replay_errors(
                                client,
                                ClientKind::Control,
                                context.pane,
                                &mut report,
                            );
                        } else {
                            report.delivered_replay_issues = report.replay_issues().len();
                        }
                    } else {
                        self.route_config_replay_errors(
                            source_client,
                            source_kind,
                            context.pane,
                            &mut report,
                        );
                    }
                    if control_target.is_none()
                        && source_kind == ClientKind::Command
                        && let Some(skipped) = report.skipped_summary()
                    {
                        self.record_command_stderr(source_client, &skipped);
                    }
                    let summary = if control_target.is_some() && report.control_guarded {
                        report.skipped_summary()
                    } else {
                        report.message()
                    };
                    if let Some(summary) = summary {
                        self.publish_to_client(
                            control_client.unwrap_or(source_client),
                            EventPayload::ClientMessage {
                                pane: context.pane,
                                kind: ClientMessageKind::Warning,
                                text: summary,
                            },
                        );
                    }
                }
            }
        }
        self.publish_deferred_control_config_warnings(deferred_control_config_warnings);
        if control_source_invocation {
            self.publish_control_source_complete(control_target);
        }
        append_inserted_output(&mut execution.output, &source_verbose_output);
        append_inserted_output(&mut execution.output, &source_replay_output);
        append_inserted_output(&mut execution.output, &source_diagnostics_output);
        if !control_source_errors.is_empty() {
            let mut inner = self.inner.lock();
            let target = control_target.expect("Control source errors have a target");
            let streams = inner
                .client_entry(target.0)
                .command_streams
                .get_or_insert_default();
            if control_source_matched {
                for error in control_source_errors {
                    streams.stdout.push_str(&error);
                    streams.stdout.push('\n');
                }
                streams.exit_code = 1;
            } else {
                streams.control_error = control_source_errors.join("\n");
            }
        }
        if reload_config
            && let Err(error) = self.reload_user_config_with_source_base(
                source_client,
                context,
                source_client_base.as_deref(),
                SourceFileLoadOptions::default(),
            )
            && source_file_error.is_none()
        {
            source_file_error = Some(error);
        }
        if !read_only {
            self.publish_key_tables_if_changed();
        }
        if client_timers_changed || status_formats_changed || !status_refresh_sessions.is_empty() {
            self.nudge_client_timers();
        }
        pending_hook_events.extend(std::mem::take(
            &mut self.inner.lock().waits.deferred_event_hooks,
        ));
        if !pending_hook_events.is_empty() {
            self.wake_control_queue(client, kind);
        }
        if notifications_only {
            self.run_event_hooks(pending_hook_events);
        } else if let Some(queue_execution) = queue_execution {
            self.publish_event_control_notifications(&mut pending_hook_events);
            queue_execution
                .pending_event_hooks
                .borrow_mut()
                .extend(pending_hook_events);
        } else {
            self.run_event_hooks(pending_hook_events);
        }
        if let Some(submission) = incremental_start {
            self.submit_command_prompt(source_client, source_kind, context, &submission);
        }
        if suppress_source_replay_output
            && source_path_matched
            && let Some(queue_execution) = queue_execution
        {
            queue_execution.suppress_output.set(true);
        }
        let report_sets_request_status = reported_source_failure
            && (source_kind == ClientKind::Command
                || control_target.is_some_and(|(_, flags)| {
                    flags == CONTROL_COMMAND_FRAME_FLAGS_NONE
                        || flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL
                            && reported_source_callback_failure
                }));
        if report_sets_request_status && let Some(queue_execution) = queue_execution {
            queue_execution.note_reported_failure();
        }
        match source_file_error {
            Some(error) => Err(prepend_command_output(execution.output, error)),
            None if source_invocation
                && report_sets_request_status
                && queue_execution.is_none()
                && matches!(source_kind, ClientKind::Command | ClientKind::Control) =>
            {
                Err(DaemonError::ReportedCommandExit {
                    output: execution.output,
                    exit_code: 1,
                })
            }
            None if source_path_error && !source_path_matched => Err(DaemonError::CommandExit {
                output: execution.output,
                exit_code: 1,
            }),
            None => match terminal_wait {
                Some(wait) => wait.finish(self, execution),
                None => Ok(execution),
            },
        }
    }
}
