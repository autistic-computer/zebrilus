use super::agent_flip::FlipWhenBusy;
use super::*;

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
            use crate::agent::accounts;

            let descriptor = self.agent_descriptor(pane)?;
            self.inner.lock().armed_accounts.remove(&pane);
            if descriptor.account == args.account {
                return Ok(Execution::default());
            }
            if let Some(id) = args.account.as_deref() {
                let provider = descriptor.provider;
                accounts::existing(provider, &accounts::Account::resolve(provider, id))
                    .map_err(ServerError::InvalidCommand)?;
            }
            if let Some(busy) = self.agent_busy(pane) {
                match args.when_busy {
                    FlipWhenBusy::Now => {}
                    FlipWhenBusy::WhenIdle => {
                        self.arm_account(pane, args.account);
                        return Ok(Execution::default());
                    }
                    FlipWhenBusy::Refuse | FlipWhenBusy::Cancel => {
                        return Err(ServerError::InvalidCommand(busy).into());
                    }
                }
            }
            let shared = Arc::downgrade(&self.server_owner());
            let account = args.account;
            let spawned = std::thread::Builder::new()
                .name("zz-agent-account".to_owned())
                .spawn(move || {
                    if let Some(shared) = shared.upgrade() {
                        shared.switch_agent_account(pane, account);
                    }
                });
            if let Err(error) = spawned {
                return Err(ServerError::Internal(format!(
                    "could not move {pane} to another account: {error}"
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
    fn agent_busy(&self, pane: PaneId) -> Option<String> {
        use crate::agent::cli_session;

        let state = self
            .open_agent_runtime()
            .and_then(|runtime| runtime.wire_state(pane))
            .unwrap_or_default();
        cli_session::busy(&cli_session::chat_running(&state))
    }

    #[cfg(all(feature = "agent", unix))]
    fn arm_account(self: &Arc<Self>, pane: PaneId, account: Option<String>) {
        self.inner
            .lock()
            .armed_accounts
            .insert(pane, account.clone());
        let shared = Arc::downgrade(&self.server_owner());
        let spawned = std::thread::Builder::new()
            .name("zz-agent-account-armed".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(super::agent_flip::ARMED_POLL);
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    if shared.inner.lock().armed_accounts.get(&pane) != Some(&account) {
                        return;
                    }
                    if shared.agent_descriptor(pane).is_err() {
                        shared.inner.lock().armed_accounts.remove(&pane);
                        return;
                    }
                    if shared.agent_busy(pane).is_none() {
                        shared.inner.lock().armed_accounts.remove(&pane);
                        shared.switch_agent_account(pane, account);
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            log::warn!(target: "zz::agent", "could not wait to move {pane} to another account: {error}");
            self.inner.lock().armed_accounts.remove(&pane);
        }
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn switch_agent_account(self: &Arc<Self>, pane: PaneId, account: Option<String>) {
        if let Some(runtime) = self.open_agent_runtime() {
            runtime.close_for_reopen(pane);
        }
        let changed = self
            .inner
            .lock()
            .engine
            .state
            .update_agent(pane, |agent| agent.account = account);
        match changed {
            Ok(true) => self.publish_snapshot_state(),
            Ok(false) => {}
            Err(error) => {
                log::warn!(target: "zz::agent", "could not move {pane} to another account: {error}");
                return;
            }
        }
        self.open_agent_pane(pane, None);
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
