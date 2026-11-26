use super::*;

impl Shared {
    pub(super) fn run_shell(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command_name: &str,
        invocation: &CommandInvocation,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let args = &invocation.args;
        let parsed = parse_run_shell_args(args)?;
        let inserted_target = ExecutionContext::new(context.session, context.window, context.pane);
        let draining = queue_execution.is_some_and(CommandQueueExecution::is_draining);
        let detached = queue_execution.is_some_and(|execution| execution.detached);
        let (
            target,
            command_context,
            command,
            cwd,
            tmux,
            environment,
            default_terminal,
            environment_timing,
        ) = {
            let mut inner = self.inner.lock();
            let target = parsed
                .target
                .as_deref()
                .and_then(|target| {
                    inner
                        .engine
                        .resolve_pane(Some(target), context.window, context.pane)
                        .ok()
                })
                .filter(|pane| {
                    inner
                        .engine
                        .state
                        .pane(*pane)
                        .is_some_and(|pane| !pane.dead)
                });
            let mut command_context = context.clone();
            if let Some(pane) = target {
                command_context.retarget_to_pane(&inner.engine.state, pane);
            } else if parsed.target.is_some() && !parsed.command_mode {
                command_context.session = None;
                command_context.window = None;
                command_context.pane = None;
            }
            inner.engine.set_format_now(unix_timestamp());
            let missing_format_target = parsed.target.is_some() && target.is_none();
            let (format_session, format_window, format_pane, active_session, format_client) =
                if missing_format_target {
                    (None, None, None, None, FormatClient::NoClient)
                } else {
                    (
                        command_context.session,
                        command_context.window,
                        command_context.pane,
                        command_context.session,
                        command_context.target_format_client(),
                    )
                };
            let format_context = server_format_context_with_format_client(
                &inner.engine,
                &inner.config.config_files,
                format_session,
                format_window,
                format_pane,
                active_session,
                format_client,
            );
            let facts = borrowed_format_hook_facts_for_client(&inner, client, &command_context);
            let command = parsed.positional.first().map(|command| {
                if parsed.command_mode {
                    if invocation.argument_is_command_block(parsed.positional_start) {
                        typed_inserted_command_source(
                            invocation
                                .args
                                .get(parsed.positional_start)
                                .expect("run-shell command argument"),
                            true,
                        )
                    } else {
                        let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                            &facts,
                            command_context.format_variables(),
                        )
                        .with_option_engine(&inner.engine)
                        .with_command_item(command_name);
                        InsertedCommandSource::String(expand_format_values(
                            command,
                            &format_context,
                            &mut hooks,
                        ))
                    }
                } else {
                    let mut variables = command_context.format_variables.clone();
                    variables.extend(parsed.positional.iter().skip(1).enumerate().map(
                        |(index, value)| {
                            (run_shell_position_context_name(index + 1), value.clone())
                        },
                    ));
                    let mut hooks = DaemonFormatHooks::command_with_variables(&facts, &variables)
                        .with_option_engine(&inner.engine)
                        .with_command_item(command_name);
                    InsertedCommandSource::Shell(expand_format_values(
                        command,
                        &format_context,
                        &mut hooks,
                    ))
                }
            });
            let cwd = shell_job_working_directory(
                &inner,
                client,
                &command_context,
                parsed.cwd.as_deref(),
            );
            let tmux = tmux_environment(&self.socket_path, command_context.session);
            let environment_timing = if parsed.positive_delay
                && matches!(&command, Some(InsertedCommandSource::Shell(_)))
            {
                ShellJobEnvironmentTiming::LaunchTime {
                    session_environment: command_context
                        .session
                        .and_then(|session| inner.engine.retain_session_job_environment(session)),
                }
            } else {
                ShellJobEnvironmentTiming::CommandTime
            };
            let environment = inner.engine.job_environment(command_context.session);
            let default_terminal = inner.engine.default_terminal_for_spawn().to_owned();
            (
                target,
                command_context,
                command,
                cwd,
                tmux,
                environment,
                default_terminal,
                environment_timing,
            )
        };

