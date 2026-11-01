use super::*;
use zz_protocol::AgentProvider;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FlipWhenBusy {
    #[default]
    Refuse,
    Now,
    WhenIdle,
    Cancel,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct FlipAgentArgs {
    pub(super) target: Option<String>,
    pub(super) when_busy: FlipWhenBusy,
}

pub(super) fn parse_flip_agent_args(args: &[RawText]) -> Result<FlipAgentArgs, ServerError> {
    let spec = zz_protocol::catalog_command_spec("flip-agent").expect("registered command");
    let parsed = zz_protocol::parse_tmux_options(spec, args)?;
    if !parsed.positionals.is_empty() {
        return Err(ServerError::CommandParse(
            "flip-agent takes no positional arguments".to_owned(),
        ));
    }
    let mut flip = FlipAgentArgs::default();
    for option in parsed.options {
        let when_busy = match option {
            zz_protocol::TmuxOption::Value("-t", target) => {
                flip.target = Some(target.to_owned());
                continue;
            }
            zz_protocol::TmuxOption::Flag("--now") => FlipWhenBusy::Now,
            zz_protocol::TmuxOption::Flag("--when-idle") => FlipWhenBusy::WhenIdle,
            zz_protocol::TmuxOption::Flag("--cancel") => FlipWhenBusy::Cancel,
            zz_protocol::TmuxOption::Flag(name) | zz_protocol::TmuxOption::Value(name, _) => {
                return Err(ServerError::CommandParse(format!(
                    "flip-agent: unknown option {name}"
                )));
            }
        };
        if flip.when_busy != FlipWhenBusy::Refuse && flip.when_busy != when_busy {
            return Err(ServerError::CommandParse(
                "flip-agent takes at most one of --now, --when-idle, --cancel".to_owned(),
            ));
        }
        flip.when_busy = when_busy;
    }
    Ok(flip)
}

#[cfg(all(feature = "agent", unix))]
const ARMED_POLL: Duration = Duration::from_millis(250);

#[cfg(all(feature = "agent", unix))]
pub(super) enum FlipStep {
    ToChat {
        provider: AgentProvider,
        session: crate::agent::cli_session::CliSession,
    },
    ToCli {
        cwd: Option<PathBuf>,
        command: Vec<String>,
    },
}

#[cfg(all(feature = "agent", unix))]
pub(super) struct FlipPlan {
    pub(super) running: Vec<String>,
    pub(super) step: FlipStep,
}

#[cfg(all(feature = "agent", unix))]
impl FlipStep {
    pub(super) fn command(&self, pane: PaneId) -> CommandInvocation {
        let mut args = vec!["-t".to_owned(), pane.to_string()];
        match self {
            Self::ToChat { provider, session } => {
                if let Some(cwd) = &session.cwd {
                    args.extend(["-c".to_owned(), cwd.display().to_string()]);
                }
                for (option, value) in [
                    ("--resume", &session.session_id),
                    ("--model", &session.model),
                    ("--effort", &session.effort),
                ] {
                    if let Some(value) = value {
                        args.extend([option.to_owned(), value.clone()]);
                    }
                }
                args.extend(["agent".to_owned(), provider.as_str().to_owned()]);
            }
            Self::ToCli { cwd, command } => {
                if let Some(cwd) = cwd {
                    args.extend(["-c".to_owned(), cwd.display().to_string()]);
                }
                args.push("terminal".to_owned());
                args.extend(command.iter().cloned());
            }
        }
        CommandInvocation::new("select-pane-kind", args)
    }
}

pub(super) fn cli_provider(process: &[String]) -> Option<AgentProvider> {
    process.iter().find_map(|candidate| {
        let name = candidate.rsplit(['/', '\\']).next()?;
        let name = name.strip_suffix(".exe").unwrap_or(name);
        match name.to_ascii_lowercase().as_str() {
            "claude" => Some(AgentProvider::ClaudeCode),
            "codex" => Some(AgentProvider::Codex),
            _ => None,
        }
    })
}

impl Shared {
    pub(super) fn terminal_agent(&self, pane: PaneId) -> Option<AgentProvider> {
        let inner = self.inner.lock();
        let state = inner.engine.state.pane(pane)?;
        if !matches!(state.kind, PaneKind::Terminal) {
            return None;
        }
        cli_provider(&state.summary.process)
    }

    #[cfg(feature = "agent")]
    pub(super) fn recent_agent_terminal(&self, origin: PaneId) -> Option<PaneId> {
        let candidates = {
            let inner = self.inner.lock();
            let window =
                &inner.engine.state.windows[&inner.engine.state.window_for_pane(origin)?];
            std::iter::once(&window.active_pane)
                .chain(window.last_panes())
                .chain(window.pane_order())
                .copied()
                .filter(|pane| *pane != origin)
                .collect::<Vec<_>>()
        };
        candidates
            .into_iter()
            .find(|pane| self.terminal_agent(*pane).is_some())
    }

    #[cfg(all(test, feature = "agent"))]
    pub(super) fn open_test_agent(self: &Arc<Self>, pane: PaneId) -> bool {
        let Some(runtime) = self.agent_runtime() else {
            return false;
        };
        {
            let mut inner = self.inner.lock();
            if matches!(
                inner.engine.state.pane(pane).map(|pane| &pane.kind),
                Some(PaneKind::Terminal)
            ) && inner
                .engine
                .state
                .convert_pane(
                    pane,
                    PaneKind::Agent(zz_protocol::AgentDescriptor::default()),
                )
                .is_err()
            {
                return false;
            }
            inner.agent_panes.insert(pane);
        }
        self.refresh_agent_visibility();
        let spec = AgentPaneSpec {
            provider: AgentProvider::Codex,
            cwd: PathBuf::from("/"),
            resume_session: None,
            auto_approve: None,
            workspace: AgentWorkspaceEnvironment::default(),
            model: None,
            effort: None,
        };
        runtime.open(pane, spec)
    }
}

impl Shared {
    pub(super) fn flip_agent(
        self: &Arc<Self>,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let flip = parse_flip_agent_args(args)?;
        let pane = self.inner.lock().engine.resolve_pane(
            flip.target.as_deref(),
            context.window,
            context.pane,
        )?;
        #[cfg(not(all(feature = "agent", unix)))]
        {
            let _ = (flip, pane);
            Err(ServerError::InvalidCommand(
                "flip-agent needs zz built with agent support, on macOS or Linux".to_owned(),
            )
            .into())
        }
        #[cfg(all(feature = "agent", unix))]
        {
            if flip.when_busy == FlipWhenBusy::Cancel {
                self.disarm_flip(pane);
                return Ok(Execution::default());
            }
            let plan = self.flip_plan(pane)?;
            if let Some(busy) = crate::agent::cli_session::busy(&plan.running) {
                match flip.when_busy {
                    FlipWhenBusy::Refuse => return Err(ServerError::InvalidCommand(busy).into()),
                    FlipWhenBusy::WhenIdle => {
                        self.arm_flip(pane);
                        return Ok(Execution::default());
                    }
                    FlipWhenBusy::Now | FlipWhenBusy::Cancel => {}
                }
            }
            self.disarm_flip(pane);
            self.start_flip(pane, plan.step);
            Ok(Execution::default())
        }
    }
}

#[cfg(all(feature = "agent", unix))]
impl Shared {
    pub(super) fn flip_plan(&self, pane: PaneId) -> Result<FlipPlan, ServerError> {
        use crate::agent::cli_session;

        let (kind, pane_pid, cwd) = {
            let inner = self.inner.lock();
            let state = inner
                .engine
                .state
                .pane(pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            let cwd = inner
                .terminals
                .get(&pane)
                .and_then(|terminal| terminal_working_directory(terminal));
            (
                state.kind.clone(),
                inner
                    .engine
                    .pane_runtime_facts(pane)
                    .and_then(|runtime| runtime.pid),
                cwd,
            )
        };
        match kind {
            PaneKind::Agent(descriptor) => {
                let state = self
                    .open_agent_runtime()
                    .and_then(|runtime| runtime.wire_state(pane))
                    .unwrap_or_default();
                let (model, effort) = cli_session::chat_settings(&state);
                let model = model.or(descriptor.model);
                let effort = effort.or(descriptor.effort);
                let session = state.session_id.clone().or(descriptor.session_id);
                let running = if descriptor.provider == AgentProvider::Codex
                    && self.codex_chat_on_shared_server()
                {
                    Vec::new()
                } else {
                    cli_session::chat_running(&state)
                };
                Ok(FlipPlan {
                    running,
                    step: FlipStep::ToCli {
                        cwd: descriptor.cwd,
                        command: cli_session::interactive_shell(&cli_session::cli_command(
                            descriptor.provider,
                            session.as_deref(),
                            model.as_deref(),
                            effort.as_deref(),
                        )),
                    },
                })
            }
            PaneKind::Terminal => {
                let provider = self.terminal_agent(pane).ok_or_else(|| {
                    ServerError::InvalidTarget(format!("{pane} runs neither Claude Code nor Codex"))
                })?;
                let pane_pid = pane_pid.ok_or(ServerError::PaneExited(pane))?;
                let session = match provider {
                    AgentProvider::ClaudeCode => {
                        let config = crate::agent::claude::sessions::config_home();
                        crate::agent::claude_peers::registry_dir()
                            .ok()
                            .and_then(|registry| {
                                cli_session::claude_session(
                                    &registry,
                                    config.as_deref(),
                                    pane_pid,
                                    zz_daemon_client::process_info::parent,
                                    crate::pid_alive,
                                    cli_session::arguments,
                                )
                            })
                            .unwrap_or_else(|| {
                                let pid = cli_session::agent_pid(pane_pid, "claude");
                                let args = pid.map(cli_session::arguments).unwrap_or_default();
                                cli_session::claude_fresh(pid, config.as_deref(), &args, cwd)
                            })
                    }
                    AgentProvider::Codex => codex_cli_session(pane, pane_pid, cwd)?,
                };
                Ok(FlipPlan {
                    running: session.running.clone(),
                    step: FlipStep::ToChat { provider, session },
                })
            }
            _ => Err(ServerError::InvalidTarget(format!(
                "{pane} is neither a chat nor a terminal running Claude Code or Codex"
            ))),
        }
    }

    fn codex_chat_on_shared_server(&self) -> bool {
        crate::agent::codex::CodexCommand::parse(
            self.agent_spawn_config().command_for(AgentProvider::Codex),
        )
        .is_some_and(|command| crate::agent::codex::uses_managed_daemon(&command))
            && crate::agent::codex::managed_socket().is_some()
    }

    fn start_flip(self: &Arc<Self>, pane: PaneId, step: FlipStep) {
        let shared = Arc::downgrade(&self.server_owner());
        let spawned = std::thread::Builder::new()
            .name("zz-agent-flip".to_owned())
            .spawn(move || {
                if let Some(shared) = shared.upgrade() {
                    shared.run_flip(pane, &step);
                }
            });
        if let Err(error) = spawned {
            log::warn!(target: "zz::agent", "could not flip {pane}: {error}");
        }
    }

    pub(super) fn run_flip(self: &Arc<Self>, pane: PaneId, step: &FlipStep) {
        if matches!(step, FlipStep::ToCli { .. })
            && let Some(runtime) = self.open_agent_runtime()
        {
            runtime.close_and_wait(pane);
        }
        if let FlipStep::ToChat { session, .. } = step
            && let Some(pid) = session.pid
        {
            self.inner.lock().cli_exits.insert(pane, pid);
        }
        let Some(mut context) = ExecutionContext::for_pane(&self.inner.lock().engine.state, pane)
        else {
            return;
        };
        context.no_hooks = true;
        if let Err(error) = self.execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &step.command(pane),
        ) {
            log::warn!(target: "zz::agent", "could not flip {pane}: {error}");
        }
    }

    pub(super) fn arm_flip(self: &Arc<Self>, pane: PaneId) {
        let first = {
            let mut inner = self.inner.lock();
            let first = inner.armed_flips.insert(pane);
            inner
                .engine
                .state
                .update_pane_summary(pane, |summary| summary.flip_armed = true);
            first
        };
        self.publish_snapshot_state();
        if !first {
            return;
        }
        let shared = Arc::downgrade(&self.server_owner());
        let spawned = std::thread::Builder::new()
            .name("zz-agent-flip-armed".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(ARMED_POLL);
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    if !shared.inner.lock().armed_flips.contains(&pane) {
                        return;
                    }
                    match shared.flip_plan(pane) {
                        Ok(plan) if plan.running.is_empty() => {
                            shared.disarm_flip(pane);
                            shared.run_flip(pane, &plan.step);
                            return;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            log::info!(target: "zz::agent", "dropping the armed flip of {pane}: {error}");
                            shared.disarm_flip(pane);
                            return;
                        }
                    }
                }
            });
        if let Err(error) = spawned {
            log::warn!(target: "zz::agent", "could not wait to flip {pane}: {error}");
            self.disarm_flip(pane);
        }
    }

    pub(super) fn disarm_flip(&self, pane: PaneId) {
        let changed = {
            let mut inner = self.inner.lock();
            let armed = inner.armed_flips.remove(&pane);
            let cleared = inner
                .engine
                .state
                .update_pane_summary(pane, |summary| summary.flip_armed = false);
            armed || cleared
        };
        if changed {
            self.publish_snapshot_state();
        }
    }
}

