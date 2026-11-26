use super::*;

impl Shared {
    pub(super) fn server_owner(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self.owner.as_ref().unwrap_or(self))
    }

    pub(super) fn command_item(self: &Arc<Self>, park: Option<(ClientId, u64)>) -> Arc<Self> {
        Arc::new(Self {
            server: Arc::clone(&self.server),
            command_item: Some(Mutex::new(cmdq::CommandItem::new(
                None,
                CommandItemContext {
                    park,
                    ..CommandItemContext::default()
                },
            ))),
            owner: Some(self.server_owner()),
            watcher_effects: None,
        })
    }

    pub(super) fn command_queue_execution(
        &self,
        state: CommandExecutionState,
    ) -> CommandQueueExecution {
        let queue = self.command_item.as_ref().map(|item| item.lock().queue);
        CommandQueueExecution {
            item: cmdq::CommandItem::new(queue, state),
        }
    }

    pub(super) fn execution_item(self: &Arc<Self>) -> Arc<Self> {
        if self.command_item.is_some() {
            Arc::clone(self)
        } else {
            self.command_item(None)
        }
    }

    #[cfg(test)]
    pub(super) fn execute_command_request(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        request_id: u64,
        command: &CommandInvocation,
    ) -> CommandResponse {
        self.execute_command_request_with_prepared(
            client, kind, context, request_id, command, false,
        )
    }

    pub(super) fn execute_command_request_with_prepared(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        request_id: u64,
        command: &CommandInvocation,
        prepared: bool,
    ) -> CommandResponse {
        self.execute_command_request_with_streams(
            client, kind, context, request_id, command, prepared,
        )
        .0
    }

    pub(super) fn execute_command_request_with_streams(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        request_id: u64,
        command: &CommandInvocation,
        prepared: bool,
    ) -> (CommandResponse, bool) {
        self.execution_item()
            .execute_command_request_with_streams_in_item(
                client, kind, context, request_id, command, prepared,
            )
    }

    fn execute_command_request_with_streams_in_item(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        request_id: u64,
        command: &CommandInvocation,
        prepared: bool,
    ) -> (CommandResponse, bool) {
        {
            let item = self.command_item.as_ref().expect("command item").lock();
            if item.state() == cmdq::State::Done {
                return item.result.clone().expect("completed command result");
            }
        }
        let result = self
            .execute_command_request_segment(client, kind, context, request_id, command, prepared);
        let mut item = self.command_item.as_ref().expect("command item").lock();
        item.result = Some(result.clone());
        item.finish();
        result
    }

    fn execute_command_request_segment(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        request_id: u64,
        command: &CommandInvocation,
        prepared: bool,
    ) -> (CommandResponse, bool) {
        let stdin_available = kind == ClientKind::Command && command.stdin_available();
        let (command, client_name) = {
            let mut inner = self.inner.lock();
            let refusal = match prepare_command_request(&mut inner, client, command, prepared) {
                Ok((command, false)) => Ok(command),
                Ok((_, true)) => Err(ServerError::InvalidCommand(
                    "client is read-only".to_owned(),
                )),
                Err(error) => Err(error),
            };
            let command = match refusal {
                Ok(command) => command,
                Err(error) => {
                    return (
                        CommandResponse::Error {
                            request_id,
                            error,
                            output: RawText::default(),
                        },
                        false,
                    );
                }
            };
            let client_name = server_log_client_name(&inner, client);
            let command_line = command_log_line(&command);
            push_server_message(&mut inner, format!("{client_name} command: {command_line}"));
            if kind == ClientKind::Command
                || kind == ClientKind::Control && command.source.is_none()
            {
                inner
                    .client_entry(client)
                    .command_streams
                    .replace(CommandStreams {
                        stdin_available,
                        stdin: (kind == ClientKind::Command)
                            .then(|| command.stdin().cloned().map(SourceStream::Bytes))
                            .flatten(),
                        ..CommandStreams::default()
                    });
            }
            (command, client_name)
        };
        let previous_control_target = context.control_command_target();
        if kind == ClientKind::Control {
            let flags = if command.source.is_none() {
                CONTROL_COMMAND_FRAME_FLAGS_NONE
            } else {
                CONTROL_COMMAND_FRAME_FLAGS_CONTROL
            };
            context.set_control_command_target(Some((client, flags)));
        }
        let defer_notifications = kind == ClientKind::Control;
        let execution = {
            let _deferral = defer_notifications.then(|| self.defer_control_notifications());
            let execution = self.execute_with_mux_source_routed(
                client,
                kind,
                context,
                &command,
                MuxOptionSource::RuntimeCommand,
            );
            if defer_notifications {
                self.publish_deferred_control_notifications();
            }
            execution
        };
        context.set_control_command_target(previous_control_target);
        self.finish_command_request_segment(
            client,
            kind,
            context,
            request_id,
            &command,
            &client_name,
            execution,
        )
    }

    pub(super) fn finish_command_request_segment(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        request_id: u64,
        command: &CommandInvocation,
        client_name: &str,
        execution: Result<Execution, DaemonError>,
    ) -> (CommandResponse, bool) {
        let response = match execution {
            Ok(execution) => CommandResponse::Success {
                request_id,
                output: execution.output,
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
            Err(
                DaemonError::CommandExit { output, exit_code }
                | DaemonError::ReportedCommandExit { output, exit_code },
            ) => CommandResponse::Success {
                request_id,
                output,
                exit_code,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
            Err(DaemonError::CommandFailed { output, error }) => {
                let error = daemon_server_error(*error);
                let message = server_error_text(&error);
                let mut inner = self.inner.lock();
                push_server_message(&mut inner, format!("{client_name} message: {message}"));
                CommandResponse::Error {
                    request_id,
                    error,
                    output,
                }
            }
            Err(error) => {
                let error = daemon_server_error(error);
                let message = server_error_text(&error);
                let mut inner = self.inner.lock();
                push_server_message(&mut inner, format!("{client_name} message: {message}"));
                CommandResponse::Error {
                    request_id,
                    error,
                    output: RawText::default(),
                }
            }
        };
        let (streams, sanitizes) = {
            let mut inner = self.inner.lock();
            (
                inner
                    .client_mut(client)
                    .and_then(|c| c.command_streams.take()),
                sanitizes_output_for(&inner, client, kind, &command.name),
            )
        };
        let client_exit = streams.as_ref().is_some_and(|streams| streams.client_exit);
        let recorded_claim = streams.as_ref().and_then(|streams| streams.stdout_claim);
        let mut response = match streams {
            Some(streams) if !streams.is_empty() => merge_command_streams(response, &streams),
            _ => response,
        };
        if recorded_claim != Some(StdoutClaim::Raw) && sanitizes {
            let output = match &mut response {
                CommandResponse::Success { output, .. } | CommandResponse::Error { output, .. } => {
                    output
                }
            };
            if !output.is_empty() {
                *output = sanitize_client_output(output);
            }
        }
        if let CommandResponse::Success {
            output,
            stdout_claim,
            ..
        } = &mut response
        {
            *stdout_claim =
                recorded_claim.unwrap_or_else(|| default_stdout_claim(&command.name, output));
        }
        let output = match &response {
            CommandResponse::Success { output, .. } | CommandResponse::Error { output, .. } => {
                output
            }
        };
        if kind == ClientKind::Interactive
            && !self.read_client(client, |client| client.is_some_and(|c| c.ctrl_initializing))
            && !output.is_empty()
            && !matches!(
                canonical_command(&command.name),
                "new-session" | "tailnet" | "html" | "select-child-chat"
            )
            && let Err(error) =
                self.open_command_output(client, context.pane, command.name.clone(), output)
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
        (response, client_exit)
    }

    #[cfg(test)]
    pub(super) fn sanitizes_output_for(
        &self,
        client: ClientId,
        kind: ClientKind,
        command: &str,
    ) -> bool {
        sanitizes_output_for(&self.inner.lock(), client, kind, command)
    }

    pub(super) fn execute_command_request_with_prepared_into(
        self: &Arc<Self>,
        outbound: &OutboundMailbox,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        request_id: u64,
        command: &CommandInvocation,
        prepared: bool,
    ) -> bool {
        let admission =
            if kind == ClientKind::Command && self.shutdown_pending.load(Ordering::Acquire) {
                None
            } else {
                ResponseAdmissionGuard::new(self)
            };
        let Some(mut admission) = admission else {
            if kind == ClientKind::Command {
                let _ = outbound.enqueue_reliable(&server_stopping_response(request_id));
            }
            return false;
        };
        let response = self.execute_command_request_with_prepared(
            client, kind, context, request_id, command, prepared,
        );
        #[cfg(test)]
        if let Some(hook) = self.response_admission_hook.lock().take() {
            let _ = hook.reached.send(());
            let _ = hook.release.recv();
        }
        let admitted = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(response));
        admission.finish();
        admitted
    }

    pub(super) fn wake_control_queue(&self, client: ClientId, kind: ClientKind) {
        if kind == ClientKind::Control
            && let Some(writer) = self.client_writers.lock().get(&client).cloned()
        {
            writer.release_control_query();
        }
    }

    pub(super) fn report_command_queue_park(&self) {
        hook_events::release_input_change_window(self);
        if let Some(item) = &self.command_item {
            item.lock().wait();
        }
        #[cfg(windows)]
        self.go_live_current_exec();
        #[cfg(unix)]
        if let Some(writer) = self
            .command_item
            .as_ref()
            .and_then(|item| item.lock().exec_writer.take())
            .and_then(|writer| writer.upgrade())
        {
            writer.notify_one();
        }
        let Some((client, request_id)) = self
            .command_item
            .as_ref()
            .and_then(|item| item.lock().park.take())
        else {
            return;
        };
        let Some(writer) = self.client_writers.lock().get(&client).cloned() else {
            return;
        };
        let _ = writer.enqueue_reliable(&ProtocolMessage::CommandQueueParked { request_id });
        writer.release_control_query();
    }

    pub(super) fn command_queue_cancelled(&self, client: ClientId) -> bool {
        self.command_queue_cancels
            .lock()
            .get(&client)
            .is_some_and(|cancel| cancel.load(Ordering::Acquire))
    }

    #[cfg(test)]
    pub(super) fn prepare_command_list(
        &self,
        commands: Vec<CommandInvocation>,
    ) -> Vec<PreparedCommand> {
        let inner = self.inner.lock();
        Self::prepare_command_list_with_engine(&inner.engine, commands, true)
    }

    pub(super) fn prepare_command_list_for_request(
        &self,
        client: ClientId,
        mut commands: Vec<CommandInvocation>,
    ) -> Vec<PreparedCommand> {
        let abort_server_id = commands.last().and_then(|command| {
            (command.name == zz_daemon_client::COLD_START_PREPARE_ABORT_COMMAND
                && command.args.len() == 1)
                .then(|| command.args[0].parse::<u64>().ok())
                .flatten()
        });
        if abort_server_id.is_some() {
            commands.pop();
        }
        let mut inner = self.inner.lock();
        let preflight_unaliased =
            inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Command);
        let commands =
            Self::prepare_command_list_with_engine(&inner.engine, commands, preflight_unaliased);
        if abort_server_id == Some(self.server_id) {
            let failed = commands
                .iter()
                .any(|command| matches!(&command.result, PreparedCommandResult::Error(_)));
            inner.attachment.cold_bootstrap.prepare(client, failed);
        }
        commands
    }

    pub(super) fn resolve_environment(&self, names: &[String]) -> Vec<Option<String>> {
        if names.is_empty() {
            return Vec::new();
        }
        let inner = self.inner.lock();
        names
            .iter()
            .map(|name| inner.engine.global_environment_variable(name))
            .collect()
    }

    pub(super) fn resolve_home_directories(&self, users: &[String]) -> Vec<Option<String>> {
        if users.is_empty() {
            return Vec::new();
        }
        let inner = self.inner.lock();
        users
            .iter()
            .map(|user| home_directory_for(&inner.engine, user))
            .collect()
    }

    pub(super) fn prepare_command_list_with_engine(
        engine: &MuxEngine,
        commands: Vec<CommandInvocation>,
        preflight_unaliased: bool,
    ) -> Vec<PreparedCommand> {
        commands
            .into_iter()
            .enumerate()
            .map(|(index, typed)| {
                let exact_native_attach =
                    index == 0 && matches!(typed.name.as_str(), "attach" | "attach-session");
                let (mut invocation, alias_matched, alias_error) =
                    match engine.resolve_command_alias(&typed) {
                        CommandAliasResolution::Miss => (typed, false, None),
                        CommandAliasResolution::Expanded(invocation) => (invocation, true, None),
                        CommandAliasResolution::MatchedUnsupported(_) => (
                            typed.clone(),
                            true,
                            Some(ServerError::CommandParse(format!(
                                "unknown command: {}",
                                typed.name
                            ))),
                        ),
                    };
                if let Some(error) = alias_error {
                    return PreparedCommand {
                        invocation,
                        canonical_name: None,
                        alias_matched,
                        result: PreparedCommandResult::Error(error),
                    };
                }
                let resolution = resolve_command(&invocation.name);
                let canonical_name = match &resolution {
                    CommandResolution::Canonical(name) | CommandResolution::Unimplemented(name) => {
                        Some((*name).to_owned())
                    }
                    CommandResolution::Ambiguous(_) | CommandResolution::Unknown => None,
                };
                let has_command_blocks = (0..invocation.args.len())
                    .any(|index| invocation.argument_is_command_block(index));
                let preflight_args = canonical_name
                    .as_deref()
                    .is_some_and(|name| COMMAND_ARGS_PARSE_BEHAVES.contains(&name));
                if has_command_blocks || preflight_args {
                    let owner = canonical_name
                        .clone()
                        .unwrap_or_else(|| invocation.name.clone());
                    if let Err(error) = engine.prepare_expanded_callback_command(
                        &mut invocation,
                        &owner,
                        alias_matched,
                    ) {
                        return PreparedCommand {
                            invocation,
                            canonical_name,
                            alias_matched,
                            result: PreparedCommandResult::Error(error),
                        };
                    }
                } else if (alias_matched || (preflight_unaliased && !exact_native_attach))
                    && let Err(error) =
                        validate_static_command_chain(std::slice::from_ref(&invocation))
                {
                    return PreparedCommand {
                        invocation,
                        canonical_name,
                        alias_matched,
                        result: PreparedCommandResult::Error(error),
                    };
                }
                if MuxEngine::is_command_alias_group(&invocation) {
                    return PreparedCommand {
                        invocation,
                        canonical_name: None,
                        alias_matched,
                        result: PreparedCommandResult::Ready,
                    };
                }
                match resolution {
                    CommandResolution::Canonical(_) | CommandResolution::Unimplemented(_) => {
                        PreparedCommand {
                            invocation,
                            canonical_name,
                            alias_matched,
                            result: PreparedCommandResult::Ready,
                        }
                    }
                    CommandResolution::Ambiguous(message) => PreparedCommand {
                        invocation,
                        canonical_name: None,
                        alias_matched,
                        result: PreparedCommandResult::Error(ServerError::CommandParse(message)),
                    },
                    CommandResolution::Unknown => PreparedCommand {
                        result: PreparedCommandResult::Error(ServerError::CommandParse(format!(
                            "unknown command: {}",
                            invocation.name
                        ))),
                        invocation,
                        canonical_name: None,
                        alias_matched,
                    },
                }
            })
            .collect()
    }
}