        let route = RunShellRoute {
            client,
            kind,
            control_target: command_context
                .control_command_target()
                .map(|(client, _)| client)
                .or_else(|| (kind == ClientKind::Control).then_some(client)),
            foreground: !parsed.background && !draining,
            target_requested: parsed.target.is_some(),
            target,
        };
        let delay = parsed.delay.unwrap_or_default();
        match command {
            None if parsed.delay.is_none() => Ok(Execution::default()),
            None => {
                if parsed.background || draining {
                    if draining && !parsed.background {
                        queue_execution
                            .expect("draining command queue")
                            .yield_queue();
                    }
                    self.spawn_delay(delay, || {})?;
                    Ok(Execution::default())
                } else {
                    self.park_shell_delay(delay, true, queue_execution)?;
                    Ok(Execution::default())
                }
            }
            Some(
                source @ (InsertedCommandSource::String(_)
                | InsertedCommandSource::Block(_)
                | InsertedCommandSource::Commands(_)),
            ) => {
                if parsed.background || draining && !delay.is_zero() {
                    if draining && !parsed.background {
                        queue_execution
                            .expect("draining command queue")
                            .yield_queue();
                    }
                    let shared = self.server_owner();
                    let mut command_context = command_context;
                    let control_target = command_context
                        .control_command_target()
                        .map(|(client, _)| (client, CONTROL_COMMAND_FRAME_FLAGS_NONE));
                    let replay_client =
                        selected_deferred_client(&self.inner.lock(), &command_context, client);
                    command_context.set_replay_client(replay_client);
                    command_context.set_control_command_target(control_target);
                    let mut detached_callback_blocker = if parsed.background && delay.is_zero() {
                        Some(
                            queue_execution
                                .and_then(|execution| execution.fork_shutdown_blocker(true))
                                .or_else(|| ShutdownBlocker::acquire(self, true))
                                .ok_or_else(|| {
                                    ServerError::InvalidCommand("failed to run command".to_owned())
                                })?,
                        )
                    } else {
                        None
                    };
                    if let Some(blocker) = &mut detached_callback_blocker {
                        blocker.shared = blocker.shared.server_owner();
                    }
                    self.spawn_delay(delay, move || {
                        if !parsed.background && shared.stopping.load(Ordering::Acquire) {
                            return;
                        }
                        if control_target
                            .is_some_and(|(client, _)| !shared.control_client_is_connected(client))
                        {
                            return;
                        }
                        let mut context = command_context;
                        context.retarget(&inserted_target);
                        #[cfg(unix)]
                        if shared.loop_active.load(Ordering::Acquire) {
                            shared.enqueue_inserted_task(
                                client,
                                kind,
                                &context,
                                &source,
                                "<run-shell -C>",
                                control_target,
                                parsed.background,
                                detached_callback_blocker,
                                Box::new(move |shared, context, execution| match execution {
                                    Ok(result) => {
                                        let _ = shared.finish_inserted_run_shell(
                                            route,
                                            "run-shell".to_owned(),
                                            &result,
                                        );
                                    }
                                    Err(_) if control_target.is_some() => {}
                                    Err(error) => {
                                        if let Some(output) = daemon_error_output(&error) {
                                            shared.route_background_inserted_output(
                                                client,
                                                kind,
                                                context,
                                                "run-shell".to_owned(),
                                                output,
                                            );
                                        }
                                        shared.publish_background_command_error(
                                            client, context, &error, true,
                                        );
                                    }
                                }),
                            );
                            return;
                        }
                        let execution = if parsed.background {
                            shared.execute_detached_inserted_commands_with_control_target(
                                client,
                                kind,
                                &mut context,
                                &source,
                                "<run-shell -C>",
                                control_target,
                                detached_callback_blocker,
                            )
                        } else {
                            shared.execute_inserted_commands_with_control_target(
                                client,
                                kind,
                                &mut context,
                                &source,
                                "<run-shell -C>",
                                control_target,
                            )
                        };
                        match execution {
                            Ok(result) => {
                                let _ = shared.finish_inserted_run_shell(
                                    route,
                                    "run-shell".to_owned(),
                                    &result,
                                );
                            }
                            Err(_) if control_target.is_some() => {}
                            Err(error) => {
                                if let Some(output) = daemon_error_output(&error) {
                                    shared.route_background_inserted_output(
                                        client,
                                        kind,
                                        &context,
                                        "run-shell".to_owned(),
                                        output,
                                    );
                                }
                                shared.publish_background_command_error(
                                    client, &context, &error, true,
                                );
                            }
                        }
                    })?;
                    Ok(Execution::default())
                } else {
                    self.park_shell_delay(delay, !delay.is_zero(), queue_execution)?;
                    let mut command_context = command_context;
                    command_context.retarget(&inserted_target);
                    let result = self.execute_foreground_inserted_commands(
                        client,
                        kind,
                        &mut command_context,
                        &source,
                        "<run-shell -C>",
                        queue_execution,
                    )?;
                    *context = command_context;
                    if let Some(queue) = queue_execution
                        && let Some(child) = queue.child.borrow_mut().as_mut()
                    {
                        child.kind = InsertedQueueChildKind::Foreground(Some(route));
                        return Ok(Execution::default());
                    }
                    self.finish_inserted_run_shell(route, "run-shell".to_owned(), &result)
                }
            }
            Some(InsertedCommandSource::Hooks(_) | InsertedCommandSource::Events(_)) => {
                unreachable!()
            }
            Some(InsertedCommandSource::Shell(command)) => {
                if parsed.background || draining {
                    if draining && !parsed.background {
                        queue_execution
                            .expect("draining command queue")
                            .yield_queue();
                    }
                    let shared = self.server_owner();
                    let ticket = self.background_insertion_ticket();
                    let background_command = command.clone();
                    let worker_context = command_context.clone();
                    let policy = ShellJobSpawnPolicy {
                        #[cfg(not(unix))]
                        wait_for_start: delay.is_zero(),
                        shutdown_blocking: draining && !parsed.background && delay.is_zero(),
                        detached,
                    };
                    let callback = move |result: Result<ShellJobResult, ()>| {
                        if draining {
                            return;
                        }
                        if let Ok(result) = result {
                            let applier = Arc::clone(&shared);
                            applier.apply_background_insertion(ticket, move || {
                                let _ =
                                    shared.finish_run_shell(route, &background_command, &result);
                            });
                        } else {
                            let error = ServerError::InvalidCommand(format!(
                                "failed to run command: {background_command}"
                            ))
                            .into();
                            shared.publish_background_command_error(
                                route.client,
                                &worker_context,
                                &error,
                                false,
                            );
                        }
                    };
                    if parsed.background
                        && !draining
                        && delay.is_zero()
                        && let Some(queue_execution) = queue_execution
                    {
                        let session_environment = command_context.session.and_then(|session| {
                            self.inner
                                .lock()
                                .engine
                                .retain_session_job_environment(session)
                        });
                        let show_stderr = parsed.show_stderr;
                        let error_context = command_context.clone();
                        queue_execution
                            .deferred_shell_jobs
                            .borrow_mut()
                            .push(Box::new(move |shared: &Arc<Self>| {
                                let (environment, default_terminal) = {
                                    let inner = shared.inner.lock();
                                    (
                                        inner.engine.job_environment_with_retained_session(
                                            session_environment.as_ref(),
                                        ),
                                        inner.engine.default_terminal_for_spawn().to_owned(),
                                    )
                                };
                                if let Err(error) = shared.spawn_shell_job(
                                    command,
                                    cwd,
                                    tmux,
                                    environment,
                                    default_terminal,
                                    ShellJobEnvironmentTiming::CommandTime,
                                    show_stderr,
                                    delay,
                                    policy,
                                    None,
                                    callback,
                                ) {
                                    shared.publish_background_command_error(
                                        client,
                                        &error_context,
                                        &error,
                                        false,
                                    );
                                }
                            }));
                        return Ok(Execution::default());
                    }
                    if let Err(error) = self.spawn_shell_job(
                        command,
                        cwd,
                        tmux,
                        environment,
                        default_terminal,
                        environment_timing,
                        parsed.show_stderr,
                        delay,
                        policy,
                        queue_execution,
                        callback,
                    ) {
                        self.publish_background_command_error(
                            client,
                            &command_context,
                            &error,
                            false,
                        );
                    }
                    Ok(Execution::default())
                } else {
                    #[cfg(unix)]
                    {
                        let failed_command = command.clone();
                        let finish_command = command.clone();
                        let (wait, callback) = ShellWait::new(self, move |shared, _, _, result| {
                            let result = result.map_err(|()| {
                                ServerError::InvalidCommand(format!(
                                    "failed to run command: {failed_command}"
                                ))
                            })?;
                            shared.finish_run_shell(route, &finish_command, &result)
                        });
                        self.spawn_shell_job(
                            command,
                            cwd,
                            tmux,
                            environment,
                            default_terminal,
                            environment_timing,
                            parsed.show_stderr,
                            delay,
                            ShellJobSpawnPolicy {
                                #[cfg(not(unix))]
                                wait_for_start: false,
                                shutdown_blocking: delay.is_zero(),
                                detached,
                            },
                            queue_execution,
                            callback,
                        )?;
                        self.park_shell_job(wait, context, queue_execution)
                    }
                    #[cfg(not(unix))]
                    {
                        let (sender, receiver) = mpsc::sync_channel(1);
                        let failed_command = command.clone();
                        self.spawn_shell_job(
                            command.clone(),
                            cwd,
                            tmux,
                            environment,
                            default_terminal,
                            environment_timing,
                            parsed.show_stderr,
                            delay,
                            ShellJobSpawnPolicy {
                                #[cfg(not(unix))]
                                wait_for_start: false,
                                shutdown_blocking: delay.is_zero(),
                                detached,
                            },
                            queue_execution,
                            move |result| {
                                let _ = sender.send(result);
                            },
                        )?;
                        self.report_command_queue_park();
                        let result = receiver.recv().map_err(|error| {
                            DaemonError::Thread(format!("run-shell worker stopped: {error}"))
                        })?;
                        let result = result.map_err(|()| {
                            ServerError::InvalidCommand(format!(
                                "failed to run command: {failed_command}"
                            ))
                        })?;
                        self.finish_run_shell(route, &command, &result)
                    }
                }
            }
        }
    }

    pub(super) fn if_shell(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command_name: &str,
        command: &CommandInvocation,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_if_shell_args(&command.args)?;
        let inserted_target = ExecutionContext::new(context.session, context.window, context.pane);
        let draining = queue_execution.is_some_and(CommandQueueExecution::is_draining);
        let detached = queue_execution.is_some_and(|execution| execution.detached);
        let branches = parsed
            .positional
            .iter()
            .enumerate()
            .skip(1)
            .map(|(position, _)| {
                typed_inserted_command_source(
                    &command.args[parsed.positional_start + position],
                    command.argument_is_command_block(parsed.positional_start + position),
                )
            })
            .collect::<Vec<_>>();
        let (condition, command_context, cwd, tmux, environment, default_terminal) = {
            let mut inner = self.inner.lock();
            let target = parsed.target.as_deref().and_then(|target| {
                inner
                    .engine
                    .resolve_pane(Some(target), context.window, context.pane)
                    .ok()
            });
            let mut command_context = context.clone();
            if let Some(pane) = target {
                command_context.retarget_to_pane(&inner.engine.state, pane);
            } else if parsed.target.is_some() && !parsed.format {
                command_context.session = None;
                command_context.window = None;
                command_context.pane = None;
            }
            inner.engine.set_format_now(unix_timestamp());
            let missing_format_target = parsed.target.is_some() && target.is_none();
            let (format_session, format_window, format_pane, active_session, format_client) =
                if missing_format_target {
                    (None, None, None, None, FormatClient::NoClient)
                } else {
                    (
                        command_context.session,
                        command_context.window,
                        command_context.pane,
                        command_context.session,
                        command_context.target_format_client(),
                    )
                };
            let format_context = server_format_context_with_format_client(
                &inner.engine,
                &inner.config.config_files,
                format_session,
                format_window,
                format_pane,
                active_session,
                format_client,
            );
            let facts = borrowed_format_hook_facts_for_client(&inner, client, &command_context);
            let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                &facts,
                command_context.format_variables(),
            )
            .with_option_engine(&inner.engine)
            .with_command_item(command_name);
            let condition =
                expand_format_values(&parsed.positional[0], &format_context, &mut hooks);
            let cwd = shell_job_working_directory(&inner, client, &command_context, None);
            let tmux = tmux_environment(&self.socket_path, command_context.session);
            let environment = inner.engine.job_environment(command_context.session);
            let default_terminal = inner.engine.default_terminal_for_spawn().to_owned();
            (
                condition,
                command_context,
                cwd,
                tmux,
                environment,
                default_terminal,
            )
        };

        if parsed.format {
            let Some(source) = select_if_shell_branch(&branches, if_shell_truthy(&condition))
            else {
                return Ok(Execution::default());
            };
            let mut command_context = command_context;
            command_context.retarget(&inserted_target);
            let result = self.execute_foreground_inserted_commands(
                client,
                kind,
                &mut command_context,
                source,
                "<if-shell>",
                queue_execution,
            )?;
            *context = command_context;
            return inserted_execution(client, kind, result);
        }

        if parsed.background || draining {
            if draining && !parsed.background {
                queue_execution
                    .expect("draining command queue")
                    .yield_queue();
            }
            let shared = self.server_owner();
            let ticket = self.background_insertion_ticket();
            let condition_for_error = condition.clone();
            let mut command_context = command_context;
            let control_target = command_context
                .control_command_target()
                .map(|(client, _)| (client, CONTROL_COMMAND_FRAME_FLAGS_NONE));
            let replay_client =
                selected_deferred_client(&self.inner.lock(), &command_context, client);
            command_context.set_replay_client(replay_client);
            command_context.set_control_command_target(control_target);
            self.spawn_shell_job(
                condition,
                cwd,
                tmux,
                environment,
                default_terminal,
                ShellJobEnvironmentTiming::CommandTime,
                false,
                Duration::ZERO,
                ShellJobSpawnPolicy {
                    #[cfg(not(unix))]
                    wait_for_start: true,
                    shutdown_blocking: draining && !parsed.background,
                    detached,
                },
                queue_execution,
                move |result| {
                    if draining {
                        return;
                    }
                    if control_target
                        .is_some_and(|(client, _)| !shared.control_client_is_connected(client))
                    {
                        return;
                    }
                    if let Ok(result) = result {
                        let Some(source) =
                            select_if_shell_branch(&branches, result.status.success()).cloned()
                        else {
                            return;
                        };
                        let applier = Arc::clone(&shared);
                        applier.apply_background_insertion(ticket, move || {
                            let mut context = command_context;
                            context.retarget(&inserted_target);
                            #[cfg(unix)]
                            if shared.loop_active.load(Ordering::Acquire) {
                                shared.enqueue_inserted_task(
                                    client,
                                    kind,
                                    &context,
                                    &source,
                                    "<if-shell>",
                                    control_target,
                                    false,
                                    None,
                                    Box::new(move |shared, context, execution| match execution {
                                        Ok(result) => shared.route_background_inserted_output(
                                            client,
                                            kind,
                                            context,
                                            "if-shell".to_owned(),
                                            &result.output,
                                        ),
                                        Err(_) if control_target.is_some() => {}
                                        Err(error) => {
                                            if let Some(output) = daemon_error_output(&error) {
                                                shared.route_background_inserted_output(
                                                    client,
                                                    kind,
                                                    context,
                                                    "if-shell".to_owned(),
                                                    output,
                                                );
                                            }
                                            shared.publish_background_command_error(
                                                client, context, &error, true,
                                            );
                                        }
                                    }),
                                );
                                return;
                            }
                            match shared.execute_inserted_commands_with_control_target(
                                client,
                                kind,
                                &mut context,
                                &source,
                                "<if-shell>",
                                control_target,
                            ) {
                                Ok(result) => shared.route_background_inserted_output(
                                    client,
                                    kind,
                                    &context,
                                    "if-shell".to_owned(),
                                    &result.output,
                                ),
                                Err(_) if control_target.is_some() => {}
                                Err(error) => {
                                    if let Some(output) = daemon_error_output(&error) {
                                        shared.route_background_inserted_output(
                                            client,
                                            kind,
                                            &context,
                                            "if-shell".to_owned(),
                                            output,
                                        );
                                    }
                                    shared.publish_background_command_error(
                                        client, &context, &error, true,
                                    );
                                }
                            }
                        });
                    } else if !draining {
                        let error = ServerError::InvalidCommand(format!(
                            "failed to run command: {condition_for_error}"
                        ))
                        .into();
                        shared.publish_background_command_error(
                            client,
                            &command_context,
                            &error,
                            false,
                        );
                    }
                },
            )?;
            return Ok(Execution::default());
        }

        #[cfg(unix)]
        {
            let failed_condition = condition.clone();
            let (wait, callback) = ShellWait::new(self, move |shared, context, queue, result| {
                let result = result.map_err(|()| {
                    ServerError::InvalidCommand(format!(
                        "failed to run command: {failed_condition}"
                    ))
                })?;
                let Some(source) = select_if_shell_branch(&branches, result.status.success())
                else {
                    return Ok(Execution::default());
                };
                let mut command_context = command_context;
                command_context.retarget(&inserted_target);
                let result = shared.execute_foreground_inserted_commands(
                    client,
                    kind,
                    &mut command_context,
                    source,
                    "<if-shell>",
                    queue,
                )?;
                *context = command_context;
                inserted_execution(client, kind, result)
            });
            self.spawn_shell_job(
                condition,
                cwd,
                tmux,
                environment,
                default_terminal,
                ShellJobEnvironmentTiming::CommandTime,
                false,
                Duration::ZERO,
                ShellJobSpawnPolicy {
                    #[cfg(not(unix))]
                    wait_for_start: false,
                    shutdown_blocking: true,
                    detached,
                },
                queue_execution,
                callback,
            )?;
            self.park_shell_job(wait, context, queue_execution)
        }
        #[cfg(not(unix))]
        {
            let (sender, receiver) = mpsc::sync_channel(1);
            let failed_condition = condition.clone();
            self.spawn_shell_job(
                condition,
                cwd,
                tmux,
                environment,
                default_terminal,
                ShellJobEnvironmentTiming::CommandTime,
                false,
                Duration::ZERO,
                ShellJobSpawnPolicy {
                    #[cfg(not(unix))]
                    wait_for_start: false,
                    shutdown_blocking: true,
                    detached,
                },
                queue_execution,
                move |result| {
                    let _ = sender.send(result);
                },
            )?;
            self.report_command_queue_park();
            let result = receiver.recv().map_err(|error| {
                DaemonError::Thread(format!("if-shell worker stopped: {error}"))
            })?;
            let result = result.map_err(|()| {
                ServerError::InvalidCommand(format!("failed to run command: {failed_condition}"))
            })?;
            let Some(source) = select_if_shell_branch(&branches, result.status.success()) else {
                return Ok(Execution::default());
            };
            let mut command_context = command_context;
            command_context.retarget(&inserted_target);
            let result = self.execute_foreground_inserted_commands(
                client,
                kind,
                &mut command_context,
                source,
                "<if-shell>",
                queue_execution,
            )?;
            *context = command_context;
            inserted_execution(client, kind, result)
        }
    }

    fn finish_run_shell(
        self: &Arc<Self>,
        route: RunShellRoute,
        command: &str,
        result: &ShellJobResult,
    ) -> Result<Execution, DaemonError> {
        let (output, exit_code) = shell_job_output(command, result);
        let output = self.route_shell_job_output(route, command.to_owned(), &output);
        if route.foreground
            && matches!(route.kind, ClientKind::Command | ClientKind::Control)
            && route.client != ClientId(u64::MAX)
            && exit_code != 0
        {
            Err(DaemonError::CommandExit {
                output: output.into(),
                exit_code,
            })
        } else {
            Ok(Execution {
                output: output.into(),
                effects: Vec::new(),
            })
        }
    }

    pub(super) fn finish_inserted_run_shell(
        self: &Arc<Self>,
        route: RunShellRoute,
        title: String,
        result: &InsertedCommandResult,
    ) -> Result<Execution, DaemonError> {
        let output = self.route_run_shell_output(route, title, &result.output);
        inserted_execution(
            route.client,
            route.kind,
            InsertedCommandResult {
                output: output.into(),
                exit_code: result.exit_code,
                yielded: result.yielded,
            },
        )
    }

    fn route_shell_job_output(
        self: &Arc<Self>,
        route: RunShellRoute,
        title: String,
        output: &str,
    ) -> String {
        if output.is_empty() {
            return String::new();
        }
        if let Some(target) = route.target
            && self
                .inner
                .lock()
                .engine
                .state
                .pane(target)
                .is_some_and(|pane| !pane.dead)
        {
            self.open_command_output_for_pane(target, &title, output);
            return String::new();
        }
        if route.foreground {
            if let Some(control_target) = route.control_target {
                self.publish_to_client(
                    control_target,
                    EventPayload::ControlCommandOutput {
                        output: output.to_owned(),
                    },
                );
                return String::new();
            }
            return self.route_run_shell_output(route, title, output);
        }
        let pane = self
            .inner
            .lock()
            .engine
            .state
            .most_recent_context()
            .map(|(_, _, pane)| pane);
        if let Some(pane) = pane {
            self.open_command_output_for_pane(pane, &title, output);
        }
        String::new()
    }

    fn route_run_shell_output(
        self: &Arc<Self>,
        route: RunShellRoute,
        title: String,
        output: &str,
    ) -> String {
        if output.is_empty() {
            return String::new();
        }
        if route.kind == ClientKind::Control {
            return if route.foreground {
                output.to_owned()
            } else {
                String::new()
            };
        }
        if route.target_requested
            && let Some(target) = route.target
            && self
                .inner
                .lock()
                .engine
                .state
                .pane(target)
                .is_some_and(|pane| !pane.dead)
        {
            self.open_command_output_for_pane(target, &title, output);
            return String::new();
        }
        if route.foreground {
            if matches!(route.kind, ClientKind::Command | ClientKind::Control) {
                return output.to_owned();
            }
            let _ = self.open_command_output(route.client, None, title, output);
            return String::new();
        }
        let pane = self
            .inner
            .lock()
            .engine
            .state
            .most_recent_context()
            .map(|(_, _, pane)| pane);
        if let Some(pane) = pane {
            self.open_command_output_for_pane(pane, &title, output);
        }
        String::new()
    }

    pub(super) fn spawn_delay(
        self: &Arc<Self>,
        delay: Duration,
        callback: impl FnOnce() + Send + 'static,
    ) -> Result<(), DaemonError> {
        #[cfg(all(test, unix))]
        if !self.loop_active.load(Ordering::Acquire) {
            self.start_timers()?;
        }
        #[cfg(all(test, windows))]
        self.start_timers()?;
        self.timer_tx
            .send(timers::TimerInput::Callback {
                deadline: (!delay.is_zero()).then(|| Instant::now() + delay),
                callback: Box::new(callback),
            })
            .map_err(|error| DaemonError::Thread(error.to_string()))
    }

    fn park_shell_delay(
        self: &Arc<Self>,
        delay: Duration,
        report: bool,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<(), DaemonError> {
        let continuation = cmdq::WaitContinuation::new(
            self.command_item
                .as_ref()
                .and_then(|item| item.lock().wait()),
            None,
        );
        let completed = continuation.clone();
        self.spawn_delay(delay, move || {
            completed.complete();
        })?;
        if report {
            self.report_command_queue_park();
        }
        if let Some(item) = &self.command_item {
            let mut item = item.lock();
            #[cfg(unix)]
            let loop_leaf = item.loop_leaf;
            #[cfg(not(unix))]
            let loop_leaf = false;
            if (item.loop_wait || loop_leaf)
                && queue_execution.is_some_and(|queue| queue.frame_active.get())
            {
                item.pending_wait = Some(Box::new(RegisteredWait {
                    name: String::new(),
                    continuation,
                    #[cfg(unix)]
                    shell: None,
                    overlay: None,
                    leaf: None,
                    guard: None,
                    terminal: None,
                    file: None,
                }));
                return Ok(());
            }
        }
        continuation.wait();
        Ok(())
    }

    #[cfg(unix)]
    fn park_shell_job(
        self: &Arc<Self>,
        wait: ShellWait,
        context: &mut ExecutionContext,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        self.report_command_queue_park();
        if let Some(item) = &self.command_item {
            let mut item = item.lock();
            if (item.loop_wait || item.loop_leaf)
                && queue_execution.is_some_and(|queue| queue.frame_active.get())
            {
                item.pending_wait = Some(Box::new(RegisteredWait {
                    name: String::new(),
                    continuation: wait.continuation.clone(),
                    shell: Some(wait),
                    overlay: None,
                    leaf: None,
                    guard: None,
                    terminal: None,
                    file: None,
                }));
                return Ok(Execution::default());
            }
        }
        wait.continuation.wait();
        wait.finish(self, context, queue_execution)
    }

    pub(super) fn spawn_shell_job(
        self: &Arc<Self>,
        command: String,
        cwd: PathBuf,
        tmux: String,
        environment: Vec<(RawText, Option<RawText>)>,
        default_terminal: String,
        environment_timing: ShellJobEnvironmentTiming,
        show_stderr: bool,
        delay: Duration,
        policy: ShellJobSpawnPolicy,
        queue_execution: Option<&CommandQueueExecution>,
        callback: impl FnOnce(Result<ShellJobResult, ()>) + Send + 'static,
    ) -> Result<(), DaemonError> {
        let inherited_blocker = if policy.shutdown_blocking {
            queue_execution.and_then(|execution| execution.fork_shutdown_blocker(policy.detached))
        } else {
            None
        };
        let mut permit = ShellJobPermit::acquire(
            self,
            policy.detached,
            policy.shutdown_blocking,
            inherited_blocker,
        )
        .ok_or_else(|| ServerError::InvalidCommand(format!("failed to run command: {command}")))?;
        permit.shared = permit.shared.server_owner();
        if let Some(blocker) = &mut permit.shutdown_blocker {
            blocker.shared = blocker.shared.server_owner();
        }
        #[cfg(not(unix))]
        let failed_command = command.clone();
        #[cfg(not(unix))]
        let zz_socket = self.socket_path.clone();
        let startup_reentry =
            if matches!(&environment_timing, ShellJobEnvironmentTiming::CommandTime) {
                (!*self.startup_ready.lock()).then(|| self.server_id.to_string())
            } else {
                None
            };
        #[cfg(unix)]
        let (tmux_shim, zz_executable) = self.tmux_shim.lock().as_ref().map_or_else(
            || (None, None),
            |shim| (Some(shim.directory.clone()), Some(shim.executable.clone())),
        );
        #[cfg(not(unix))]
        let (tmux_shim, zz_executable) = (None::<PathBuf>, None::<PathBuf>);
        #[cfg(unix)]
        {
            #[cfg(test)]
            shell_jobs_tests::start_loop(&self.server_owner())?;
            let shared = self.server_owner();
            let launch = move || {
                if shared.stopping.load(Ordering::Acquire)
                    && !policy.detached
                    && !policy.shutdown_blocking
                {
                    drop(permit);
                    callback(Err(()));
                    return;
                }
                let (environment, default_terminal, startup_reentry) = match environment_timing {
                    ShellJobEnvironmentTiming::CommandTime => {
                        (environment, default_terminal, startup_reentry)
                    }
                    ShellJobEnvironmentTiming::LaunchTime {
                        session_environment,
                    } => {
                        let inner = shared.inner.lock();
                        (
                            inner.engine.job_environment_with_retained_session(
                                session_environment.as_ref(),
                            ),
                            inner.engine.default_terminal_for_spawn().to_owned(),
                            (!*shared.startup_ready.lock()).then(|| shared.server_id.to_string()),
                        )
                    }
                };
                let callback = Arc::new(Mutex::new(Some(callback)));
                let completed = Arc::clone(&callback);
                if jobs::launch_shell(
                    &shared,
                    &command,
                    &cwd,
                    &tmux,
                    &environment,
                    &default_terminal,
                    startup_reentry,
                    tmux_shim.as_deref(),
                    zz_executable.as_deref(),
                    show_stderr,
                    permit,
                    move |result| {
                        if let Some(callback) = completed.lock().take() {
                            callback(result);
                        }
                    },
                )
                .is_err()
                    && let Some(callback) = callback.lock().take()
                {
                    callback(Err(()));
                }
            };
            if delay.is_zero() {
                launch();
                return Ok(());
            }
            self.spawn_delay(delay, launch)
        }
        #[cfg(not(unix))]
        {
            let (started_sender, started_receiver) = if policy.wait_for_start {
                let (sender, receiver) = mpsc::sync_channel(1);
                (Some(sender), Some(receiver))
            } else {
                (None, None)
            };
            let launch = move || {
                thread::Builder::new()
                    .name("zz-run-shell".to_owned())
                    .spawn(move || {
                        if permit.shared.stopping.load(Ordering::Acquire)
                            && !policy.detached
                            && !policy.shutdown_blocking
                        {
                            drop(permit);
                            callback(Err(()));
                            return;
                        }
                        let (environment, default_terminal, startup_reentry) =
                            match environment_timing {
                                ShellJobEnvironmentTiming::CommandTime => {
                                    (environment, default_terminal, startup_reentry)
                                }
                                ShellJobEnvironmentTiming::LaunchTime {
                                    session_environment,
                                } => {
                                    let (environment, default_terminal, startup_reentry) = {
                                        let inner = permit.shared.inner.lock();
                                        let startup_ready = *permit.shared.startup_ready.lock();
                                        (
                                            inner.engine.job_environment_with_retained_session(
                                                session_environment.as_ref(),
                                            ),
                                            inner.engine.default_terminal_for_spawn().to_owned(),
                                            (!startup_ready)
                                                .then(|| permit.shared.server_id.to_string()),
                                        )
                                    };
                                    (environment, default_terminal, startup_reentry)
                                }
                            };
                        let result = run_shell_job(
                            &command,
                            &cwd,
                            &tmux,
                            &environment,
                            &default_terminal,
                            &zz_socket,
                            startup_reentry.as_deref(),
                            tmux_shim.as_deref(),
                            zz_executable.as_deref(),
                            show_stderr,
                            &permit.process,
                            &permit.shared.stopping,
                            policy.detached || policy.shutdown_blocking,
                            started_sender.as_ref(),
                        );
                        drop(permit);
                        callback(result);
                    })
                    .map(drop)
                    .map_err(|error| DaemonError::Thread(error.to_string()))
            };
            if delay.is_zero() {
                launch()?;
            } else {
                self.spawn_delay(delay, move || {
                    if let Err(error) = launch() {
                        log::error!("failed to run shell worker: {error}");
                    }
                })?;
            }
            if let Some(started_receiver) = started_receiver {
                started_receiver.recv().map_err(|_| {
                    ServerError::InvalidCommand(format!("failed to run command: {failed_command}"))
                })?;
            }
            Ok(())
        }
    }
}

