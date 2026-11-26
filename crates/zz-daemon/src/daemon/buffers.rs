use super::*;

impl Shared {
    #[cfg(test)]
    pub(super) fn buffer_command(
        self: &Arc<Self>,
        context: &ExecutionContext,
        name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        self.buffer_command_for_client(
            None,
            ClientKind::Command,
            context,
            canonical_command(name),
            args,
        )
    }

    pub(super) fn buffer_command_for_client(
        self: &Arc<Self>,
        invoking_client: Option<ClientId>,
        kind: ClientKind,
        context: &ExecutionContext,
        name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        match name {
            "set-buffer" | "setb" => {
                let parsed = parse_buffer_command_args(name, args, &['b', 'n', 't'], &['a', 'w'])?;
                if let Some(new_name) = parsed.value('n') {
                    let events = {
                        let mut inner = self.inner.lock();
                        rename_paste_buffer(&mut inner, parsed.value('b'), new_name)?
                    };
                    if !context.no_hooks {
                        self.run_event_hooks(events);
                    }
                    self.refresh_choose_buffers();
                    return Ok(Execution::default());
                }
                let (1, Some(data)) = (parsed.positional.len(), args.last()) else {
                    return Err(ServerError::CommandParse(
                        "set-buffer requires exactly one data argument".to_owned(),
                    )
                    .into());
                };
                if data.is_empty() {
                    return Ok(Execution::default());
                }
                let requested_name = parsed.value('b');
                validate_paste_buffer_size(data.byte_len())?;
                if let Some(name) = requested_name {
                    validate_paste_buffer_name(name)?;
                }
                let mut data = data.as_bytes().to_vec();
                let mut inner = self.inner.lock();
                if parsed.has('a')
                    && let Some(name) = requested_name
                    && let Some(buffer) = inner
                        .buffers
                        .paste_buffers
                        .iter()
                        .find(|buffer| buffer.name == name)
                {
                    let combined = buffer.data.len().saturating_add(data.len());
                    validate_paste_buffer_size(combined)?;
                    let mut appended = Vec::with_capacity(combined);
                    appended.extend_from_slice(&buffer.data);
                    appended.append(&mut data);
                    data = appended;
                }
                let clipboard = parsed.has('w').then(|| data.clone());
                let events = insert_paste_buffer(
                    &mut inner,
                    requested_name,
                    "buffer",
                    data,
                    !parsed.has('a'),
                )?;
                drop(inner);
                if let Some(clipboard) = clipboard {
                    self.deliver_buffer_clipboard_write(
                        invoking_client,
                        parsed.value('t'),
                        &clipboard,
                    );
                }
                if !context.no_hooks {
                    self.run_event_hooks(events);
                }
                self.refresh_choose_buffers();
                Ok(Execution::default())
            }
            "show-buffer" | "showb" => {
                let parsed = parse_buffer_command_args(name, args, &['b'], &[])?;
                require_no_positionals(name, &parsed)?;
                let (data, control, utf8) = {
                    let inner = self.inner.lock();
                    let buffer =
                        resolve_buffer(&inner, parsed.value('b'), BufferMissing::NoBuffer)?;
                    let printer = invoking_client
                        .filter(|client| *client != ClientId(u64::MAX))
                        .map(|client| (client, kind))
                        .or_else(|| {
                            context
                                .control_command_target()
                                .map(|(client, _)| client)
                                .or_else(|| context.replay_client())
                                .map(|client| {
                                    let kind = inner.client(client).and_then(|c| c.kind);
                                    (client, kind.unwrap_or(ClientKind::Command))
                                })
                        });
                    let control = printer.is_some_and(|(_, kind)| kind == ClientKind::Control);
                    let utf8 = printer
                        .is_some_and(|(client, _)| inner.client(client).is_some_and(|c| c.utf8));
                    (Arc::clone(&buffer.data), control, utf8)
                };
                let output = if control {
                    let printed = data.split(|byte| *byte == 0).next().unwrap_or_default();
                    if utf8 {
                        RawText::from_bytes(printed)
                    } else {
                        RawText::from(utf8_sanitize(printed))
                    }
                } else {
                    RawText::from_bytes(data.as_ref())
                };
                Ok(Execution {
                    output,
                    effects: Vec::new(),
                })
            }
            "list-buffers" | "lsb" => {
                let parsed = parse_buffer_command_args(name, args, &['F', 'f', 'O'], &['r'])?;
                require_no_positionals(name, &parsed)?;
                let sort = TmuxSort::parse(parsed.value('O'), parsed.has('r'), None)?;
                let mut inner = self.inner.lock();
                inner.engine.set_format_now(unix_timestamp());
                let format_context = server_format_context(
                    &inner.engine,
                    &inner.config.config_files,
                    None,
                    None,
                    None,
                );
                let mut buffers = inner.buffers.paste_buffers.clone();
                sort.apply(&mut buffers, |left, right| {
                    let ordering = match sort.order() {
                        Some(TmuxSortOrder::Creation) => right.created.cmp(&left.created),
                        Some(TmuxSortOrder::Name) => left.name.cmp(&right.name),
                        Some(TmuxSortOrder::Size) => left.data.len().cmp(&right.data.len()),
                        _ => std::cmp::Ordering::Equal,
                    };
                    ordering.then_with(|| left.name.cmp(&right.name))
                });
                let format = parsed
                    .value('F')
                    .unwrap_or("#{buffer_name}: #{buffer_size} bytes: \"#{buffer_sample}\"");
                let mut output = Vec::new();
                for buffer in &buffers {
                    let facts = FormatHookFacts {
                        buffer: Some(buffer_format_facts(buffer)),
                        ..FormatHookFacts::default()
                    };
                    if let Some(filter) = parsed.value('f') {
                        let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                            &facts,
                            context.format_variables(),
                        )
                        .with_option_engine(&inner.engine)
                        .with_command_item(name);
                        if !format_true(&expand_format_values(filter, &format_context, &mut hooks))
                        {
                            continue;
                        }
                    }
                    let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                        &facts,
                        context.format_variables(),
                    )
                    .with_option_engine(&inner.engine)
                    .with_command_item(name);
                    output.push(expand_format_bytes(format, &format_context, &mut hooks));
                }
                Ok(Execution {
                    output: RawText::join(&output, b"\n"),
                    effects: Vec::new(),
                })
            }
            "load-buffer" | "loadb" => {
                let stdin = load_buffer_stdin_payload(args);
                let path_args = if stdin.is_some() {
                    &args[..args.len() - 1]
                } else {
                    args
                };
                let parsed = parse_buffer_command_args(name, path_args, &['b', 't'], &['w'])?;
                let path = require_one_positional(name, &parsed)?;
                let path = {
                    let mut inner = self.inner.lock();
                    expand_buffer_path(
                        &mut inner,
                        invoking_client,
                        context,
                        parsed.value('t'),
                        name,
                        path,
                    )
                };
                let data = if path == "-" {
                    let payload = stdin.ok_or_else(|| ServerError::InvalidCommand(
                        "load-buffer from standard input requires a command client stdin payload".to_owned(),
                    ))?;
                    if payload.as_bytes().len() > MAX_AGENT_SEND_BYTES {
                        return Err(ServerError::InvalidCommand(format!(
                            "load-buffer stdin exceeds {MAX_AGENT_SEND_BYTES} bytes"
                        ))
                        .into());
                    }
                    payload.as_bytes().to_vec()
                } else {
                    let path = buffer_file_path(&self.inner.lock(), invoking_client, &path);
                    return file_commands::load_buffer(
                        self,
                        invoking_client,
                        context,
                        &path,
                        parsed.value('b').map(str::to_owned),
                        parsed.value('t').map(str::to_owned),
                        parsed.has('w'),
                    );
                };
                if data.is_empty() {
                    return Ok(Execution::default());
                }
                if let Some(name) = parsed.value('b') {
                    validate_paste_buffer_name(name)?;
                }
                let mut inner = self.inner.lock();
                let clipboard = parsed.has('w').then(|| data.clone());
                let events =
                    insert_paste_buffer(&mut inner, parsed.value('b'), "buffer", data, true)?;
                drop(inner);
                if let Some(clipboard) = clipboard {
                    self.deliver_buffer_clipboard_write(
                        invoking_client,
                        parsed.value('t'),
                        &clipboard,
                    );
                }
                if !context.no_hooks {
                    self.run_event_hooks(events);
                }
                self.refresh_choose_buffers();
                Ok(Execution::default())
            }
            "save-buffer" | "saveb" => {
                let parsed = parse_buffer_command_args(name, args, &['b'], &['a'])?;
                let path = require_one_positional(name, &parsed)?;
                let (data, path) = {
                    let mut inner = self.inner.lock();
                    let data = Arc::clone(
                        &resolve_buffer(&inner, parsed.value('b'), BufferMissing::NoBuffer)?.data,
                    );
                    let path =
                        expand_buffer_path(&mut inner, invoking_client, context, None, name, path);
                    (data, path)
                };
                if path == "-" {
                    return Ok(Execution {
                        output: RawText::from_bytes(data.to_vec()),
                        effects: Vec::new(),
                    });
                }
                let path = buffer_file_path(&self.inner.lock(), invoking_client, &path);
                let append = parsed.has('a');
                file_commands::save_buffer(self, invoking_client, &path, &data, append)
            }
            "delete-buffer" | "deleteb" => {
                let parsed = parse_buffer_command_args(name, args, &['b'], &[])?;
                require_no_positionals(name, &parsed)?;
                let mut inner = self.inner.lock();
                let name = resolve_buffer(&inner, parsed.value('b'), BufferMissing::UnknownBuffer)?
                    .name
                    .clone();
                inner
                    .buffers
                    .paste_buffers
                    .retain(|buffer| buffer.name != name);
                drop(inner);
                if !context.no_hooks {
                    self.run_event_hooks(vec![PendingHookEvent::paste_buffer(
                        "paste-buffer-deleted",
                        name,
                    )]);
                }
                self.refresh_choose_buffers();
                Ok(Execution::default())
            }
            "paste-buffer" | "pasteb" => {
                let parsed =
                    parse_buffer_command_args(name, args, &['b', 's', 't'], &['d', 'p', 'r', 'S'])?;
                require_no_positionals(name, &parsed)?;
                let pane = {
                    let inner = self.inner.lock();
                    let pane = inner.engine.resolve_pane(
                        parsed.value('t'),
                        context.window,
                        context.pane,
                    )?;
                    if is_agent_chat(&inner, pane) {
                        return Err(agent_chat_refusal("paste-buffer", pane).into());
                    }
                    pane
                };
                let separator = parsed.value('s').map_or_else(
                    || {
                        if parsed.has('r') {
                            b"\n".as_slice()
                        } else {
                            b"\r".as_slice()
                        }
                    },
                    str::as_bytes,
                );
                self.paste_buffer_to_pane(
                    pane,
                    PasteBufferPaste {
                        requested_name: parsed.value('b'),
                        delete: parsed.has('d'),
                        bracketed: parsed.has('p'),
                        separator,
                        literal: parsed.has('S'),
                        expected_client: None,
                        event_hooks_enabled: !context.no_hooks,
                    },
                )?;
                Ok(Execution::default())
            }
            _ => Err(ServerError::UnsupportedCommand(name.to_owned()).into()),
        }
    }

    pub(super) fn paste_buffer_to_pane(
        self: &Arc<Self>,
        pane: PaneId,
        request: PasteBufferPaste<'_>,
    ) -> Result<(), DaemonError> {
        let PasteBufferPaste {
            requested_name,
            delete,
            bracketed,
            separator,
            literal,
            expected_client,
            event_hooks_enabled,
        } = request;
        let (client, sinks, buffer_name, data) = {
            let inner = self.inner.lock();
            let Some(buffer) = find_buffer(&inner, requested_name) else {
                if requested_name.is_some() {
                    return Err(BufferMissing::NoBuffer.error(requested_name).into());
                }
                return Ok(());
            };
            let data = Arc::clone(&buffer.data);
            let buffer_name = buffer.name.clone();
            if expected_client
                .is_some_and(|expected| !client_is_attached_to_pane(&inner, expected, pane))
            {
                return Err(ServerError::PaneNotAttached(pane).into());
            }
            let sinks = resolve_input_sinks(&inner, pane)?;
            (expected_client, sinks, buffer_name, data)
        };
        let mut prepared = prepare_paste_buffer(&data, separator, literal)
            .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
        let has_terminal = sinks
            .iter()
            .any(|sink| matches!(sink, PaneSink::Terminal(_)));
        let browser_text = sinks
            .iter()
            .any(|sink| matches!(sink, PaneSink::Browser(_)))
            .then(|| {
                let bytes = if has_terminal {
                    prepared.clone()
                } else {
                    std::mem::take(&mut prepared)
                };
                String::from_utf8(bytes)
            })
            .transpose()
            .map_err(|_| {
                ServerError::InvalidCommand(format!(
                    "buffer {buffer_name} is not valid UTF-8 for a browser pane"
                ))
            })?;
        let prepared = has_terminal.then(|| Arc::<[u8]>::from(prepared));
        for sink in sinks {
            match sink {
                PaneSink::Terminal(terminal) => {
                    terminal.paste_prepared_bytes(
                        client.map(|client| TerminalViewId(client.0)),
                        Arc::clone(
                            prepared
                                .as_ref()
                                .expect("terminal paste retains prepared bytes"),
                        ),
                        bracketed,
                    );
                }
                PaneSink::Browser(target) => self.publish_for_pane(
                    target,
                    &EventPayload::BrowserCommand {
                        pane: target,
                        command: BrowserCommand::SendKeys(vec![zz_protocol::KeyToken::Literal(
                            browser_text
                                .as_ref()
                                .expect("browser text is validated before dispatch")
                                .clone(),
                        )]),
                    },
                ),
            }
        }
        let removed = delete && {
            let mut inner = self.inner.lock();
            let before = inner.buffers.paste_buffers.len();
            inner
                .buffers
                .paste_buffers
                .retain(|buffer| buffer.name != buffer_name || !Arc::ptr_eq(&buffer.data, &data));
            inner.buffers.paste_buffers.len() != before
        };
        if removed {
            if event_hooks_enabled {
                self.run_event_hooks(vec![PendingHookEvent::paste_buffer(
                    "paste-buffer-deleted",
                    buffer_name,
                )]);
            }
            self.refresh_choose_buffers();
        }
        Ok(())
    }

    pub(super) fn deliver_buffer_clipboard_write(
        self: &Arc<Self>,
        invoking_client: Option<ClientId>,
        target_client: Option<&str>,
        data: &[u8],
    ) {
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let destination = {
            let inner = self.inner.lock();
            match target_client {
                Some(target) => find_attached_client(&inner, target),
                None => invoking_client.and_then(|client| current_format_client(&inner, client)),
            }
            .filter(|client| client_colour_count(&inner, *client).is_some())
            .and_then(|client| {
                buffer_path_client_context(&inner, client).map(|(_, _, pane)| (client, pane))
            })
        };
        let Some((client, pane)) = destination else {
            return;
        };
        self.publish_to_client(
            client,
            EventPayload::Clipboard {
                pane,
                request_id: 0,
                target: ClipboardTarget::Clipboard,
                text: text.to_owned(),
                producer: ClipboardProducer::Server,
            },
        );
    }

    pub(super) fn store_copy_buffer(self: &Arc<Self>, data: String, action: PasteBufferAction) {
        if data.is_empty() {
            return;
        }
        let mut data = data.into_bytes();
        let mut inner = self.inner.lock();
        let (requested_name, prefix, replacement_delete) = match action {
            PasteBufferAction::Create { prefix } => {
                (None, prefix.unwrap_or_else(|| "buffer".to_owned()), true)
            }
            PasteBufferAction::Append => {
                let requested_name = inner
                    .buffers
                    .paste_buffers
                    .iter()
                    .find(|buffer| buffer.automatic)
                    .map(|buffer| {
                        let combined = buffer.data.len().saturating_add(data.len());
                        validate_paste_buffer_size(combined)?;
                        let mut appended = Vec::with_capacity(combined);
                        appended.extend_from_slice(&buffer.data);
                        appended.append(&mut data);
                        data = appended;
                        Ok::<_, ServerError>(buffer.name.clone())
                    })
                    .transpose();
                let requested_name = match requested_name {
                    Ok(name) => name,
                    Err(error) => {
                        log::warn!("could not append native copy buffer: {error}");
                        return;
                    }
                };
                (requested_name, "buffer".to_owned(), false)
            }
        };
        let events = match insert_paste_buffer(
            &mut inner,
            requested_name.as_deref(),
            &prefix,
            data,
            replacement_delete,
        ) {
            Ok(events) => events,
            Err(error) => {
                log::warn!("could not create native copy buffer: {error}");
                return;
            }
        };
        drop(inner);
        self.run_event_hooks(events);
        self.refresh_choose_buffers();
    }
}

