use super::*;

#[cfg(feature = "agent")]
impl Shared {
    pub(super) fn agent_runtime(self: &Arc<Self>) -> Option<Arc<AgentRuntime>> {
        if self.agent_stopped.load(Ordering::Acquire) {
            return None;
        }
        if let Some(runtime) = self.agent.lock().as_ref() {
            return Some(Arc::clone(runtime));
        }
        let journal = if cfg!(test) {
            None
        } else {
            load_persistent_journal()
        };
        self.build_agent_runtime(journal)
    }

    pub(super) fn build_agent_runtime(
        self: &Arc<Self>,
        journal: Option<Arc<crate::agent::journal::AgentJournal>>,
    ) -> Option<Arc<AgentRuntime>> {
        #[cfg(any(test, windows))]
        self.start_watcher_consumer().ok()?;
        let config = self.agent_spawn_config();
        let mut slot = self.agent.lock();
        if self.agent_stopped.load(Ordering::Acquire) {
            return None;
        }
        if let Some(runtime) = slot.as_ref() {
            return Some(Arc::clone(runtime));
        }
        let publisher: Arc<dyn AgentPublisher> = Arc::new(self.agent_tx.publisher());
        let runtime = Arc::new(AgentRuntime::new(&publisher, config, journal));
        runtime.prewarm();
        #[cfg(test)]
        runtime.set_runner_factory(Box::new(|_| {
            Box::new(|_| Box::pin(async { Err("no agent adapter in tests".to_owned()) }))
        }));
        *slot = Some(Arc::clone(&runtime));
        Some(runtime)
    }

    pub(super) fn open_agent_runtime(&self) -> Option<Arc<AgentRuntime>> {
        self.agent.lock().clone()
    }

    pub(super) fn agent_spawn_config(&self) -> AgentSpawnConfig {
        #[cfg(unix)]
        let (tools, tools_executable) =
            self.tmux_shim.lock().as_ref().map_or((None, None), |shim| {
                (
                    Some(shim.directory.display().to_string()),
                    Some(shim.executable.display().to_string()),
                )
            });
        #[cfg(not(unix))]
        let (tools, tools_executable) = (None, None);
        let inner = self.inner.lock();
        let options = inner.engine.agent_options();
        AgentSpawnConfig {
            command: options.command.clone(),
            claude_code_command: options.claude_code_command.clone(),
            auto_approve: options.auto_approve,
            workspace: AgentWorkspaceEnvironment {
                pane: None,
                agent: None,
                session: None,
                socket: Some(self.socket_path.display().to_string()),
                tmux: None,
                tmux_pane: None,
                tools,
                tools_executable,
                managed_login: false,
            },
        }
    }

