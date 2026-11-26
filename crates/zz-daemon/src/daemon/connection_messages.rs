use super::*;

pub(super) fn gui_client(
    inner: &ServerState,
    session: SessionId,
) -> Option<(ClientId, Arc<OutboundMailbox>)> {
    let clients = inner.attachment.attached.get(&session)?;
    let find = |desktop: bool| {
        clients.iter().find_map(|id| {
            let client = inner.client(*id)?;
            let subscriber = client.subscriber.as_ref()?;
            (client.kind == Some(ClientKind::Interactive) && (client.path_picker || !desktop))
                .then(|| (*id, Arc::clone(subscriber)))
        })
    };
    find(true).or_else(|| find(false))
}

#[cfg(windows)]
enum QueuedCommand {
    Request(CommandRequest),
    Exec(zz_protocol::ExecRequest),
}

#[cfg(windows)]
pub(super) fn handle_connection_message<S: TransportStream>(
    mut stream: S,
    shared: &Arc<Shared>,
    first_message: ProtocolMessage,
) -> Result<(), DaemonError> {
    let connection_started = diagnostic_timer();
    let mut inbound_frame = Vec::new();
    let (hello, compact_hello) = match first_message {
        ProtocolMessage::Hello(hello) => (hello.clone().into_client(), Some(hello)),
        ProtocolMessage::ClientHello(hello) => (hello, None),
        ProtocolMessage::Exec(request) => return exec::serve_exec(stream, shared, request),
        _ => {
            return Err(ServerError::InvalidCommand(
                "first protocol message must be ClientHello".to_owned(),
            )
            .into());
        }
    };
    if let Err(error) = validate_hello(&hello) {
        best_effort_error_reply(&mut stream, error.clone());
        log::info!(
            target: "zz_daemon::diagnostics::connection",
            "rejected client hello protocol={} revision={} server_protocol={PROTOCOL_VERSION}",
            hello.protocol_version,
            hello.revision,
        );
        return Err(error.into());
    }
    let startup_reentry_capability = format!(
        "{}{}",
        zz_daemon_client::STARTUP_REENTRY_CAPABILITY_PREFIX,
        shared.server_id
    );
    let startup_reentry = hello.kind == ClientKind::Command
        && hello.capabilities.contains(&startup_reentry_capability);
    if !startup_reentry && !shared.wait_for_startup() {
        return Ok(());
    }

    let outbound = OutboundMailbox::new();
    outbound.accept_client_capabilities(&hello.capabilities);
    let registration = if let Some(hello) = &compact_hello {
        shared
            .register_welcome(hello)
            .map(|(client, welcome)| (client, ProtocolMessage::Welcome(welcome)))
    } else {
        shared
            .register(
                hello.kind,
                hello.client_instance_id,
                hello.device_name.clone(),
                hello.color_scheme,
                hello.kind == ClientKind::Interactive
                    && hello
                        .capabilities
                        .iter()
                        .any(|capability| capability == ClientHello::CLIENT_TERMINAL_CAPABILITY),
                startup_reentry,
            )
            .map(|(client, hello)| (client, ProtocolMessage::ServerHello(Box::new(hello))))
    };
    let Some((client, greeting)) = registration else {
        best_effort_server_stopping_reply(&mut stream);
        return Ok(());
    };
    {
        let mut inner = shared.inner.lock();
        if hello.kind == ClientKind::Interactive
            && hello.capabilities.iter().any(|capability| {
                capability == ClientHello::CLIENT_NATIVE_TERMINAL_SEARCH_CAPABILITY
            })
        {
            inner.client_entry(client).native_terminal_search = true;
        }
        if hello.kind == ClientKind::Interactive
            && hello
                .capabilities
                .iter()
                .any(|capability| capability == ClientHello::CLIENT_NATIVE_CHOOSER_CAPABILITY)
        {
            inner.client_entry(client).native_chooser = true;
        }
        if hello.kind == ClientKind::Interactive
            && hello
                .capabilities
                .iter()
                .any(|capability| capability == ClientHello::CLIENT_PATH_PICKER_CAPABILITY)
        {
            inner.client_entry(client).path_picker = true;
        }
        let registered = inner.client_mut(client).expect("registered client");
        registered.origin = hello.origin;
        registered.nested = client_nested_fact(&hello.capabilities);
        registered.remote = client_remote_fact(&hello.capabilities);
        registered.utf8 = client_utf8_fact(&hello.capabilities);
        registered.exits_on_detach = attach::client_exits_on_detach_fact(&hello.capabilities);
        let features = client_features_fact(&hello.capabilities);
        registered.features = (features != 0).then_some(features);
        registered.tty = client_tty_fact(&hello.capabilities);
        registered.size = client_size_fact(&hello.capabilities);
        registered.cell_pixels = attach::client_cell_fact(&hello.capabilities);
        if let Some(viewport) = compact_hello.as_ref().and_then(|hello| hello.viewport) {
            registered.size = Some((viewport.columns, viewport.rows));
            registered.cell_pixels = Some((viewport.cell_width_px, viewport.cell_height_px));
        }
        registered.pid = Some(hello.process_id);
        registered.working_directory =
            client_working_directory_fact(hello.working_directory.as_ref());
        registered.environment = Some(client_environment_fact(&hello.environment));
    }
    shared.warm_terminfo(&hello.environment)?;
    let mut registration = ClientRegistrationGuard::new(shared, client);
    shared.client_lifecycle_hook("client-created", client);
    log::debug!(
        target: "zz_daemon::diagnostics::connection",
        "registered client={client} kind={:?} hello={hello:#?}",
        hello.kind,
    );
    let mut writer = stream.try_clone()?;
    let _ = writer.set_send_buffer_size(attach::MAX_BATCHED_WRITE_BYTES);
    #[cfg(unix)]
    if compact_hello.is_some() && hello.kind == ClientKind::Control {
        let socket = stream.receive_fd().ok();
        #[cfg(target_vendor = "apple")]
        let socket = socket
            .filter(|socket| rustix::net::sockopt::set_socket_nosigpipe(socket, true).is_ok());
        outbound.state.lock().quiet_socket = socket;
    }
    let writer_mailbox = Arc::clone(&outbound);
    let writer_shared = Arc::downgrade(shared);
    let writer_thread = attach::spawn_writer(&shared.connection_threads, move || {
        write_outbound(&mut writer, &writer_mailbox, &writer_shared, client);
    })
    .map_err(|error| DaemonError::Thread(error.to_string()))?;
    let mut writer_registration =
        ClientWriterRegistrationGuard::new(shared, client, Arc::clone(&outbound));
    if compact_hello.is_some() {
        let mut state = outbound.state.lock();
        state.attach_batch = true;
        state.ctrl_collecting = ControlCollection::Attach;
        state.terminals_held = true;
    }
    let _ = outbound.enqueue_reliable(&greeting);
    if matches!(hello.kind, ClientKind::Interactive | ClientKind::Control) {
        shared.subscribe(client, Arc::clone(&outbound));
    }
    if hello.kind == ClientKind::Control
        && hello
            .capabilities
            .iter()
            .any(|capability| capability == ClientHello::STARTUP_CONFIG_OWNER_CAPABILITY)
    {
        shared.try_deliver_startup_config_causes(client, &outbound, true);
    }

    let mut context = {
        let inner = shared.inner.lock();
        hello
            .origin
            .and_then(|pane| ExecutionContext::for_pane(&inner.engine.state, pane))
            .or_else(|| {
                let fallback = if matches!(hello.kind, ClientKind::Command | ClientKind::Control) {
                    inner.engine.state.most_recent_context()
                } else {
                    inner.engine.state.default_context()
                };
                fallback.map(|(session, window, pane)| {
                    ExecutionContext::new(Some(session), Some(window), Some(pane))
                })
            })
            .unwrap_or_default()
    };
    if let Some(hello) = &compact_hello {
        shared.initialize_compact(client, hello, &outbound, &mut context);
    }
    let command_queue_cancel = Arc::new(AtomicBool::new(false));
    let (command_sender, command_worker, mut context) =
        if matches!(hello.kind, ClientKind::Command | ClientKind::Control) {
            let (sender, receiver) = crossbeam_channel::unbounded::<QueuedCommand>();
            let worker_shared = Arc::clone(shared);
            let worker_outbound = Arc::clone(&outbound);
            let worker_cancel = Arc::clone(&command_queue_cancel);
            let worker_kind = hello.kind;
            let reader_context = (worker_kind == ClientKind::Control).then(|| context.clone());
            shared
                .command_queue_cancels
                .lock()
                .insert(client, Arc::clone(&command_queue_cancel));
            let worker = match thread::Builder::new()
                .name(format!("zz-client-command-{}", client.0))
                .spawn(move || {
                    let mut context = context;
                    let mut select = crossbeam_channel::Select::new();
                    select.recv(&receiver);
                    while let Ok(work) = select.select().recv(&receiver) {
                        if worker_cancel.load(Ordering::Acquire) {
                            continue;
                        }
                        let QueuedCommand::Request(CommandRequest {
                            request_id,
                            command,
                            prepared,
                        }) = work
                        else {
                            if let QueuedCommand::Exec(request) = work {
                                worker_shared.execute_compact_request(
                                    client,
                                    worker_kind,
                                    &mut context,
                                    request,
                                    &worker_outbound,
                                );
                            }
                            continue;
                        };
                        if worker_kind == ClientKind::Control {
                            sync_context_with_attachment(
                                &worker_shared.inner.lock(),
                                client,
                                &mut context,
                            );
                        }
                        let item = worker_shared.command_item(
                            (worker_kind == ClientKind::Control).then_some((client, request_id)),
                        );
                        let _ = item.execute_command_request_with_prepared_into(
                            &worker_outbound,
                            client,
                            worker_kind,
                            &mut context,
                            request_id,
                            &command,
                            prepared,
                        );
                    }
                }) {
                Ok(worker) => worker,
                Err(error) => {
                    shared.command_queue_cancels.lock().remove(&client);
                    outbound.close();
                    let _ = writer_thread.join();
                    return Err(DaemonError::Thread(error.to_string()));
                }
            };
            (Some(sender), Some(worker), reader_context)
        } else {
            (None, None, Some(context))
        };

    let mut path_list: Option<(u64, Arc<AtomicBool>)> = None;
    let path_list_turn = Arc::new(Mutex::new(()));
    let terminal_themes = Arc::new(Mutex::new(None));
    let mut stream = attach::inbound_reader(stream);
    let result = loop {
        let message = match read_protocol_message_into(&mut stream, &mut inbound_frame) {
            Ok(message) => message,
            Err(ProtocolError::Io(error))
                if matches!(
                    error.kind(),
                    ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
                ) =>
            {
                break Ok(());
            }
            Err(error) => break Err(error.into()),
        };
        let message_started = diagnostic_timer();
        log::trace!(
            target: "zz_daemon::diagnostics::connection",
            "message begin client={client} bytes={} frame_capacity={} message={:#?}",
            inbound_frame.len(),
            inbound_frame.capacity(),
            zz_daemon_client::TracedMessage(&message),
        );
        if shared.shutdown_pending.load(Ordering::Acquire)
            && !(hello.kind == ClientKind::Command
                && matches!(
                    message,
                    ProtocolMessage::CommandRequest(_)
                        | ProtocolMessage::PrepareCommandList { .. }
                        | ProtocolMessage::ClientFileResponse(_)
                ))
        {
            continue;
        }
        match message {
            ProtocolMessage::Exec(request) => {
                if let Some(sender) = &command_sender {
                    if sender.send(QueuedCommand::Exec(request)).is_err() {
                        break Ok(());
                    }
                } else if let Some(context) = context.as_mut() {
                    shared.execute_compact_request(client, hello.kind, context, request, &outbound);
                }
            }
            ProtocolMessage::TreeSync => {
                shared.send_compact_state(client, &outbound, true);
            }
            ProtocolMessage::ControlStdio => {
                let _ = outbound.enqueue_reliable(&ProtocolMessage::ControlStdioClosed);
            }
            ProtocolMessage::TtyInput { handoff }
            | ProtocolMessage::TtyInputRelease { handoff } => {
                let _ = outbound.enqueue_reliable(&ProtocolMessage::TtyInputClosed { handoff });
            }
            ProtocolMessage::GetKeyTables => {
                let tables = shared.inner.lock().engine.keys.snapshot();
                Shared::send_event(&outbound, EventPayload::KeyTablesChanged { tables });
            }
            ProtocolMessage::CommandRequest(CommandRequest {
                request_id,
                command,
                prepared,
            }) => {
                shared
                    .inner
                    .lock()
                    .attachment
                    .cold_bootstrap
                    .command(client);
                if command_sender.is_none()
                    && let Some(context) = context.as_mut()
                {
                    sync_context_with_attachment(&shared.inner.lock(), client, context);
                }
                if let Some(sender) = &command_sender {
                    if sender
                        .send(QueuedCommand::Request(CommandRequest {
                            request_id,
                            command,
                            prepared,
                        }))
                        .is_err()
                    {
                        break Err(DaemonError::Thread(
                            "command client worker stopped".to_owned(),
                        ));
                    }
                } else {
                    let _ = shared.execute_command_request_with_prepared_into(
                        &outbound,
                        client,
                        hello.kind,
                        context.as_mut().expect("interactive client owns context"),
                        request_id,
                        &command,
                        prepared,
                    );
                }
            }
            ProtocolMessage::ClientFileResponse(response) => {
                shared.complete_client_file(client, response);
            }
            ProtocolMessage::IdentityChallenge { nonce } => {
                if let Some(proof) = crate::tailnet::identity_proof(&nonce) {
                    let _ = outbound.enqueue_reliable(&proof);
                }
            }
            ProtocolMessage::HomeDirectoryRequest { request_id, users } => {
                let homes = shared.resolve_home_directories(&users);
                let _ = outbound.enqueue_reliable(&ProtocolMessage::HomeDirectoryResponse {
                    request_id,
                    homes,
                });
            }
            ProtocolMessage::TerminalThemesRequest { request_id } => {
                send_terminal_themes(&outbound, &terminal_themes, request_id);
            }
            ProtocolMessage::FileReadRequest {
                request_id,
                pane,
                path,
            } => {
                shared.start_file_read(client, hello.kind, (request_id, pane, path), &outbound);
            }
            ProtocolMessage::FileWriteRequest {
                request_id,
                pane,
                path,
                offset,
                last,
                bytes,
            } => {
                shared.receive_file_write(
                    client,
                    hello.kind,
                    &file_edit::FileWriteChunk {
                        request_id,
                        pane,
                        path,
                        offset,
                        last,
                        bytes,
                    },
                    &outbound,
                );
            }
            ProtocolMessage::AgentAttachFile {
                request_id,
                pane,
                name,
                bytes,
            } => {
                shared.save_attachment(
                    client,
                    hello.kind,
                    (request_id, pane, name, bytes),
                    &outbound,
                );
            }
            ProtocolMessage::PathListRequest {
                request_id,
                pane,
                dir,
            } => {
                if let Some((_, cancel)) = path_list.take() {
                    cancel.store(true, Ordering::Release);
                }
                let cancel = Arc::new(AtomicBool::new(false));
                path_list = Some((request_id, Arc::clone(&cancel)));
                shared.start_path_list(
                    client,
                    hello.kind,
                    (request_id, pane, dir),
                    &outbound,
                    (&cancel, &path_list_turn),
                );
            }
            ProtocolMessage::PathListCancel { request_id } => {
                if path_list
                    .as_ref()
                    .is_some_and(|(active, _)| *active == request_id)
                    && let Some((_, cancel)) = path_list.take()
                {
                    cancel.store(true, Ordering::Release);
                }
            }
            ProtocolMessage::EnvironmentRequest { request_id, names } => {
                let values = shared.resolve_environment(&names);
                let _ = outbound
                    .enqueue_reliable(&ProtocolMessage::EnvironmentResponse { request_id, values });
            }
            ProtocolMessage::PrepareCommandList {
                request_id,
                commands,
            } => {
                let commands = shared.prepare_command_list_for_request(client, commands);
                let _ = outbound.enqueue_reliable(&ProtocolMessage::PreparedCommandList {
                    request_id,
                    commands,
                });
            }
            ProtocolMessage::Attach { session } => {
                let Some(context) = context.as_mut() else {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand(
                                "command client cannot attach".to_owned(),
                            ),
                            output: RawText::default(),
                        },
                    ));
                    continue;
                };
                outbound.hold_terminals();
                match shared.attach_target(client, hello.kind, context, &session) {
                    Ok((session, snapshot)) => {
                        shared.clear_pending_committed_text(client);
                        shared.send_attached(client, &outbound, session, snapshot);
                        shared.publish_snapshot();
                        outbound.release_terminals();
                    }
                    Err(error) => {
                        outbound.release_terminals();
                        let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                            CommandResponse::Error {
                                request_id: 0,
                                error,
                                output: RawText::default(),
                            },
                        ));
                    }
                }
            }
            ProtocolMessage::Detach => {
                let control_session = (hello.kind == ClientKind::Control)
                    .then(|| {
                        shared.inner.lock().attachment.attached.iter().find_map(
                            |(session, clients)| clients.contains(&client).then_some(*session),
                        )
                    })
                    .flatten();
                shared.detach(client);
                if let Some(session) = control_session {
                    shared
                        .publish_to_client(client, EventPayload::detached_requested(session, None));
                }
            }
            ProtocolMessage::SetColorScheme(color_scheme) => {
                if hello.kind == ClientKind::Interactive {
                    shared.set_client_color_scheme(client, color_scheme);
                }
            }
            ProtocolMessage::SetConfigOverrides { entries } => {
                shared.set_config_overrides(client, hello.kind, &entries);
            }
            ProtocolMessage::SetTerminalPreview { enabled } => {
                shared.set_terminal_preview(client, hello.kind, enabled);
            }
            ProtocolMessage::ClientTerminalFeatures { features } => {
                shared.add_client_terminal_features(client, hello.kind, &features);
            }
            ProtocolMessage::ClientTerminalType { term_type } => {
                shared.set_client_terminal_type(client, hello.kind, &term_type);
            }
            ProtocolMessage::Input(input) => {
                if let Some(context) = context.as_mut() {
                    sync_context_with_attachment(&shared.inner.lock(), client, context);
                }
                if hello.kind != ClientKind::Interactive {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand(
                                "command client cannot send input".to_owned(),
                            ),
                            output: RawText::default(),
                        },
                    ));
                    continue;
                }
                let Some(context) = context.as_mut() else {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand(
                                "command client cannot send input".to_owned(),
                            ),
                            output: RawText::default(),
                        },
                    ));
                    continue;
                };
                if let Err(error) = shared.input(client, hello.kind, context, input) {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: daemon_server_error(error),
                            output: RawText::default(),
                        },
                    ));
                }
            }
            ProtocolMessage::GuiResponse(response) => {
                shared.complete_gui_request(client, response);
            }
            ProtocolMessage::BrowserScreenshot {
                request_id,
                last,
                png,
            } => {
                shared.receive_browser_screenshot(client, request_id, last, &png);
            }
            ProtocolMessage::Resync => shared.send_resync(client, &outbound),
            ProtocolMessage::RequestFull { pane } => {
                shared.request_full(client, pane, &outbound);
            }
            ProtocolMessage::HistoryRequest { pane, start, count } => {
                shared.send_history(client, pane, start, count, &outbound);
            }
            ProtocolMessage::PasteUploadBegin {
                upload_id,
                pane,
                purpose,
                extension,
                total_bytes,
            } => {
                shared.begin_paste_upload(
                    client,
                    hello.kind,
                    upload_id,
                    pane,
                    purpose,
                    extension,
                    total_bytes,
                );
            }
            ProtocolMessage::PasteUploadChunk { upload_id, bytes } => {
                shared.extend_paste_upload(client, upload_id, &bytes);
            }
            ProtocolMessage::FetchPastedImage { pane, number }
                if hello.kind == ClientKind::Interactive =>
            {
                shared.fetch_pasted_image(client, pane, number);
            }
            ProtocolMessage::FetchArtifact {
                request_id,
                token,
                path,
            } if hello.kind == ClientKind::Interactive => {
                shared.fetch_artifact(client, request_id, &token, &path);
            }
            ProtocolMessage::ArtifactReply { token, text }
                if hello.kind == ClientKind::Interactive =>
            {
                shared.reply_from_artifact(client, &token, &text);
            }
            ProtocolMessage::AgentLogin(request) if hello.kind == ClientKind::Interactive => {
                if shared.inner.lock().attachment.client_flags.contains(client) {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand("client is read-only".to_owned()),
                            output: RawText::default(),
                        },
                    ));
                } else {
                    shared.handle_agent_login(request);
                }
            }
            message @ (ProtocolMessage::AgentPrompt { .. }
            | ProtocolMessage::AgentCancel { .. }
            | ProtocolMessage::AgentUnqueue { .. }
            | ProtocolMessage::AgentRespondPermission { .. }
            | ProtocolMessage::AgentSetConfigOption { .. }
            | ProtocolMessage::AgentSetMode { .. }
            | ProtocolMessage::AgentAuthenticate { .. }
            | ProtocolMessage::AgentSessionOp { .. }
            | ProtocolMessage::AgentReplay { .. }
            | ProtocolMessage::AgentUnwatch { .. }
            | ProtocolMessage::AgentAcknowledgePromptRestore { .. }
            | ProtocolMessage::AgentAnswerQuestion { .. }
            | ProtocolMessage::AgentStopTask { .. }
            | ProtocolMessage::AgentSteer { .. }) => {
                let read_only_blocked =
                    !matches!(
                        message,
                        ProtocolMessage::AgentReplay { .. }
                            | ProtocolMessage::AgentUnwatch { .. }
                            | ProtocolMessage::AgentAcknowledgePromptRestore { .. }
                    ) && shared.inner.lock().attachment.client_flags.contains(client);
                if read_only_blocked {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand("client is read-only".to_owned()),
                            output: RawText::default(),
                        },
                    ));
                } else if let Err(error) = handle_agent_message(shared, client, message) {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error,
                            output: RawText::default(),
                        },
                    ));
                }
            }
            _ => {}
        }
        log::trace!(
            target: "zz_daemon::diagnostics::connection",
            "message end client={client} elapsed_us={} context={context:#?}",
            diagnostic_elapsed_us(message_started),
        );
    };

    command_queue_cancel.store(true, Ordering::Release);
    if let Some((_, cancel)) = path_list.take() {
        cancel.store(true, Ordering::Release);
    }
    shared.detach(client);
    registration.unregister();
    drop(command_sender);
    if command_worker.is_some_and(|worker| worker.join().is_err()) {
        log::error!(
            target: "zz_daemon::diagnostics::connection",
            "command worker panicked for client={client}",
        );
    }
    shared.command_queue_cancels.lock().remove(&client);
    outbound.close();
    if writer_thread.join().is_err() {
        log::error!(
            target: "zz_daemon::diagnostics::connection",
            "writer thread panicked for client={client}",
        );
    }
    writer_registration.unregister();
    log::debug!(
        target: "zz_daemon::diagnostics::connection",
        "unregistered client={client} success={} connection_elapsed_us={}",
        result.is_ok(),
        diagnostic_elapsed_us(connection_started),
    );
    result
}

