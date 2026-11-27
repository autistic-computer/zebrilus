use super::*;

pub(super) type Completion = Box<dyn FnOnce(&Arc<Shared>, Result<Vec<u8>, DaemonError>) + Send>;
pub(super) type Resume = Box<
    dyn FnOnce(
            &Arc<Shared>,
            &mut ExecutionContext,
            &CommandQueueExecution,
        ) -> Result<Execution, DaemonError>
        + Send,
>;

#[allow(clippy::too_many_arguments)]
pub(super) fn load_buffer(
    shared: &Arc<Shared>,
    client: Option<ClientId>,
    context: &ExecutionContext,
    path: &Path,
    name: Option<String>,
    target: Option<String>,
    clipboard: bool,
) -> Result<Execution, DaemonError> {
    let wait = terminal_requests::CommandWait::new(shared);
    let state = wait.start();
    let failed = Arc::clone(&state);
    let no_hooks = context.no_hooks;
    let context = context.clone();
    let submitted = shared.client_file_operation(
        client,
        path,
        ClientFileOperation::Read,
        None,
        Box::new(move |shared, result| {
            let result = result.and_then(|data| {
                validate_paste_buffer_size(data.len())?;
                if data.is_empty() {
                    return Ok(Execution::default());
                }
                if let Some(name) = &name {
                    validate_paste_buffer_name(name)?;
                }
                let copied = clipboard.then(|| data.clone());
                let events = insert_paste_buffer(
                    &mut shared.inner.lock(),
                    name.as_deref(),
                    "buffer",
                    data,
                    true,
                )?;
                if let Some(data) = copied {
                    shared.deliver_buffer_clipboard_write(client, target.as_deref(), &data);
                }
                if !no_hooks {
                    shared.run_event_hooks(events);
                }
                shared.refresh_choose_buffers();
                Ok(Execution::default())
            });
            state.resolve(shared.settle_client_file(client, &context, result));
        }),
    );
    if let Err(error) = submitted {
        failed.resolve(Err(error));
    }
    wait.finish(shared, Execution::default())
}

pub(super) fn save_buffer(
    shared: &Arc<Shared>,
    client: Option<ClientId>,
    context: &ExecutionContext,
    path: &Path,
    data: &Arc<[u8]>,
    append: bool,
) -> Result<Execution, DaemonError> {
    let wait = terminal_requests::CommandWait::new(shared);
    let state = wait.start();
    let failed = Arc::clone(&state);
    let context = context.clone();
    if let Err(error) = shared.client_file_operation(
        client,
        path,
        ClientFileOperation::Write {
            append,
            data: data.to_vec(),
        },
        None,
        Box::new(move |shared, result| {
            let result = result.map(|_| Execution::default());
            state.resolve(shared.settle_client_file(client, &context, result));
        }),
    ) {
        failed.resolve(Err(error));
    }
    wait.finish(shared, Execution::default())
}

pub(super) fn read_stdin(
    shared: &Arc<Shared>,
    client: ClientId,
    sink: CommandStdinSink,
    command: &CommandInvocation,
) -> Result<(), DaemonError> {
    let wait = terminal_requests::CommandWait::new(shared);
    let state = wait.start();
    let failed = Arc::clone(&state);
    let load_buffer = canonical_command(&command.name) == "load-buffer";
    if let Some(item) = &shared.command_item
        && let Some(wait) = item.lock().pending_wait.as_mut()
    {
        wait.file = Some(Box::new(|_, _, _| Ok(Execution::default())));
    }
    if let Err(error) = shared.client_file_operation(
        Some(client),
        Path::new("-"),
        ClientFileOperation::ReadStdin {
            binary: sink.accepts_binary(),
        },
        None,
        Box::new(move |shared, result| {
            let result = result
                .or_else(|error| {
                    if (sink == CommandStdinSink::Config || load_buffer)
                        && caller_stdin_read_failure(&daemon_error_text(&error))
                    {
                        if let Some(streams) = shared
                            .inner
                            .lock()
                            .client_mut(client)
                            .and_then(|c| c.command_streams.as_mut())
                        {
                            streams.stdin_error = Some(daemon_error_text(&error));
                        }
                        Ok(Vec::new())
                    } else {
                        Err(error)
                    }
                })
                .map(|bytes| {
                    if let Some(streams) = shared
                        .inner
                        .lock()
                        .client_mut(client)
                        .and_then(|c| c.command_streams.as_mut())
                    {
                        streams.stdin = Some(if streams.stdin_error.is_some() {
                            SourceStream::Spent
                        } else {
                            SourceStream::Bytes(RawText::from_bytes(bytes))
                        });
                    }
                    Execution::default()
                });
            state.resolve(result);
        }),
    ) {
        failed.resolve(Err(error));
    }
    wait.finish(shared, Execution::default()).map(|_| ())
}