#[cfg(all(feature = "agent", unix))]
fn codex_cli_session(
    pane: PaneId,
    pane_pid: u32,
    cwd: Option<PathBuf>,
) -> Result<crate::agent::cli_session::CliSession, ServerError> {
    use crate::agent::cli_session;

    let pid = cli_session::agent_pid(pane_pid, "codex")
        .ok_or_else(|| ServerError::InvalidTarget(format!("{pane} runs no codex process")))?;
    let args = cli_session::arguments(pid);
    let cwd = zz_daemon_client::process_info::working_directory(pid).or(cwd);
    let sessions = cli_session::codex_home().map(|home| home.join("sessions"));
    let held = cli_session::open_rollout(pid);
    let embedded = held.is_some() || cli_session::codex_embedded(&args);
    let rollout = held.or_else(|| {
        let sessions = sessions.as_deref()?;
        let cwd = cwd.as_deref()?;
        let since = std::time::SystemTime::now()
            .checked_sub(zz_daemon_client::process_info::running_for(pid)?)?;
        let mut found = cli_session::rollouts_since(sessions, cwd, since);
        (found.len() == 1).then(|| found.remove(0))
    });
    let mut session = cli_session::codex_session(
        sessions.as_deref(),
        cli_session::codex_home().as_deref(),
        &args,
        cwd,
        rollout,
    );
    session.pid = Some(pid);
    if !embedded {
        session.running.clear();
    }
    if session.session_id.is_none()
        && let (Some(sessions), Some(cwd)) = (sessions.as_deref(), session.cwd.as_deref())
        && let Some(since) = zz_daemon_client::process_info::running_for(pid)
            .and_then(|running| std::time::SystemTime::now().checked_sub(running))
        && cli_session::rollouts_since(sessions, cwd, since).len() > 1
    {
        return Err(ServerError::InvalidTarget(format!(
            "{pane}: several Codex threads in {} started after this TUI; resume the one you want with codex resume <id>, then flip",
            cwd.display()
        )));
    }
    Ok(session)
}