pub(super) const MAX_SHELL_JOBS: usize = 256;

pub(super) struct ShellJobPermit {
    shared: Arc<Shared>,
    token: u64,
    #[cfg(unix)]
    pub(super) process: Arc<Mutex<Option<ShellJobProcess>>>,
    #[cfg(unix)]
    pub(super) cancel: Arc<AtomicBool>,
    #[cfg(not(unix))]
    pub(super) process: Arc<Mutex<Option<Child>>>,
    managed: bool,
    shutdown_blocker: Option<ShutdownBlocker>,
}

impl ShellJobPermit {
    pub(super) fn acquire(
        shared: &Arc<Shared>,
        detached: bool,
        shutdown_blocking: bool,
        shutdown_blocker: Option<ShutdownBlocker>,
    ) -> Option<Self> {
        let mut shutdown_blocker = if shutdown_blocking {
            Some(match shutdown_blocker {
                Some(blocker) => blocker,
                None => ShutdownBlocker::acquire(shared, detached)?,
            })
        } else {
            None
        };
        let mut inner = shared.inner.lock();
        if shared.stopping.load(Ordering::Acquire) && !detached
            || inner.jobs.active_shell_jobs >= MAX_SHELL_JOBS
        {
            drop(inner);
            drop(shutdown_blocker.take());
            return None;
        }
        inner.jobs.next_shell_job_token = inner.jobs.next_shell_job_token.wrapping_add(1).max(1);
        let token = inner.jobs.next_shell_job_token;
        let process = Arc::new(Mutex::new(None));
        let managed = !detached || shutdown_blocking;
        inner.jobs.active_shell_jobs += 1;
        if managed {
            inner.jobs.shell_jobs.insert(token, Arc::clone(&process));
        }
        drop(inner);
        Some(Self {
            shared: Arc::clone(shared),
            token,
            process,
            #[cfg(unix)]
            cancel: Arc::new(AtomicBool::new(false)),
            managed,
            shutdown_blocker,
        })
    }
}