fn sanitizes_output_for(
    inner: &ServerState,
    client: ClientId,
    kind: ClientKind,
    command: &str,
) -> bool {
    if !matches!(kind, ClientKind::Command | ClientKind::Control) {
        return false;
    }
    if prints_stored_bytes(command) {
        return false;
    }
    !inner.client(client).is_some_and(|c| c.utf8)
}

fn merge_command_streams(response: CommandResponse, streams: &CommandStreams) -> CommandResponse {
    match response {
        CommandResponse::Success {
            request_id,
            mut output,
            exit_code,
            mut stderr,
            stdout_claim,
        } => {
            append_inserted_output(&mut output, &streams.stdout);
            if !streams.control_error.is_empty() {
                return CommandResponse::Error {
                    request_id,
                    error: ServerError::InvalidCommand(streams.control_error.clone()),
                    output,
                };
            }
            stderr.push_str(&streams.stderr);
            let exit_code = if exit_code == 0 {
                streams.exit_code
            } else {
                exit_code
            };
            CommandResponse::Success {
                request_id,
                output,
                exit_code,
                stderr,
                stdout_claim,
            }
        }
        CommandResponse::Error {
            request_id,
            error,
            mut output,
        } => {
            append_inserted_output(&mut output, &streams.stdout);
            append_inserted_output(&mut output, &streams.control_error);
            CommandResponse::Error {
                request_id,
                error,
                output,
            }
        }
    }
}
