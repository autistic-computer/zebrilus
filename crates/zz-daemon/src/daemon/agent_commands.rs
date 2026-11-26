use super::*;

impl Shared {
    pub(super) fn agent_send(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_agent_send_args(args)?;
        if let Some(host) = &parsed.relay {
            #[cfg(feature = "agent")]
            return self.relay_agent_send(client, kind, context, host, &parsed);
            #[cfg(not(feature = "agent"))]
            return Err(ServerError::NativeCommandParse(format!(
                "agent-send: --relay {host} needs agent support"
            ))
            .into());
        }
        let payload = if parsed.wait_only() {
            String::new()
        } else {
            parsed.payload()?
        };
        let chat_only = parsed.final_only
            || parsed.notify
            || matches!(
                parsed.on_block,
                AgentBlockPolicy::Allow | AgentBlockPolicy::Deny
            );
        if chat_only && parsed.wait_only() {
            return Err(ServerError::NativeCommandParse(
                "agent-send: --wait without text takes only --timeout, --json and --on-block wait|fail"
                    .to_owned(),
            )
            .into());
        }
        #[cfg(feature = "agent")]
        let moved = self.moved_child(client, context, parsed.target.as_deref())?;
        #[cfg(not(feature = "agent"))]
        let moved = None;
        let (pane, agent_pane) = {
            let inner = self.inner.lock();
            let pane = match moved {
                Some(pane) => pane,
                None => inner.engine.resolve_pane(
                    parsed.target.as_deref(),
                    context.window,
                    context.pane,
                )?,
            };
            let agent_pane = matches!(
                inner.engine.state.pane(pane).map(|pane| &pane.kind),
                Some(PaneKind::Agent(_))
            );
            (pane, agent_pane)
        };
        #[cfg(feature = "agent")]
        let caller = self.chat_caller(client, context, Some(pane));
        #[cfg(feature = "agent")]
        if let Some(caller) = caller
            && self.inner.lock().engine.agent_parent(caller) == Some(pane)
        {
            return Err(ServerError::InvalidCommand(format!(
                "{pane} opened this chat; your reply reaches it when your turn ends"
            ))
            .into());
        }
        #[cfg(feature = "agent")]
        let parsed = {
            let mut parsed = parsed;
            if caller.is_some() && !parsed.on_block_set {
                parsed.on_block = AgentBlockPolicy::Fail;
            }
            parsed
        };
        #[cfg(feature = "agent")]
        let payload = if agent_pane && !parsed.wait_only() {
            self.sent_message(client, kind, context, caller, payload)
        } else {
            payload
        };
        #[cfg(feature = "agent")]
        if agent_pane {
            if parsed.wait_only() {
                if !matches!(kind, ClientKind::Command | ClientKind::Control) {
                    return Err(ServerError::InvalidCommand(
                        "agent-send --wait needs a command client".to_owned(),
                    )
                    .into());
                }
                return self.wait_for_agent_turn(client, pane, None, &parsed, caller.is_some());
            }
            self.refuse_full_chat(pane)?;
            if let Some(caller) = caller {
                self.count_chat_send(caller, pane)?;
            }
            if parsed.notify && caller.is_none() {
                let caller = self
                    .inner
                    .lock()
                    .client(context.replay_client().unwrap_or(client))
                    .and_then(|client| client.origin)
                    .or(context.pane.filter(|_| kind == ClientKind::Interactive));
                return self.submit_agent_prompt_and_notify(caller, pane, payload, kind);
            }
            if parsed.wait || (chat_only && !parsed.notify) {
                if !matches!(kind, ClientKind::Command | ClientKind::Control) {
                    return Err(ServerError::InvalidCommand(
                        "agent-send --wait needs a command client".to_owned(),
                    )
                    .into());
                }
                return self.wait_for_agent_turn(
                    client,
                    pane,
                    Some(payload),
                    &parsed,
                    caller.is_some(),
                );
            }
            let runtime = self.agent_runtime().ok_or(ServerError::PaneExited(pane))?;
            let waiter = caller.map(|caller| self.chat_report_waiter(caller, pane));
            if !runtime.command_prompt(
                pane,
                AgentPrompt {
                    owner: ClientInstanceId(u64::MAX),
                    text: payload,
                    images: Vec::new(),
                },
                waiter,
                caller.map_or(PromptSource::Command, |_| PromptSource::Chat),
                None,
            ) {
                return Err(self.untaken_prompt(pane).into());
            }
            let route = if caller.is_some() {
                zz_protocol::agent_mention::ReplyRoute::Back
            } else {
                zz_protocol::agent_mention::ReplyRoute::Read
            };
            self.record_command_stderr(
                client,
                &zz_protocol::agent_mention::ReplyNotice::Sent { pane, route }.to_string(),
            );
            return Ok(Execution {
                output: if kind == ClientKind::Interactive {
                    RawText::default()
                } else {
                    pane.to_string().into()
                },
                effects: Vec::new(),
            });
        }
        #[cfg(not(feature = "agent"))]
        if agent_pane {
            return Err(ServerError::InvalidCommand(format!(
                "{pane} is a chat and this zz was built without agent support"
            ))
            .into());
        }
        if let Some(provider) = self.terminal_agent(pane).filter(|_| !parsed.wait_only()) {
            return Err(ServerError::InvalidTarget(format!(
                "{pane} runs {} in its terminal; flip the pane to chat first: flip-agent -t {pane}",
                provider.label()
            ))
            .into());
        }
        if chat_only {
            let flag = if parsed.final_only {
                "--final"
            } else if parsed.notify {
                "--notify"
            } else {
                "--on-block allow|deny"
            };
            return Err(ServerError::NativeCommandParse(format!(
                "agent-send: {flag} needs an agent chat pane"
            ))
            .into());
        }
        let terminal = {
            let inner = self.inner.lock();
            matches!(
                inner.engine.state.pane(pane).map(|pane| &pane.kind),
                Some(PaneKind::Terminal)
            )
            .then(|| inner.terminals.get(&pane).cloned())
            .flatten()
        };
        let Some(terminal) = terminal else {
            return Err(ServerError::InvalidTarget(format!(
                "{pane} is not a terminal running an agent"
            ))
            .into());
        };
        if parsed.wait {
            if !matches!(kind, ClientKind::Command | ClientKind::Control) {
                return Err(ServerError::InvalidCommand(
                    "agent-send --wait needs a command client".to_owned(),
                )
                .into());
            }
            return self.send_terminal_message_and_wait(client, pane, &payload, &parsed);
        }
        if self.pane_agent_state(pane).is_empty() {
            return Err(ServerError::MissingTarget(format!(
                "no agent runs in {pane}; send-text types into any pane"
            ))
            .into());
        }
        terminal_reads::send_text(
            self,
            client,
            pane,
            terminal,
            &payload,
            SEND_TEXT_TIMEOUT,
            parsed.submit,
        )
    }

