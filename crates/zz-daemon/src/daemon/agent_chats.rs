use super::*;

#[cfg(feature = "agent")]
impl Shared {
    pub(super) fn chat_caller(
        &self,
        client: ClientId,
        context: &ExecutionContext,
        target: Option<PaneId>,
    ) -> Option<PaneId> {
        let inner = self.inner.lock();
        let origin = inner
            .client(context.replay_client().unwrap_or(client))
            .and_then(|client| client.origin)?;
        (Some(origin) != target && is_agent_chat(&inner, origin)).then_some(origin)
    }

    pub(super) fn sent_message(
        &self,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        caller: Option<PaneId>,
        text: String,
    ) -> String {
        use zz_protocol::agent_mention::{SENDER_VARIABLE, Sender, SentMessage, read_sender_fact};
        if kind == ClientKind::Interactive
            || SentMessage::parse(&text).is_some_and(|message| message.host.is_some())
        {
            return text;
        }
        let (sender, host) = if let Some(pane) = caller {
            let provider = self.agent_provider_of(pane);
            (Sender::Chat { pane, provider }, None)
        } else {
            let inner = self.inner.lock();
            let client = context.replay_client().unwrap_or(client);
            let entry = inner.client(client);
            if entry.is_some_and(|entry| entry.remote) {
                client_environment_value(&inner, client, SENDER_VARIABLE)
                    .and_then(read_sender_fact)
                    .unwrap_or((Sender::Command { pane: None }, None))
            } else {
                let pane = entry.and_then(|entry| entry.origin);
                (Sender::Command { pane }, None)
            }
        };
        SentMessage { sender, host, text }.to_string()
    }

    pub(super) fn agent_provider_of(&self, pane: PaneId) -> AgentProvider {
        match self
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .map(|pane| &pane.kind)
        {
            Some(PaneKind::Agent(agent)) => agent.provider,
            _ => AgentProvider::default(),
        }
    }

    pub(super) fn chat_report_waiter(
        self: &Arc<Self>,
        caller: PaneId,
        pane: PaneId,
    ) -> crate::agent::host::AgentTurnWaiter {
        let provider = self.agent_provider_of(pane);
        let shared = Arc::downgrade(&self.server_owner());
        let asking = shared.clone();
        let asked = Arc::new(Mutex::new(BTreeSet::new()));
        crate::agent::host::AgentTurnWaiter {
            reply: crate::daemon::cmdq::Reply::new(move |result| {
                if let Some(shared) = shared.upgrade() {
                    shared.deliver_to_chat(
                        caller,
                        ChatDelivery {
                            from: pane,
                            request: None,
                            text: agent_report(pane, provider, result).to_string(),
                        },
                    );
                }
            }),
            on_block: AgentBlockPolicy::Wait,
            audit: Arc::default(),
            asks: Some(crate::agent::host::AgentAsks(Arc::new(move |permission| {
                if asked.lock().insert(permission.request_id)
                    && let Some(shared) = asking.upgrade()
                {
                    shared.deliver_to_chat(
                        caller,
                        ChatDelivery {
                            from: pane,
                            request: Some(permission.request_id),
                            text: agent_ask(pane, provider, permission).to_string(),
                        },
                    );
                }
            }))),
        }
    }

    pub(super) fn deliver_to_chat(self: &Arc<Self>, caller: PaneId, delivery: ChatDelivery) {
        {
            let mut queues = self.chat_deliveries.lock();
            let queue = queues.entry(caller).or_default();
            queue.push_back(delivery);
            if queue.len() > 1 {
                return;
            }
        }
        let shared = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("zz-agent-report".to_owned())
            .spawn(move || {
                loop {
                    let Some(delivery) = shared
                        .chat_deliveries
                        .lock()
                        .get(&caller)
                        .and_then(|queue| queue.front().cloned())
                    else {
                        return;
                    };
                    shared.deliver_one(caller, delivery);
                    let mut queues = shared.chat_deliveries.lock();
                    if let Some(queue) = queues.get_mut(&caller) {
                        queue.pop_front();
                        if queue.is_empty() {
                            queues.remove(&caller);
                        }
                    }
                }
            });
        if let Err(error) = spawned {
            self.chat_deliveries.lock().remove(&caller);
            log::warn!(target: "zz::agent", "could not report back to {caller}: {error}");
        }
    }

