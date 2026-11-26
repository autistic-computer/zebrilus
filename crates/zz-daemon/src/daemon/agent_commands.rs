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