pub(super) fn stream_stdin(shared: &Arc<Shared>, client: ClientId, pane: PaneId) {
    let wait = terminal_requests::CommandWait::new(shared);
    chunk(shared, client, pane, wait.start());
    let _ = wait.finish(shared, Execution::default());
}

fn chunk(
    shared: &Arc<Shared>,
    client: ClientId,
    pane: PaneId,
    state: Arc<terminal_requests::CommandState>,
) {
    if shared.pane_stream_target_lost(pane) {
        shared.request_command_client_exit(client);
        state.resolve(Ok(Execution::default()));
        return;
    }
    let failed = Arc::clone(&state);
    if let Err(error) = shared.client_file_operation(
        Some(client),
        Path::new("-"),
        ClientFileOperation::ReadStdinChunk,
        Some(pane),
        Box::new(move |shared, result| {
            if shared.pane_stream_target_lost(pane) {
                shared.request_command_client_exit(client);
                state.resolve(Ok(Execution::default()));
            } else if let Ok(bytes) = result
                && !bytes.is_empty()
                && shared.feed_pane_stream_input(pane, &bytes)
            {
                chunk(shared, client, pane, state);
            } else {
                state.resolve(Ok(Execution::default()));
            }
        }),
    ) {
        failed.resolve(Err(error));
    }
}

pub(super) fn root(
    context: ExecutionContext,
    command: CommandInvocation,
    execution: Box<CommandQueueExecution>,
    terminal: ClientTerminal,
    mux_source: MuxOptionSource,
) -> InsertedQueueFrame<Box<CommandQueueExecution>> {
    execution.frame_active.set(true);
    InsertedQueueFrame {
        request_root: true,
        request_result: None,
        wait_boundary: None,
        execution,
        context,
        commands: vec![command].into_iter(),
        prepared: true,
        control_target: None,
        mux_source,
        alias_terminal: Some(terminal),
        stdin: None,
        carried_a_stream: false,
        label: "<config-command>".to_owned(),
        result: InsertedCommandResult::default(),
        stdout_claim: StdoutClaim::None,
        first_error: None,
        failed_group: None,
        boundary: None,
        terminal_error: None,
        parked_boundary: None,
        hook: None,
        events: None,
    }
}

impl Shared {
    pub(super) fn read_file(args: &[RawText]) -> Result<Execution, DaemonError> {
        use std::io::{Read as _, Seek as _};
        let usage =
            || ServerError::CommandParse("usage: read-file [-s OFFSET] [-n BYTES] PATH".to_owned());
        let number = |value: &str| value.parse::<u64>().map_err(|_| usage());
        let (mut offset, mut length, mut path) = (0, FILE_CHUNK_BYTES, None);
        let mut index = 0;
        while let Some(argument) = args.get(index) {
            if let Some(value) = option_value("read-file", args, index, &["-s"])? {
                offset = number(&value.value)?;
                index += value.consumed;
            } else if let Some(value) = option_value("read-file", args, index, &["-n"])? {
                length = number(&value.value)?.min(FILE_CHUNK_BYTES);
                index += value.consumed;
            } else if argument == "--" && path.is_none() && index + 2 == args.len() {
                index += 1;
            } else if path.is_none() {
                path = Some(file_path("read-file", argument.as_str())?);
                index += 1;
            } else {
                return Err(usage().into());
            }
        }
        let path = path.ok_or_else(usage)?;
        let failed = |error: std::io::Error| {
            ServerError::InvalidCommand(format!("read-file: {}: {error}", path.display()))
        };
        let mut file = std::fs::File::open(&path).map_err(failed)?;
        if !file.metadata().map_err(failed)?.is_file() {
            return Err(ServerError::InvalidCommand(format!(
                "read-file: {} is not a file",
                path.display()
            ))
            .into());
        }
        file.seek(std::io::SeekFrom::Start(offset))
            .map_err(failed)?;
        let mut bytes = Vec::new();
        file.take(length).read_to_end(&mut bytes).map_err(failed)?;
        Ok(Execution {
            output: zz_protocol::file_hex::encode(&bytes).into(),
            effects: Vec::new(),
        })
    }