    fn deliver_one(self: &Arc<Self>, caller: PaneId, delivery: ChatDelivery) {
        let ChatDelivery {
            from,
            request,
            text,
        } = delivery;
        let runtime = self.agent_runtime();
        if let Some(request) = request
            && !runtime
                .as_ref()
                .is_some_and(|runtime| runtime.has_pending_permission(from, request))
        {
            log::info!(target: "zz::agent", "{from}'s request {request} was answered; dropped its ask to {caller}");
            return;
        }
        let chat = match self
            .inner
            .lock()
            .engine
            .state
            .pane(caller)
            .map(|pane| &pane.kind)
        {
            None => {
                log::info!(target: "zz::agent", "{caller} is gone; dropped {from}'s report");
                return;
            }
            Some(PaneKind::Agent(_)) => true,
            Some(PaneKind::Terminal) => false,
            Some(_) => {
                log::info!(target: "zz::agent", "{caller} is no chat; dropped {from}'s report");
                return;
            }
        };
        if chat {
            let injected = runtime.is_some_and(|runtime| {
                runtime.inject(
                    caller,
                    AgentPrompt {
                        owner: ClientInstanceId(u64::MAX),
                        text,
                        images: Vec::new(),
                    },
                    request.map(|request| (from, request)),
                )
            });
            if !injected {
                log::warn!(target: "zz::agent", "{caller} did not take {from}'s report");
            }
            return;
        }
        #[cfg(unix)]
        let delivered = self.deliver_to_terminal_pane(caller, &text);
        #[cfg(not(unix))]
        let delivered = self.paste_and_submit(caller, &text, SEND_TEXT_TIMEOUT, true);
        if let Err(error) = delivered {
            log::warn!(target: "zz::agent", "could not tell {caller} about {from}: {error}");
        }
    }

    pub(super) fn open_created_chat(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        verb: &str,
        created: Option<PaneId>,
        briefs: Vec<(PaneId, String)>,
    ) {
        use zz_protocol::agent_mention::{ReplyNotice, ReplyRoute};

        let Some(pane) = created else {
            return;
        };
        let caller = self.chat_caller(client, context, Some(pane));
        let mut queued = false;
        if let Some(runtime) = self.agent_runtime() {
            for (pane, text) in briefs {
                let waiter = caller.map(|caller| self.chat_report_waiter(caller, pane));
                let note = caller.map(|caller| {
                    zz_protocol::agent_mention::child_note(
                        caller,
                        self.agent_provider_of(caller),
                        self.agent_provider_of(pane),
                    )
                });
                queued |= runtime.command_prompt(
                    pane,
                    AgentPrompt {
                        owner: ClientInstanceId(u64::MAX),
                        text: self.sent_message(client, kind, context, caller, text),
                        images: Vec::new(),
                    },
                    waiter,
                    caller.map_or(PromptSource::Command, |_| PromptSource::Chat),
                    note,
                );
                if !queued {
                    log::warn!(target: "zz::agent", "{pane} did not take its brief");
                }
            }
        }
        if queued && let (Some(caller), Some(runtime)) = (caller, self.open_agent_runtime()) {
            runtime.count_send(caller, pane);
        }
        if let Some(caller) = caller {
            self.remember_child(caller, pane);
        }
        let route = match (queued, caller) {
            (false, _) => None,
            (true, Some(_)) => Some(ReplyRoute::Back),
            (true, None) => Some(ReplyRoute::Read),
        };
        let notice = ReplyNotice::Opened {
            verb: verb.to_owned(),
            pane,
            provider: self.agent_provider_of(pane),
            route,
        };
        self.record_command_stderr(client, &notice.to_string());
    }

    pub(super) fn untaken_prompt(&self, pane: PaneId) -> ServerError {
        let state = self
            .inner
            .lock()
            .engine
            .agent_chat(pane)
            .map_or("starting", |chat| chat.state);
        ServerError::InvalidCommand(format!(
            "{pane}'s agent is not running (agent_state {state}); the prompt was not sent"
        ))
    }

    fn chat_descriptor(&self, pane: PaneId) -> Option<zz_protocol::AgentDescriptor> {
        match self
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .map(|pane| &pane.kind)
        {
            Some(PaneKind::Agent(agent)) => Some(agent.clone()),
            _ => None,
        }
    }

    fn host_key(&self, host: PaneId) -> Option<(AgentProvider, String)> {
        let agent = self.chat_descriptor(host)?;
        Some((agent.provider, agent.session_id?))
    }

    fn remember_child(self: &Arc<Self>, host: PaneId, pane: PaneId) {
        let (Some((provider, session)), Some(agent), Some(runtime)) = (
            self.host_key(host),
            self.chat_descriptor(pane),
            self.open_agent_runtime(),
        ) else {
            return;
        };
        runtime.update_child(provider, &session, pane, |record| {
            record.provider = agent.provider;
            record.session = agent.session_id;
            record.cwd = agent.cwd;
            record.model = agent.model;
            record.effort = agent.effort;
            record.account = agent.account;
            record.current = Some(pane.0);
        });
    }

    pub(super) fn note_child_descriptor(self: &Arc<Self>, pane: PaneId) {
        let Some(host) = self.inner.lock().engine.agent_parent(pane) else {
            return;
        };
        let (Some((provider, host_session)), Some(agent), Some(runtime)) = (
            self.host_key(host),
            self.chat_descriptor(pane),
            self.open_agent_runtime(),
        ) else {
            return;
        };
        let Some(record) = runtime
            .child_records(provider, &host_session)
            .into_iter()
            .find(|record| record.current == Some(pane.0))
        else {
            return;
        };
        runtime.update_child(provider, &host_session, PaneId(record.pane), |record| {
            if agent.session_id.is_some() {
                record.session = agent.session_id;
            }
            record.model = agent.model;
            record.effort = agent.effort;
            record.account = agent.account;
        });
    }

