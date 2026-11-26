use super::*;

impl Shared {
    pub(super) fn record_command_stdout(&self, client: ClientId, line: &str) {
        if let Some(streams) = self
            .inner
            .lock()
            .client_mut(client)
            .and_then(|c| c.command_streams.as_mut())
        {
            streams.stdout.push_str(line);
            streams.stdout.push('\n');
        }
    }

    pub(super) fn feed_pane_stream_input(&self, pane: PaneId, bytes: &[u8]) -> bool {
        let (terminal, geometry) = {
            let inner = self.inner.lock();
            (
                inner.terminals.get(&pane).cloned(),
                inner.engine.pane_geometry(pane),
            )
        };
        let Some(terminal) = terminal else {
            return false;
        };
        if let Some((columns, rows)) = geometry
            && terminal.size() != (columns, rows)
        {
            terminal.resize(columns, rows, 0, 0);
        }
        terminal.feed(Arc::from(bytes))
    }

    fn stream_caller_stdin_to_pane(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        command: &CommandInvocation,
    ) {
        let target = self
            .inner
            .lock()
            .engine
            .display_message_format_target(context, &command.args);
        let Some(pane) = target.ok().and_then(|(target, _)| target?.pane) else {
            return;
        };
        self.stream_caller_stdin_to_created_pane(client, pane);
    }

    fn stream_caller_stdin_to_created_pane(self: &Arc<Self>, client: ClientId, pane: PaneId) {
        file_commands::stream_stdin(self, client, pane);
    }

    pub(super) fn pane_stream_target_lost(&self, pane: PaneId) -> bool {
        self.inner.lock().engine.state.pane(pane).is_none()
    }

    pub(super) fn take_caller_stdin_error(&self, client: ClientId) -> String {
        self.inner
            .lock()
            .client_mut(client)
            .and_then(|c| c.command_streams.as_mut())
            .and_then(|streams| streams.stdin_error.take())
            .unwrap_or_else(spent_source_stream_error)
    }