    pub(super) fn pane_agent_state(&self, pane: PaneId) -> String {
        let inner = self.inner.lock();
        let Some(window) = inner.engine.state.window_for_pane(pane) else {
            return String::new();
        };
        let session = inner.engine.state.windows[&window].session.to_string();
        inner
            .engine
            .format_facts()
            .user_option(
                &pane.to_string(),
                &window.to_string(),
                &session,
                "@agent_state",
            )
            .unwrap_or_default()
            .to_owned()
    }

    fn send_terminal_message_and_wait(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
        payload: &str,
        parsed: &ParsedAgentSend,
    ) -> Result<Execution, DaemonError> {
        let initial = self.pane_agent_state(pane);
        if initial.is_empty() {
            return Err(ServerError::InvalidCommand(format!(
                "{pane}: agent-send --wait needs an agent state; the pane reports none (@agent_state is unset)"
            )).into());
        }
        if payload.is_empty() && initial == "idle" {
            self.record_command_stderr(client, &pane.to_string());
            return Ok(Execution {
                output: agent_terminal_reply_output(pane, "", Duration::ZERO, "end_turn", parsed)
                    .into(),
                effects: Vec::new(),
            });
        }
        let wait = terminal_requests::CommandWait::new(self);
        let terminal_wait = agent_waits::TerminalWait::subscribe(
            self,
            client,
            pane,
            parsed,
            wait.start(),
            &initial,
        );
        if payload.is_empty() {
            terminal_wait.submitted(Ok(()));
            agent_waits::state_changed(self, pane);
            self.report_command_queue_park();
            return wait.finish(self, Execution::default());
        }
        #[cfg(all(feature = "agent", unix))]
        let queued = if let Some((pid, title, cwd)) = self.codex_terminal_context(pane) {
            match crate::agent::codex_queue::deliver(pid, &title, &cwd, payload) {
                Ok(_) => true,
                Err(crate::agent::codex_queue::QueueError::NoName) => false,
                Err(error) => {
                    terminal_wait.submitted(Err(ServerError::InvalidCommand(
                        error.for_pane(pane, &title, &cwd),
                    )
                    .into()));
                    return wait.finish(self, Execution::default());
                }
            }
        } else {
            false
        };
        #[cfg(not(all(feature = "agent", unix)))]
        let queued = false;
        if queued {
            terminal_wait.submitted(Ok(()));
            agent_waits::state_changed(self, pane);
        } else {
            let terminal = self.inner.lock().terminals.get(&pane).cloned();
            if let Some(terminal) = terminal {
                terminal_reads::submit_agent_text(
                    self,
                    client,
                    pane,
                    terminal,
                    payload,
                    move |shared, result| {
                        terminal_wait.submitted(result);
                        agent_waits::state_changed(shared, pane);
                    },
                );
            } else {
                terminal_wait.submitted(Err(ServerError::PaneExited(pane).into()));
            }
        }
        self.report_command_queue_park();
        wait.finish(self, Execution::default())
    }

    fn resolve_agent_permission_pane(
        &self,
        context: &ExecutionContext,
        target: Option<&str>,
    ) -> Result<PaneId, DaemonError> {
        let pane = self
            .inner
            .lock()
            .engine
            .resolve_pane(target, context.window, context.pane)?;
        #[cfg(feature = "agent")]
        if self
            .open_agent_runtime()
            .is_some_and(|runtime| runtime.pane_generation(pane).is_some())
        {
            return Ok(pane);
        }
        Err(ServerError::InvalidTarget(format!(
            "{pane} is not an agent chat; agent-respond answers permissions in agent panes"
        ))
        .into())
    }