pub(super) const MAX_PASTE_BUFFER_BYTES: usize = 32 * 1024 * 1024;

const MAX_PASTE_BUFFER_NAME_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug)]
pub(super) struct AutomaticPasteBufferLimit(pub(super) usize);

impl Default for AutomaticPasteBufferLimit {
    fn default() -> Self {
        Self(DEFAULT_BUFFER_LIMIT)
    }
}

#[derive(Clone, Debug)]
pub(super) struct PasteBuffer {
    pub(super) name: String,
    pub(super) data: Arc<[u8]>,
    pub(super) created: SystemTime,
    pub(super) automatic: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PasteBufferPaste<'a> {
    pub(super) requested_name: Option<&'a str>,
    pub(super) separator: &'a [u8],
    pub(super) expected_client: Option<ClientId>,
    pub(super) delete: bool,
    pub(super) bracketed: bool,
    pub(super) literal: bool,
    pub(super) event_hooks_enabled: bool,
}

fn resolve_display_panes_client(
    inner: &ServerState,
    target: &str,
) -> Result<ClientId, ServerError> {
    find_attached_client(inner, target).ok_or_else(|| client_target_error(target))
}

pub(super) fn client_colour_count(inner: &ServerState, client: ClientId) -> Option<u32> {
    client_colour_count_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn find_buffer<'a>(inner: &'a ServerState, name: Option<&str>) -> Option<&'a PasteBuffer> {
    inner
        .buffers
        .paste_buffers
        .iter()
        .find(|buffer| name.map_or(buffer.automatic, |name| buffer.name == name))
}

