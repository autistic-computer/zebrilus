use super::agent_flip::FlipWhenBusy;
use super::*;
#[cfg(all(feature = "agent", unix))]
use zz_protocol::AgentProvider;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct AgentAccountArgs {
    pub(super) target: Option<String>,
    pub(super) account: Option<String>,
    pub(super) when_busy: FlipWhenBusy,
}

pub(super) fn parse_agent_account_args(args: &[RawText]) -> Result<AgentAccountArgs, ServerError> {
    let spec = zz_protocol::catalog_command_spec("agent-account").expect("registered command");
    let parsed = zz_protocol::parse_tmux_options(spec, args)?;
    let mut target = None;
    let mut when_busy = FlipWhenBusy::Refuse;
    for option in parsed.options {
        let mode = match option {
            zz_protocol::TmuxOption::Value("-t", value) => {
                target = Some(value.to_owned());
                continue;
            }
            zz_protocol::TmuxOption::Flag("--now") => FlipWhenBusy::Now,
            zz_protocol::TmuxOption::Flag("--when-idle") => FlipWhenBusy::WhenIdle,
            zz_protocol::TmuxOption::Flag(name) | zz_protocol::TmuxOption::Value(name, _) => {
                return Err(ServerError::CommandParse(format!(
                    "agent-account: unknown option {name}"
                )));
            }
        };
        if when_busy != FlipWhenBusy::Refuse && when_busy != mode {
            return Err(ServerError::CommandParse(
                "agent-account takes at most one of --now, --when-idle".to_owned(),
            ));
        }
        when_busy = mode;
    }
    let [account] = parsed.positionals else {
        return Err(ServerError::CommandParse(
            "usage: agent-account [-t pane] [--now | --when-idle] account|default".to_owned(),
        ));
    };
    let account = match account.as_str() {
        zz_protocol::DEFAULT_AGENT_ACCOUNT => None,
        id if zz_protocol::valid_agent_account(id) => Some(id.to_owned()),
        _ => {
            return Err(ServerError::CommandParse(
                "agent-account takes an account id (a-z, 0-9, - and _, starting with a letter or digit, at most 32 bytes) or default"
                    .to_owned(),
            ));
        }
    };
    Ok(AgentAccountArgs {
        target,
        account,
        when_busy,
    })
}

impl Shared {
    pub(super) fn agent_account(
        self: &Arc<Self>,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let args = parse_agent_account_args(args)?;
        let pane = self.inner.lock().engine.resolve_pane(
            args.target.as_deref(),
            context.window,
            context.pane,
        )?;
        #[cfg(not(all(feature = "agent", unix)))]
        {
            let _ = (args, pane);
            Err(ServerError::InvalidCommand(
                "agent-account needs zz built with agent support, on macOS or Linux".to_owned(),
            )
            .into())
        }
        #[cfg(all(feature = "agent", unix))]
        {
            use crate::agent::logins;

            let provider = self.agent_descriptor(pane)?.provider;
            let Some(target) = args.account else {
                return Ok(Execution::default());
            };
            let label = provider.label();
            if !logins::swaps(provider) {
                return Err(ServerError::InvalidCommand(format!(
                    "agent-account: {label} keeps one account"
                ))
                .into());
            }
            self.inner.lock().agents.armed_accounts.remove(&provider);
            let record = logins::load(provider).unwrap_or_default();
            if record.active() == Some(target.as_str()) {
                if !self
                    .align_agent_accounts(provider, record.active())
                    .is_empty()
                {
                    self.publish_snapshot_state();
                }
                return Ok(Execution::default());
            }
            if record.slot(&target).is_none() {
                return Err(ServerError::InvalidCommand(format!(
                    "agent-account: {label} has no account {target}"
                ))
                .into());
            }
            if let Some(busy) = self.provider_busy(provider) {
                match args.when_busy {
                    FlipWhenBusy::Now => {}
                    FlipWhenBusy::WhenIdle => {
                        self.arm_account(provider, target);
                        return Ok(Execution::default());
                    }
                    FlipWhenBusy::Refuse | FlipWhenBusy::Cancel => {
                        return Err(ServerError::InvalidCommand(busy).into());
                    }
                }
            }
            let shared = Arc::downgrade(&self.server_owner());
            let spawned = std::thread::Builder::new()
                .name("zz-agent-account".to_owned())
                .spawn(move || {
                    if let Some(shared) = shared.upgrade() {
                        shared.switch_agent_login(provider, &target);
                    }
                });
            if let Err(error) = spawned {
                return Err(ServerError::Internal(format!(
                    "could not switch the {label} account: {error}"
                ))
                .into());
            }
            Ok(Execution::default())
        }
    }