    pub(super) fn agent_catalog(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let spec = zz_protocol::catalog_command_spec("agent-catalog").expect("registered command");
        let parsed = zz_protocol::parse_tmux_options(spec, args)?;
        let [provider, request_id] = parsed.positionals else {
            return Err(ServerError::CommandParse(
                "usage: agent-catalog [-t pane] provider request-id".into(),
            )
            .into());
        };
        let provider = provider
            .parse::<zz_protocol::AgentProvider>()
            .map_err(ServerError::CommandParse)?;
        let request_id = request_id
            .parse::<u64>()
            .map_err(|_| ServerError::CommandParse("invalid catalog request id".into()))?;
        let target = parsed.options.iter().find_map(|option| match option {
            zz_protocol::TmuxOption::Value("-t", value) => Some(*value),
            _ => None,
        });
        let pane = self
            .inner
            .lock()
            .engine
            .resolve_pane(target, context.window, context.pane)?;
        #[cfg(feature = "agent")]
        {
            let cwd = self
                .inner
                .lock()
                .terminals
                .get(&pane)
                .and_then(|terminal| terminal_working_directory(terminal))
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("/"));
            let mut result = zz_protocol::agent_stream::AgentCatalogResult {
                catalog_provider: provider,
                cwd: cwd.clone(),
                request_id,
                config_options: None,
                error: None,
            };
            let key = (client, pane, provider);
            {
                let mut pending = self.agent_catalog_pending.lock();
                if pending.len() >= 4 || !pending.insert(key) {
                    result.error =
                        Some("A catalog request is already running. Try again shortly.".into());
                    self.publish_agent_catalog(client, pane, result);
                    return Ok(Execution::default());
                }
            }
            let config = self.agent_spawn_config();
            if let Err(error) = self.submit_helper(helpers::Task::Catalog {
                client,
                pane,
                config: Box::new(config),
                result: result.clone(),
            }) {
                self.agent_catalog_pending.lock().remove(&key);
                result.error = Some(format!("Could not load the model catalog: {error}"));
                self.publish_agent_catalog(client, pane, result);
            }
            Ok(Execution::default())
        }
        #[cfg(not(feature = "agent"))]
        {
            let _ = (client, pane, provider, request_id);
            Err(ServerError::InvalidCommand("agent support is disabled".into()).into())
        }
    }

    #[cfg(feature = "agent")]
    pub(super) fn publish_agent_catalog(
        &self,
        client: ClientId,
        pane: PaneId,
        mut result: zz_protocol::agent_stream::AgentCatalogResult,
    ) {
        let mut encoded = serde_json::to_string(&result).expect("catalog JSON");
        if encoded.len() > zz_protocol::MAX_AGENT_RESULT_BYTES {
            result.config_options = None;
            result.error = Some("The model catalog exceeds the response size limit.".into());
            encoded = serde_json::to_string(&result).expect("catalog error JSON");
        }
        self.publish_to_client(
            client,
            EventPayload::AgentSessions {
                pane,
                request_id: result.request_id,
                result: encoded,
            },
        );
    }

    pub(super) fn agent_respond(
        self: &Arc<Self>,
        kind: ClientKind,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_agent_respond_args(args)?;
        if !matches!(kind, ClientKind::Command | ClientKind::Control) {
            return Err(ServerError::InvalidCommand(
                "agent-respond needs a command client".to_owned(),
            )
            .into());
        }
        let pane = self.resolve_agent_permission_pane(context, parsed.target.as_deref())?;
        let none_pending = move || {
            ServerError::InvalidCommand(format!(
                "{pane} has no pending request (already answered, or none was asked)"
            ))
        };
        #[cfg(feature = "agent")]
        {
            use crate::agent::host::PermissionChoice;
            if let Some(text) = &parsed.answer {
                let runtime = self.open_agent_runtime().ok_or_else(none_pending)?;
                let request = runtime
                    .answer_question(pane, parsed.request_id, text)
                    .map_err(|error| {
                        if error == "no pending permission" {
                            none_pending()
                        } else {
                            ServerError::InvalidCommand(format!("{pane}: {error}"))
                        }
                    })?;
                return Ok(Execution {
                    output: format!("answered request {request}").into(),
                    effects: Vec::new(),
                });
            }
            let choice = if parsed.allow {
                PermissionChoice::Allow
            } else if parsed.deny {
                PermissionChoice::Deny
            } else {
                PermissionChoice::Option(parsed.option.expect("validated permission choice"))
            };
            let runtime = self.open_agent_runtime().ok_or_else(none_pending)?;
            let (wait, reply) =
                agent_waits::reply::<Result<String, String>>(self, None, move |_, outcome| {
                    let option = match outcome {
                        terminal_requests::ReplyOutcome::Ready(result) => {
                            result.map_err(|error| {
                                if error == "no pending permission" {
                                    none_pending()
                                } else {
                                    ServerError::InvalidCommand(format!("{error}: {pane}"))
                                }
                            })?
                        }
                        _ => return Err(ServerError::PaneExited(pane).into()),
                    };
                    Ok(Execution {
                        output: option.into(),
                        effects: Vec::new(),
                    })
                });
            if !runtime.command(
                pane,
                HostCommand::RespondPermission {
                    response: PermissionResponse::Select {
                        request_id: parsed.request_id,
                        choice,
                        reply,
                    },
                },
            ) {
                return Err(ServerError::PaneExited(pane).into());
            }
            self.report_command_queue_park();
            wait.finish(self, Execution::default())
        }
        #[cfg(not(feature = "agent"))]
        Err(none_pending().into())
    }

    pub(super) fn send_last_output(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        self.capture_last_command_for(
            client,
            "send-last-output",
            context,
            args,
            |shared, pane, capture, status| {
                #[cfg(feature = "agent")]
                {
                    let agent = shared.recent_agent_terminal(pane).ok_or_else(|| {
                        ServerError::MissingTarget(format!(
                            "no Claude Code or Codex terminal in the window holding {pane}"
                        ))
                    })?;
                    let text = last_command_block(pane, &capture, status);
                    let shared = Arc::clone(shared);
                    std::thread::Builder::new()
                        .name("zz-send-last-output".to_owned())
                        .spawn(move || {
                            if let Err(error) =
                                shared.paste_and_submit(agent, &text, SEND_TEXT_TIMEOUT, false)
                            {
                                log::warn!(target: "zz::agent", "could not draft into {agent}: {error}");
                            }
                        })
                        .map_err(|error| ServerError::Internal(error.to_string()))?;
                    Ok(Execution {
                        output: format!("sent the last command from {pane} to {agent}").into(),
                        effects: Vec::new(),
                    })
                }
                #[cfg(not(feature = "agent"))]
                {
                    let _ = (shared, capture, status);
                    Err(ServerError::MissingTarget(format!(
                        "no agent terminal in the window holding {pane}"
                    ))
                    .into())
                }
            },
        )
    }

    pub(super) fn show_last_output(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        self.capture_last_command_for(
            client,
            "show-last-output",
            context,
            args,
            |_, pane, capture, status| {
                Ok(Execution {
                    output: last_command_block(pane, &capture, status).into(),
                    effects: Vec::new(),
                })
            },
        )
    }

    pub(super) fn send_text(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_send_text_args(args)?;
        let text = parsed.payload()?;
        let (pane, terminal) = self.terminal_wait_target(
            "send-text",
            ClientKind::Command,
            context,
            parsed.target.as_deref(),
        )?;
        terminal_reads::send_text(
            self,
            client,
            pane,
            terminal,
            &text,
            parsed.timeout,
            !parsed.no_enter,
        )
    }

    #[cfg(all(feature = "agent", unix))]
    fn codex_terminal_context(&self, pane: PaneId) -> Option<(u32, String, PathBuf)> {
        let (pid, title, cwd) = {
            let inner = self.inner.lock();
            let state = inner.engine.state.pane(pane)?;
            if !matches!(state.kind, PaneKind::Terminal) {
                return None;
            }
            let runtime = inner.engine.pane_runtime_facts(pane)?;
            let path = if runtime.current_path.is_empty() {
                &runtime.start_path
            } else {
                &runtime.current_path
            };
            (runtime.pid?, state.title.clone(), PathBuf::from(path))
        };
        crate::agent::codex_queue::pane_runs_codex(pid).then_some((pid, title, cwd))
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn deliver_to_terminal_pane(
        self: &Arc<Self>,
        pane: PaneId,
        text: &str,
    ) -> Result<(), DaemonError> {
        use crate::agent::codex_queue::{self, QueueError};

        if let Some((pid, title, cwd)) = self.codex_terminal_context(pane) {
            match codex_queue::deliver(pid, &title, &cwd, text) {
                Ok(delivered) => {
                    log::info!(target: "zz::agent", "queued Codex peer message {} for thread {} in {pane}", delivered.message_id, delivered.thread_id);
                    return Ok(());
                }
                Err(error @ QueueError::NoName) => {
                    log::info!(target: "zz::agent", "{}; delivering peer prompt through the terminal", error.for_pane(pane, &title, &cwd));
                }
                Err(error) => {
                    return Err(
                        ServerError::InvalidCommand(error.for_pane(pane, &title, &cwd)).into(),
                    );
                }
            }
        }
        self.paste_and_submit(pane, text, SEND_TEXT_TIMEOUT, true)
    }

    fn terminal_wait_target(
        &self,
        verb: &str,
        kind: ClientKind,
        context: &ExecutionContext,
        target: Option<&str>,
    ) -> Result<(PaneId, Arc<TerminalSession>), DaemonError> {
        if !matches!(kind, ClientKind::Command | ClientKind::Control) {
            return Err(ServerError::InvalidCommand(format!(
                "{verb} requires a command or control client"
            ))
            .into());
        }
        let inner = self.inner.lock();
        let pane = inner
            .engine
            .resolve_pane(target, context.window, context.pane)?;
        match inner.engine.state.pane(pane).map(|pane| &pane.kind) {
            Some(PaneKind::Terminal) => {}
            Some(PaneKind::Agent(_)) if matches!(verb, "send-text" | "run-pane" | "wait-pane") => {
                return Err(agent_chat_refusal(verb, pane).into());
            }
            _ => {
                return Err(
                    ServerError::InvalidTarget(format!("{pane} is not a terminal pane")).into(),
                );
            }
        }
        let terminal = inner
            .terminals
            .get(&pane)
            .cloned()
            .ok_or(ServerError::PaneExited(pane))?;
        Ok((pane, terminal))
    }

    pub(super) fn wait_pane(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_wait_pane_args(args)?;
        let (pane, terminal) =
            self.terminal_wait_target("wait-pane", kind, context, parsed.target.as_deref())?;
        if matches!(parsed.condition, PaneWaitCondition::Exit) {
            pane_exit::wait(self, client, pane, &terminal, parsed.timeout)
        } else {
            terminal_reads::wait_pane(self, client, pane, terminal, parsed)
        }
    }

    pub(super) fn run_pane(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_run_pane_args(args)?;
        let (pane, terminal) =
            self.terminal_wait_target("run-pane", kind, context, parsed.target.as_deref())?;
        terminal_reads::run_pane(self, client, pane, terminal, parsed)
    }

    #[cfg(feature = "agent")]
    pub(super) fn paste_and_submit(
        self: &Arc<Self>,
        pane: PaneId,
        text: &str,
        timeout: Duration,
        enter: bool,
    ) -> Result<(), DaemonError> {
        let context = ExecutionContext::for_pane(&self.inner.lock().engine.state, pane)
            .ok_or(ServerError::PaneExited(pane))?;
        let (pane, terminal) = self.terminal_wait_target(
            "send-text",
            ClientKind::Command,
            &context,
            Some(&pane.to_string()),
        )?;
        let detached = self.server_owner().command_item(None);
        terminal_reads::send_text(
            &detached,
            ClientId(u64::MAX),
            pane,
            terminal,
            text,
            timeout,
            enter,
        )
        .map(|_| ())
    }

    fn capture_last_command_for(
        self: &Arc<Self>,
        client: ClientId,
        verb: &str,
        context: &ExecutionContext,
        args: &[RawText],
        finish: impl FnOnce(
            &Arc<Shared>,
            PaneId,
            LastCommandCapture,
            Option<i32>,
        ) -> Result<Execution, DaemonError>
        + Send
        + 'static,
    ) -> Result<Execution, DaemonError> {
        let target = parse_target_only_args(verb, args)?;
        let (pane, terminal) = {
            let inner = self.inner.lock();
            let pane =
                inner
                    .engine
                    .resolve_pane(target.as_deref(), context.window, context.pane)?;
            if is_agent_chat(&inner, pane) {
                return Err(agent_chat_refusal(verb, pane).into());
            }
            if !matches!(
                inner.engine.state.pane(pane).map(|pane| &pane.kind),
                Some(PaneKind::Terminal)
            ) {
                return Err(
                    ServerError::InvalidTarget(format!("{pane} is not a terminal pane")).into(),
                );
            }
            let terminal = inner
                .terminals
                .get(&pane)
                .cloned()
                .ok_or(ServerError::PaneExited(pane))?;
            (pane, terminal)
        };
        let wait = terminal_requests::CommandWait::new(self);
        let request = terminal.capture_last_command_request(self.terminal_requests.notifier());
        let captured = Arc::clone(&terminal);
        let verb = verb.to_owned();
        wait.read(
            self,
            client,
            pane,
            terminal,
            request,
            move |shared, result| {
                let capture = match result
                    .map_err(TerminalCaptureError::from)
                    .and_then(|result| result)
                {
                    Ok(capture) => capture,
                    Err(TerminalCaptureError::NoSemanticMarks) => {
                        return Err(ServerError::InvalidCommand(format!(
                    "{pane} has no shell-integration marks; {verb} needs a shell that emits \
                     OSC 133 prompt marks (ghostty, kitty, wezterm, or starship shell \
                     integration all do)"
                ))
                .into());
                    }
                    Err(TerminalCaptureError::ActorStopped) => {
                        return Err(ServerError::PaneExited(pane).into());
                    }
                    Err(TerminalCaptureError::TimedOut) => {
                        return Err(
                            ServerError::Internal("terminal capture timed out".to_owned()).into(),
                        );
                    }
                    Err(error) => return Err(ServerError::Internal(error.to_string()).into()),
                };
                if capture.command.trim().is_empty() {
                    return Err(ServerError::InvalidCommand(format!(
                        "{pane} has not completed a command yet"
                    ))
                    .into());
                }
                finish(shared, pane, capture, captured.last_command_status())
            },
        );
        wait.finish(self, Execution::default())
    }

    pub(super) fn write_pane_agent_state(self: &Arc<Self>, pane: PaneId, value: &str) {
        let target = pane.to_string();
        {
            let inner = self.inner.lock();
            let Some(window) = inner.engine.state.window_for_pane(pane) else {
                return;
            };
            let session = inner.engine.state.windows[&window].session.to_string();
            if inner.engine.format_facts().user_option(
                &target,
                &window.to_string(),
                &session,
                "@agent_state",
            ) == Some(value)
            {
                return;
            }
        }
        if let Err(error) = self.execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                ["-p", "-t", target.as_str(), "@agent_state", value],
            ),
        ) {
            log::warn!(
                target: "zz_daemon::diagnostics::terminal",
                "agent state update failed pane={pane} state={value} error={error}"
            );
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ParsedAgentRespond {
    pub(super) target: Option<String>,
    allow: bool,
    deny: bool,
    pub(super) option: Option<String>,
    answer: Option<String>,
    pub(super) request_id: Option<u64>,
}

pub(super) fn parse_agent_respond_args(
    args: &[RawText],
) -> Result<ParsedAgentRespond, ServerError> {
    let mut parsed = ParsedAgentRespond::default();
    let mut index = 0;
    let mut positional = false;
    let mut choices = 0;
    while let Some(argument) = args.get(index) {
        if !positional && argument == "--" {
            positional = true;
            index += 1;
            continue;
        }
        if !positional {
            if argument == "--allow" || argument == "--deny" {
                parsed.allow = argument == "--allow";
                parsed.deny = argument == "--deny";
                choices += 1;
                index += 1;
                continue;
            }
            if let Some(value) = option_value("agent-respond", args, index, &["-t"])? {
                parsed.target = Some(value.value);
                index += value.consumed;
                continue;
            }
            if let Some(value) = option_value("agent-respond", args, index, &["--option"])? {
                parsed.option = Some(value.value);
                choices += 1;
                index += value.consumed;
                continue;
            }
            if let Some(value) = option_value("agent-respond", args, index, &["--answer"])? {
                parsed.answer = Some(value.value);
                choices += 1;
                index += value.consumed;
                continue;
            }
        }
        if parsed.request_id.is_some() {
            return Err(ServerError::CommandParse(
                "agent-respond accepts one request ID".to_owned(),
            ));
        }
        parsed.request_id = Some(argument.parse().map_err(|_| {
            ServerError::CommandParse(format!("invalid permission request ID: {argument}"))
        })?);
        index += 1;
    }
    if choices != 1 {
        return Err(ServerError::CommandParse(
            "agent-respond needs exactly one of --allow, --deny, --option ID or --answer TEXT"
                .to_owned(),
        ));
    }
    Ok(parsed)
}

pub(super) fn parse_wait_pane_args(args: &[RawText]) -> Result<ParsedWaitPane, ServerError> {
    let mut parsed = ParsedWaitPane {
        target: None,
        condition: PaneWaitCondition::Idle(Duration::from_millis(500)),
        timeout: Duration::from_mins(1),
        tail: None,
    };
    let mut condition_set = false;
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" && index + 1 == args.len() {
            break;
        }
        if let Some(value) = option_value("wait-pane", args, index, &["-t"])? {
            parsed.target = Some(value.value);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("wait-pane", args, index, &["--timeout"])? {
            parsed.timeout = parse_pane_timeout("wait-pane", &value.value)?;
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("wait-pane", args, index, &["--tail"])? {
            parsed.tail = Some(value.value.parse().map_err(|_| {
                ServerError::CommandParse(
                    "wait-pane --tail needs a nonnegative whole number of lines".to_owned(),
                )
            })?);
            index += value.consumed;
            continue;
        }
        if argument == "--exit" {
            if condition_set {
                return Err(ServerError::CommandParse(
                    "wait-pane accepts exactly one of --idle, --until, --regex, or --exit"
                        .to_owned(),
                ));
            }
            parsed.condition = PaneWaitCondition::Exit;
            condition_set = true;
            index += 1;
            continue;
        }
        let mut condition = None;
        for name in ["--idle", "--until", "--regex"] {
            let Some(value) = option_value("wait-pane", args, index, &[name])? else {
                continue;
            };
            if condition_set {
                return Err(ServerError::CommandParse(
                    "wait-pane accepts exactly one of --idle, --until, --regex, or --exit"
                        .to_owned(),
                ));
            }
            condition = Some(match name {
                "--idle" => PaneWaitCondition::Idle(Duration::from_millis(
                    value.value.parse().map_err(|_| {
                        ServerError::CommandParse(
                            "wait-pane --idle needs a nonnegative whole number of milliseconds"
                                .to_owned(),
                        )
                    })?,
                )),
                "--regex" => {
                    PaneWaitCondition::Regex(regex::Regex::new(&value.value).map_err(|error| {
                        ServerError::CommandParse(format!("wait-pane --regex: {error}"))
                    })?)
                }
                _ => PaneWaitCondition::Until(value.value),
            });
            index += value.consumed;
            break;
        }
        if let Some(condition) = condition {
            parsed.condition = condition;
            condition_set = true;
            continue;
        }
        return Err(ServerError::CommandParse(format!(
            "unexpected wait-pane argument: {argument}"
        )));
    }
    if matches!(parsed.condition, PaneWaitCondition::Exit) && parsed.tail.is_some() {
        return Err(ServerError::CommandParse(
            "wait-pane --tail cannot be combined with --exit".to_owned(),
        ));
    }
    Ok(parsed)
}

pub(super) fn parse_run_pane_args(args: &[RawText]) -> Result<ParsedRunPane, ServerError> {
    let mut parsed = ParsedRunPane {
        target: None,
        timeout: Duration::from_mins(2),
        command: String::new(),
    };
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        if let Some(value) = option_value("run-pane", args, index, &["-t"])? {
            parsed.target = Some(value.value);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("run-pane", args, index, &["--timeout"])? {
            parsed.timeout = parse_pane_timeout("run-pane", &value.value)?;
            index += value.consumed;
            continue;
        }
        if argument.starts_with('-') {
            return Err(ServerError::CommandParse(format!(
                "unexpected run-pane option: {argument}"
            )));
        }
        break;
    }
    parsed.command = match &args[index..] {
        [line] => line.to_string(),
        words => words
            .iter()
            .map(|word| run_pane_word(word.as_ref()))
            .collect::<Vec<_>>()
            .join(" "),
    };
    if parsed.command.trim().is_empty() {
        return Err(ServerError::CommandParse(
            "usage: run-pane [-t target-pane] [--timeout SECS] [--] COMMAND...".to_owned(),
        ));
    }
    if parsed
        .command
        .chars()
        .any(|character| character.is_control() && character != '\t')
    {
        return Err(ServerError::CommandParse(
            "run-pane requires a single command line without control characters".to_owned(),
        ));
    }
    Ok(parsed)
}

fn run_pane_word(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_./,:=+@%-".contains(character));
    if plain {
        word.to_owned()
    } else {
        zz_daemon_client::shell_quote(word)
    }
}

pub(super) fn unwrap_run_pane(
    capture: &LastCommandCapture,
) -> Option<(LastCommandCapture, Option<u8>)> {
    let start = capture.command.find("printf '\\n%s\\n' 'ZZRUN-")?;
    let (marker, rest) =
        capture.command[start + "printf '\\n%s\\n' '".len()..].split_once("-BEGIN'; ")?;
    let command = run_pane_command(
        rest.strip_prefix("command ")
            .unwrap_or(rest)
            .strip_prefix("eval \"$(printf '%b' '")?
            .strip_suffix(&format!("')\"; printf '\\n{marker}-RC=%d=END\\n' $?"))?,
    )?;
    let begin = format!("{marker}-BEGIN");
    let mut lines = capture.output.split_inclusive('\n').collect::<Vec<_>>();
    if let Some(index) = lines.iter().position(|line| line.trim_end() == begin) {
        lines.remove(index);
        if index > 0 && lines[index - 1].trim().is_empty() {
            lines.remove(index - 1);
        }
    }
    let output = lines.concat();
    let output = output.as_str();
    let rc = format!("{marker}-RC=");
    let (output, code) = match output.rfind(&rc) {
        Some(end) => {
            let code = output[end + rc.len()..]
                .lines()
                .next()
                .and_then(|line| line.strip_suffix("=END"))
                .and_then(|code| code.parse::<u8>().ok());
            let output = &output[..end];
            let output = output.strip_suffix('\n').unwrap_or(output);
            (output.strip_suffix('\n').unwrap_or(output), code)
        }
        None => (output, None),
    };
    Some((
        LastCommandCapture {
            command,
            output: output.to_owned(),
            truncated_rows: capture.truncated_rows,
            running: capture.running && code.is_none(),
        },
        code,
    ))
}

fn last_command_block(pane: PaneId, capture: &LastCommandCapture, status: Option<i32>) -> String {
    let unwrapped = unwrap_run_pane(capture);
    let (capture, status) = match &unwrapped {
        Some((capture, code)) => (capture, code.map(i32::from).or(status)),
        None => (capture, status),
    };
    let mut body = String::new();
    if capture.truncated_rows > 0 {
        let _ = writeln!(
            body,
            "truncated: dropped the first {} lines",
            capture.truncated_rows
        );
    }
    body.push_str(&capture.output);
    let mut header = format!("{pane} $ {}", capture.command);
    if capture.running {
        header.push_str("\nexit: running");
    } else if let Some(status) = status {
        let _ = write!(header, "\nexit: {status}");
    }
    fenced_block(&header, &body)
}

pub(super) fn parse_target_only_args(
    command: &str,
    args: &[RawText],
) -> Result<Option<String>, ServerError> {
    let mut target = None;
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            if index.saturating_add(1) != args.len() {
                return Err(ServerError::CommandParse(format!(
                    "{command} does not accept positional arguments"
                )));
            }
            break;
        }
        if let Some(value) = option_value(command, args, index, &["-t"])? {
            target = Some(value.value);
            index += value.consumed;
            continue;
        }
        return Err(ServerError::CommandParse(format!(
            "unexpected {command} argument: {argument}"
        )));
    }
    Ok(target)
}