    pub(super) fn write_file(args: &[RawText]) -> Result<Execution, DaemonError> {
        use std::io::Write as _;
        let usage = || ServerError::CommandParse("usage: write-file [-a] PATH HEX".to_owned());
        let (append, rest) = match args {
            [flag, rest @ ..] if flag == "-a" => (true, rest),
            rest => (false, rest),
        };
        let rest = match rest {
            [dashes, rest @ ..] if dashes == "--" => rest,
            rest => rest,
        };
        let [path, hex] = rest else {
            return Err(usage().into());
        };
        let path = file_path("write-file", path.as_str())?;
        let bytes = zz_protocol::file_hex::decode(hex.as_str()).ok_or_else(usage)?;
        let failed = |error: std::io::Error| {
            ServerError::InvalidCommand(format!("write-file: {}: {error}", path.display()))
        };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&path)
            .map_err(failed)?;
        file.write_all(&bytes).map_err(failed)?;
        Ok(Execution::default())
    }

    fn settle_client_file(
        self: &Arc<Self>,
        client: Option<ClientId>,
        context: &ExecutionContext,
        result: Result<Execution, DaemonError>,
    ) -> Result<Execution, DaemonError> {
        let error = match result {
            Ok(execution) => return Ok(execution),
            Err(error) => error,
        };
        let Some((client, kind)) = client.and_then(|client| {
            self.read_client(client, |c| c.and_then(|c| c.kind))
                .map(|kind| (client, kind))
        }) else {
            return Err(error);
        };
        match kind {
            ClientKind::Interactive => {
                self.publish_background_command_error(client, context, &error, true);
            }
            ClientKind::Command | ClientKind::Control => {
                self.route_source_error(client, kind, context.pane, &daemon_error_text(&error));
            }
        }
        Ok(Execution::default())
    }

    fn client_file_operation(
        self: &Arc<Self>,
        client: Option<ClientId>,
        path: &Path,
        operation: ClientFileOperation,
        pane: Option<PaneId>,
        complete: file_commands::Completion,
    ) -> Result<(), DaemonError> {
        let client = client.filter(|client| {
            self.read_client(*client, |c| c.and_then(|c| c.kind)) == Some(ClientKind::Command)
        });
        let Some(client) = client else {
            return self
                .submit_helper(helpers::Task::File {
                    path: path.to_owned(),
                    operation,
                    complete,
                })
                .map_err(Into::into);
        };
        #[cfg(windows)]
        self.go_live_exec(client);
        let writer = self.client_writers.lock().get(&client).cloned();
        let Some(writer) = writer else {
            return self
                .submit_helper(helpers::Task::File {
                    path: path.to_owned(),
                    operation,
                    complete,
                })
                .map_err(Into::into);
        };
        let request_id = self.insert_client_file_waiter(ClientFileWaiter {
            client,
            path: path.to_owned(),
            pane,
            complete,
        })?;
        if !writer.enqueue_reliable(&ProtocolMessage::ClientFileRequest(ClientFileRequest {
            request_id,
            path: path.to_string_lossy().into_owned(),
            operation,
        })) {
            self.complete_client_file(
                client,
                ClientFileResponse {
                    request_id,
                    data: Vec::new(),
                    error: Some("the invoking client is gone".to_owned()),
                },
            );
        }
        Ok(())
    }

    pub(super) fn insert_client_file_waiter(
        &self,
        waiter: ClientFileWaiter,
    ) -> Result<u64, DaemonError> {
        let mut inner = self.inner.lock();
        if !inner.clients.contains_key(&waiter.client) {
            return Err(client_file_failure("the invoking client is gone", &waiter.path).into());
        }
        inner.transfers.next_client_file_request = inner
            .transfers
            .next_client_file_request
            .wrapping_add(1)
            .max(1);
        let request_id = inner.transfers.next_client_file_request;
        inner
            .transfers
            .client_file_waiters
            .insert(request_id, waiter);
        Ok(request_id)
    }

    pub(super) fn cancel_pane_files(self: &Arc<Self>, pane: PaneId) {
        let files = self
            .inner
            .lock()
            .transfers
            .client_file_waiters
            .extract_if(.., |_, waiter| waiter.pane == Some(pane))
            .map(|(_, waiter)| waiter)
            .collect::<Vec<_>>();
        for waiter in files {
            (waiter.complete)(self, Err(ServerError::PaneExited(pane).into()));
        }
    }

    pub(super) fn complete_client_file(
        self: &Arc<Self>,
        client: ClientId,
        response: ClientFileResponse,
    ) {
        let waiter = {
            let mut inner = self.inner.lock();
            if inner
                .transfers
                .client_file_waiters
                .get(&response.request_id)
                .is_none_or(|waiter| waiter.client != client)
            {
                return;
            }
            inner
                .transfers
                .client_file_waiters
                .remove(&response.request_id)
                .unwrap()
        };
        let result = response.error.map_or(Ok(response.data), |error| {
            Err(client_file_failure(&error, &waiter.path).into())
        });
        (waiter.complete)(self, result);
        self.accept_wake.wake();
    }
}

const FILE_CHUNK_BYTES: u64 = zz_protocol::file_hex::CHUNK_BYTES as u64;

fn file_path(verb: &str, path: &str) -> Result<PathBuf, ServerError> {
    let expanded = expand_path(path);
    if expanded.is_absolute() {
        Ok(expanded)
    } else {
        Err(ServerError::InvalidCommand(format!(
            "{verb}: {path} must be an absolute path or start with ~/"
        )))
    }
}