    pub(super) fn child_record(
        self: &Arc<Self>,
        host: PaneId,
        child: PaneId,
    ) -> Option<crate::agent::journal::ChildRecord> {
        let (provider, session) = self.host_key(host)?;
        self.open_agent_runtime()?
            .child_records(provider, &session)
            .into_iter()
            .find(|record| record.answers_to(child.0))
    }

    fn child_pane_now(&self, record: &crate::agent::journal::ChildRecord) -> Option<PaneId> {
        let inner = self.inner.lock();
        let holds = |pane: PaneId| match inner.engine.state.pane(pane).map(|pane| &pane.kind) {
            Some(PaneKind::Agent(agent)) => {
                agent.provider == record.provider
                    && (record.session.is_none() || agent.session_id == record.session)
            }
            _ => false,
        };
        let recorded = record
            .current
            .into_iter()
            .chain(std::iter::once(record.pane))
            .map(PaneId)
            .find(|pane| holds(*pane));
        recorded.or_else(|| {
            record.session.as_ref()?;
            inner
                .agents
                .agent_panes
                .iter()
                .copied()
                .find(|pane| holds(*pane))
        })
    }

    pub(super) fn select_child_chat(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let spec = zz_protocol::catalog_command_spec("select-child-chat").expect("spec");
        let options = zz_protocol::parse_tmux_options(spec, args)?;
        let mut target = None;
        for option in options.options {
            if let zz_protocol::TmuxOption::Value("-t", value) = option {
                target = Some(value);
            }
        }
        let [child] = options.positionals else {
            return Err(ServerError::NativeCommandParse(
                "usage: select-child-chat [-t host-pane] child-pane".to_owned(),
            )
            .into());
        };
        let child = child.parse::<PaneId>().map_err(|_| {
            ServerError::NativeCommandParse(format!("select-child-chat: {child} is not a pane id"))
        })?;
        let host = self
            .inner
            .lock()
            .engine
            .resolve_pane(target, context.window, context.pane)?;
        let record = self.child_record(host, child).ok_or_else(|| {
            ServerError::InvalidCommand(format!("{child} is not a chat {host} opened"))
        })?;
        let pane = match self.child_pane_now(&record) {
            Some(pane) => pane,
            None => self.reopen_child(host, &record)?,
        };
        if let Some((provider, session)) = self.host_key(host)
            && let Some(runtime) = self.open_agent_runtime()
        {
            runtime.update_child(provider, &session, PaneId(record.pane), |record| {
                record.current = Some(pane.0);
                if !record.answers_to(pane.0) {
                    record.reopened.push(pane.0);
                }
            });
        }
        let target = pane.to_string();
        for verb in ["select-window", "select-pane"] {
            let mut selection = context.clone();
            self.execute(
                client,
                kind,
                &mut selection,
                &CommandInvocation::new(verb, ["-t", target.as_str()]),
            )?;
        }
        Ok(Execution {
            output: target.into(),
            effects: Vec::new(),
        })
    }