pub(super) const MAX_CONTEXT_PATH_BYTES: usize = 4 * 1024;

pub(super) const AGENT_SEND_WAIT_TIMEOUT: Duration = Duration::from_mins(10);

const SEND_TEXT_TAIL_CHARS: usize = 40;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ContextReference {
    pub(super) path: String,
    pub(super) start: Option<u32>,
    pub(super) end: Option<u32>,
}

impl ContextReference {
    pub(super) fn parse(value: &str) -> Result<Self, ServerError> {
        let invalid = |message: &str| ServerError::InvalidCommand(message.to_owned());
        let (path, range) = match value.rsplit_once(':') {
            Some((path, range)) if !path.is_empty() && parse_line_range(range).is_some() => {
                (path, parse_line_range(range))
            }
            _ => (value, None),
        };
        if path.is_empty() || path.len() > MAX_CONTEXT_PATH_BYTES {
            return Err(invalid("--context path must be 1..=4096 bytes"));
        }
        if path.chars().any(char::is_control) {
            return Err(invalid(
                "--context path must not contain control characters",
            ));
        }
        let (start, end) = range.unwrap_or((None, None));
        if let (Some(start), Some(end)) = (start, end)
            && start > end
        {
            return Err(invalid("--context line range must not run backwards"));
        }
        Ok(Self {
            path: path.to_owned(),
            start,
            end,
        })
    }