pub(super) fn buffer_format_facts(buffer: &PasteBuffer) -> BufferFormatFacts {
    BufferFormatFacts {
        name: buffer.name.clone(),
        data: Arc::clone(&buffer.data),
        created: buffer.created,
    }
}

fn buffer_path_client_context(
    inner: &ServerState,
    client: ClientId,
) -> Option<(SessionId, WindowId, PaneId)> {
    let session = client_attached_session(inner, client)?;
    let session_state = inner.engine.state.sessions.get(&session)?;
    let window = client_focused_window(inner, client, session_state);
    let pane = inner.engine.state.windows.get(&window)?.active_pane;
    Some((session, window, pane))
}

fn buffer_path_format_target(
    inner: &ServerState,
    invoking_client: Option<ClientId>,
    context: &ExecutionContext,
    target_client: Option<&str>,
) -> (
    Option<ClientId>,
    (Option<SessionId>, Option<WindowId>, Option<PaneId>),
) {
    let client = match target_client {
        Some(target) => resolve_display_panes_client(inner, target).ok(),
        None => invoking_client.and_then(|client| current_format_client(inner, client)),
    };
    let target = client
        .and_then(|client| buffer_path_client_context(inner, client))
        .or_else(|| {
            invoking_client
                .is_none()
                .then_some((context.session?, context.window?, context.pane?))
        })
        .or_else(|| inner.engine.state.most_recent_context());
    let target = target.map_or((None, None, None), |(session, window, pane)| {
        (Some(session), Some(window), Some(pane))
    });
    (client, target)
}

