use super::*;

impl CommandRoute {
    pub(super) fn exec_request(
        &self,
        commands: Vec<CommandInvocation>,
        flags: ExecFlags,
        spawned_server_id: Option<u64>,
        expect_server_id: Option<u64>,
    ) -> ExecRequest {
        let mut flags = flags;
        flags.set(ExecFlags::UTF8, client_takes_utf8_terminal());
        flags.set(
            ExecFlags::NESTED,
            self.facts.includes_tty()
                && std::env::var_os("TMUX").is_some_and(|value| !value.is_empty()),
        );
        flags.set(ExecFlags::REMOTE, self.facts.is_remote());
        ExecRequest {
            protocol_version: PROTOCOL_VERSION,
            flags,
            client_instance_id: client_instance_id(),
            origin: self
                .send_origin
                .then(|| std::env::var("ZZ_PANE").ok())
                .flatten()
                .and_then(|pane| pane.parse().ok()),
            working_directory: client_working_directory(self.facts, logical_current_dir),
            tty: self
                .facts
                .tty_scope()
                .and_then(caller_tty)
                .filter(|tty| tty.len() <= zz_protocol::MAX_EXEC_TTY_BYTES),
            size: self
                .facts
                .includes_terminal_size()
                .then(caller_terminal_size)
                .flatten()
                .map(|(cols, rows, _, _)| (cols, rows)),
            features: exec_feature_mask(),
            startup_reentry: std::env::var(crate::STARTUP_REENTRY_ENVIRONMENT_VARIABLE)
                .ok()
                .and_then(|token| token.parse().ok()),
            spawned_server_id,
            expect_server_id,
            process_id: std::process::id(),
            environment: exec_environment(self.facts),
            commands,
            raw_control_line: None,
        }
    }

    fn connect_legacy(&self) -> Result<(CommandLink, bool), DaemonError> {
        let stream = LocalTransport::connect(&self.socket)?;
        let (reader, writer, hello) = connect_stream(
            stream,
            &self.display,
            ClientKind::Command,
            None,
            None,
            false,
            self.send_origin,
            self.facts,
        )?;
        let speaks_exec = hello
            .capabilities
            .iter()
            .any(|capability| capability == EXEC_CAPABILITY);
        Ok((
            CommandLink::Legacy {
                reader,
                writer,
                server_id: hello.server_id,
            },
            speaks_exec,
        ))
    }

    fn connect(&self) -> Result<CommandLink, DaemonError> {
        CommandLink::exec(LocalTransport::connect(&self.socket)?)
    }

    #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
    fn probe(&self) -> ProtocolMessage {
        ProtocolMessage::Exec(self.exec_request(Vec::new(), ExecFlags::default(), None, None))
    }

    #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
    fn answers_at(&self, socket: &Path) -> bool {
        let Ok(stream) = LocalTransport::connect(socket) else {
            return false;
        };
        let Ok(reader) = stream.try_clone() else {
            return false;
        };
        stream
            .set_timeout(Some(crate::endpoint::MASTER_STEP_TIMEOUT))
            .is_ok()
            && ProtocolSender::new(stream).send(&self.probe()).is_ok()
            && matches!(
                ProtocolReceiver::new(reader).recv(),
                Ok(ProtocolMessage::ExecExit(_))
            )
    }
}

impl CommandLink {
    pub(super) fn exec(stream: LocalStream) -> Result<Self, DaemonError> {
        Ok(Self::Exec {
            reader: ProtocolReceiver::new(stream.try_clone()?),
            writer: ProtocolSender::new(stream),
            answered: false,
            spent: false,
        })
    }
}