    pub(super) fn header(&self) -> String {
        match (self.start, self.end) {
            (Some(start), Some(end)) => format!("{}:{start}-{end}", self.path),
            (Some(start), None) => format!("{}:{start}", self.path),
            _ => self.path.clone(),
        }
    }
}

fn parse_line_range(value: &str) -> Option<(Option<u32>, Option<u32>)> {
    if let Some((start, end)) = value.split_once('-') {
        return Some((Some(start.parse().ok()?), Some(end.parse().ok()?)));
    }
    Some((Some(value.parse().ok()?), None))
}

#[derive(Clone, Debug, Default)]
pub(super) struct ParsedAgentSend {
    pub(super) on_block: AgentBlockPolicy,
    pub(super) on_block_set: bool,
    pub(super) json: bool,
    pub(super) final_only: bool,
    pub(super) target: Option<String>,
    pub(super) submit: bool,
    pub(super) wait: bool,
    pub(super) notify: bool,
    pub(super) timeout: Option<Duration>,
    pub(super) context: Option<ContextReference>,
    pub(super) relay: Option<String>,
    pub(super) text: Vec<String>,
}

pub(super) fn pane_wait_exit_code(
    terminal: &TerminalSession,
    status: &zz_terminal::SessionStatus,
) -> u8 {
    if let Some(completion) = terminal.completion() {
        return completion.signal.map_or_else(
            || u8::try_from(completion.code).unwrap_or(u8::MAX),
            |signal| signal.saturating_add(128),
        );
    }
    match status {
        zz_terminal::SessionStatus::Exited(status) => u8::try_from(status.code).unwrap_or(u8::MAX),
        _ => 0,
    }
}