    pub(super) fn record_command_stderr(&self, client: ClientId, line: &str) {
        let deliver = {
            let mut inner = self.inner.lock();
            if let Some(streams) = inner
                .client_mut(client)
                .and_then(|c| c.command_streams.as_mut())
            {
                streams.stderr.push_str(line);
                streams.stderr.push('\n');
                inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Command)
            } else {
                false
            }
        };
        if deliver && let Some(writer) = self.client_writers.lock().get(&client).cloned() {
            Self::send_event(
                &writer,
                EventPayload::ClientMessage {
                    pane: None,
                    kind: ClientMessageKind::Error,
                    text: line.to_owned(),
                },
            );
        }
    }

    pub(super) fn append_alias_output(
        &self,
        result: &mut InsertedCommandResult,
        output: &RawText,
        name: &str,
        claim: &mut StdoutClaim,
        client: Option<(ClientId, usize)>,
    ) {
        let Some((client, sequence)) = client else {
            append_inserted_output(&mut result.output, output);
            return;
        };
        let recorded_raw = self.command_raw_stdout_since(client, sequence);
        let next = if !output.is_empty() && *claim != StdoutClaim::Raw && recorded_raw {
            StdoutClaim::Raw
        } else {
            default_stdout_claim(name, output)
        };
        if next == StdoutClaim::None {
            return;
        }
        if next == StdoutClaim::Raw && *claim != StdoutClaim::None {
            self.record_command_stderr(client, &spent_source_stream_error());
            self.record_command_failure(client);
            if let Some(streams) = self
                .inner
                .lock()
                .client_mut(client)
                .and_then(|c| c.command_streams.as_mut())
            {
                streams.stdout_claim = Some(*claim);
                streams.stdout_write = (streams.stdout_write.0 + 1, *claim);
            }
            result.exit_code = 1;
            return;
        }
        if *claim == StdoutClaim::Raw {
            return;
        }
        append_inserted_output(&mut result.output, output);
        *claim = next;
        self.record_command_stdout_claim(client, next);
    }

    pub(super) fn command_stdout_sequence(&self, client: ClientId) -> usize {
        self.read_client(client, |c| {
            c.and_then(|c| c.command_streams.as_ref())
                .map_or(0, |streams| streams.stdout_write.0)
        })
    }

    pub(super) fn command_raw_stdout_since(&self, client: ClientId, sequence: usize) -> bool {
        self.read_client(client, |c| {
            c.and_then(|c| c.command_streams.as_ref())
                .is_some_and(|streams| {
                    streams.stdout_write.0 > sequence && streams.stdout_write.1 == StdoutClaim::Raw
                })
        })
    }

    pub(super) fn command_with_caller_stdin(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        command: &CommandInvocation,
    ) -> Result<Option<CommandInvocation>, DaemonError> {
        let Some(sink) = command_stdin_sink(canonical_command(&command.name), &command.args) else {
            return Ok(None);
        };
        if sink == CommandStdinSink::ConfigReplay
            || !self
                .inner
                .lock()
                .engine
                .command_stdin_destination_ready(context, command)?
        {
            return Ok(None);
        }
        let client = context.replay_client().unwrap_or(client);
        let (stdin, available) = {
            let mut inner = self.inner.lock();
            let Some(streams) = inner
                .client_mut(client)
                .and_then(|c| c.command_streams.as_mut())
            else {
                return Ok(None);
            };
            let stdin = streams.stdin.take();
            let available = std::mem::take(&mut streams.stdin_available);
            if stdin.is_some() || available {
                streams.stdin = Some(SourceStream::Spent);
            }
            (stdin, available)
        };
        let stdin = if stdin.is_none() && available && sink == CommandStdinSink::PaneInput {
            self.stream_caller_stdin_to_pane(client, context, command);
            Some(SourceStream::Spent)
        } else if stdin.is_none() && available {
            file_commands::read_stdin(self, client, sink, command)?;
            if self
                .command_item
                .as_ref()
                .is_some_and(|item| item.lock().pending_wait.is_some())
            {
                return Ok(None);
            }
            self.inner
                .lock()
                .client_mut(client)
                .and_then(|c| c.command_streams.as_mut())
                .and_then(|streams| streams.stdin.replace(SourceStream::Spent))
        } else {
            stdin
        };
        let Some(stdin) = stdin else {
            return Ok(None);
        };
        let mut command = command.clone();
        match stdin {
            SourceStream::Bytes(bytes) => command.set_stdin(bytes),
            SourceStream::Spent => command.set_stdin_spent(),
        }
        Ok(Some(command))
    }

    pub(super) fn record_command_stdout_claim(&self, client: ClientId, claim: StdoutClaim) {
        if let Some(streams) = self
            .inner
            .lock()
            .client_mut(client)
            .and_then(|c| c.command_streams.as_mut())
        {
            streams.stdout_write = (streams.stdout_write.0 + 1, claim);
            let current = streams.stdout_claim.unwrap_or(StdoutClaim::None);
            if stdout_claim_rank(claim) > stdout_claim_rank(current) {
                streams.stdout_claim = Some(claim);
            }
        }
    }

    pub(super) fn record_command_failure(&self, client: ClientId) {
        if let Some(streams) = self
            .inner
            .lock()
            .client_mut(client)
            .and_then(|c| c.command_streams.as_mut())
        {
            streams.exit_code = 1;
        }
    }

    pub(super) fn request_command_client_exit(&self, client: ClientId) {
        let announce = {
            let mut inner = self.inner.lock();
            let command_client =
                inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Command);
            match inner
                .client_mut(client)
                .and_then(|c| c.command_streams.as_mut())
            {
                Some(streams) if !streams.client_exit => {
                    streams.client_exit = true;
                    streams.exit_code = 1;
                    command_client
                }
                _ => false,
            }
        };
        if announce && let Some(writer) = self.client_writers.lock().get(&client).cloned() {
            Self::send_event(&writer, EventPayload::CommandClientExit);
        }
    }

    pub(super) fn command_client_exiting(&self, client: ClientId) -> bool {
        self.read_client(client, |c| {
            c.and_then(|c| c.command_streams.as_ref())
                .is_some_and(|streams| streams.client_exit)
        })
    }

    pub(super) fn release_command_stdout(&self, client: ClientId, output: &mut RawText) {
        if output.is_empty() {
            return;
        }
        let command_client = {
            let inner = self.inner.lock();
            inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Command)
        };
        if !command_client {
            return;
        }
        let Some(writer) = self.client_writers.lock().get(&client).cloned() else {
            return;
        };
        Self::send_event(
            &writer,
            EventPayload::CommandStdout {
                output: std::mem::take(output),
            },
        );
        self.record_command_stdout_claim(client, StdoutClaim::Print);
    }
}

