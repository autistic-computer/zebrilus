use super::*;

impl InteractiveClient {
    pub fn connect_endpoint_with_attach(
        endpoint: &Endpoint,
        attach: zz_protocol::AttachOperation,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_attach(
            endpoint,
            None,
            None,
            &[
                crate::CLIENT_EXITS_ON_DETACH_CAPABILITY,
                zz_protocol::TTY_INPUT_CAPABILITY,
            ],
            Some(attach),
            true,
        )
    }

    pub fn connect_endpoint_with_prompts_and_attach(
        endpoint: &Endpoint,
        color_scheme: Option<TerminalColorScheme>,
        prompts: Option<crate::askpass::SshPrompts>,
        capabilities: &[&str],
        attach: Option<zz_protocol::AttachOperation>,
        terminal_surface: bool,
    ) -> Result<Self, DaemonError> {
        let client_has_terminal = !terminal_surface
            || (std::io::stdin().is_terminal() && std::io::stdout().is_terminal());
        match endpoint {
            Endpoint::Local(path) => {
                let stream = LocalTransport::connect(path)?;
                let connected = connect_stream_hello(
                    ClientStream::Local(stream),
                    path.display(),
                    ClientKind::Interactive,
                    short_device_name(),
                    color_scheme,
                    client_has_terminal,
                    false,
                    false,
                    capabilities,
                    if terminal_surface {
                        EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal
                    } else {
                        EndpointFactsScope::LocalHostWorkingDirectory
                    },
                    attach,
                )?;
                Ok(Self::from_connected(connected))
            }
            Endpoint::Tailnet(endpoint) => {
                #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
                {
                    let (forward, stream) = start_tailnet_forward(endpoint)?;
                    let connected = connect_stream_hello(
                        ClientStream::Local(stream),
                        endpoint,
                        ClientKind::Interactive,
                        short_device_name(),
                        color_scheme,
                        client_has_terminal,
                        false,
                        false,
                        capabilities,
                        if terminal_surface {
                            EndpointFactsScope::PortableTerminalSize
                        } else {
                            EndpointFactsScope::None
                        },
                        attach,
                    )
                    .map_err(|error| tailnet_closed(endpoint, error))?;
                    let mut client = Self::from_connected(connected);
                    client.tailnet_forward = Some(forward);
                    Ok(client)
                }
                #[cfg(any(target_os = "ios", target_os = "android"))]
                {
                    let stream = connect_tailnet_tcp(endpoint)?;
                    let connected = connect_stream_hello(
                        ClientStream::Tcp(stream),
                        endpoint,
                        ClientKind::Interactive,
                        short_device_name(),
                        color_scheme,
                        client_has_terminal,
                        false,
                        false,
                        capabilities,
                        if terminal_surface {
                            EndpointFactsScope::PortableTerminalSize
                        } else {
                            EndpointFactsScope::None
                        },
                        attach,
                    )
                    .map_err(|error| tailnet_closed(endpoint, error))?;
                    Ok(Self::from_connected(connected))
                }
                #[cfg(not(any(
                    all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))),
                    target_os = "ios",
                    target_os = "android"
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
                    let connected = connect_stream_hello(
                        ClientStream::Local(stream),
                        endpoint,
                        ClientKind::Interactive,
                        short_device_name(),
                        color_scheme,
                        client_has_terminal,
                        false,
                        false,
                        capabilities,
                        if terminal_surface {
                            EndpointFactsScope::PortableTerminalSize
                        } else {
                            EndpointFactsScope::None
                        },
                        attach,
                    )?;
                    let mut client = Self::from_connected(connected);
                    client.wsl_forward = Some(forward);
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
                    let (forward, stream) = RusshForward::start(endpoint, prompts)?;
                    let connected = connect_stream_hello(
                        ClientStream::Ssh(stream),
                        endpoint,
                        ClientKind::Interactive,
                        short_device_name(),
                        color_scheme,
                        client_has_terminal,
                        false,
                        false,
                        capabilities,
                        if terminal_surface {
                            EndpointFactsScope::PortableTerminalSize
                        } else {
                            EndpointFactsScope::None
                        },
                        attach,
                    )?;
                    let mut client = Self::from_connected(connected);
                    client.russh_forward = Some(forward);
                    Ok(client)
                }
                #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
                {
                    let resendable = attach_resendable(attach.as_ref());
                    let (connected, forward) = connect_kept(
                        endpoint,
                        SshLogin::Prompts(prompts),
                        resendable,
                        |_, stream| {
                            connect_stream_hello(
                                ClientStream::Local(stream),
                                endpoint,
                                ClientKind::Interactive,
                                short_device_name(),
                                color_scheme,
                                client_has_terminal,
                                false,
                                false,
                                capabilities,
                                if terminal_surface {
                                    EndpointFactsScope::PortableTerminalSize
                                } else {
                                    EndpointFactsScope::None
                                },
                                attach.clone(),
                            )
                        },
                    )?;
                    Ok(Self::from_connected_with_ssh(connected, forward))
                }
                #[cfg(not(any(unix, windows)))]
                {
                    let _ = (endpoint, prompts, capabilities, attach, terminal_surface);
                    Err(crate::EndpointError::UnsupportedPlatform.into())
                }
            }
        }
    }

    pub fn take_pending_message(&self) -> Option<ProtocolMessage> {
        loop {
            let message = self.reader.lock().pending.pop_front()?;
            if self.answer_ping(&message).is_err() {
                return None;
            }
            if !matches!(message, ProtocolMessage::Ping { .. }) {
                return Some(message);
            }
        }
    }

    fn answer_ping(&self, message: &ProtocolMessage) -> Result<(), DaemonError> {
        if let ProtocolMessage::Ping { token, .. } = message {
            self.send(&ProtocolMessage::Pong { token: *token })?;
        }
        Ok(())
    }

    pub fn is_initially_attached(&self) -> bool {
        self.reader
            .lock()
            .pending
            .iter()
            .any(|message| match message {
                ProtocolMessage::Batch(batch) => batch.messages().is_ok_and(|messages| {
                    messages.into_iter().any(|message| {
                        matches!(
                            message,
                            ProtocolMessage::Event(zz_protocol::Event {
                                payload: zz_protocol::EventPayload::ClientView(
                                    zz_protocol::ClientView {
                                        session: Some(_),
                                        ..
                                    }
                                ),
                                ..
                            }) | ProtocolMessage::Attached { .. }
                        )
                    })
                }),
                ProtocolMessage::Attached { .. } => true,
                _ => false,
            })
    }

    pub fn take_initial_responses(&self) -> Vec<CommandResponse> {
        let attached = self.is_initially_attached();
        let mut reader = self.reader.lock();
        let mut responses = Vec::new();
        for message in &mut reader.pending {
            if let ProtocolMessage::Batch(batch) = message {
                batch.frames.retain(|frame| {
                    if let Ok(ProtocolMessage::CommandResponse(response)) =
                        zz_protocol::decode_protocol_frame(frame)
                    {
                        if attached && matches!(response, CommandResponse::Error { .. }) {
                            return true;
                        }
                        responses.push(response);
                        false
                    } else {
                        true
                    }
                });
            }
        }
        responses
    }

    pub fn execute_chain(&self, commands: Vec<CommandInvocation>) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::Exec(ExecRequest {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: self.hello.client_instance_id,
            process_id: std::process::id(),
            origin: None,
            working_directory: None,
            tty: None,
            size: None,
            features: 0,
            environment: ClientEnvironmentBlob::default(),
            startup_reentry: None,
            spawned_server_id: None,
            expect_server_id: Some(self.hello.server_id),
            flags: ExecFlags::default(),
            commands,
            raw_control_line: None,
        }))
    }

    pub fn execute_control_line(&self, line: String) -> Result<(), DaemonError> {
        let request = ExecRequest {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: self.hello.client_instance_id,
            process_id: std::process::id(),
            origin: None,
            working_directory: None,
            tty: None,
            size: None,
            features: 0,
            environment: ClientEnvironmentBlob::default(),
            startup_reentry: None,
            spawned_server_id: None,
            expect_server_id: Some(self.hello.server_id),
            flags: ExecFlags::default(),
            commands: Vec::new(),
            raw_control_line: Some(line),
        };
        self.send(&ProtocolMessage::Exec(request))
    }

    pub fn request_tree_sync(&self) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::TreeSync)
    }

    pub fn connect(path: &Path) -> Result<Self, DaemonError> {
        Self::connect_with_color_scheme(path, TerminalColorScheme::Dark)
    }

    pub fn connect_control(path: &Path) -> Result<Self, DaemonError> {
        Self::connect_control_with_startup_owner(path, false)
    }

    pub fn connect_control_endpoint(endpoint: &Endpoint) -> Result<Self, DaemonError> {
        match endpoint {
            Endpoint::Local(path) => Self::connect_control(path),
            Endpoint::Tailnet(endpoint) => {
                #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
                {
                    let (forward, stream) = start_tailnet_forward(endpoint)?;
                    let connected = connect_stream_with_startup_owner(
                        ClientStream::Local(stream),
                        endpoint,
                        ClientKind::Control,
                        None,
                        None,
                        false,
                        false,
                        false,
                        &[],
                        EndpointFactsScope::None,
                    )
                    .map_err(|error| tailnet_closed(endpoint, error))?;
                    let mut client = Self::from_connected(connected);
                    client.tailnet_forward = Some(forward);
                    Ok(client)
                }
                #[cfg(any(target_os = "ios", target_os = "android"))]
                {
                    let stream = connect_tailnet_tcp(endpoint)?;
                    let connected = connect_stream_with_startup_owner(
                        ClientStream::Tcp(stream),
                        endpoint,
                        ClientKind::Control,
                        None,
                        None,
                        false,
                        false,
                        false,
                        &[],
                        EndpointFactsScope::None,
                    )
                    .map_err(|error| tailnet_closed(endpoint, error))?;
                    Ok(Self::from_connected(connected))
                }
                #[cfg(not(any(
                    all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))),
                    target_os = "ios",
                    target_os = "android"
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
                    let connected = connect_stream_with_startup_owner(
                        ClientStream::Local(stream),
                        endpoint,
                        ClientKind::Control,
                        None,
                        None,
                        false,
                        false,
                        false,
                        &[],
                        EndpointFactsScope::None,
                    )?;
                    let mut client = Self::from_connected(connected);
                    client.wsl_forward = Some(forward);
                    Ok(client)
                }
                #[cfg(not(windows))]
                {
                    let _ = endpoint;
                    Err(crate::EndpointError::UnsupportedPlatform.into())
                }
            }
            Endpoint::Ssh(endpoint) => {
                #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
                {
                    let (connected, forward) =
                        connect_kept(endpoint, SshLogin::Prompts(None), true, |_, stream| {
                            connect_stream_with_startup_owner(
                                ClientStream::Local(stream),
                                endpoint,
                                ClientKind::Control,
                                None,
                                None,
                                false,
                                false,
                                false,
                                &[],
                                EndpointFactsScope::None,
                            )
                        })?;
                    Ok(Self::from_connected_with_ssh(connected, forward))
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
        }
    }

    pub fn connect_control_with_startup_owner(
        path: &Path,
        startup_config_owner: bool,
    ) -> Result<Self, DaemonError> {
        let stream = LocalTransport::connect(path)?;
        let connected = connect_stream_with_startup_owner(
            ClientStream::Local(stream),
            path.display(),
            ClientKind::Control,
            None,
            None,
            false,
            false,
            startup_config_owner,
            &[],
            EndpointFactsScope::LocalControlTerminalIdentity,
        )?;
        Ok(Self::from_connected(connected))
    }

    pub fn connect_with_color_scheme(
        path: &Path,
        color_scheme: TerminalColorScheme,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            &Endpoint::Local(path.to_owned()),
            Some(color_scheme),
            None,
            true,
            false,
            &[],
            false,
        )
    }

    pub fn connect_with_color_scheme_and_terminal(
        path: &Path,
        color_scheme: TerminalColorScheme,
        client_has_terminal: bool,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            &Endpoint::Local(path.to_owned()),
            Some(color_scheme),
            None,
            client_has_terminal,
            false,
            &[],
            false,
        )
    }

    pub fn connect_with_capabilities(
        path: &Path,
        color_scheme: TerminalColorScheme,
        client_has_terminal: bool,
        capabilities: &[&str],
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            &Endpoint::Local(path.to_owned()),
            Some(color_scheme),
            None,
            client_has_terminal,
            false,
            capabilities,
            false,
        )
    }

    #[cfg(unix)]
    pub fn connect_terminal_surface_with_timeout(
        path: &Path,
        color_scheme: TerminalColorScheme,
        timeout: std::time::Duration,
    ) -> Result<Self, DaemonError> {
        let stream = LocalTransport::connect(path)?;
        stream.set_timeout(Some(timeout))?;
        let control = stream.try_clone()?;
        let connected = connect_stream(
            ClientStream::Local(stream),
            path.display(),
            ClientKind::Interactive,
            short_device_name(),
            Some(color_scheme),
            true,
            false,
            EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
        )?;
        control.set_timeout(None)?;
        Ok(Self::from_connected(connected))
    }

    pub fn connect_terminal_surface(
        path: &Path,
        color_scheme: TerminalColorScheme,
        client_has_terminal: bool,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            &Endpoint::Local(path.to_owned()),
            Some(color_scheme),
            None,
            client_has_terminal,
            true,
            &[],
            false,
        )
    }

    pub fn connect_endpoint(
        endpoint: &Endpoint,
        color_scheme: TerminalColorScheme,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            endpoint,
            Some(color_scheme),
            None,
            true,
            true,
            &[],
            false,
        )
    }

    pub fn connect_endpoint_with_terminal(
        endpoint: &Endpoint,
        color_scheme: TerminalColorScheme,
        client_has_terminal: bool,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            endpoint,
            Some(color_scheme),
            None,
            client_has_terminal,
            true,
            &[],
            false,
        )
    }

    pub fn connect_endpoint_with_prompts(
        endpoint: &Endpoint,
        color_scheme: TerminalColorScheme,
        prompts: Option<crate::askpass::SshPrompts>,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            endpoint,
            Some(color_scheme),
            prompts,
            true,
            false,
            &[],
            true,
        )
    }

    pub fn connect_endpoint_with_prompts_and_capabilities(
        endpoint: &Endpoint,
        color_scheme: TerminalColorScheme,
        prompts: Option<crate::askpass::SshPrompts>,
        capabilities: &[&str],
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            endpoint,
            Some(color_scheme),
            prompts,
            true,
            false,
            capabilities,
            true,
        )
    }

    pub fn connect_terminal_surface_endpoint_with_prompts(
        endpoint: &Endpoint,
        color_scheme: TerminalColorScheme,
        prompts: Option<crate::askpass::SshPrompts>,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            endpoint,
            Some(color_scheme),
            prompts,
            true,
            true,
            &[],
            true,
        )
    }

    pub fn connect_endpoint_without_theme(
        endpoint: &Endpoint,
        client_has_terminal: bool,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_with_prompts_and_terminal(
            endpoint,
            None,
            None,
            client_has_terminal,
            true,
            &[],
            false,
        )
    }

    pub fn connect_terminal_surface_without_theme(
        path: &Path,
        client_has_terminal: bool,
    ) -> Result<Self, DaemonError> {
        Self::connect_endpoint_without_theme(&Endpoint::Local(path.to_owned()), client_has_terminal)
    }

    fn connect_endpoint_with_prompts_and_terminal(
        endpoint: &Endpoint,
        color_scheme: Option<TerminalColorScheme>,
        prompts: Option<crate::askpass::SshPrompts>,
        client_has_terminal: bool,
        terminal_surface: bool,
        extra_capabilities: &[&str],
        socks: bool,
    ) -> Result<Self, DaemonError> {
        let device_name = short_device_name();
        match endpoint {
            Endpoint::Local(path) => {
                let stream = LocalTransport::connect(path)?;
                let connected = connect_stream_with_startup_owner(
                    ClientStream::Local(stream),
                    path.display(),
                    ClientKind::Interactive,
                    device_name.clone(),
                    color_scheme,
                    client_has_terminal,
                    false,
                    false,
                    extra_capabilities,
                    if terminal_surface {
                        EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal
                    } else {
                        EndpointFactsScope::LocalHostWorkingDirectory
                    },
                )?;
                Ok(Self::from_connected(connected))
            }
            Endpoint::Tailnet(endpoint) => {
                #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
                {
                    let (forward, stream) = start_tailnet_forward(endpoint)?;
                    let connected = connect_stream_with_startup_owner(
                        ClientStream::Local(stream),
                        endpoint,
                        ClientKind::Interactive,
                        device_name.clone(),
                        color_scheme,
                        client_has_terminal,
                        false,
                        false,
                        extra_capabilities,
                        if terminal_surface {
                            EndpointFactsScope::PortableTerminalSize
                        } else {
                            EndpointFactsScope::None
                        },
                    )
                    .map_err(|error| tailnet_closed(endpoint, error))?;
                    let mut client = Self::from_connected(connected);
                    client.tailnet_forward = Some(forward);
                    Ok(client)
                }
                #[cfg(any(target_os = "ios", target_os = "android"))]
                {
                    let stream = connect_tailnet_tcp(endpoint)?;
                    let connected = connect_stream_with_startup_owner(
                        ClientStream::Tcp(stream),
                        endpoint,
                        ClientKind::Interactive,
                        device_name.clone(),
                        color_scheme,
                        client_has_terminal,
                        false,
                        false,
                        extra_capabilities,
                        if terminal_surface {
                            EndpointFactsScope::PortableTerminalSize
                        } else {
                            EndpointFactsScope::None
                        },
                    )
                    .map_err(|error| tailnet_closed(endpoint, error))?;
                    Ok(Self::from_connected(connected))
                }
                #[cfg(not(any(
                    all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))),
                    target_os = "ios",
                    target_os = "android"
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
                    let connected = connect_stream_with_startup_owner(
                        ClientStream::Local(stream),
                        endpoint,
                        ClientKind::Interactive,
                        device_name,
                        color_scheme,
                        client_has_terminal,
                        false,
                        false,
                        extra_capabilities,
                        if terminal_surface {
                            EndpointFactsScope::PortableTerminalSize
                        } else {
                            EndpointFactsScope::None
                        },
                    )?;
                    let mut client = Self::from_connected(connected);
                    client.wsl_forward = Some(forward);
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
                    let _ = socks;
                    let (russh_forward, stream) = RusshForward::start(endpoint, prompts)?;
                    let connected = connect_stream_with_startup_owner(
                        ClientStream::Ssh(stream),
                        endpoint,
                        ClientKind::Interactive,
                        device_name,
                        color_scheme,
                        client_has_terminal,
                        false,
                        false,
                        extra_capabilities,
                        if terminal_surface {
                            EndpointFactsScope::PortableTerminalSize
                        } else {
                            EndpointFactsScope::None
                        },
                    )?;
                    let mut client = Self::from_connected(connected);
                    client.russh_forward = Some(russh_forward);
                    Ok(client)
                }
                #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
                {
                    let connect = |stream: LocalStream| {
                        connect_stream_with_startup_owner(
                            ClientStream::Local(stream),
                            endpoint,
                            ClientKind::Interactive,
                            device_name.clone(),
                            color_scheme,
                            client_has_terminal,
                            false,
                            false,
                            extra_capabilities,
                            if terminal_surface {
                                EndpointFactsScope::PortableTerminalSize
                            } else {
                                EndpointFactsScope::None
                            },
                        )
                    };
                    let (connected, ssh_forward) = if socks {
                        let ssh_forward = SshForward::start(endpoint, SshLogin::Prompts(prompts))?;
                        let stream = LocalTransport::connect(ssh_forward.local_socket())?;
                        (connect(stream)?, Some(ssh_forward))
                    } else {
                        connect_kept(endpoint, SshLogin::Prompts(prompts), true, |_, stream| {
                            connect(stream)
                        })?
                    };
                    Ok(Self::from_connected_with_ssh(connected, ssh_forward))
                }
                #[cfg(not(any(unix, windows)))]
                {
                    let _ = (endpoint, prompts, socks);
                    Err(crate::EndpointError::UnsupportedPlatform.into())
                }
            }
        }
    }

    pub(super) fn from_connected((reader, writer, hello): Connected<ClientStream>) -> Self {
        let writer = Arc::new(Mutex::new(writer));
        register_interactive_writer(&writer);
        Self {
            reader: Mutex::new(reader),
            writer,
            hello,
            #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
            ssh_forward: None,
            #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
            tailnet_forward: None,
            #[cfg(windows)]
            wsl_forward: None,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            russh_forward: None,
        }
    }

    #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
    fn from_connected_with_ssh(
        connected: Connected<ClientStream>,
        ssh_forward: Option<SshForward>,
    ) -> Self {
        let mut client = Self::from_connected(connected);
        client.ssh_forward = ssh_forward;
        client
    }

    #[must_use]
    pub fn server_hello(&self) -> &ServerHello {
        &self.hello
    }

    #[must_use]
    pub fn socks_port(&self) -> Option<u16> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.russh_forward
                .as_ref()
                .and_then(RusshForward::socks_port)
        }
        #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
        {
            self.ssh_forward.as_ref().and_then(SshForward::socks_port)
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }

    pub fn set_loopback_discovery(&self, enabled: bool) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(forward) = &self.russh_forward {
            forward.set_loopback_discovery(enabled);
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let _ = enabled;
    }

    pub fn forward_loopback(&self, port: u16) -> io::Result<()> {
        if port == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "A nonzero localhost port is required.",
            ));
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(forward) = &self.russh_forward {
            return forward.forward_loopback(port);
        }
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Localhost forwarding requires an embedded SSH connection.",
        ))
    }

    pub fn attach(&self, session: impl Into<String>) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::Attach {
            session: session.into(),
        })
    }

    pub fn attach_default_in(&self, working_directory: &Path) -> Result<(), DaemonError> {
        if working_directory.is_absolute() && working_directory.is_dir() {
            self.send(&ProtocolMessage::CommandRequest(CommandRequest {
                request_id: 0,
                command: CommandInvocation::new(
                    "new-session",
                    [
                        "-A".to_owned(),
                        "-d".to_owned(),
                        "-c".to_owned(),
                        working_directory.to_string_lossy().into_owned(),
                    ],
                ),
                prepared: false,
            }))?;
        }
        self.attach("")
    }

    pub fn attach_session(
        &self,
        session: impl Into<String>,
        detach_others: bool,
        read_only: bool,
        client_flags: Option<&str>,
    ) -> Result<(), DaemonError> {
        let session = session.into();
        if !detach_others && !read_only && client_flags.is_none() {
            return self.attach(session);
        }
        self.send(&ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 0,
            command: attach_session_command(session, detach_others, read_only, client_flags),
            prepared: false,
        }))
    }

    pub fn request_attach_session(
        &self,
        session: impl Into<String>,
        detach_others: bool,
        read_only: bool,
        client_flags: Option<&str>,
    ) -> Result<u64, DaemonError> {
        self.execute(attach_session_command(
            session.into(),
            detach_others,
            read_only,
            client_flags,
        ))
    }

    pub fn detach(&self) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::Detach)
    }

    pub fn send_input(&self, input: InputMessage) -> Result<(), DaemonError> {
        if let InputMessage::Key {
            pane,
            input: key,
            text_follows,
        } = &input
        {
            return self.send_locked(&input, |writer| {
                writer.send_encoded(&input, |frame| {
                    encode_key_input_into(*pane, key, *text_follows, frame)
                })
            });
        }
        self.send(&ProtocolMessage::Input(input))
    }

    pub fn send_paste_upload(
        &self,
        upload_id: u64,
        pane: PaneId,
        extension: String,
        bytes: &[u8],
    ) -> Result<(), DaemonError> {
        self.send_paste_upload_with_purpose(
            upload_id,
            pane,
            PasteUploadPurpose::PastePath,
            extension,
            bytes,
        )
    }

    pub fn record_pasted_image(
        &self,
        upload_id: u64,
        pane: PaneId,
        extension: String,
        bytes: &[u8],
    ) -> Result<(), DaemonError> {
        self.send_paste_upload_with_purpose(
            upload_id,
            pane,
            PasteUploadPurpose::RecordPastedImage,
            extension,
            bytes,
        )
    }

    fn send_paste_upload_with_purpose(
        &self,
        upload_id: u64,
        pane: PaneId,
        purpose: PasteUploadPurpose,
        extension: String,
        bytes: &[u8],
    ) -> Result<(), DaemonError> {
        let total_bytes = u32::try_from(bytes.len()).map_err(|_| {
            DaemonError::Server(ServerError::InvalidCommand(
                "pasted image exceeds the upload limit".to_owned(),
            ))
        })?;
        let mut writer = self.writer.lock();
        writer.send(&ProtocolMessage::PasteUploadBegin {
            upload_id,
            pane,
            purpose,
            extension,
            total_bytes,
        })?;
        for chunk in bytes.chunks(MAX_PASTE_UPLOAD_CHUNK_BYTES) {
            writer.send(&ProtocolMessage::PasteUploadChunk {
                upload_id,
                bytes: chunk.to_vec(),
            })?;
        }
        Ok(())
    }

    pub fn fetch_pasted_image(&self, pane: PaneId, number: u32) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::FetchPastedImage { pane, number })
    }

    pub fn fetch_artifact(&self, token: &str, path: &str) -> Result<u64, DaemonError> {
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        self.send(&ProtocolMessage::FetchArtifact {
            request_id,
            token: token.to_owned(),
            path: path.to_owned(),
        })?;
        Ok(request_id)
    }

    pub fn send_artifact_reply(&self, token: &str, text: &str) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::ArtifactReply {
            token: token.to_owned(),
            text: text.to_owned(),
        })
    }

    pub fn execute(&self, command: CommandInvocation) -> Result<u64, DaemonError> {
        self.execute_with_prepared(command, false)
    }

    pub fn execute_prepared(&self, command: CommandInvocation) -> Result<u64, DaemonError> {
        self.execute_with_prepared(command, true)
    }

    fn execute_with_prepared(
        &self,
        command: CommandInvocation,
        prepared: bool,
    ) -> Result<u64, DaemonError> {
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        self.send(&ProtocolMessage::CommandRequest(CommandRequest {
            request_id,
            command,
            prepared,
        }))?;
        Ok(request_id)
    }

    pub fn prepare_commands(&self, commands: Vec<CommandInvocation>) -> Result<u64, DaemonError> {
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        self.send(&ProtocolMessage::PrepareCommandList {
            request_id,
            commands,
        })?;
        Ok(request_id)
    }

    pub fn request_home_directories(&self, users: Vec<String>) -> Result<u64, DaemonError> {
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        self.send(&ProtocolMessage::HomeDirectoryRequest { request_id, users })?;
        Ok(request_id)
    }

    pub fn request_path_list(
        &self,
        pane: Option<PaneId>,
        dir: Option<String>,
    ) -> Result<u64, DaemonError> {
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        self.send(&ProtocolMessage::PathListRequest {
            request_id,
            pane,
            dir,
        })?;
        Ok(request_id)
    }

    pub fn cancel_path_list(&self, request_id: u64) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::PathListCancel { request_id })
    }

    pub fn request_environment(&self, names: Vec<String>) -> Result<u64, DaemonError> {
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        self.send(&ProtocolMessage::EnvironmentRequest { request_id, names })?;
        Ok(request_id)
    }

    pub fn request_resync(&self) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::Resync)
    }

    pub fn request_full(&self, pane: PaneId) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::RequestFull { pane })
    }

    pub fn set_terminal_preview(&self, enabled: bool) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::SetTerminalPreview { enabled })
    }

    pub fn request_history(&self, pane: PaneId, start: u32, count: u32) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::HistoryRequest { pane, start, count })
    }

    pub fn client_instance_id(&self) -> ClientInstanceId {
        client_instance_id()
    }

    pub fn agent_prompt(
        &self,
        pane: PaneId,
        text: String,
        images: Vec<AgentImage>,
    ) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentPrompt { pane, text, images })
    }

    pub fn agent_cancel(&self, pane: PaneId) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentCancel { pane })
    }

    pub fn agent_unqueue(&self, pane: PaneId) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentUnqueue { pane })
    }

    pub fn agent_respond_permission(
        &self,
        pane: PaneId,
        request_id: u64,
        option_id: Option<String>,
    ) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentRespondPermission {
            pane,
            request_id,
            option_id,
        })
    }

    pub fn agent_set_config_option(
        &self,
        pane: PaneId,
        option_id: String,
        value: String,
    ) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentSetConfigOption {
            pane,
            option_id,
            value,
        })
    }

    pub fn agent_set_mode(&self, pane: PaneId, mode_id: String) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentSetMode { pane, mode_id })
    }

    pub fn agent_authenticate(&self, pane: PaneId, method_id: String) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentAuthenticate { pane, method_id })
    }

    pub fn agent_session_op(
        &self,
        pane: PaneId,
        op: AgentSessionOpKind,
    ) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentSessionOp { pane, op })
    }

    pub fn agent_replay(&self, pane: PaneId, from_seq: u64) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentReplay { pane, from_seq })
    }

    pub fn agent_acknowledge_prompt_restore(
        &self,
        pane: PaneId,
        reclaim_id: u64,
    ) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::AgentAcknowledgePromptRestore { pane, reclaim_id })
    }

    pub fn send_gui_response(&self, response: GuiResponse) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::GuiResponse(response))
    }

    pub fn set_color_scheme(&self, color_scheme: TerminalColorScheme) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::SetColorScheme(color_scheme))
    }

    pub fn set_config_overrides(
        &self,
        entries: Vec<ConfigOverrideEntry>,
    ) -> Result<(), DaemonError> {
        self.send(&ProtocolMessage::SetConfigOverrides { entries })
    }

    #[cfg(unix)]
    pub fn shutdown(&self) -> Result<(), DaemonError> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(forward) = &self.russh_forward {
            forward.shutdown();
            return Ok(());
        }
        self.writer.lock().stream.shutdown()?;
        Ok(())
    }

    #[cfg(unix)]
    pub fn receive_fd(&self) -> io::Result<std::os::fd::OwnedFd> {
        self.reader.lock().stream.get_ref().receive_fd()
    }

    #[cfg(unix)]
    pub fn send_with_fd(
        &self,
        message: &ProtocolMessage,
        fd: std::os::fd::BorrowedFd<'_>,
    ) -> io::Result<()> {
        let frame = zz_protocol::encode_protocol_message(message).map_err(io::Error::other)?;
        let writer = self.writer.lock();
        writer.admit(message).map_err(io::Error::other)?;
        let socket = writer.stream.receive_fd()?;
        let fds = [fd];
        let mut space = [std::mem::MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut ancillary = rustix::net::SendAncillaryBuffer::new(&mut space);
        ancillary.push(rustix::net::SendAncillaryMessage::ScmRights(&fds));
        let mut sent = loop {
            match rustix::net::sendmsg(
                &socket,
                &[io::IoSlice::new(&frame)],
                &mut ancillary,
                rustix::net::SendFlags::empty(),
            ) {
                Ok(sent) => break sent,
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
        };
        while sent < frame.len() {
            match rustix::io::write(&socket, &frame[sent..]) {
                Ok(written) => sent += written,
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
        }
        drop(writer);
        Ok(())
    }

    #[cfg(unix)]
    pub fn try_recv(&self) -> Result<Option<Box<ProtocolMessage>>, DaemonError> {
        self.try_recv_with_read(true)
    }

    #[cfg(unix)]
    pub fn try_recv_buffered(&self) -> Result<Option<Box<ProtocolMessage>>, DaemonError> {
        self.try_recv_with_read(false)
    }

    #[cfg(unix)]
    fn try_recv_with_read(
        &self,
        read_socket: bool,
    ) -> Result<Option<Box<ProtocolMessage>>, DaemonError> {
        loop {
            let result = self.reader.lock().try_recv_decodable(read_socket)?;
            let Some((message, skipped)) = result else {
                return Ok(None);
            };
            if skipped {
                self.request_resync()?;
            }
            if let ProtocolMessage::Ping { .. } = *message {
                self.answer_ping(&message)?;
                continue;
            }
            return Ok(Some(message));
        }
    }

    pub fn recv(&self) -> Result<ProtocolMessage, DaemonError> {
        loop {
            let message = self.recv_any()?;
            if let ProtocolMessage::Ping { .. } = message {
                self.answer_ping(&message)?;
                continue;
            }
            return Ok(message);
        }
    }

    fn recv_any(&self) -> Result<ProtocolMessage, DaemonError> {
        let started = diagnostic_timer();
        let lock_started = diagnostic_timer();
        let mut reader = self.reader.lock();
        let lock_wait_us = diagnostic_elapsed_us(lock_started);
        let result = reader.recv_decodable();
        drop(reader);
        log::trace!(
            target: "zz_daemon::diagnostics::client",
            "interactive_recv success={} lock_wait_us={} total_elapsed_us={}",
            result.is_ok(),
            lock_wait_us,
            diagnostic_elapsed_us(started),
        );
        match result {
            Ok((message, false)) => Ok(message),
            Ok((message, true)) => {
                self.request_resync()?;
                Ok(message)
            }
            Err(error) => Err(error),
        }
    }

    pub fn send(&self, message: &ProtocolMessage) -> Result<(), DaemonError> {
        self.send_locked(&TracedMessage(message), |writer| writer.send(message))
    }

    #[must_use]
    pub fn daemon_has(&self, capability: &str) -> bool {
        self.writer
            .lock()
            .daemon_capabilities
            .iter()
            .any(|name| name == capability)
    }

    fn send_locked(
        &self,
        message: &impl std::fmt::Debug,
        send: impl FnOnce(&mut ProtocolSender<ClientStream>) -> Result<(), DaemonError>,
    ) -> Result<(), DaemonError> {
        let started = diagnostic_timer();
        let lock_started = diagnostic_timer();
        let mut writer = self.writer.lock();
        let lock_wait_us = diagnostic_elapsed_us(lock_started);
        let result = send(&mut writer);
        log::trace!(
            target: "zz_daemon::diagnostics::client",
            "interactive_send success={} lock_wait_us={} total_elapsed_us={} message={message:#?}",
            result.is_ok(),
            lock_wait_us,
            diagnostic_elapsed_us(started),
        );
        result
    }
}