pub(super) fn expand_buffer_path(
    inner: &mut ServerState,
    invoking_client: Option<ClientId>,
    context: &ExecutionContext,
    target_client: Option<&str>,
    command_name: &str,
    path: &str,
) -> String {
    let (format_client, (session, window, pane)) =
        buffer_path_format_target(inner, invoking_client, context, target_client);
    inner.engine.set_format_now(unix_timestamp());
    let mut format_context = format_client
        .and_then(|client| client_attached_session(inner, client))
        .map_or_else(
            || inner.engine.format_status_context(session, window, pane),
            |client_session| {
                inner
                    .engine
                    .format_status_context_for_client(session, window, pane, client_session)
            },
        );
    format_context.set_format_value("config_files", inner.config.config_files.clone());
    let facts = borrowed_format_hook_facts(inner);
    let mut hooks =
        DaemonFormatHooks::command_with_optional_variables(&facts, context.format_variables())
            .with_option_engine(&inner.engine)
            .with_command_item(command_name);
    expand_format_values(path, &format_context, &mut hooks)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BufferMissing {
    NoBuffer,
    UnknownBuffer,
}

impl BufferMissing {
    pub(super) fn error(self, name: Option<&str>) -> ServerError {
        ServerError::InvalidCommand(match (self, name) {
            (Self::NoBuffer, Some(name)) => format!("no buffer {name}"),
            (Self::NoBuffer, None) => "no buffers".to_owned(),
            (Self::UnknownBuffer, Some(name)) => format!("unknown buffer: {name}"),
            (Self::UnknownBuffer, None) => "no buffer".to_owned(),
        })
    }
}

pub(super) fn resolve_buffer<'a>(
    inner: &'a ServerState,
    name: Option<&str>,
    missing: BufferMissing,
) -> Result<&'a PasteBuffer, ServerError> {
    find_buffer(inner, name).ok_or_else(|| missing.error(name))
}