#[cfg(feature = "agent")]
pub(super) fn handle_agent_message(
    shared: &Arc<Shared>,
    client: ClientId,
    message: ProtocolMessage,
) -> Result<(), ServerError> {
    shared.agent_message(client, message)
}

#[cfg(not(feature = "agent"))]
pub(super) fn handle_agent_message(
    _shared: &Arc<Shared>,
    _client: ClientId,
    _message: ProtocolMessage,
) -> Result<(), ServerError> {
    Err(ServerError::UnsupportedCommand(
        "this daemon was built without agent support".to_owned(),
    ))
}

#[cfg(windows)]
pub(super) fn best_effort_protocol_mismatch_reply(stream: &mut impl Write, client: u16) {
    best_effort_error_reply(
        stream,
        ServerError::ProtocolMismatch {
            client,
            server: PROTOCOL_VERSION,
        },
    );
}

#[cfg(windows)]
fn best_effort_error_reply(stream: &mut impl Write, error: ServerError) {
    let message = ProtocolMessage::CommandResponse(CommandResponse::Error {
        request_id: 0,
        error,
        output: RawText::default(),
    });
    let mut frame = Vec::new();
    if encode_protocol_message_into(&message, &mut frame).is_ok() {
        let _ = stream.write_all(&frame).and_then(|()| stream.flush());
    }
}