pub(super) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn echo_tail(text: &str) -> String {
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    let collapsed = collapse_whitespace(line);
    let start = collapsed
        .char_indices()
        .rev()
        .nth(SEND_TEXT_TAIL_CHARS.saturating_sub(1))
        .map_or(0, |(index, _)| index);
    collapsed[start..].to_owned()
}

#[derive(Debug)]
pub(super) struct ParsedSendText {
    pub(super) target: Option<String>,
    pub(super) no_enter: bool,
    pub(super) timeout: Duration,
    text: Vec<String>,
}

impl ParsedSendText {
    pub(super) fn payload(&self) -> Result<String, ServerError> {
        let text = self.text.join(" ");
        let text = text.trim_end_matches(['\n', '\r']);
        check_plain_text("send-text", text)?;
        Ok(text.to_owned())
    }
}

#[must_use]
pub fn send_text_reads_stdin(args: &[RawText]) -> bool {
    parse_send_text_args(args).is_ok_and(|parsed| parsed.text.is_empty())
}

#[derive(Debug)]
pub(super) enum PaneWaitCondition {
    Exit,
    Idle(Duration),
    Until(String),
    Regex(regex::Regex),
}

#[derive(Debug)]
pub(super) struct ParsedWaitPane {
    target: Option<String>,
    pub(super) condition: PaneWaitCondition,
    pub(super) timeout: Duration,
    pub(super) tail: Option<usize>,
}