impl Drop for ShellJobPermit {
    fn drop(&mut self) {
        let mut inner = self.shared.inner.lock();
        if self.managed {
            inner.jobs.shell_jobs.remove(&self.token);
        }
        inner.jobs.active_shell_jobs = inner.jobs.active_shell_jobs.saturating_sub(1);
        drop(inner);
        drop(self.shutdown_blocker.take());
    }
}

#[cfg(not(unix))]
pub(super) fn run_shell_job(
    command: &str,
    cwd: &Path,
    tmux: &str,
    environment: &[(RawText, Option<RawText>)],
    default_terminal: &str,
    zz_socket: &Path,
    startup_reentry: Option<&str>,
    tmux_shim: Option<&Path>,
    zz_executable: Option<&Path>,
    show_stderr: bool,
    job_process: &Mutex<Option<Child>>,
    stopping: &AtomicBool,
    detached: bool,
    started: Option<&mpsc::SyncSender<()>>,
) -> Result<ShellJobResult, ()> {
    let cwd = existing_job_working_directory(cwd);
    let mut process = shell_process(command);
    configure_shell_job_environment(
        &mut process,
        environment,
        default_terminal,
        startup_reentry.is_some(),
        tmux,
        zz_socket.as_os_str(),
        tmux_shim,
        zz_executable,
    );
    process
        .current_dir(&cwd)
        .env("PWD", cwd.as_os_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(if show_stderr {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    if let Some(startup_reentry) = startup_reentry {
        process.env(
            zz_daemon_client::STARTUP_REENTRY_ENVIRONMENT_VARIABLE,
            startup_reentry,
        );
    }
    let mut child = process.spawn_unmasked().map_err(|_| ())?;
    drop(child.stdin.take());
    let Some(mut stdout) = child.stdout.take() else {
        let _ = terminate_copy_pipe(&mut child);
        return Err(());
    };
    let stderr = child.stderr.take();
    install_shell_job_process(job_process, stopping, child, detached)?;
    if let Some(started) = started {
        let _ = started.send(());
    }
    let stdout = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    let stderr = stderr.map(|mut stderr| {
        thread::spawn(move || {
            let mut output = Vec::new();
            stderr.read_to_end(&mut output).map(|_| output)
        })
    });
    let status = wait_shell_job_process(job_process, || thread::sleep(COPY_PIPE_POLL_INTERVAL));
    let mut output = stdout.join().map_err(|_| ())?.map_err(|_| ())?;
    if let Some(stderr) = stderr {
        output.extend(stderr.join().map_err(|_| ())?.map_err(|_| ())?);
    }
    let status = status?;
    Ok(ShellJobResult { output, status })
}

#[cfg(not(unix))]
fn install_shell_job_process(
    process: &Mutex<Option<Child>>,
    stopping: &AtomicBool,
    mut child: Child,
    detached: bool,
) -> Result<(), ()> {
    let mut process = process.lock();
    if stopping.load(Ordering::Acquire) && !detached {
        drop(process);
        let _ = terminate_copy_pipe(&mut child);
        return Err(());
    }
    *process = Some(child);
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn try_reap_shell_job_child(child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
    loop {
        match child.try_wait() {
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

#[cfg(not(unix))]
fn reap_shell_job_process(process: &Mutex<Option<Child>>) -> Result<Option<ExitStatus>, ()> {
    let mut process = process.lock();
    let Some(child) = process.as_mut() else {
        return Err(());
    };
    match try_reap_shell_job_child(child) {
        Ok(Some(status)) => {
            process.take();
            Ok(Some(status))
        }
        Ok(None) => Ok(None),
        Err(_) => {
            let mut child = process.take().expect("shell job process was present");
            drop(process);
            let _ = terminate_copy_pipe(&mut child);
            Err(())
        }
    }
}

#[cfg(not(unix))]
fn wait_shell_job_process(
    process: &Mutex<Option<Child>>,
    mut pause: impl FnMut(),
) -> Result<ExitStatus, ()> {
    loop {
        if let Some(status) = reap_shell_job_process(process)? {
            return Ok(status);
        }
        pause();
    }
}

fn shell_job_output(command: &str, result: &ShellJobResult) -> (RawText, u8) {
    let mut output = RawText::from_bytes(result.output.clone());
    let (exit_code, message) = shell_exit(result.status).map_or_else(
        || {
            let exit_code = result
                .status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(1);
            let message = (exit_code != 0).then(|| format!("'{command}' returned {exit_code}"));
            (exit_code, message)
        },
        |signal| {
            let exit_code = u8::try_from(128_i32.saturating_add(signal)).unwrap_or(u8::MAX);
            (
                exit_code,
                Some(format!("'{command}' terminated by signal {signal}")),
            )
        },
    );
    if let Some(message) = message {
        append_inserted_output(&mut output, &message);
    }
    (output, exit_code)
}

#[cfg(unix)]
fn shell_exit(status: ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt as _;

    status.signal()
}

#[cfg(not(unix))]
fn shell_exit(_: ExitStatus) -> Option<i32> {
    None
}

fn selected_deferred_client(
    inner: &ServerState,
    context: &ExecutionContext,
    invoking_client: ClientId,
) -> Option<ClientId> {
    if context.has_no_client() {
        return None;
    }
    context
        .control_command_target()
        .map(|(client, _)| client)
        .or_else(|| {
            format_provenance_client(context, invoking_client)
                .and_then(|client| current_format_client(inner, client))
        })
}

pub(super) enum ShellJobEnvironmentTiming {
    CommandTime,
    LaunchTime {
        session_environment: Option<RetainedJobEnvironment>,
    },
}

#[derive(Clone, Copy)]
pub(super) struct ShellJobSpawnPolicy {
    #[cfg(not(unix))]
    wait_for_start: bool,
    shutdown_blocking: bool,
    detached: bool,
}

pub(super) fn run_shell_position_context_name(position: usize) -> String {
    position.to_string()
}

fn typed_inserted_command_source(argument: &RawText, command_block: bool) -> InsertedCommandSource {
    if command_block
        && let Some(body) = argument
            .as_bytes()
            .strip_prefix(b"{")
            .and_then(|rest| rest.strip_suffix(b"}"))
    {
        return InsertedCommandSource::Block(RawText::from_bytes(body));
    }
    InsertedCommandSource::String(argument.to_string())
}

fn select_if_shell_branch(
    branches: &[InsertedCommandSource],
    truthy: bool,
) -> Option<&InsertedCommandSource> {
    if truthy {
        branches.first()
    } else {
        branches.get(1)
    }
}

pub(super) fn shell_job_working_directory(
    inner: &ServerState,
    invoking_client: ClientId,
    context: &ExecutionContext,
    requested: Option<&str>,
) -> PathBuf {
    if let Some(requested) = requested {
        return PathBuf::from(requested);
    }
    let client = (!context.has_no_client())
        .then(|| format_provenance_client(context, invoking_client))
        .flatten();
    let attached_session = client.and_then(|client| client_attached_session(inner, client));
    inner
        .config
        .startup_source_client_working_directory
        .clone()
        .or_else(|| {
            attached_session
                .is_none()
                .then(|| {
                    client.and_then(|client| {
                        inner
                            .client(client)
                            .and_then(|c| c.working_directory.as_ref())
                            .cloned()
                    })
                })
                .flatten()
        })
        .or_else(|| {
            context
                .session
                .and_then(|session| inner.engine.state.session_working_directory(session))
                .map(Path::to_owned)
        })
        .or_else(|| {
            attached_session
                .and_then(|session| inner.engine.state.session_working_directory(session))
                .map(Path::to_owned)
        })
        .unwrap_or_else(default_job_working_directory)
}

pub(super) fn fallback_job_working_directory() -> PathBuf {
    home_directory()
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub(super) fn existing_job_working_directory(path: &Path) -> Cow<'_, Path> {
    if path.is_dir() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(fallback_job_working_directory())
    }
}

#[cfg(unix)]
pub(super) struct ShellJobProcess {
    pub(super) pid: u32,
    pub(super) cancel: Arc<AtomicBool>,
}

#[cfg(unix)]
type ShellFinish = Box<
    dyn FnOnce(
            &Arc<Shared>,
            &mut ExecutionContext,
            Option<&CommandQueueExecution>,
            Result<ShellJobResult, ()>,
        ) -> Result<Execution, DaemonError>
        + Send,
>;

#[cfg(unix)]
pub(super) struct ShellWait {
    continuation: cmdq::WaitContinuation,
    result: Arc<Mutex<Option<Result<ShellJobResult, ()>>>>,
    finish: ShellFinish,
}

#[cfg(unix)]
impl ShellWait {
    pub(super) fn new(
        shared: &Arc<Shared>,
        finish: impl FnOnce(
            &Arc<Shared>,
            &mut ExecutionContext,
            Option<&CommandQueueExecution>,
            Result<ShellJobResult, ()>,
        ) -> Result<Execution, DaemonError>
        + Send
        + 'static,
    ) -> (
        Self,
        impl FnOnce(Result<ShellJobResult, ()>) + Send + 'static,
    ) {
        let continuation = cmdq::WaitContinuation::new(
            shared
                .command_item
                .as_ref()
                .and_then(|item| item.lock().wait()),
            None,
        );
        let result = Arc::new(Mutex::new(None));
        let completed = continuation.clone();
        let output = Arc::clone(&result);
        let wake = shared.timer_tx.clone();
        (
            Self {
                continuation,
                result,
                finish: Box::new(finish),
            },
            move |result| {
                *output.lock() = Some(result);
                completed.complete();
                let _ = wake.send(timers::TimerInput::HookReady);
            },
        )
    }

    pub(super) fn finish(
        self,
        shared: &Arc<Shared>,
        context: &mut ExecutionContext,
        queue: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let result = self.result.lock().take().expect("completed shell job");
        (self.finish)(shared, context, queue, result)
    }
}

pub(super) struct ShellJobResult {
    pub(super) output: Vec<u8>,
    pub(super) status: ExitStatus,
}

#[derive(Clone, Copy)]
pub(super) struct RunShellRoute {
    client: ClientId,
    kind: ClientKind,
    control_target: Option<ClientId>,
    foreground: bool,
    target_requested: bool,
    target: Option<PaneId>,
}

pub(super) struct ParsedRunShellArgs {
    pub(super) background: bool,
    pub(super) command_mode: bool,
    show_stderr: bool,
    cwd: Option<String>,
    delay: Option<Duration>,
    pub(super) positive_delay: bool,
    target: Option<String>,
    pub(super) positional: Vec<String>,
    positional_start: usize,
}

pub(super) struct ParsedIfShellArgs {
    pub(super) background: bool,
    pub(super) format: bool,
    target: Option<String>,
    positional: Vec<String>,
    positional_start: usize,
}

#[cfg(test)]
pub(super) const RUN_SHELL_POSITION_CONTEXT_PATTERN: &str = "N";

pub(super) fn parse_run_shell_args(args: &[RawText]) -> Result<ParsedRunShellArgs, ServerError> {
    let parsed =
        parse_buffer_command_args("run-shell", args, &['c', 'd', 's', 't'], &['b', 'C', 'E'])?;
    let delay_value = parsed.value('d');
    let delay = delay_value.map(parse_shell_delay).transpose()?;
    let positive_delay = delay_value
        .and_then(parse_shell_delay_seconds)
        .is_some_and(|seconds| seconds > 0.0);
    let positional_start = args.len().saturating_sub(parsed.positional.len());
    Ok(ParsedRunShellArgs {
        background: parsed.has('b'),
        command_mode: parsed.has('C'),
        show_stderr: parsed.has('E'),
        cwd: parsed.value('c').map(str::to_owned),
        delay,
        positive_delay,
        target: parsed.value('t').map(str::to_owned),
        positional: parsed.positional,
        positional_start,
    })
}

pub(super) fn parse_if_shell_args(args: &[RawText]) -> Result<ParsedIfShellArgs, ServerError> {
    let parsed = parse_buffer_command_args("if-shell", args, &['t'], &['b', 'F'])?;
    let positional_start = args.len().saturating_sub(parsed.positional.len());
    Ok(ParsedIfShellArgs {
        background: parsed.has('b'),
        format: parsed.has('F'),
        target: parsed.value('t').map(str::to_owned),
        positional: parsed.positional,
        positional_start,
    })
}

pub(super) fn parse_shell_delay(value: &str) -> Result<Duration, ServerError> {
    let seconds = parse_shell_delay_seconds(value)
        .ok_or_else(|| ServerError::InvalidCommand(format!("invalid delay time: {value}")))?;
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| ServerError::InvalidCommand(format!("invalid delay time: {value}")))
}

fn parse_shell_delay_seconds(value: &str) -> Option<f64> {
    if value.is_empty() {
        return Some(0.0);
    }
    let value = value.trim_start_matches(char::is_whitespace);
    if value.is_empty() {
        return Some(0.0);
    }
    let (negative, unsigned) = value
        .strip_prefix('-')
        .map_or((false, value), |value| (true, value));
    let unsigned = unsigned.strip_prefix('+').unwrap_or(unsigned);
    let seconds = if let Some(hex) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()? as f64
    } else {
        value.parse::<f64>().ok()?
    };
    let seconds = if negative { -seconds } else { seconds };
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds)
}

pub(super) fn default_job_working_directory() -> PathBuf {
    home_directory().unwrap_or_else(|| PathBuf::from("/"))
}