    pub(super) fn reconfigure_agents(&self) {
        let _effects = self.agent_effects.lock();
        let config = self.agent_spawn_config();
        let Some(runtime) = self.open_agent_runtime() else {
            return;
        };
        runtime.reconfigure(config);
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn peer_scan_inputs(&self) -> Vec<(PaneId, String, Option<u32>)> {
        if self.agent_stopped.load(Ordering::Acquire) {
            return Vec::new();
        }
        let inner = self.inner.lock();
        inner
            .engine
            .state
            .windows
            .values()
            .flat_map(|window| &window.panes)
            .filter_map(|(pane, state)| {
                if !matches!(state.kind, PaneKind::Terminal) {
                    return None;
                }
                let target = pane.to_string();
                Some((
                    *pane,
                    target,
                    inner
                        .engine
                        .pane_runtime_facts(*pane)
                        .and_then(|runtime| runtime.pid),
                ))
            })
            .collect()
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn sync_claude_peer_states(self: &Arc<Self>) {
        if self.helpers.on_loop_thread() {
            if let Err(error) = self.submit_helper(helpers::Task::Peers {
                panes: self.peer_scan_inputs(),
                reply: None,
                completed: None,
            }) {
                log::warn!("could not scan Claude peers: {error}");
            }
            return;
        }
        let (reply, result) = mpsc::sync_channel(1);
        if let Err(error) = self.helpers.submit_wait(helpers::Task::Peers {
            panes: self.peer_scan_inputs(),
            reply: Some(reply),
            completed: None,
        }) {
            log::warn!("could not scan Claude peers: {error}");
            return;
        }
        if let Ok(result) = result.recv() {
            self.apply_peer_states(result);
        }
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn apply_peer_states(self: &Arc<Self>, result: helpers::PeerResult) {
        if self.agent_stopped.load(Ordering::Acquire) {
            return;
        }
        let updates = match result {
            Ok(updates) => updates,
            Err(error) => {
                log::warn!(target: "zz::agent", "could not read Claude peers: {error}");
                return;
            }
        };
        for (pane, pid, value) in updates {
            let value = {
                let mut inner = self.inner.lock();
                if value.is_none() && !inner.agents.claude_peer_states.contains_key(&pane) {
                    continue;
                }
                let Some(window) = inner.engine.state.window_for_pane(pane) else {
                    continue;
                };
                if !inner
                    .engine
                    .state
                    .pane(pane)
                    .is_some_and(|pane| matches!(pane.kind, PaneKind::Terminal))
                    || inner
                        .engine
                        .pane_runtime_facts(pane)
                        .and_then(|runtime| runtime.pid)
                        != pid
                    || inner.panes.program_status_panes.contains(&pane)
                {
                    continue;
                }
                let session = inner.engine.state.windows[&window].session;
                if inner.engine.format_facts().user_option(
                    &pane.to_string(),
                    &window.to_string(),
                    &session.to_string(),
                    "@agent-peer-state",
                ) == Some("off")
                {
                    continue;
                }
                if let Some(value) = value {
                    if inner.agents.claude_peer_states.get(&pane) == Some(&value) {
                        continue;
                    }
                    inner.agents.claude_peer_states.insert(pane, value.clone());
                    value
                } else if inner.agents.claude_peer_states.remove(&pane).is_none() {
                    continue;
                } else {
                    "idle".to_owned()
                }
            };
            if self.loop_active.load(Ordering::Acquire) {
                let _ = self
                    .timer_tx
                    .send(timers::TimerInput::PeerSample { pane, pid, value });
            } else {
                self.write_pane_agent_state(pane, &value);
            }
        }
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn update_terminal_peer(self: &Arc<Self>, pane: PaneId) {
        use crate::agent::claude_peers::{self, PeerEvent, PeerInbox, PeerMetadata};

        let mut peers = self.agent_peers.lock();
        if self.agent_stopped.load(Ordering::Acquire) {
            return;
        }
        let metadata = {
            let mut inner = self.inner.lock();
            if !matches!(
                inner.engine.state.pane(pane).map(|pane| &pane.kind),
                Some(PaneKind::Terminal)
            ) {
                return;
            }
            let present = inner.engine.execute_prepared(
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "show-options",
                    ["-p", "-q", "-t", &pane.to_string(), "@name"],
                ),
            );
            match present {
                Ok(execution) if execution.output.is_empty() => {
                    peers.remove(&pane);
                    return;
                }
                Err(error) => {
                    log::warn!(target: "zz::agent", "could not read terminal peer name {pane}: {error}");
                    return;
                }
                Ok(_) => {}
            }
            let Some(window) = inner.engine.state.window_for_pane(pane) else {
                return;
            };
            let session = inner.engine.state.windows[&window].session;
            let facts = inner.engine.format_facts();
            let Some(name) = facts.user_option(
                &pane.to_string(),
                &window.to_string(),
                &session.to_string(),
                "@name",
            ) else {
                peers.remove(&pane);
                return;
            };
            let Some(runtime) = inner.engine.pane_runtime_facts(pane) else {
                return;
            };
            let Some(pid) = runtime.pid else {
                log::warn!(target: "zz::agent", "could not register terminal peer {pane}: shell pid unavailable");
                return;
            };
            PeerMetadata {
                pid: Some(pid),
                pane: pane.to_string(),
                name: name.to_owned(),
                cwd: if runtime.current_path.is_empty() {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
                } else {
                    PathBuf::from(&runtime.current_path)
                },
                tmux: format!(
                    "{}:{window}.{pane}",
                    inner.engine.state.sessions[&session].name
                ),
            }
        };
        let mut records = match claude_peers::read_records() {
            Ok(records) => records,
            Err(error) => {
                log::warn!(target: "zz::agent", "could not read Claude peers: {error}");
                return;
            }
        };
        records.retain(|record| record.zz.is_none());
        if claude_peers::record_for_pane(&records, &pane.to_string(), metadata.pid).is_some() {
            peers.remove(&pane);
            return;
        }
        if let Some(peer) = peers.get_mut(&pane) {
            if let Err(error) = peer.update(false, &metadata.name) {
                log::warn!(target: "zz::agent", "could not rename terminal peer {pane}: {error}");
            }
            return;
        }
        let owner = Arc::downgrade(&self.server_owner());
        match PeerInbox::register(metadata, move |event| {
            if let PeerEvent::Message { content, .. } = event
                && let Some(shared) = owner.upgrade()
                && let Err(error) = shared.deliver_to_terminal_pane(pane, &content)
            {
                log::warn!(target: "zz::agent", "could not submit terminal peer prompt to {pane}: {error}");
            }
        }) {
            Ok(peer) => {
                peers.insert(pane, peer);
            }
            Err(error) => {
                log::warn!(target: "zz::agent", "could not register terminal peer {pane}: {error}");
            }
        }
    }

    pub(super) fn open_agent_pane(
        self: &Arc<Self>,
        pane: PaneId,
        replaced: Option<Arc<TerminalSession>>,
    ) {
        let (cli, fork) = {
            let mut inner = self.inner.lock();
            (
                inner.agents.cli_exits.remove(&pane),
                inner.agents.agent_forks.remove(&pane),
            )
        };
        let running = move |terminal: &Option<Arc<TerminalSession>>| {
            terminal
                .as_ref()
                .is_some_and(|terminal| terminal.completion().is_none())
                || cli.is_some_and(crate::pid_alive)
        };
        if let Some(at) = fork {
            let shared = Arc::downgrade(&self.server_owner());
            let spawned = std::thread::Builder::new()
                .name("zz-agent-fork".to_owned())
                .spawn(move || {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    shared.fork_agent_session(pane, at.as_deref());
                    shared.open_agent_pane(pane, replaced);
                });
            if let Err(error) = spawned {
                log::warn!(target: "zz::agent", "could not fork the session for {pane}: {error}");
            }
            return;
        }
        if running(&replaced) {
            let shared = Arc::downgrade(&self.server_owner());
            let spawned = std::thread::Builder::new()
                .name("zz-agent-flip".to_owned())
                .spawn(move || {
                    let deadline = Instant::now() + AGENT_CLI_EXIT_GRACE;
                    while running(&replaced) && Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    if let Some(shared) = shared.upgrade() {
                        shared.open_agent_pane(pane, None);
                    }
                });
            if let Err(error) = spawned {
                log::warn!(target: "zz::agent", "could not wait for {pane}'s CLI to exit: {error}");
                self.open_agent_pane(pane, None);
            }
            return;
        }
        let _effects = self.agent_effects.lock();
        let Some(spec) = self.agent_pane_spec(pane) else {
            return;
        };
        let Some(runtime) = self.agent_runtime() else {
            return;
        };
        runtime.reconfigure(self.agent_spawn_config());
        self.inner.lock().agents.agent_panes.insert(pane);
        self.refresh_agent_visibility();
        if !runtime.open(pane, spec) {
            log::warn!(target: "zz::agent", "{pane} already has an agent runtime");
        }
    }

    fn fork_agent_session(&self, pane: PaneId, at: Option<&str>) {
        let source = match &self
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .map(|pane| &pane.kind)
        {
            Some(PaneKind::Agent(agent)) => agent.session_id.clone().map(|id| (agent.provider, id)),
            _ => None,
        };
        let Some((provider, source)) = source else {
            return;
        };
        let forked = match provider {
            AgentProvider::ClaudeCode => crate::agent_fork::claude_config()
                .ok_or_else(|| "HOME is not set".to_owned())
                .and_then(|config| crate::agent_fork::claude(&config, &source, at)),
            AgentProvider::Codex => crate::agent_fork::codex_home()
                .ok_or_else(|| "HOME is not set".to_owned())
                .and_then(|home| crate::agent_fork::codex(&home, &source, at)),
            AgentProvider::Antigravity => Err("Antigravity cannot fork a chat".to_owned()),
            AgentProvider::Grok => Err("Grok cannot fork a chat".to_owned()),
            AgentProvider::Cursor => Err("Cursor cannot fork a chat".to_owned()),
        };
        let session = forked
            .inspect_err(|error| {
                log::warn!(target: "zz::agent", "could not fork {source} for {pane}, starting a new chat: {error}");
            })
            .ok();
        let changed = self
            .inner
            .lock()
            .engine
            .state
            .update_agent(pane, |agent| agent.session_id = session)
            .unwrap_or(false);
        if changed {
            self.publish_snapshot_state();
        }
    }

    pub(super) fn agent_pane_spec(&self, pane: PaneId) -> Option<AgentPaneSpec> {
        let inner = self.inner.lock();
        let PaneKind::Agent(descriptor) = &inner.engine.state.pane(pane)?.kind else {
            return None;
        };
        let session = inner
            .engine
            .state
            .window_for_pane(pane)
            .map(|window| inner.engine.state.windows[&window].session);
        Some(AgentPaneSpec {
            provider: descriptor.provider,
            cwd: descriptor
                .cwd
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("/")),
            resume_session: descriptor.session_id.clone(),
            model: descriptor.model.clone(),
            effort: descriptor.effort.clone(),
            auto_approve: child_auto_approve(&inner.engine, pane),
            workspace: AgentWorkspaceEnvironment {
                pane: Some(pane.to_string()),
                agent: Some(descriptor.provider.command_name().to_owned()),
                session: session.map(|session| session.to_string()),
                socket: None,
                tmux: session.map(|session| tmux_environment(&self.socket_path, Some(session))),
                tmux_pane: Some(pane.to_string()),
                managed_login: managed_login(descriptor.provider),
                ..AgentWorkspaceEnvironment::default()
            },
        })
    }

    pub(super) fn close_agent_panes(&self, panes: &[PaneId]) {
        if panes.is_empty() {
            return;
        }
        let _effects = self.agent_effects.lock();
        #[cfg(unix)]
        {
            let mut peers = self.agent_peers.lock();
            for pane in panes {
                peers.remove(pane);
            }
        }
        if let Some(runtime) = self.open_agent_runtime() {
            for pane in panes {
                runtime.close(*pane);
            }
        }
        let mut inner = self.inner.lock();
        for pane in panes {
            inner.agents.agent_panes.remove(pane);
            inner.engine.set_agent_chat(*pane, None);
        }
        for client in inner.clients.values_mut() {
            for pane in panes {
                client.watched_agents.remove(pane);
                client.muted_agents.remove(pane);
            }
        }
    }

    pub(super) fn shutdown_agents(&self) {
        let runtime = {
            let _effects = self.agent_effects.lock();
            self.agent_stopped.store(true, Ordering::Release);
            #[cfg(unix)]
            self.agent_peers.lock().clear();
            self.agent.lock().take()
        };
        let Some(runtime) = runtime else {
            return;
        };
        runtime.shutdown();
    }

    pub(super) fn send_agent_resync(&self, client: ClientId, outbound: &OutboundMailbox) {
        let Some(runtime) = self.open_agent_runtime() else {
            return;
        };
        let panes = {
            let inner = self.inner.lock();
            let Some(session) = client_attached_session(&inner, client) else {
                return;
            };
            session_agent_panes(&inner, session)
        };
        for pane in panes {
            let Some(state) = runtime.wire_state(pane) else {
                continue;
            };
            Self::send_event(outbound, EventPayload::AgentState { pane, state });
        }
    }

    pub(super) fn refresh_agent_visibility(&self) {
        let changes = {
            let mut inner = self.inner.lock();
            let attachments = inner
                .attachment
                .attached
                .iter()
                .flat_map(|(session, clients)| clients.iter().map(|client| (*session, *client)))
                .collect::<Vec<_>>();
            let mut changes = Vec::new();
            for (session, client) in attachments {
                let next = visible_agent_panes(&inner, client, session);
                let previous = inner
                    .client(client)
                    .and_then(|c| c.visible_agents.as_ref())
                    .cloned()
                    .unwrap_or_default();
                if next == previous {
                    continue;
                }
                let removed = previous
                    .difference(&next)
                    .filter(|pane| {
                        inner
                            .client(client)
                            .is_none_or(|c| !c.watched_agents.contains(pane))
                    })
                    .copied()
                    .collect::<Vec<_>>();
                let entered = next.difference(&previous).copied().collect::<Vec<_>>();
                if let Some(subscriber) = inner
                    .client(client)
                    .and_then(|c| c.subscriber.as_ref())
                    .cloned()
                {
                    changes.push((subscriber, removed, entered));
                }
                inner.client_entry(client).visible_agents.replace(next);
            }
            changes
        };
        if changes.is_empty() {
            return;
        }
        let runtime = self.open_agent_runtime();
        for (subscriber, removed, entered) in changes {
            for pane in removed {
                subscriber.cancel_agent(pane);
            }
            let Some(runtime) = runtime.as_ref() else {
                continue;
            };
            for pane in entered {
                let Some(state) = runtime.wire_state(pane) else {
                    continue;
                };
                Self::send_event(&subscriber, EventPayload::AgentState { pane, state });
            }
        }
    }
}

#[cfg(all(feature = "agent", unix))]
fn managed_login(provider: zz_protocol::AgentProvider) -> bool {
    crate::agent::logins::manages(provider)
}

#[cfg(all(feature = "agent", not(unix)))]
fn managed_login(_: zz_protocol::AgentProvider) -> bool {
    false
}

#[cfg(feature = "agent")]
fn session_agent_panes(inner: &ServerState, session: SessionId) -> Vec<PaneId> {
    inner
        .agents
        .agent_panes
        .iter()
        .copied()
        .filter(|pane| {
            inner
                .engine
                .state
                .window_for_pane(*pane)
                .and_then(|window| inner.engine.state.windows.get(&window))
                .is_some_and(|window| window.session == session)
        })
        .collect()
}

#[cfg(feature = "agent")]
const AGENT_CLI_EXIT_GRACE: Duration = Duration::from_secs(5);