#[derive(Debug)]
pub(super) struct ParsedRunPane {
    pub(super) target: Option<String>,
    pub(super) timeout: Duration,
    pub(super) command: String,
}

pub(super) fn parse_pane_timeout(verb: &str, value: &str) -> Result<Duration, ServerError> {
    let invalid = || {
        ServerError::CommandParse(format!(
            "{verb} --timeout needs a nonnegative number of seconds"
        ))
    };
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid());
    }
    Duration::try_from_secs_f64(value.parse().map_err(|_| invalid())?).map_err(|_| invalid())
}

const RUN_PANE_LITERAL: &str = " _./,:=+@%~-";

pub(super) fn run_pane_line(marker: &str, eval: &str, command: &str) -> String {
    let mut escaped = String::with_capacity(command.len());
    for character in command.chars() {
        if character.is_ascii_alphanumeric()
            || !character.is_ascii()
            || RUN_PANE_LITERAL.contains(character)
        {
            escaped.push(character);
        } else {
            let _ = write!(escaped, "\\0{:03o}", u32::from(character));
        }
    }
    format!(
        " printf '\\n%s\\n' '{marker}-BEGIN'; {eval} \"$(printf '%b' '{escaped}')\"; printf \
         '\\n{marker}-RC=%d=END\\n' $?"
    )
}

pub(super) fn run_pane_command(escaped: &str) -> Option<String> {
    let mut command = String::with_capacity(escaped.len());
    let mut rest = escaped;
    while let Some(index) = rest.find("\\0") {
        command.push_str(&rest[..index]);
        let digits = rest.get(index + 2..index + 5)?;
        command.push(char::from(u8::from_str_radix(digits, 8).ok()?));
        rest = &rest[index + 5..];
    }
    command.push_str(rest);
    Some(command)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RunStart {
    Marked,
    Dropped,
    Joined,
}

fn run_pane_code(text: &str, prefix: &str) -> Option<(usize, u8)> {
    text.match_indices(prefix).find_map(|(index, _)| {
        let rest = &text[index + prefix.len()..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if !rest[digits..].starts_with("=END") {
            return None;
        }
        Some((index, rest[..digits].parse().ok()?))
    })
}

pub(super) fn run_pane_result<'a>(
    screen: &'a str,
    marker: &str,
    collecting: bool,
) -> (Option<&'a str>, Option<u8>, RunStart) {
    let begin = format!("{marker}-BEGIN");
    let typed = format!("'{begin}'");
    let prefix = format!("{marker}-RC=");
    let echoed = format!("{prefix}%d=END\\n' $?");
    let lines = screen
        .split_inclusive('\n')
        .rev()
        .take(RUN_PANE_CAPTURE_LINES)
        .collect::<Vec<_>>();
    let mut offset = screen.len() - lines.iter().map(|line| line.len()).sum::<usize>();
    let capture_start = offset;
    let mut start = collecting.then_some(offset);
    let mut inline = false;
    let mut saw_begin = false;
    let mut echo_end = None;
    for line in lines.into_iter().rev() {
        let text = line.strip_suffix('\n').unwrap_or(line);
        let code = run_pane_code(text, &prefix);
        let before = &text[..code.map_or(text.len(), |(index, _)| index)];
        let boundary = |after: usize| {
            if text[after..].trim().is_empty() {
                (offset + line.len(), false)
            } else {
                (offset + after, true)
            }
        };
        if let Some(index) = before
            .match_indices(&begin)
            .map(|(index, _)| index)
            .filter(|index| !before[..*index].ends_with('\''))
            .last()
        {
            let (at, within) = boundary(index + begin.len());
            start = Some(at);
            inline = within;
            saw_begin = true;
        } else if before.contains(&typed) {
            let (at, within) = boundary(
                before
                    .rfind(&echoed)
                    .map_or(before.len(), |index| index + echoed.len()),
            );
            echo_end = Some(at);
            inline = within;
        }
        if let Some((index, code)) = code {
            let begin = if saw_begin {
                RunStart::Marked
            } else if start.is_none() && echo_end.is_some() {
                RunStart::Joined
            } else {
                RunStart::Dropped
            };
            let end = if text[..index].trim().is_empty() {
                offset
            } else {
                offset + index
            };
            let segment = &screen[start.or(echo_end).unwrap_or(capture_start).min(end)..end];
            let segment = if end > offset {
                segment.trim_end()
            } else {
                segment.strip_suffix('\n').unwrap_or(segment)
            };
            return (
                Some(if inline {
                    segment.trim_start()
                } else {
                    segment
                }),
                Some(code),
                begin,
            );
        }
        offset += line.len();
    }
    let partial = start.map(|start| {
        let rest = &screen[start..];
        let kept = rest.trim_end_matches('\n').len();
        let rest = &rest[..if kept == 0 {
            0
        } else {
            (kept + 1).min(rest.len())
        }];
        if inline { rest.trim_start() } else { rest }
    });
    (partial, None, RunStart::Marked)
}

pub(super) fn parse_send_text_args(args: &[RawText]) -> Result<ParsedSendText, ServerError> {
    let mut parsed = ParsedSendText {
        target: None,
        no_enter: false,
        timeout: SEND_TEXT_TIMEOUT,
        text: Vec::new(),
    };
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            parsed.text.extend(
                args[index.saturating_add(1)..]
                    .iter()
                    .map(ToString::to_string),
            );
            break;
        }
        if argument == "--no-enter" {
            parsed.no_enter = true;
            index += 1;
            continue;
        }
        if let Some(value) = option_value("send-text", args, index, &["--timeout"])? {
            parsed.timeout = parse_pane_timeout("send-text", &value.value)?;
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("send-text", args, index, &["-t"])? {
            parsed.target = Some(value.value);
            index += value.consumed;
            continue;
        }
        if argument.starts_with('-') && argument != "-" {
            return Err(ServerError::CommandParse(format!(
                "unsupported command: send-text {argument}"
            )));
        }
        parsed.text.push(argument.to_string());
        index += 1;
    }
    Ok(parsed)
}

#[derive(serde::Serialize)]
pub(super) struct AgentReplyEnvelope<'a> {
    pub(super) pane: String,
    pub(super) stop_reason: &'a str,
    pub(super) duration_ms: u128,
    pub(super) tool_calls: u32,
    pub(super) permissions: AgentReplyPermissions,
    pub(super) truncated: bool,
    pub(super) final_text: &'a str,
    pub(super) transcript: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) permission: Option<&'a serde_json::Value>,
}

#[derive(Default, serde::Serialize)]
pub(super) struct AgentReplyPermissions {
    pub(super) requested: u32,
    pub(super) allowed: u32,
    pub(super) denied: u32,
}