pub(super) fn server_stopping_response(request_id: u64) -> ProtocolMessage {
    ProtocolMessage::CommandResponse(CommandResponse::Error {
        request_id,
        error: ServerError::InvalidCommand("server exited unexpectedly".to_owned()),
        output: RawText::default(),
    })
}

#[cfg(windows)]
pub(super) fn best_effort_server_stopping_reply(stream: &mut impl Write) {
    let mut frame = Vec::new();
    if encode_protocol_message_into(&server_stopping_response(0), &mut frame).is_ok() {
        let _ = stream.write_all(&frame).and_then(|()| stream.flush());
    }
}

#[cfg(all(unix, test))]
pub(super) fn write_outbound(
    stream: &mut impl TransportStream,
    outbound: &OutboundMailbox,
    shared: &Weak<Shared>,
    client: ClientId,
) {
    event_loop::write_fixture(stream, outbound, shared, client);
}

#[cfg(windows)]
pub(super) fn write_outbound(
    stream: &mut impl TransportStream,
    outbound: &OutboundMailbox,
    shared: &Weak<Shared>,
    client: ClientId,
) {
    let _finished = OutboundWriterGuard(outbound);
    let mut batch = Vec::new();
    loop {
        if !outbound.recv_batch(&mut batch, attach::MAX_BATCHED_WRITE_BYTES) {
            break;
        }
        let started = diagnostic_timer();
        let frames = batch.len();
        let bytes = batch.iter().map(OutboundFrame::len).sum::<usize>();
        let result = attach::write_frames(stream, &batch);
        log::trace!(
            target: "zz_daemon::diagnostics::outbound",
            "write bytes={bytes} frames={frames} success={} elapsed_us={}",
            result.is_ok(),
            diagnostic_elapsed_us(started),
        );
        if result.is_err() {
            outbound.close();
            break;
        }
        outbound.finish_batch(&mut batch);
        if let Some(shared) = shared.upgrade() {
            shared.send_preview_refreshes(client, outbound);
        }
    }
    let _ = stream.shutdown();
}