    fn reopen_child(
        self: &Arc<Self>,
        host: PaneId,
        record: &crate::agent::journal::ChildRecord,
    ) -> Result<PaneId, DaemonError> {
        let child = PaneId(record.pane);
        let Some(session) = record.session.clone() else {
            return Err(ServerError::InvalidCommand(format!(
                "{child} was closed before its session started; it cannot be opened again"
            ))
            .into());
        };
        let mut args = vec![
            "-d".to_owned(),
            "-P".to_owned(),
            "-F".to_owned(),
            "#{pane_id}".to_owned(),
            "-t".to_owned(),
            host.to_string(),
            "-x".to_owned(),
            zz_protocol::AGENT_FLOAT_WIDTH.to_owned(),
            "-y".to_owned(),
            zz_protocol::AGENT_FLOAT_HEIGHT.to_owned(),
        ];
        if let Some(cwd) = &record.cwd {
            args.extend(["-c".to_owned(), cwd.display().to_string()]);
        }
        args.extend(["--kind".to_owned(), "agent".to_owned()]);
        for (flag, value) in [
            ("--resume", Some(&session)),
            ("--model", record.model.as_ref()),
            ("--effort", record.effort.as_ref()),
            ("--account", record.account.as_ref()),
        ] {
            if let Some(value) = value {
                args.extend([flag.to_owned(), value.clone()]);
            }
        }
        args.push(record.provider.command_name().to_owned());
        let mut context = ExecutionContext::default();
        context.set_agent_parent(Some(host));
        self.execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-pane", args),
        )?
        .output
        .to_string()
        .trim()
        .parse::<PaneId>()
        .map_err(|_| ServerError::Internal(format!("could not reopen {child}")).into())
    }

    pub(super) fn approve_child_tools(&self, pane: PaneId) {
        let tier = child_auto_approve(&self.inner.lock().engine, pane);
        if let (Some(tier), Some(runtime)) = (tier, self.open_agent_runtime()) {
            runtime.set_auto_approve(pane, tier);
        }
    }

    pub(super) fn moved_child(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        target: Option<&str>,
    ) -> Result<Option<PaneId>, DaemonError> {
        let Some(child) = target.and_then(|target| target.parse::<PaneId>().ok()) else {
            return Ok(None);
        };
        if self.inner.lock().engine.state.pane(child).is_some() {
            return Ok(None);
        }
        let Some(host) = self.chat_caller(client, context, None) else {
            return Ok(None);
        };
        let Some(record) = self.child_record(host, child) else {
            return Ok(None);
        };
        match self.child_pane_now(&record) {
            Some(pane) => Ok(Some(pane)),
            None => Err(ServerError::InvalidCommand(format!(
                "{child} was closed; open a new chat with: zz new-pane -d --kind agent {} 'BRIEF'",
                record.provider.command_name()
            ))
            .into()),
        }
    }

    fn child_key(self: &Arc<Self>, host: PaneId, pane: PaneId) -> PaneId {
        let Some((provider, session)) = self.host_key(host) else {
            return pane;
        };
        self.open_agent_runtime()
            .and_then(|runtime| {
                runtime
                    .child_records(provider, &session)
                    .into_iter()
                    .find(|record| record.current == Some(pane.0) || record.answers_to(pane.0))
            })
            .map_or(pane, |record| PaneId(record.pane))
    }

    pub(super) fn count_chat_send(
        self: &Arc<Self>,
        caller: PaneId,
        pane: PaneId,
    ) -> Result<(), DaemonError> {
        let Some(runtime) = self.open_agent_runtime() else {
            return Ok(());
        };
        let pane = self.child_key(caller, pane);
        let limit = crate::agent::fanout::MAX_SENDS_SINCE_USER;
        if runtime.sends_since_user(caller, pane) >= limit {
            return Err(ServerError::InvalidCommand(format!(
                "{pane} has had {limit} messages from you since the user last wrote; answer the user now"
            ))
            .into());
        }
        runtime.count_send(caller, pane);
        Ok(())
    }

    pub(super) fn refuse_full_chat(self: &Arc<Self>, pane: PaneId) -> Result<(), DaemonError> {
        let waiting = self
            .open_agent_runtime()
            .map_or(0, |runtime| runtime.waiting_prompts(pane));
        let limit = zz_protocol::MAX_AGENT_QUEUED_PROMPTS;
        if waiting < limit {
            return Ok(());
        }
        Err(ServerError::InvalidCommand(format!(
            "{pane} has {limit} prompts waiting; wait for its turn with: {}",
            zz_protocol::agent_mention::wait_command(pane)
        ))
        .into())
    }

    pub(super) fn refuse_grandchild_chat(
        &self,
        client: ClientId,
        context: &ExecutionContext,
        verb: &str,
        args: &[RawText],
    ) -> Result<(), DaemonError> {
        let spawns_chat = zz_protocol::catalog_command_spec(verb)
            .and_then(|spec| zz_protocol::parse_tmux_options(spec, args).ok())
            .is_some_and(|options| {
                options.options.iter().any(|option| {
                    matches!(option, zz_protocol::TmuxOption::Value("--kind", "agent"))
                })
            });
        if !spawns_chat {
            return Ok(());
        }
        let Some(caller) = self.chat_caller(client, context, None) else {
            return Ok(());
        };
        let Some(parent) = self.inner.lock().engine.agent_parent(caller) else {
            return Ok(());
        };
        Err(ServerError::InvalidCommand(format!(
            "{caller} was opened by {parent}; a chat opened by another chat cannot open one. Do the work yourself, or say in your reply what you need"
        ))
        .into())
    }

    pub(super) fn close_orphaned_chats(self: &Arc<Self>, removed: &[PaneId]) {
        let orphans = {
            let inner = self.inner.lock();
            removed
                .iter()
                .flat_map(|parent| inner.engine.agent_children(*parent))
                .filter(|child| inner.engine.state.pane(*child).is_some())
                .collect::<Vec<_>>()
        };
        if orphans.is_empty() {
            return;
        }
        let shared = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("zz-agent-cascade".to_owned())
            .spawn(move || {
                for child in orphans {
                    let target = child.to_string();
                    if let Err(error) = shared.execute(
                        ClientId(u64::MAX),
                        ClientKind::Command,
                        &mut ExecutionContext::default(),
                        &CommandInvocation::new("kill-pane", ["-t", target.as_str()]),
                    ) {
                        log::warn!(target: "zz::agent", "could not close {child}: {error}");
                    }
                }
            });
        if let Err(error) = spawned {
            log::warn!(target: "zz::agent", "could not close the chats opened by {removed:?}: {error}");
        }
    }

    pub(super) fn agent_message(
        self: &Arc<Self>,
        client: ClientId,
        message: ProtocolMessage,
    ) -> Result<(), ServerError> {
        let pane = agent_message_pane(&message)?;
        match self
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .map(|pane| &pane.kind)
        {
            Some(PaneKind::Agent(_)) => {}
            Some(_) => {
                return Err(ServerError::InvalidTarget(format!(
                    "{pane} is not an agent pane"
                )));
            }
            None => return Err(ServerError::MissingTarget(pane.to_string())),
        }
        if let ProtocolMessage::AgentSessionOp { op, .. } = &message {
            let valid_directory = |path: &Path| {
                path.is_absolute()
                    && path.as_os_str().as_encoded_bytes().len() <= MAX_GUI_TEXT_BYTES
            };
            let valid = match op {
                AgentSessionOpKind::List { cwd, .. } => cwd.as_deref().is_none_or(valid_directory),
                AgentSessionOpKind::New { cwd } => valid_directory(cwd),
                AgentSessionOpKind::Switch {
                    cwd,
                    additional_directories,
                    ..
                } => {
                    valid_directory(cwd)
                        && additional_directories.len() <= MAX_AGENT_SESSION_DIRECTORIES
                        && additional_directories
                            .iter()
                            .all(|path| valid_directory(path))
                }
                AgentSessionOpKind::Delete { .. } => true,
            };
            if !valid {
                return Err(ServerError::InvalidCommand(
                    "agent session directories must be absolute on the daemon host".to_owned(),
                ));
            }
        }
        let Some(runtime) = self.agent_runtime() else {
            return Err(ServerError::PaneExited(pane));
        };
        let command = match message {
            ProtocolMessage::AgentReplay { from_seq, .. } => {
                let watcher = {
                    let mut inner = self.inner.lock();
                    match inner.client_mut(client) {
                        Some(entry) => {
                            entry.muted_agents.remove(&pane);
                            entry
                                .watched_agents
                                .insert(pane)
                                .then(|| entry.subscriber.clone())
                                .flatten()
                        }
                        None => None,
                    }
                };
                if let Some(subscriber) = watcher
                    && let Some(state) = runtime.wire_state(pane)
                {
                    Self::send_event(&subscriber, EventPayload::AgentState { pane, state });
                }
                runtime.replay(client, pane, from_seq);
                return Ok(());
            }
            ProtocolMessage::AgentUnwatch { .. } => {
                let (subscriber, watching) = {
                    let mut inner = self.inner.lock();
                    let Some(entry) = inner.client_mut(client) else {
                        return Ok(());
                    };
                    entry.muted_agents.insert(pane);
                    (entry.subscriber.clone(), entry.watched_agents.insert(pane))
                };
                if let Some(subscriber) = subscriber {
                    subscriber.cancel_agent(pane);
                    if watching && let Some(state) = runtime.wire_state(pane) {
                        Self::send_event(&subscriber, EventPayload::AgentState { pane, state });
                    }
                }
                return Ok(());
            }
            ProtocolMessage::AgentAcknowledgePromptRestore { reclaim_id, .. } => {
                let owner = self
                    .client_instance_id(client)
                    .ok_or(ServerError::PaneExited(pane))?;
                runtime.acknowledge_prompt_restore(owner, pane, reclaim_id);
                return Ok(());
            }
            ProtocolMessage::AgentPrompt { text, images, .. } => {
                let owner = self
                    .client_instance_id(client)
                    .ok_or(ServerError::PaneExited(pane))?;
                return runtime
                    .prompt(
                        pane,
                        AgentPrompt {
                            owner,
                            text,
                            images: images
                                .into_iter()
                                .map(|image| AgentImage {
                                    format: image.format,
                                    data: image.data,
                                })
                                .collect(),
                        },
                    )
                    .then_some(())
                    .ok_or(ServerError::PaneExited(pane));
            }
            ProtocolMessage::AgentCancel { .. } => HostCommand::Cancel,
            ProtocolMessage::AgentUnqueue { .. } => HostCommand::Unqueue,
            ProtocolMessage::AgentRespondPermission {
                request_id,
                option_id,
                ..
            } => HostCommand::RespondPermission {
                response: PermissionResponse::Exact {
                    request_id,
                    option_id,
                },
            },
            ProtocolMessage::AgentAnswerQuestion {
                request_id,
                answers,
                ..
            } => HostCommand::AnswerQuestion {
                request_id,
                answers,
            },
            ProtocolMessage::AgentStopTask { task_id, .. } => HostCommand::StopTask { task_id },
            ProtocolMessage::AgentSteer { text, .. } => HostCommand::Steer(AgentPrompt {
                owner: self
                    .client_instance_id(client)
                    .ok_or(ServerError::PaneExited(pane))?,
                text,
                images: Vec::new(),
            }),
            ProtocolMessage::AgentSetConfigOption {
                option_id, value, ..
            } => HostCommand::SetConfigOption { option_id, value },
            ProtocolMessage::AgentSetMode { mode_id, .. } => HostCommand::SetMode { mode_id },
            ProtocolMessage::AgentAuthenticate { method_id, .. } => {
                HostCommand::Authenticate { method_id }
            }
            ProtocolMessage::AgentSessionOp { op, .. } => match op {
                AgentSessionOpKind::List {
                    cwd,
                    cursor,
                    replace,
                } => HostCommand::ListSessions {
                    client,
                    cwd,
                    cursor,
                    replace,
                },
                AgentSessionOpKind::New { cwd } => HostCommand::NewSession { cwd, reply: None },
                AgentSessionOpKind::Switch {
                    session_id,
                    cwd,
                    additional_directories,
                } => HostCommand::SwitchSession {
                    session: AgentSessionSummary {
                        session_id,
                        cwd,
                        additional_directories,
                        title: None,
                        updated_at: None,
                    },
                },
                AgentSessionOpKind::Delete { session_id } => {
                    HostCommand::DeleteSession { client, session_id }
                }
            },
            _ => return Ok(()),
        };
        runtime
            .command(pane, command)
            .then_some(())
            .ok_or(ServerError::PaneExited(pane))
    }

    #[cfg(feature = "agent")]
    pub(super) fn submit_agent_prompt_and_notify(
        self: &Arc<Self>,
        caller: Option<PaneId>,
        pane: PaneId,
        text: String,
        kind: ClientKind,
    ) -> Result<Execution, DaemonError> {
        let caller = caller
            .filter(|caller| *caller != pane)
            .filter(|caller| {
                matches!(
                    self.inner
                        .lock()
                        .engine
                        .state
                        .pane(*caller)
                        .map(|pane| &pane.kind),
                    Some(PaneKind::Terminal)
                )
            })
            .ok_or_else(|| {
                ServerError::InvalidCommand(
                    "agent-send --notify runs from inside another terminal pane".to_owned(),
                )
            })?;
        let runtime = self.agent_runtime().ok_or(ServerError::PaneExited(pane))?;
        let shared = Arc::downgrade(&self.server_owner());
        let waiter = crate::agent::host::AgentTurnWaiter {
            reply: crate::daemon::cmdq::Reply::new(move |result| {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let message = agent_notify_message(pane, result);
                let spawned = std::thread::Builder::new()
                    .name("zz-agent-notify".to_owned())
                    .spawn(move || {
                        #[cfg(unix)]
                        let delivered = shared.deliver_to_terminal_pane(caller, &message);
                        #[cfg(not(unix))]
                        let delivered =
                            shared.paste_and_submit(caller, &message, SEND_TEXT_TIMEOUT, true);
                        if let Err(error) = delivered {
                            log::warn!(target: "zz::agent", "could not tell {caller} that {pane} finished: {error}");
                        }
                    });
                if let Err(error) = spawned {
                    log::warn!(target: "zz::agent", "could not report {pane} back to {caller}: {error}");
                }
            }),
            on_block: AgentBlockPolicy::Wait,
            audit: Arc::default(),
            asks: None,
        };
        if !runtime.command_prompt(
            pane,
            AgentPrompt {
                owner: ClientInstanceId(u64::MAX),
                text,
                images: Vec::new(),
            },
            Some(waiter),
            PromptSource::Command,
            None,
        ) {
            return Err(ServerError::PaneExited(pane).into());
        }
        Ok(Execution {
            output: if kind == ClientKind::Interactive {
                RawText::default()
            } else {
                pane.to_string().into()
            },
            effects: Vec::new(),
        })
    }

    #[cfg(feature = "agent")]
    pub(super) fn wait_for_agent_turn(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
        text: Option<String>,
        parsed: &ParsedAgentSend,
        chat_caller: bool,
    ) -> Result<Execution, DaemonError> {
        let runtime = self.agent_runtime().ok_or(ServerError::PaneExited(pane))?;
        let audit = Arc::new(Mutex::new(Vec::<String>::new()));
        let collected = Arc::clone(&audit);
        let parsed = parsed.clone();
        let on_block = parsed.on_block;
        let timeout = parsed.wait_timeout();
        let (wait, waiter) = agent_waits::reply::<crate::agent::host::AgentTurnResult>(
            self,
            timeout,
            move |shared, outcome| {
                for line in collected.lock().iter() {
                    shared.record_command_stderr(client, line);
                }
                let result = match outcome {
                    terminal_requests::ReplyOutcome::Ready(result) => result,
                    terminal_requests::ReplyOutcome::Closed => {
                        return Err(ServerError::PaneExited(pane).into());
                    }
                    terminal_requests::ReplyOutcome::TimedOut => {
                        return Err(DaemonError::CommandExit {
                            output: format!(
                                "{pane}: no reply within {} seconds; the turn is still running. \
                                 `agent-send -t {pane} --wait ''` waits again without sending",
                                timeout.unwrap().as_secs()
                            )
                            .into(),
                            exit_code: 124,
                        });
                    }
                };
                let result = (|| {
                    let output = match result {
                        Ok(reply) => agent_turn_reply_output(pane, &reply, &parsed, None),
                        Err(AgentTurnFailure::Blocked { permission, reply }) => {
                            let permission = agent_permission_json(&permission);
                            return Err(DaemonError::CommandExit {
                                output: if parsed.json {
                                    agent_turn_reply_output(
                                        pane,
                                        &reply,
                                        &parsed,
                                        Some(&permission),
                                    )
                                } else if chat_caller {
                                    format!(
                                        "{permission}\nzz agent-respond -t {pane} --allow\n\
                                         zz agent-respond -t {pane} --deny\n\
                                         zz agent-send -t {pane} --wait ''"
                                    )
                                } else {
                                    permission.to_string()
                                }
                                .into(),
                                exit_code: 3,
                            });
                        }
                        Err(AgentTurnFailure::Stopped { reason, reply }) => {
                            shared.record_command_stderr(
                                client,
                                &format!("agent-send: turn stopped: {reason}"),
                            );
                            return Err(DaemonError::CommandExit {
                                output: agent_turn_reply_output(pane, &reply, &parsed, None).into(),
                                exit_code: 1,
                            });
                        }
                        Err(AgentTurnFailure::Failed(message)) => {
                            return Err(ServerError::InvalidCommand(format!(
                                "{pane}: turn failed: {message}"
                            ))
                            .into());
                        }
                        Err(AgentTurnFailure::Cancelled) => {
                            return Err(ServerError::InvalidCommand(format!(
                                "{pane}: turn cancelled"
                            ))
                            .into());
                        }
                        Err(AgentTurnFailure::Reclaimed) => {
                            return Err(ServerError::InvalidCommand(format!(
                                "{pane}: prompt handed back before its turn ran"
                            ))
                            .into());
                        }
                        Err(AgentTurnFailure::Closed) => {
                            return Err(ServerError::PaneExited(pane).into());
                        }
                        Err(AgentTurnFailure::NoTurn) => {
                            return Err(ServerError::InvalidCommand(format!(
                                "{pane} has no running turn; read its last reply with: zz capture-pane -p -t {pane}"
                            ))
                            .into());
                        }
                    };
                    Ok(Execution {
                        output: output.into(),
                        effects: Vec::new(),
                    })
                })();
                if result.is_ok()
                    || matches!(
                        &result,
                        Err(DaemonError::CommandExit {
                            exit_code: 1 | 3,
                            ..
                        })
                    )
                {
                    shared.record_command_stderr(client, &pane.to_string());
                }
                result
            },
        );
        let waiter = crate::agent::host::AgentTurnWaiter {
            reply: waiter,
            on_block,
            audit,
            asks: None,
        };
        let submitted = match text {
            Some(text) => runtime.command_prompt(
                pane,
                AgentPrompt {
                    owner: ClientInstanceId(u64::MAX),
                    text,
                    images: Vec::new(),
                },
                Some(waiter),
                if chat_caller {
                    PromptSource::Chat
                } else {
                    PromptSource::Command
                },
                None,
            ),
            None => runtime.join(pane, waiter),
        };
        if !submitted {
            return Err(self.untaken_prompt(pane).into());
        }
        self.report_command_queue_park();
        wait.finish(self, Execution::default())
    }
}