    #[cfg(all(feature = "agent", unix))]
    fn agent_descriptor(&self, pane: PaneId) -> Result<zz_protocol::AgentDescriptor, ServerError> {
        let kind = self
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .map(|state| state.kind.clone());
        match kind {
            Some(PaneKind::Agent(descriptor)) => Ok(descriptor),
            Some(_) => Err(ServerError::InvalidTarget(format!(
                "{pane} is not an agent chat"
            ))),
            None => Err(ServerError::MissingTarget(pane.to_string())),
        }
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn provider_chats(&self, provider: AgentProvider) -> Vec<PaneId> {
        self.inner
            .lock()
            .engine
            .state
            .windows
            .values()
            .flat_map(|window| window.panes.iter())
            .filter(|(_, state)| {
                matches!(&state.kind, PaneKind::Agent(descriptor) if descriptor.provider == provider)
            })
            .map(|(pane, _)| *pane)
            .collect()
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn provider_busy(&self, provider: AgentProvider) -> Option<String> {
        use crate::agent::cli_session;

        let runtime = self.open_agent_runtime()?;
        self.provider_chats(provider).into_iter().find_map(|pane| {
            let state = runtime.wire_state(pane).unwrap_or_default();
            cli_session::busy(&cli_session::chat_running(&state))
        })
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn align_agent_accounts(
        &self,
        provider: AgentProvider,
        active: Option<&str>,
    ) -> Vec<PaneId> {
        let Some(active) = active else {
            return Vec::new();
        };
        let chats = self.provider_chats(provider);
        let mut inner = self.inner.lock();
        chats
            .into_iter()
            .filter(|pane| {
                inner
                    .engine
                    .state
                    .update_agent(*pane, |agent| agent.account = Some(active.to_owned()))
                    .unwrap_or(false)
            })
            .collect()
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn relaunch_agent_chats(
        self: &Arc<Self>,
        provider: AgentProvider,
        change: impl FnOnce() -> Option<String>,
    ) {
        let chats = self.provider_chats(provider);
        let open = {
            let inner = self.inner.lock();
            chats
                .into_iter()
                .filter(|pane| inner.agents.agent_panes.contains(pane))
                .collect::<Vec<_>>()
        };
        if let Some(runtime) = self.open_agent_runtime() {
            for pane in &open {
                runtime.close_and_wait(*pane);
            }
        }
        let active = change();
        if provider == AgentProvider::Codex
            && let Some(program) = providers::locate(provider, self.provider_search().as_ref())
        {
            crate::agent::codex::restart_managed_daemon(&program);
        }
        let moved = self.align_agent_accounts(provider, active.as_deref());
        if !moved.is_empty() {
            self.publish_snapshot_state();
            for pane in moved {
                self.note_child_descriptor(pane);
            }
        }
        for pane in open {
            self.open_agent_pane(pane, None);
        }
    }

    #[cfg(all(feature = "agent", unix))]
    fn arm_account(self: &Arc<Self>, provider: AgentProvider, target: String) {
        self.inner
            .lock()
            .agents
            .armed_accounts
            .insert(provider, target.clone());
        let shared = Arc::downgrade(&self.server_owner());
        let spawned = std::thread::Builder::new()
            .name("zz-agent-account-armed".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(super::agent_flip::ARMED_POLL);
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    if shared.inner.lock().agents.armed_accounts.get(&provider) != Some(&target) {
                        return;
                    }
                    if shared.provider_busy(provider).is_none() {
                        shared.inner.lock().agents.armed_accounts.remove(&provider);
                        shared.switch_agent_login(provider, &target);
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            log::warn!(target: "zz::agent", "could not wait to switch the {} account: {error}", provider.label());
            self.inner.lock().agents.armed_accounts.remove(&provider);
        }
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn switch_agent_login(self: &Arc<Self>, provider: AgentProvider, target: &str) {
        use crate::agent::logins;

        let search = self.provider_search();
        if let Some(program) = providers::locate(provider, search.as_ref()) {
            let native = super::accounts::identify(provider, &program, search.as_ref(), None)
                .map(|(probe, _)| probe);
            super::accounts::settle(provider, &program, search.as_ref(), native.as_ref());
        }
        self.relaunch_agent_chats(provider, || {
            let _guard = logins::guard();
            let mut record = logins::load(provider).unwrap_or_default();
            match logins::swap(&logins::System, provider, &mut record, target) {
                Ok(()) => {
                    if let Err(error) = logins::store(provider, &record) {
                        log::warn!(target: "zz::agent", "could not record the {} accounts: {error}", provider.label());
                    }
                }
                Err(error) => log::warn!(target: "zz::agent", "{error}"),
            }
            record.active
        });
        self.refresh_agent_accounts();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<AgentAccountArgs, ServerError> {
        let args = args
            .iter()
            .map(|arg| RawText::from(*arg))
            .collect::<Vec<_>>();
        parse_agent_account_args(&args)
    }

    #[test]
    fn an_account_switch_waits_for_idle_or_goes_now() {
        assert_eq!(
            parse(&["-t", "%3", "--when-idle", "work"]).unwrap(),
            AgentAccountArgs {
                target: Some("%3".to_owned()),
                account: Some("work".to_owned()),
                when_busy: FlipWhenBusy::WhenIdle,
            }
        );
        assert_eq!(
            parse(&["--now", "default"]).unwrap().when_busy,
            FlipWhenBusy::Now
        );
        assert_eq!(parse(&["default"]).unwrap().when_busy, FlipWhenBusy::Refuse);
        assert!(parse(&["--now", "--when-idle", "work"]).is_err());
        assert!(parse(&["--cancel", "work"]).is_err());
    }
}