impl CommandClient {
    pub fn into_interactive(
        self,
        attach: zz_protocol::AttachOperation,
    ) -> Result<InteractiveClient, DaemonError> {
        let stream = match self.link {
            CommandLink::Exec { reader, writer, .. } => {
                drop(reader);
                writer.stream
            }
            CommandLink::Legacy { reader, writer, .. } => {
                drop(reader);
                drop(writer);
                LocalTransport::connect(&self.route.socket)?
            }
        };
        let connected = connect_stream_hello(
            ClientStream::Local(stream),
            &self.route.display,
            ClientKind::Interactive,
            short_device_name(),
            None,
            std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
            false,
            false,
            &[
                crate::CLIENT_EXITS_ON_DETACH_CAPABILITY,
                zz_protocol::TTY_INPUT_CAPABILITY,
            ],
            self.route.facts,
            Some(attach),
        )?;
        let client = InteractiveClient::from_connected(connected);
        #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
        let client = {
            let mut client = client;
            client.ssh_forward = self._ssh_forward;
            client
        };
        Ok(client)
    }

    pub fn connect(path: &Path) -> Result<Self, DaemonError> {
        Self::connect_route(CommandRoute {
            socket: path.to_path_buf(),
            display: path.display().to_string(),
            facts: EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
            send_origin: true,
        })
    }

    fn connect_route(route: CommandRoute) -> Result<Self, DaemonError> {
        let link = route.connect()?;
        Ok(Self::with_link(route, link))
    }

    pub(super) fn with_link(route: CommandRoute, link: CommandLink) -> Self {
        let server_id = match &link {
            CommandLink::Legacy { server_id, .. } => Some(*server_id),
            CommandLink::Exec { .. } => None,
        };
        Self {
            stdin_enabled: false,
            stdin_spent: false,
            file_grants: None,
            stderr_handler: None,
            stdout_handler: None,
            link,
            route,
            server_id,
            #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
            _ssh_forward: None,
            #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
            _tailnet_forward: None,
            #[cfg(windows)]
            _wsl_forward: None,
            #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
            unverified: None,
            #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
            read_only: false,
        }
    }

    pub fn connect_endpoint(endpoint: &Endpoint, read_only: bool) -> Result<Self, DaemonError> {
        Self::connect_endpoint_as(endpoint, read_only, false)
    }

    pub fn connect_endpoint_unattended(endpoint: &Endpoint) -> Result<Self, DaemonError> {
        Self::connect_endpoint_as(endpoint, false, true)
    }