#[cfg(feature = "agent")]
fn agent_message_pane(message: &ProtocolMessage) -> Result<PaneId, ServerError> {
    match message {
        ProtocolMessage::AgentPrompt { pane, .. }
        | ProtocolMessage::AgentCancel { pane }
        | ProtocolMessage::AgentUnqueue { pane }
        | ProtocolMessage::AgentRespondPermission { pane, .. }
        | ProtocolMessage::AgentSetConfigOption { pane, .. }
        | ProtocolMessage::AgentSetMode { pane, .. }
        | ProtocolMessage::AgentAuthenticate { pane, .. }
        | ProtocolMessage::AgentSessionOp { pane, .. }
        | ProtocolMessage::AgentReplay { pane, .. }
        | ProtocolMessage::AgentUnwatch { pane }
        | ProtocolMessage::AgentAcknowledgePromptRestore { pane, .. }
        | ProtocolMessage::AgentAnswerQuestion { pane, .. }
        | ProtocolMessage::AgentStopTask { pane, .. }
        | ProtocolMessage::AgentSteer { pane, .. } => Ok(*pane),
        _ => Err(ServerError::InvalidCommand(
            "not an agent message".to_owned(),
        )),
    }
}

#[cfg(feature = "agent")]
pub(super) fn agent_turn_reply_output(
    pane: PaneId,
    reply: &crate::agent::host::AgentTurnReply,
    parsed: &ParsedAgentSend,
    permission: Option<&serde_json::Value>,
) -> String {
    if parsed.json {
        return serde_json::to_string(&AgentReplyEnvelope {
            pane: pane.to_string(),
            stop_reason: &reply.stop_reason,
            duration_ms: reply.duration.as_millis(),
            tool_calls: reply.tool_calls,
            permissions: AgentReplyPermissions {
                requested: reply.permissions_requested,
                allowed: reply.permissions_allowed,
                denied: reply.permissions_denied,
            },
            truncated: reply.truncated,
            final_text: &reply.final_text,
            transcript: &reply.text,
            permission,
        })
        .expect("turn reply JSON");
    }
    if parsed.final_only {
        return reply.final_text.clone();
    }
    let mut text = reply.text.clone();
    if reply.truncated {
        text.push('\n');
        text.push_str(COMMAND_PROMPT_OUTPUT_TRUNCATED);
    }
    text
}