#[derive(Debug, Default)]
pub(super) struct ParsedBufferCommandArgs {
    pub(super) flags: BTreeSet<char>,
    pub(super) values: BTreeMap<char, String>,
    repeated_values: BTreeMap<char, Vec<String>>,
    pub(super) positional: Vec<String>,
}

impl ParsedBufferCommandArgs {
    pub(super) fn has(&self, flag: char) -> bool {
        self.flags.contains(&flag)
    }

    pub(super) fn value(&self, option: char) -> Option<&str> {
        self.values.get(&option).map(String::as_str)
    }

    pub(super) fn values(&self, option: char) -> impl Iterator<Item = &str> {
        self.repeated_values
            .get(&option)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }
}

#[must_use]
pub fn load_buffer_reads_stdin(args: &[RawText]) -> bool {
    parse_buffer_command_args("load-buffer", args, &['b', 't'], &['w'])
        .is_ok_and(|parsed| parsed.positional == ["-"])
}

pub(super) fn load_buffer_stdin_payload(args: &[RawText]) -> Option<&RawText> {
    let (payload, path_args) = args.split_last()?;
    load_buffer_reads_stdin(path_args).then_some(payload)
}

pub(super) fn parse_buffer_command_args(
    command: &str,
    args: &[RawText],
    value_options: &[char],
    flags: &[char],
) -> Result<ParsedBufferCommandArgs, ServerError> {
    let mut parsed = ParsedBufferCommandArgs::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            parsed
                .positional
                .extend(args[index + 1..].iter().map(ToString::to_string));
            break;
        }
        if !argument.starts_with('-') || argument == "-" {
            parsed
                .positional
                .extend(args[index..].iter().map(ToString::to_string));
            break;
        }

        let mut consumed_next = false;
        for (offset, option) in argument[1..].char_indices() {
            if value_options.contains(&option) {
                let value_start = 1 + offset + option.len_utf8();
                let value = if value_start < argument.len() {
                    argument[value_start..].to_owned()
                } else {
                    consumed_next = true;
                    args.get(index + 1)
                        .map(ToString::to_string)
                        .ok_or_else(|| {
                            ServerError::CommandParse(format!(
                                "{command} -{option} requires a value"
                            ))
                        })?
                };
                parsed.values.insert(option, value.clone());
                parsed
                    .repeated_values
                    .entry(option)
                    .or_default()
                    .push(value);
                break;
            }
            if flags.contains(&option) {
                parsed.flags.insert(option);
            } else {
                return Err(ServerError::CommandParse(format!(
                    "unsupported command: {command} -{option}"
                )));
            }
        }
        index += usize::from(consumed_next) + 1;
    }
    if let Some(spec) = zz_protocol::catalog_command_spec(command) {
        spec.validate_positional_minimum(parsed.positional.len())?;
        spec.validate_positional_maximum(parsed.positional.len())?;
    }
    Ok(parsed)
}