pub(super) fn validate_hello(hello: &ClientHello) -> Result<(), ServerError> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(ServerError::ProtocolMismatch {
            client: hello.protocol_version,
            server: PROTOCOL_VERSION,
        });
    }
    if hello.revision < zz_protocol::MIN_CLIENT_REVISION {
        return Err(ServerError::ClientTooOld {
            client: hello.revision,
            minimum: zz_protocol::MIN_CLIENT_REVISION,
        });
    }
    Ok(())
}

pub(super) fn daemon_server_error(error: DaemonError) -> ServerError {
    match error {
        DaemonError::Server(error) => error,
        DaemonError::InsertedCommandParse(message) => ServerError::CommandParse(message),
        DaemonError::CommandFailed { error, .. } => daemon_server_error(*error),
        DaemonError::CommandExit { exit_code, .. }
        | DaemonError::ReportedCommandExit { exit_code, .. } => {
            ServerError::Internal(format!("command exited with status {exit_code}"))
        }
        other => ServerError::Internal(other.to_string()),
    }
}

pub(super) fn post_admission_callback_error(error: DaemonError) -> DaemonError {
    match error {
        error @ (DaemonError::CommandExit { .. } | DaemonError::ReportedCommandExit { .. }) => {
            error
        }
        DaemonError::CommandFailed { output, error } => DaemonError::CommandFailed {
            output,
            error: Box::new(DaemonError::Server(ServerError::PostAdmissionCallback(
                Box::new(daemon_server_error(*error)),
            ))),
        },
        error => DaemonError::Server(ServerError::PostAdmissionCallback(Box::new(
            daemon_server_error(error),
        ))),
    }
}