    fn connect_endpoint_as(
        endpoint: &Endpoint,
        read_only: bool,
        unattended: bool,
    ) -> Result<Self, DaemonError> {
        #[cfg(not(all(unix, not(any(target_os = "ios", target_os = "android")))))]
        let _ = read_only;
        #[cfg(not(all(any(unix, windows), not(any(target_os = "ios", target_os = "android")))))]
        let _ = unattended;
        match endpoint {
            Endpoint::Local(path) => Self::connect_route(CommandRoute {
                socket: path.clone(),
                display: path.display().to_string(),
                facts: EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
                send_origin: false,
            }),
            Endpoint::Tailnet(endpoint) => {
                #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
                {
                    let forward = crate::tailnet::TailnetForward::start(endpoint)
                        .map_err(|error| tailnet_failure(endpoint, &error))?;
                    let mut client = Self::connect_route(CommandRoute {
                        socket: forward.socket().to_path_buf(),
                        display: endpoint.to_string(),
                        facts: EndpointFactsScope::PortableTerminalSize,
                        send_origin: false,
                    })
                    .map_err(|error| tailnet_closed(endpoint, error))?;
                    client._tailnet_forward = Some(forward);
                    Ok(client)
                }
                #[cfg(not(all(
                    any(unix, windows),
                    not(any(target_os = "ios", target_os = "android"))
                )))]
                {
                    let _ = endpoint;
                    Err(crate::EndpointError::UnsupportedPlatform.into())
                }
            }
            Endpoint::Wsl(endpoint) => {
                #[cfg(windows)]
                {
                    let (forward, stream) = start_wsl_forward(endpoint)?;
                    let mut client = Self::with_link(
                        CommandRoute {
                            socket: forward.local_socket().to_path_buf(),
                            display: endpoint.to_string(),
                            facts: EndpointFactsScope::PortableTerminalSize,
                            send_origin: false,
                        },
                        CommandLink::exec(stream)?,
                    );
                    client._wsl_forward = Some(forward);
                    Ok(client)
                }
                #[cfg(not(windows))]
                {
                    let _ = endpoint;
                    Err(crate::EndpointError::UnsupportedPlatform.into())
                }
            }
            Endpoint::Ssh(endpoint) => {
                #[cfg(any(target_os = "ios", target_os = "android"))]
                {
                    let _ = endpoint;
                    Err(crate::EndpointError::UnsupportedPlatform.into())
                }
                #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
                {
                    let login = if unattended {
                        SshLogin::Unattended
                    } else {
                        SshLogin::Prompts(None)
                    };
                    let (mut client, ssh_forward) =
                        connect_kept(endpoint, login, true, |socket, stream| {
                            Ok(Self::with_link(
                                CommandRoute {
                                    socket: socket.to_path_buf(),
                                    display: endpoint.to_string(),
                                    facts: EndpointFactsScope::PortableTerminalSize,
                                    send_origin: false,
                                },
                                CommandLink::exec(stream)?,
                            ))
                        })?;
                    #[cfg(unix)]
                    {
                        client.unverified = ssh_forward.is_none().then(|| endpoint.clone());
                        client.read_only = read_only;
                    }
                    client._ssh_forward = ssh_forward;
                    Ok(client)
                }
                #[cfg(not(any(unix, windows)))]
                {
                    let _ = endpoint;
                    Err(crate::EndpointError::UnsupportedPlatform.into())
                }
            }
        }
    }

    pub fn server_id(&mut self) -> Result<u64, DaemonError> {
        if let Some(server_id) = self.server_id {
            return Ok(server_id);
        }
        self.exec_chain(ExecChain::new(Vec::new()), |_| 0)?;
        self.server_id.ok_or_else(|| {
            DaemonError::Server(ServerError::Internal(
                "daemon did not report its identity".to_owned(),
            ))
        })
    }

    pub fn set_stderr_handler(&mut self, handler: fn(&str)) {
        self.stderr_handler = Some(handler);
    }

    pub fn set_stdout_handler(&mut self, handler: impl Fn(&RawText) + Send + Sync + 'static) {
        self.stdout_handler = Some(Arc::new(handler));
    }

    pub fn enable_stdin(&mut self) {
        self.stdin_enabled = true;
    }

    pub fn execute(&mut self, command: CommandInvocation) -> Result<String, DaemonError> {
        stdout_or_exit(self.execute_streams(command)?)
    }

    pub fn execute_on_server(
        &mut self,
        server_id: u64,
        command: CommandInvocation,
    ) -> Result<Option<String>, DaemonError> {
        let mut chain = ExecChain::new(vec![command]);
        chain.expect_server_id = Some(server_id);
        self.run_single(chain)?.map(stdout_or_exit).transpose()
    }

    pub fn execute_streams(
        &mut self,
        command: CommandInvocation,
    ) -> Result<CommandOutcome, DaemonError> {
        if matches!(self.link, CommandLink::Legacy { .. }) {
            self.grant_files(std::slice::from_ref(&command));
            return self.execute_streams_with_prepared(command, false);
        }
        self.run_single(ExecChain::new(vec![command]))?
            .ok_or_else(|| {
                DaemonError::Server(ServerError::Internal("daemon ran no command".to_owned()))
            })
    }

    pub fn execute_prepared_streams(
        &mut self,
        command: CommandInvocation,
    ) -> Result<CommandOutcome, DaemonError> {
        if matches!(self.link, CommandLink::Legacy { .. }) {
            self.grant_files(std::slice::from_ref(&command));
            return self.execute_streams_with_prepared(command, true);
        }
        let mut chain = ExecChain::new(vec![command]);
        chain.prepared = true;
        self.run_single(chain)?.ok_or_else(|| {
            DaemonError::Server(ServerError::Internal("daemon ran no command".to_owned()))
        })
    }

    fn grant_files(&mut self, commands: &[CommandInvocation]) {
        self.file_grants = self.route.facts.is_remote().then(|| file_grants(commands));
    }

    fn unanswered(&self, error: DaemonError) -> DaemonError {
        #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
        if let Some(forward) = &self._tailnet_forward {
            return tailnet_closed(forward.endpoint(), error);
        }
        error
    }

    fn run_single(&mut self, chain: ExecChain<'_>) -> Result<Option<CommandOutcome>, DaemonError> {
        let mut outcome = None;
        let end = self.exec_chain(chain, |ran| {
            outcome = Some(ran.clone());
            0
        })?;
        match end {
            ExecChainEnd::Rejected(error) => Err(DaemonError::Server(error)),
            ExecChainEnd::Failed(error) => Err(error),
            ExecChainEnd::ServerMismatch => Ok(None),
            ExecChainEnd::Ran { .. } | ExecChainEnd::Resume(_) => {
                outcome.map(Some).ok_or_else(|| {
                    DaemonError::Server(ServerError::Internal("daemon ran no command".to_owned()))
                })
            }
        }
    }

    pub fn exec_chain(
        &mut self,
        chain: ExecChain<'_>,
        mut emit: impl FnMut(&CommandOutcome) -> u8,
    ) -> Result<ExecChainEnd, DaemonError> {
        self.grant_files(&chain.commands);
        if matches!(self.link, CommandLink::Legacy { .. }) {
            return self.legacy_exec_chain(chain, emit);
        }
        if matches!(self.link, CommandLink::Exec { spent: true, .. }) {
            self.link = self.route.connect()?;
        }
        #[cfg(unix)]
        let _signal = self
            .stdin_enabled
            .then(StdinReadSignal::install)
            .transpose()?;
        let ExecChain {
            commands,
            spawned_server_id,
            expect_server_id,
            resume,
            prepared,
            last,
        } = chain;
        let mut flags = ExecFlags::default();
        flags.set(ExecFlags::STDIN_AVAILABLE, self.stdin_enabled);
        flags.set(ExecFlags::RESUME, resume.is_some());
        flags.set(ExecFlags::PREPARED, prepared);
        flags.set(ExecFlags::LAST, last);
        let request = ProtocolMessage::Exec(self.route.exec_request(
            commands,
            flags,
            spawned_server_id,
            expect_server_id,
        ));
        #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
        let verified = self.verify_kept_link(&request)?;
        #[cfg(not(all(unix, not(any(target_os = "ios", target_os = "android")))))]
        let verified = false;
        let CommandLink::Exec {
            reader,
            writer,
            answered,
            spent,
        } = &mut self.link
        else {
            unreachable!("legacy links returned above");
        };
        *spent = last;
        let sent = if verified {
            Ok(())
        } else {
            writer.send(&request)
        };
        let first = match sent {
            Ok(()) => reader.recv(),
            Err(error) => Err(error),
        };
        let first = match first {
            Err(error) if !*answered && exec_maybe_unsupported(&error) => {
                let Ok((link, speaks_exec)) = self.route.connect_legacy() else {
                    return Ok(ExecChainEnd::Failed(self.unanswered(error)));
                };
                if speaks_exec {
                    return Ok(ExecChainEnd::Failed(self.unanswered(error)));
                }
                log::debug!(
                    target: "zz_daemon::diagnostics::client",
                    "daemon did not take Exec ({error}); retrying over the hello path",
                );
                let ProtocolMessage::Exec(request) = request else {
                    unreachable!("request is an Exec");
                };
                self.link = link;
                if let CommandLink::Legacy { server_id, .. } = &self.link {
                    self.server_id = Some(*server_id);
                }
                return self.legacy_exec_chain(
                    ExecChain {
                        commands: request.commands,
                        spawned_server_id,
                        expect_server_id,
                        resume,
                        prepared,
                        last,
                    },
                    emit,
                );
            }
            result => result?,
        };
        if let ProtocolMessage::CommandResponse(CommandResponse::Error {
            request_id: 0,
            error,
            ..
        }) = first
        {
            return Err(DaemonError::Server(error));
        }
        *answered = true;
        let mut message = Some(first);
        let mut exit_code = 0u8;
        let mut pending_error = None;
        let mut streamed_stderr = String::new();
        let mut client_exit = false;
        let mut unanswered = None;
        loop {
            let current = match message.take() {
                Some(current) => current,
                None => match reader.recv() {
                    Ok(current) => current,
                    Err(error) => {
                        return Ok(ExecChainEnd::Failed(
                            pending_error.or(unanswered).unwrap_or(error),
                        ));
                    }
                },
            };
            match current {
                ProtocolMessage::ExecExit(finished) => {
                    self.server_id = Some(finished.server_id);
                    if let Some(error) = pending_error {
                        return Ok(ExecChainEnd::Failed(error));
                    }
                    return Ok(match finished.outcome {
                        ExecOutcome::Ran => ExecChainEnd::Ran { exit_code },
                        ExecOutcome::Resume(resume) => ExecChainEnd::Resume(resume),
                        ExecOutcome::Rejected(error) => ExecChainEnd::Rejected(error),
                        ExecOutcome::ServerMismatch => ExecChainEnd::ServerMismatch,
                    });
                }
                ProtocolMessage::CommandResponse(CommandResponse::Error {
                    error, output, ..
                }) => {
                    pending_error = Some(command_failure(error, output));
                }
                ProtocolMessage::CommandResponse(CommandResponse::Success {
                    output,
                    exit_code: status,
                    stderr,
                    stdout_claim,
                    ..
                }) => {
                    let outcome = CommandOutcome {
                        stdout: output,
                        stderr: stderr
                            .strip_prefix(&streamed_stderr)
                            .unwrap_or(&stderr)
                            .to_owned(),
                        exit_code: status,
                        stdout_claim,
                        client_exit: std::mem::take(&mut client_exit),
                    };
                    streamed_stderr.clear();
                    let output_status = emit(&outcome);
                    if outcome.exit_code != 0 {
                        exit_code = outcome.exit_code;
                    }
                    if output_status != 0 {
                        exit_code = output_status;
                    }
                }
                ProtocolMessage::Event(zz_protocol::Event {
                    payload:
                        zz_protocol::EventPayload::ClientMessage {
                            kind: zz_protocol::ClientMessageKind::Error,
                            text,
                            ..
                        },
                    ..
                }) if self.stderr_handler.is_some() => {
                    let line = format!("{text}\n");
                    self.stderr_handler.expect("stderr handler checked")(&line);
                    streamed_stderr.push_str(&line);
                }
                ProtocolMessage::Event(zz_protocol::Event {
                    payload: zz_protocol::EventPayload::CommandStdout { output },
                    ..
                }) => {
                    if let Some(handler) = &self.stdout_handler {
                        handler(&output);
                    }
                }
                ProtocolMessage::Event(zz_protocol::Event {
                    payload: zz_protocol::EventPayload::CommandClientExit,
                    ..
                }) => {
                    client_exit = true;
                }
                ProtocolMessage::ClientFileRequest(request) => {
                    let response = answer_command_file(
                        &request,
                        self.file_grants.as_deref(),
                        self.stdin_enabled,
                        &mut self.stdin_spent,
                    );
                    if let Err(error) = writer.send(&ProtocolMessage::ClientFileResponse(response))
                    {
                        unanswered.get_or_insert(error);
                    }
                }
                _ => {}
            }
        }
    }

    #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
    fn verify_kept_link(&mut self, request: &ProtocolMessage) -> Result<bool, DaemonError> {
        self.verify_kept_link_within(
            request,
            crate::endpoint::MASTER_STEP_TIMEOUT,
            crate::endpoint::give_up_on_stalled_link,
        )
    }

    #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
    pub(super) fn verify_kept_link_within(
        &mut self,
        request: &ProtocolMessage,
        timeout: std::time::Duration,
        give_up: impl FnOnce(&crate::endpoint::SshEndpoint) -> crate::EndpointError,
    ) -> Result<bool, DaemonError> {
        let Some(endpoint) = self.unverified.take() else {
            return Ok(false);
        };
        let probe = self.route.probe();
        let CommandLink::Exec {
            reader,
            writer,
            answered,
            ..
        } = &mut self.link
        else {
            return Ok(false);
        };
        let reply = writer
            .stream
            .set_timeout(Some(timeout))
            .map_err(DaemonError::from)
            .and_then(|()| writer.send(&probe))
            .and_then(|()| writer.send(request))
            .and_then(|()| reader.recv());
        let (reason, stalled) = match reply {
            Ok(ProtocolMessage::ExecExit(exit)) => {
                let _ = writer.stream.set_timeout(None);
                self.server_id = Some(exit.server_id);
                *answered = true;
                return Ok(true);
            }
            Ok(_) => ("an unexpected reply".to_owned(), false),
            Err(error) => (error.to_string(), crate::endpoint::link_stalled(&error)),
        };
        if stalled && !self.read_only {
            return Err(give_up(&endpoint).into());
        }
        log::debug!(
            target: "zz_daemon::diagnostics::client",
            "the kept forward to {endpoint} gave {reason}; making it again",
        );
        let route = &self.route;
        let (socket, forward) =
            crate::endpoint::remake_kept_forward(&endpoint, stalled, |socket| {
                route.answers_at(socket)
            })?;
        self.route.socket = socket;
        self._ssh_forward = forward;
        self.link = self.route.connect()?;
        Ok(false)
    }

    fn legacy_exec_chain(
        &mut self,
        chain: ExecChain<'_>,
        mut emit: impl FnMut(&CommandOutcome) -> u8,
    ) -> Result<ExecChainEnd, DaemonError> {
        let CommandLink::Legacy { server_id, .. } = self.link else {
            unreachable!("legacy chains run on a hello link");
        };
        if chain
            .expect_server_id
            .is_some_and(|expected| expected != server_id)
        {
            return Ok(ExecChainEnd::ServerMismatch);
        }
        if chain.commands.is_empty() {
            return Ok(ExecChainEnd::Ran { exit_code: 0 });
        }
        if chain.prepared {
            let mut exit_code = 0;
            for command in chain.commands {
                let outcome = match self.execute_streams_with_prepared(command, true) {
                    Ok(outcome) => outcome,
                    Err(error) => return Ok(ExecChainEnd::Failed(error)),
                };
                let output_status = emit(&outcome);
                if outcome.exit_code != 0 {
                    exit_code = outcome.exit_code;
                }
                if output_status != 0 {
                    exit_code = output_status;
                }
                if outcome.client_exit {
                    break;
                }
            }
            return Ok(ExecChainEnd::Ran { exit_code });
        }
        let typed = chain.resume.is_some().then(|| chain.commands.clone());
        let prepared = match chain.spawned_server_id {
            Some(spawned) => self.prepare_commands_or_stop_empty(chain.commands, spawned)?,
            None => self.prepare_commands(chain.commands)?,
        };
        if let Some(error) = prepared.iter().find_map(|command| match &command.result {
            PreparedCommandResult::Ready => None,
            PreparedCommandResult::Error(error) => Some(error.clone()),
        }) {
            return Ok(ExecChainEnd::Rejected(error));
        }
        if let (Some(classify), Some(typed)) = (chain.resume, typed)
            && let Some(kind) = classify(&typed, &prepared)
        {
            return Ok(ExecChainEnd::Resume(ExecResume {
                kind,
                commands: prepared,
            }));
        }
        let mut exit_code = 0;
        for command in prepared {
            let outcome = match self.execute_streams_with_prepared(command.invocation, true) {
                Ok(outcome) => outcome,
                Err(error) => return Ok(ExecChainEnd::Failed(error)),
            };
            let output_status = emit(&outcome);
            if outcome.exit_code != 0 {
                exit_code = outcome.exit_code;
            }
            if output_status != 0 {
                exit_code = output_status;
            }
            if outcome.client_exit {
                break;
            }
        }
        Ok(ExecChainEnd::Ran { exit_code })
    }

    fn legacy_link(
        &mut self,
    ) -> (
        &mut ProtocolReceiver<LocalStream>,
        &mut ProtocolSender<LocalStream>,
    ) {
        match &mut self.link {
            CommandLink::Legacy { reader, writer, .. }
            | CommandLink::Exec { reader, writer, .. } => (reader, writer),
        }
    }

    fn execute_streams_with_prepared(
        &mut self,
        command: CommandInvocation,
        prepared: bool,
    ) -> Result<CommandOutcome, DaemonError> {
        #[cfg(unix)]
        let _signal = self
            .stdin_enabled
            .then(StdinReadSignal::install)
            .transpose()?;
        let mut command = command;
        command.set_stdin_available(self.stdin_enabled);
        let mut streamed_stderr = String::new();
        let mut client_exit = false;
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let stdin_enabled = self.stdin_enabled;
        let stderr_handler = self.stderr_handler;
        let stdout_handler = self.stdout_handler.clone();
        let mut stdin_spent = self.stdin_spent;
        let file_grants = self.file_grants.clone();
        let (reader, writer) = self.legacy_link();
        let result = (|| {
            writer.send(&ProtocolMessage::CommandRequest(CommandRequest {
                request_id,
                command,
                prepared,
            }))?;
            loop {
                match reader.recv()? {
                    ProtocolMessage::CommandResponse(CommandResponse::Success {
                        request_id: response_id,
                        output,
                        exit_code,
                        stderr,
                        stdout_claim,
                    }) if response_id == request_id => {
                        return Ok(CommandOutcome {
                            stdout: output,
                            stderr: stderr
                                .strip_prefix(&streamed_stderr)
                                .unwrap_or(&stderr)
                                .to_owned(),
                            exit_code,
                            stdout_claim,
                            client_exit,
                        });
                    }
                    ProtocolMessage::CommandResponse(CommandResponse::Error {
                        request_id: response_id,
                        error,
                        output,
                    }) if response_id == request_id => {
                        return Err(command_failure(error, output));
                    }
                    ProtocolMessage::Event(zz_protocol::Event {
                        payload:
                            zz_protocol::EventPayload::ClientMessage {
                                kind: zz_protocol::ClientMessageKind::Error,
                                text,
                                ..
                            },
                        ..
                    }) if stderr_handler.is_some() => {
                        let line = format!("{text}\n");
                        stderr_handler.expect("stderr handler checked")(&line);
                        streamed_stderr.push_str(&line);
                    }
                    ProtocolMessage::Event(zz_protocol::Event {
                        payload: zz_protocol::EventPayload::CommandStdout { output },
                        ..
                    }) => {
                        if let Some(handler) = &stdout_handler {
                            handler(&output);
                        }
                    }
                    ProtocolMessage::Event(zz_protocol::Event {
                        payload: zz_protocol::EventPayload::CommandClientExit,
                        ..
                    }) => {
                        client_exit = true;
                    }
                    ProtocolMessage::ClientFileRequest(request) => {
                        let response = answer_command_file(
                            &request,
                            file_grants.as_deref(),
                            stdin_enabled,
                            &mut stdin_spent,
                        );
                        writer.send(&ProtocolMessage::ClientFileResponse(response))?;
                    }
                    _ => {}
                }
            }
        })();
        self.stdin_spent = stdin_spent;
        result
    }

    fn prepare_commands(
        &mut self,
        commands: Vec<CommandInvocation>,
    ) -> Result<Vec<PreparedCommand>, DaemonError> {
        self.prepare_commands_with_count(commands, None)
    }

    fn prepare_commands_or_stop_empty(
        &mut self,
        mut commands: Vec<CommandInvocation>,
        server_id: u64,
    ) -> Result<Vec<PreparedCommand>, DaemonError> {
        let command_count = commands.len();
        commands.push(CommandInvocation::new(
            crate::COLD_START_PREPARE_ABORT_COMMAND,
            [server_id.to_string()],
        ));
        self.prepare_commands_with_count(commands, Some(command_count))
    }

    fn prepare_commands_with_count(
        &mut self,
        commands: Vec<CommandInvocation>,
        command_count: Option<usize>,
    ) -> Result<Vec<PreparedCommand>, DaemonError> {
        let command_count = command_count.unwrap_or(commands.len());
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let (reader, writer) = self.legacy_link();
        writer.send(&ProtocolMessage::PrepareCommandList {
            request_id,
            commands,
        })?;
        loop {
            if let ProtocolMessage::PreparedCommandList {
                request_id: response_id,
                commands,
            } = reader.recv()?
                && response_id == request_id
            {
                if commands.len() != command_count {
                    return Err(DaemonError::Io(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "daemon returned the wrong prepared-command count",
                    )));
                }
                return Ok(commands);
            }
        }
    }
}