fn require_one_positional<'a>(
    command: &str,
    parsed: &'a ParsedBufferCommandArgs,
) -> Result<&'a str, ServerError> {
    let [value] = parsed.positional.as_slice() else {
        return Err(ServerError::CommandParse(format!(
            "{command} requires exactly one path"
        )));
    };
    Ok(value)
}

pub(super) fn validate_paste_buffer_size(size: usize) -> Result<(), ServerError> {
    if size > MAX_PASTE_BUFFER_BYTES {
        Err(ServerError::InvalidCommand(format!(
            "paste buffer exceeds {MAX_PASTE_BUFFER_BYTES} bytes"
        )))
    } else {
        Ok(())
    }
}

pub(super) fn validate_paste_buffer_name(name: &str) -> Result<(), ServerError> {
    if name.is_empty() {
        return Err(ServerError::InvalidCommand(
            "paste buffer name cannot be empty".to_owned(),
        ));
    }
    if name.len() > MAX_PASTE_BUFFER_NAME_BYTES {
        return Err(ServerError::InvalidCommand(format!(
            "paste buffer name exceeds {MAX_PASTE_BUFFER_NAME_BYTES} bytes"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(ServerError::InvalidCommand(
            "paste buffer name contains control characters".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn insert_paste_buffer(
    inner: &mut ServerState,
    requested_name: Option<&str>,
    automatic_prefix: &str,
    data: Vec<u8>,
    replacement_delete: bool,
) -> Result<Vec<PendingHookEvent>, ServerError> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let mut events = Vec::new();
    validate_paste_buffer_size(data.len())?;
    let (name, automatic) = if let Some(name) = requested_name {
        validate_paste_buffer_name(name)?;
        (name.to_owned(), false)
    } else {
        let mut name = None;
        for _ in 0..inner.buffers.paste_buffers.len().saturating_add(1) {
            let candidate = format!("{automatic_prefix}{}", inner.buffers.next_buffer_id);
            inner.buffers.next_buffer_id = inner.buffers.next_buffer_id.wrapping_add(1);
            validate_paste_buffer_name(&candidate)?;
            if !inner
                .buffers
                .paste_buffers
                .iter()
                .any(|buffer| buffer.name == candidate)
            {
                name = Some(candidate);
                break;
            }
        }
        let name = name.ok_or_else(|| {
            ServerError::InvalidCommand("no automatic paste buffer names available".to_owned())
        })?;
        while inner
            .buffers
            .paste_buffers
            .iter()
            .filter(|buffer| buffer.automatic)
            .count()
            >= inner.buffers.automatic_paste_buffer_limit.0
        {
            let Some(index) = inner
                .buffers
                .paste_buffers
                .iter()
                .rposition(|buffer| buffer.automatic)
            else {
                break;
            };
            let removed = inner.buffers.paste_buffers.remove(index);
            events.push(PendingHookEvent::paste_buffer(
                "paste-buffer-deleted",
                removed.name,
            ));
        }
        (name, true)
    };

    inner.buffers.paste_buffers.retain(|buffer| {
        if buffer.name == name {
            if replacement_delete {
                events.push(PendingHookEvent::paste_buffer(
                    "paste-buffer-deleted",
                    buffer.name.clone(),
                ));
            }
            false
        } else {
            true
        }
    });
    inner.buffers.paste_buffers.insert(
        0,
        PasteBuffer {
            name: name.clone(),
            data: Arc::from(data),
            created: SystemTime::now(),
            automatic,
        },
    );
    events.push(PendingHookEvent::paste_buffer("paste-buffer-changed", name));
    Ok(events)
}

fn rename_paste_buffer(
    inner: &mut ServerState,
    requested_name: Option<&str>,
    new_name: &str,
) -> Result<Vec<PendingHookEvent>, ServerError> {
    let mut source_index = match requested_name {
        Some(name) => inner
            .buffers
            .paste_buffers
            .iter()
            .position(|buffer| buffer.name == name)
            .ok_or_else(|| ServerError::InvalidCommand(format!("unknown buffer: {name}")))?,
        None => inner
            .buffers
            .paste_buffers
            .iter()
            .position(|buffer| buffer.automatic)
            .ok_or_else(|| ServerError::InvalidCommand("no buffer".to_owned()))?,
    };
    if new_name.is_empty() {
        return Err(ServerError::InvalidCommand("new name is empty".to_owned()));
    }
    validate_paste_buffer_name(new_name)?;
    if inner.buffers.paste_buffers[source_index].name == new_name {
        return Ok(Vec::new());
    }

    let mut events = Vec::new();
    if let Some(destination_index) = inner
        .buffers
        .paste_buffers
        .iter()
        .position(|buffer| buffer.name == new_name)
    {
        let removed = inner.buffers.paste_buffers.remove(destination_index);
        events.push(PendingHookEvent::paste_buffer(
            "paste-buffer-deleted",
            removed.name,
        ));
        if destination_index < source_index {
            source_index -= 1;
        }
    }

    let source = &mut inner.buffers.paste_buffers[source_index];
    let old_name = std::mem::replace(&mut source.name, new_name.to_owned());
    source.automatic = false;
    events.push(PendingHookEvent::paste_buffer(
        "paste-buffer-deleted",
        old_name,
    ));
    events.push(PendingHookEvent::paste_buffer(
        "paste-buffer-changed",
        new_name.to_owned(),
    ));
    Ok(events)
}

fn client_file_working_directory(inner: &ServerState, client: Option<ClientId>) -> PathBuf {
    if let Some(startup) = inner.config.startup_source_client_working_directory.clone() {
        return startup;
    }
    let attached = client.and_then(|client| client_attached_session(inner, client));
    if attached.is_none()
        && let Some(cwd) = client.and_then(|client| {
            inner
                .client(client)
                .and_then(|c| c.working_directory.as_ref())
        })
    {
        return cwd.clone();
    }
    attached
        .and_then(|session| inner.engine.state.session_working_directory(session))
        .map_or_else(default_job_working_directory, Path::to_owned)
}

pub(super) fn buffer_file_path(
    inner: &ServerState,
    client: Option<ClientId>,
    path: &str,
) -> PathBuf {
    let expanded = expand_path(path);
    if expanded.is_absolute() {
        return expanded;
    }
    client_file_working_directory(inner, client).join(expanded)
}

pub(super) fn read_paste_buffer_file(path: &Path) -> Result<Vec<u8>, ServerError> {
    let file = fs::File::open(path).map_err(|error| buffer_file_error(path, &error))?;
    if file
        .metadata()
        .map_err(|error| buffer_file_error(path, &error))?
        .len()
        > u64::try_from(MAX_PASTE_BUFFER_BYTES).expect("paste buffer limit fits in u64")
    {
        return Err(ServerError::InvalidCommand(format!(
            "{}: paste buffer exceeds {MAX_PASTE_BUFFER_BYTES} bytes",
            path.display()
        )));
    }
    let mut data = Vec::new();
    file.take(
        u64::try_from(MAX_PASTE_BUFFER_BYTES)
            .expect("paste buffer limit fits in u64")
            .saturating_add(1),
    )
    .read_to_end(&mut data)
    .map_err(|error| buffer_file_error(path, &error))?;
    validate_paste_buffer_size(data.len())?;
    Ok(data)
}

pub(super) fn write_paste_buffer_file(
    path: &Path,
    data: &[u8],
    append: bool,
) -> Result<(), ServerError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut file = options
        .open(path)
        .map_err(|error| buffer_file_error(path, &error))?;
    file.write_all(data)
        .and_then(|()| file.flush())
        .map_err(|error| buffer_file_error(path, &error))
}

fn buffer_file_error(path: &Path, error: &std::io::Error) -> ServerError {
    client_file_failure(&zz_daemon_client::strerror_text(error), path)
}