#[cfg(all(test, feature = "agent", unix))]
#[path = "agent_flip_tests.rs"]
mod flow_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<FlipAgentArgs, ServerError> {
        let args = args
            .iter()
            .map(|arg| RawText::from(*arg))
            .collect::<Vec<_>>();
        parse_flip_agent_args(&args)
    }

    #[test]
    fn a_terminal_running_claude_or_codex_can_flip() {
        let process = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            cli_provider(&process(&["/home/me/.local/bin/claude", "node"])),
            Some(AgentProvider::ClaudeCode)
        );
        assert_eq!(
            cli_provider(&process(&["node", "/opt/homebrew/bin/codex"])),
            Some(AgentProvider::Codex)
        );
        assert_eq!(
            cli_provider(&process(&["C:\\tools\\Codex.exe"])),
            Some(AgentProvider::Codex)
        );
        assert_eq!(cli_provider(&process(&["/bin/zsh", "vim"])), None);
    }

    #[test]
    fn flip_agent_parses_target_and_one_busy_mode() {
        assert_eq!(parse(&[]).unwrap(), FlipAgentArgs::default());
        assert_eq!(
            parse(&["-t", "%3", "--when-idle"]).unwrap(),
            FlipAgentArgs {
                target: Some("%3".to_owned()),
                when_busy: FlipWhenBusy::WhenIdle,
            }
        );
        assert_eq!(parse(&["--now"]).unwrap().when_busy, FlipWhenBusy::Now);
        assert_eq!(
            parse(&["--cancel"]).unwrap().when_busy,
            FlipWhenBusy::Cancel
        );
        assert_eq!(
            parse(&["--now", "--now"]).unwrap().when_busy,
            FlipWhenBusy::Now
        );
        for args in [
            &["--now", "--cancel"][..],
            &["--when-idle", "--now"],
            &["claude"],
            &["--bogus"],
            &["-t"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
    }
}