#[cfg(feature = "agent")]
fn agent_reply_tail(reply: &crate::agent::host::AgentTurnReply) -> String {
    reply_tail(if reply.final_text.trim().is_empty() {
        &reply.text
    } else {
        &reply.final_text
    })
}

#[cfg(feature = "agent")]
fn agent_report(
    pane: PaneId,
    provider: AgentProvider,
    result: Option<crate::agent::host::AgentTurnResult>,
) -> zz_protocol::agent_mention::AgentReport {
    use crate::agent::host::AgentTurnFailure;
    use zz_protocol::agent_mention::AgentReport;

    let failed = |message: &str| AgentReport::Failed {
        pane,
        provider,
        message: message.to_owned(),
    };
    match result {
        Some(Ok(done)) => AgentReport::Finished {
            pane,
            provider,
            text: agent_reply_tail(&done),
        },
        Some(Err(AgentTurnFailure::Stopped { reason, reply })) => AgentReport::Stopped {
            pane,
            provider,
            reason,
            text: agent_reply_tail(&reply),
        },
        Some(Err(AgentTurnFailure::Cancelled)) => AgentReport::Stopped {
            pane,
            provider,
            reason: "cancelled".to_owned(),
            text: String::new(),
        },
        Some(Err(AgentTurnFailure::Failed(message))) => failed(&message),
        Some(Err(AgentTurnFailure::Reclaimed)) => {
            failed("the message was handed back before its turn ran")
        }
        Some(Err(AgentTurnFailure::Blocked { .. })) => failed("the turn stopped on a permission"),
        Some(Err(AgentTurnFailure::NoTurn)) => failed("no turn ran"),
        Some(Err(AgentTurnFailure::Closed)) | None => AgentReport::Closed { pane, provider },
    }
}