const fn stdout_claim_rank(claim: StdoutClaim) -> u8 {
    match claim {
        StdoutClaim::None => 0,
        StdoutClaim::Print => 1,
        StdoutClaim::Raw => 2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandStdinSink {
    Argument { binary: bool },
    Config,
    ConfigReplay,
    PaneInput,
}

impl CommandStdinSink {
    #[must_use]
    pub const fn accepts_binary(self) -> bool {
        match self {
            Self::Argument { binary } => binary,
            Self::Config | Self::PaneInput | Self::ConfigReplay => true,
        }
    }

    #[must_use]
    pub const fn is_argument(self) -> bool {
        matches!(self, Self::Argument { .. })
    }
}

#[must_use]
pub fn command_stdin_sink(canonical_name: &str, args: &[RawText]) -> Option<CommandStdinSink> {
    match canonical_name {
        "agent-send" => {
            agent_send_reads_stdin(args).then_some(CommandStdinSink::Argument { binary: false })
        }
        "send-text" => {
            send_text_reads_stdin(args).then_some(CommandStdinSink::Argument { binary: false })
        }
        "html" => artifacts::html_reads_stdin(args)
            .then_some(CommandStdinSink::Argument { binary: false }),
        "load-buffer" => {
            load_buffer_reads_stdin(args).then_some(CommandStdinSink::Argument { binary: true })
        }
        "source-file" => Some(if source_file_reads_stdin(args) {
            CommandStdinSink::Config
        } else {
            CommandStdinSink::ConfigReplay
        }),
        "display-message" | "split-window" | "new-pane" => (args
            .iter()
            .any(|argument| argument.as_bytes().contains(&b'I'))
            && command_has_flag(canonical_name, args, "-I")
            && !(canonical_name == "display-message"
                && command_has_flag(canonical_name, args, "-j")))
        .then_some(CommandStdinSink::PaneInput),
        _ => None,
    }
}

fn stdin_payload_has_argument_boundary(canonical_name: &str, arguments: &[RawText]) -> bool {
    if canonical_name == "load-buffer" {
        return true;
    }
    let Some(spec) = zz_protocol::catalog_command_spec(canonical_name) else {
        return false;
    };
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        if argument == "--" {
            return true;
        }
        if !argument.starts_with('-') || argument == "-" {
            return false;
        }
        let consumes_next = spec
            .option(argument)
            .is_some_and(|option| option.value.is_some());
        index += if consumes_next { 2 } else { 1 };
    }
    false
}

pub fn append_stdin_payload(
    canonical_name: &str,
    arguments: &mut Vec<RawText>,
    payload: impl Into<RawText>,
) {
    if !stdin_payload_has_argument_boundary(canonical_name, arguments) {
        arguments.push("--".into());
    }
    arguments.push(payload.into());
}

pub(super) fn source_file_reads_stdin(args: &[RawText]) -> bool {
    zz_protocol::command_spec("source-file").is_some_and(|spec| {
        zz_protocol::parse_tmux_options(spec, args)
            .is_ok_and(|parsed| parsed.positionals.iter().any(|path| path == "-"))
    })
}

fn command_has_flag(canonical_name: &str, args: &[RawText], flag: &str) -> bool {
    zz_protocol::command_spec(canonical_name).is_some_and(|spec| {
        zz_protocol::parse_tmux_options(spec, args).is_ok_and(|parsed| {
            parsed.options.iter().any(
                |option| matches!(option, zz_protocol::TmuxOption::Flag(name) if *name == flag),
            )
        })
    })
}
