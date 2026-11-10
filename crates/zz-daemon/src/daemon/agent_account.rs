use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct AgentAccountArgs {
    pub(super) target: Option<String>,
    pub(super) account: Option<String>,
}

pub(super) fn parse_agent_account_args(args: &[RawText]) -> Result<AgentAccountArgs, ServerError> {
    let spec = zz_protocol::catalog_command_spec("agent-account").expect("registered command");
    let parsed = zz_protocol::parse_tmux_options(spec, args)?;
    let mut target = None;
    for option in parsed.options {
        match option {
            zz_protocol::TmuxOption::Value("-t", value) => target = Some(value.to_owned()),
            zz_protocol::TmuxOption::Flag(name) | zz_protocol::TmuxOption::Value(name, _) => {
                return Err(ServerError::CommandParse(format!(
                    "agent-account: unknown option {name}"
                )));
            }
        }
    }
    let [account] = parsed.positionals else {
        return Err(ServerError::CommandParse(
            "usage: agent-account [-t pane] account|default".to_owned(),
        ));
    };
    let account = match account.as_str() {
        "default" => None,
        id if zz_protocol::valid_agent_account(id) => Some(id.to_owned()),
        _ => {
            return Err(ServerError::CommandParse(
                "agent-account takes an account id (a-z, 0-9, - and _, starting with a letter or digit, at most 32 bytes) or default"
                    .to_owned(),
            ));
        }
    };
    Ok(AgentAccountArgs { target, account })
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
            use crate::agent::{accounts, cli_session};

            let kind = self
                .inner
                .lock()
                .engine
                .state
                .pane(pane)
                .map(|state| state.kind.clone());
            let descriptor = match kind {
                Some(PaneKind::Agent(descriptor)) => descriptor,
                Some(_) => {
                    return Err(
                        ServerError::InvalidTarget(format!("{pane} is not an agent chat")).into(),
                    );
                }
                None => return Err(ServerError::MissingTarget(pane.to_string()).into()),
            };
            if descriptor.account == args.account {
                return Ok(Execution::default());
            }
            if let Some(id) = args.account.as_deref() {
                let provider = descriptor.provider;
                accounts::existing(provider, &accounts::Account::resolve(provider, id))
                    .map_err(ServerError::InvalidCommand)?;
            }
            let state = self
                .open_agent_runtime()
                .and_then(|runtime| runtime.wire_state(pane))
                .unwrap_or_default();
            if let Some(busy) = cli_session::busy(&cli_session::chat_running(&state)) {
                return Err(ServerError::InvalidCommand(busy).into());
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