#[cfg(feature = "agent")]
pub(super) fn agent_ask(
    pane: PaneId,
    provider: AgentProvider,
    permission: &crate::agent::host::AgentPendingPermission,
) -> zz_protocol::agent_mention::AgentAsk {
    use zz_protocol::agent_mention::{AgentAsk, AskAnswers, AskOption};

    let text = |key: &str| {
        permission
            .tool_call
            .get(key)
            .and_then(serde_json::Value::as_str)
    };
    let title = text("title").or_else(|| text("toolCallId")).unwrap_or("?");
    let what = text("kind")
        .filter(|kind| *kind != "other")
        .map_or_else(|| title.to_owned(), |kind| format!("{kind} {title}"));
    let options = permission
        .options
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let plain = permission.questions.is_empty()
        && !options.is_empty()
        && options.iter().all(|option| {
            matches!(
                option.get("kind").and_then(serde_json::Value::as_str),
                Some("allow_once" | "allow_always" | "reject_once" | "reject_always")
            )
        });
    let answers = if plain {
        AskAnswers::AllowDeny
    } else if options.is_empty() {
        AskAnswers::FreeText
    } else {
        AskAnswers::Options(
            options
                .iter()
                .map(|option| {
                    let field = |key: &str| {
                        option
                            .get(key)
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    AskOption {
                        id: field("optionId"),
                        label: field("name"),
                    }
                })
                .collect(),
        )
    };
    AgentAsk {
        pane,
        provider,
        what,
        answers,
        request: Some(permission.request_id),
    }
}

#[cfg(feature = "agent")]
pub(super) fn agent_notify_message(
    pane: PaneId,
    result: Option<crate::agent::host::AgentTurnResult>,
) -> String {
    use crate::agent::host::AgentTurnFailure;

    let reply = agent_reply_tail;
    match result {
        Some(Ok(done)) => format!("[zz] Agent {pane} finished:\n\n{}", reply(&done)),
        Some(Err(AgentTurnFailure::Stopped {
            reason,
            reply: done,
        })) => format!("[zz] Agent {pane} stopped ({reason}):\n\n{}", reply(&done)),
        Some(Err(AgentTurnFailure::Blocked { .. })) => {
            format!("[zz] Agent {pane} is waiting for a permission answer.")
        }
        Some(Err(AgentTurnFailure::Failed(message))) => {
            format!("[zz] Agent {pane} failed: {message}")
        }
        Some(Err(AgentTurnFailure::Cancelled)) => format!("[zz] Agent {pane} was stopped."),
        Some(Err(
            AgentTurnFailure::Reclaimed | AgentTurnFailure::Closed | AgentTurnFailure::NoTurn,
        ))
        | None => {
            format!("[zz] Agent {pane} did not take the message.")
        }
    }
}