pub(super) fn agent_terminal_reply_output(
    pane: PaneId,
    text: &str,
    duration: Duration,
    stop_reason: &str,
    parsed: &ParsedAgentSend,
) -> String {
    if !parsed.json {
        return text.to_owned();
    }
    serde_json::to_string(&AgentReplyEnvelope {
        pane: pane.to_string(),
        stop_reason,
        duration_ms: duration.as_millis(),
        tool_calls: 0,
        permissions: AgentReplyPermissions::default(),
        truncated: false,
        final_text: text,
        transcript: text,
        permission: None,
    })
    .expect("turn reply JSON")
}

#[cfg(feature = "agent")]
pub(super) fn agent_permission_json(
    permission: &zz_protocol::AgentPermissionWire,
) -> serde_json::Value {
    let payload: serde_json::Value =
        serde_json::from_str(&permission.payload).expect("permission payload JSON");
    serde_json::json!({ "request_id": permission.request_id, "tool_call": payload["toolCall"], "options": payload["options"] })
}

impl ParsedAgentSend {
    fn wait_only(&self) -> bool {
        self.wait && self.context.is_none() && self.text.iter().all(|text| text.trim().is_empty())
    }

    pub(super) fn wait_timeout(&self) -> Option<Duration> {
        let timeout = self.timeout.unwrap_or(AGENT_SEND_WAIT_TIMEOUT);
        (!timeout.is_zero()).then_some(timeout)
    }

    pub(super) fn payload(&self) -> Result<String, ServerError> {
        let text = self.text.join(" ");
        let text = text.trim_end_matches(['\n', '\r']);
        check_plain_text("agent-send", text)?;
        let payload = match &self.context {
            Some(reference) => fenced_block(&reference.header(), text),
            None => text.to_owned(),
        };
        if payload.len() > MAX_AGENT_SEND_BYTES {
            return Err(ServerError::InvalidCommand(format!(
                "agent-send payload is {} bytes with its context header; the limit is \
                 {MAX_AGENT_SEND_BYTES}",
                payload.len()
            )));
        }
        Ok(payload)
    }
}

fn fenced_block(header: &str, text: &str) -> String {
    let widest = text
        .split('\n')
        .flat_map(|line| line.split(|character| character != '`'))
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(widest.saturating_add(1).max(3));
    format!("{header}\n{fence}\n{text}\n{fence}")
}

#[must_use]
pub fn agent_send_reads_stdin(args: &[RawText]) -> bool {
    parse_agent_send_args(args).is_ok_and(|parsed| parsed.text.is_empty())
}

#[cfg(feature = "agent")]
const MAX_AGENT_NOTIFY_BYTES: usize = 64 * 1024;

#[cfg(feature = "agent")]
pub(super) fn reply_tail(text: &str) -> String {
    let mut start = text.len().saturating_sub(MAX_AGENT_NOTIFY_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].trim().to_owned()
}

#[cfg(feature = "agent")]
#[derive(Clone, Debug)]
pub(super) struct ChatDelivery {
    pub(super) from: PaneId,
    pub(super) request: Option<u64>,
    pub(super) text: String,
}

pub(super) fn parse_agent_send_args(args: &[RawText]) -> Result<ParsedAgentSend, ServerError> {
    let mut parsed = ParsedAgentSend::default();
    let mut on_block_set = false;
    let mut progress = false;
    let mut explicit_target = false;
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            parsed.text.extend(
                args[index.saturating_add(1)..]
                    .iter()
                    .map(ToString::to_string),
            );
            break;
        }
        if argument == "--submit" {
            parsed.submit = true;
            index += 1;
            continue;
        }
        if argument == "--progress" {
            progress = true;
            index += 1;
            continue;
        }
        if argument == "--wait" {
            parsed.submit = true;
            parsed.wait = true;
            index += 1;
            continue;
        }
        if argument == "--notify" {
            parsed.submit = true;
            parsed.notify = true;
            index += 1;
            continue;
        }
        if argument == "--json" || argument == "--final" {
            parsed.json |= argument == "--json";
            parsed.final_only |= argument == "--final";
            index += 1;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["--on-block"])? {
            parsed.on_block = match value.value.as_str() {
                "wait" => AgentBlockPolicy::Wait,
                "fail" => AgentBlockPolicy::Fail,
                "allow" => AgentBlockPolicy::Allow,
                "deny" => AgentBlockPolicy::Deny,
                _ => {
                    return Err(ServerError::CommandParse(
                        "agent-send --on-block needs wait, fail, allow, or deny".to_owned(),
                    ));
                }
            };
            on_block_set = true;
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["--timeout"])? {
            parsed.timeout = Some(parse_pane_timeout("agent-send", &value.value)?);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["-t"])? {
            explicit_target = true;
            parsed.target = Some(value.value);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["--context"])? {
            parsed.context = Some(ContextReference::parse(&value.value)?);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["--relay"])? {
            parsed.relay = Some(value.value);
            index += value.consumed;
            continue;
        }
        if argument.starts_with('-') && argument != "-" {
            return Err(ServerError::CommandParse(format!(
                "unsupported command: agent-send {argument}"
            )));
        }
        parsed.text.push(argument.to_string());
        index += 1;
    }
    if (parsed.json || parsed.final_only) && !parsed.wait {
        return Err(ServerError::CommandParse(
            "agent-send --json and --final need --wait".to_owned(),
        ));
    }
    if parsed.json && parsed.final_only {
        return Err(ServerError::CommandParse(
            "agent-send --json and --final cannot be combined".to_owned(),
        ));
    }
    if parsed.notify && parsed.wait {
        return Err(ServerError::CommandParse(
            "agent-send --notify and --wait cannot be combined".to_owned(),
        ));
    }
    if progress
        && (!parsed.wait
            || !explicit_target
            || !parsed.target.as_deref().is_some_and(|target| {
                target
                    .strip_prefix('%')
                    .is_some_and(|id| !id.is_empty() && id.bytes().all(|ch| ch.is_ascii_digit()))
            }))
    {
        return Err(ServerError::NativeCommandParse(
            "agent-send: --progress requires --wait and an explicit -t %N".to_owned(),
        ));
    }
    parsed.on_block_set = on_block_set;
    if on_block_set && !parsed.wait {
        return Err(ServerError::CommandParse(
            "agent-send --on-block needs --wait".to_owned(),
        ));
    }
    if parsed.timeout.is_some() && !parsed.wait {
        return Err(ServerError::CommandParse(
            "agent-send --timeout needs --wait".to_owned(),
        ));
    }
    Ok(parsed)
}
