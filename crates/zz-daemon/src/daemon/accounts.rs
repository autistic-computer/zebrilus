use super::*;
#[cfg(all(feature = "agent", unix))]
use std::{
    ffi::OsString,
    io::{BufRead as _, BufReader},
    process::{Child, Command, Stdio},
};

#[cfg(all(feature = "agent", unix))]
use zz_daemon_client::unmasked::SpawnUnmasked as _;
use zz_protocol::AgentProvider;
#[cfg(feature = "agent")]
use zz_protocol::QuotaWindows;
#[cfg(all(feature = "agent", unix))]
use zz_protocol::{AgentAccount, AgentQuota};

#[cfg(all(feature = "agent", unix))]
const PROVIDERS: [AgentProvider; 5] = [
    AgentProvider::ClaudeCode,
    AgentProvider::Codex,
    AgentProvider::Antigravity,
    AgentProvider::Grok,
    AgentProvider::Cursor,
];
#[cfg(all(feature = "agent", unix))]
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(all(feature = "agent", unix))]
const MAX_STATUS_BYTES: usize = 64 * 1024;
#[cfg(all(feature = "agent", unix))]
const RPC_REPLY: Duration = Duration::from_secs(5);
#[cfg(all(feature = "agent", unix))]
const RPC_EXIT: Duration = Duration::from_secs(2);
#[cfg(all(feature = "agent", unix))]
const ACP_AUTH_REQUIRED: i64 = -32000;

#[cfg(all(feature = "agent", unix))]
#[derive(Default)]
pub(super) struct Accounts {
    pub(super) list: Option<Vec<AgentAccount>>,
    quotas: BTreeMap<(AgentProvider, Option<String>), QuotaWindows>,
    watchers: BTreeSet<ClientId>,
    refreshing: bool,
    again: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AccountChoice {
    New,
    Default,
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AddAccount {
    pub(super) provider: AgentProvider,
    pub(super) account: AccountChoice,
    pub(super) pane: Option<String>,
    pub(super) browser: zz_protocol::AgentLoginBrowser,
    pub(super) terminal: bool,
}

pub(super) fn parse_agent_accounts_args(
    args: &[RawText],
) -> Result<Option<AddAccount>, ServerError> {
    let spec = zz_protocol::catalog_command_spec("agent-accounts").expect("registered command");
    let parsed = zz_protocol::parse_tmux_options(spec, args)?;
    spec.validate_positional_maximum(parsed.positionals.len())?;
    let mut provider = None;
    let mut account = AccountChoice::New;
    let mut pane = None;
    let mut browser = None;
    let mut terminal = false;
    for option in parsed.options {
        match option {
            zz_protocol::TmuxOption::Value("--add", value) => {
                provider = Some(
                    value
                        .parse::<AgentProvider>()
                        .map_err(ServerError::CommandParse)?,
                );
            }
            zz_protocol::TmuxOption::Value("--account", zz_protocol::DEFAULT_AGENT_ACCOUNT) => {
                account = AccountChoice::Default;
            }
            zz_protocol::TmuxOption::Value("--account", id)
                if zz_protocol::valid_agent_account(id) =>
            {
                account = AccountChoice::Named(id.to_owned());
            }
            zz_protocol::TmuxOption::Value("--account", id) => {
                return Err(ServerError::CommandParse(format!(
                    "agent-accounts: `{id}` is not an account id or default"
                )));
            }
            zz_protocol::TmuxOption::Value("--pane", value) => pane = Some(value.to_owned()),
            zz_protocol::TmuxOption::Value("--browser", value) => {
                browser = Some(value.parse().map_err(ServerError::CommandParse)?);
            }
            zz_protocol::TmuxOption::Flag("--terminal") => terminal = true,
            zz_protocol::TmuxOption::Flag(name) | zz_protocol::TmuxOption::Value(name, _) => {
                return Err(ServerError::CommandParse(format!(
                    "agent-accounts: unknown option {name}"
                )));
            }
        }
    }
    let Some(provider) = provider else {
        if account != AccountChoice::New || pane.is_some() || browser.is_some() || terminal {
            return Err(ServerError::CommandParse(
                "agent-accounts: --account, --pane, --browser and --terminal go with --add"
                    .to_owned(),
            ));
        }
        return Ok(None);
    };
    if !provider.accounts() && matches!(account, AccountChoice::Named(_)) {
        return Err(ServerError::InvalidCommand(format!(
            "agent-accounts: {} keeps one account",
            provider.label()
        )));
    }
    Ok(Some(AddAccount {
        provider,
        account,
        pane,
        browser: browser.unwrap_or_default(),
        terminal,
    }))
}

impl Shared {
    pub(super) fn agent_accounts(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let add = parse_agent_accounts_args(args)?;
        #[cfg(not(all(feature = "agent", unix)))]
        {
            let _ = (client, context, add);
            Err(ServerError::InvalidCommand(
                "agent-accounts needs zz built with agent support, on macOS or Linux".to_owned(),
            )
            .into())
        }
        #[cfg(all(feature = "agent", unix))]
        {
            self.agent_accounts.lock().watchers.insert(client);
            self.publish_agent_accounts();
            self.publish_agent_quotas();
            let Some(add) = add else {
                self.publish_agent_logins_to(client);
                self.refresh_agent_accounts();
                return Ok(Execution::default());
            };
            if !add.terminal && self.client_draws_agent_logins(client) {
                let pane = add
                    .pane
                    .as_deref()
                    .map(|target| {
                        self.inner.lock().engine.resolve_pane(
                            Some(target),
                            context.window,
                            context.pane,
                        )
                    })
                    .transpose()?;
                return self
                    .start_agent_login(client, add, pane)
                    .map(|()| Execution::default())
                    .map_err(DaemonError::from);
            }
            let provider = add.provider;
            let shared = Arc::downgrade(&self.server_owner());
            let context = ExecutionContext::new(context.session, context.window, context.pane);
            let spawned = thread::Builder::new()
                .name("zz-agent-accounts-add".to_owned())
                .spawn(move || {
                    if let Some(shared) = shared.upgrade() {
                        shared.add_agent_account(provider, &add.account, context);
                    }
                });
            if let Err(error) = spawned {
                return Err(ServerError::Internal(format!(
                    "could not add a {} account: {error}",
                    provider.label()
                ))
                .into());
            }
            Ok(Execution::default())
        }
    }
}

impl Shared {
    #[cfg_attr(
        not(all(feature = "agent", unix)),
        expect(
            clippy::needless_pass_by_value,
            reason = "only zz with agent support on macOS or Linux signs in agent CLIs"
        )
    )]
    pub(super) fn handle_agent_login(&self, request: zz_protocol::AgentLoginRequest) {
        #[cfg(all(feature = "agent", unix))]
        self.agent_login_request(request);
        #[cfg(not(all(feature = "agent", unix)))]
        let _ = request;
    }
}

#[cfg(feature = "agent")]
impl Shared {
    pub(super) fn apply_agent_quota(&self, pane: PaneId, quota: QuotaWindows) {
        #[cfg(not(unix))]
        {
            let _ = (pane, quota);
        }
        #[cfg(unix)]
        {
            let account = match self
                .inner
                .lock()
                .engine
                .state
                .pane(pane)
                .map(|state| &state.kind)
            {
                Some(PaneKind::Agent(descriptor)) => {
                    (descriptor.provider, descriptor.account.clone())
                }
                _ => return,
            };
            let changed = self
                .agent_accounts
                .lock()
                .quotas
                .entry(account)
                .or_default()
                .merge(quota);
            if changed {
                self.publish_agent_quotas();
            }
        }
    }
}

#[cfg(all(feature = "agent", unix))]
impl Shared {
    fn publish_agent_quotas(&self) {
        let (quotas, watchers) = {
            let accounts = self.agent_accounts.lock();
            let quotas = accounts
                .quotas
                .iter()
                .filter(|(_, windows)| !windows.is_empty())
                .map(|((provider, account), windows)| AgentQuota {
                    provider: *provider,
                    account: account.clone(),
                    windows: *windows,
                })
                .collect::<Vec<_>>();
            if quotas.is_empty() {
                return;
            }
            (quotas, accounts.watchers.clone())
        };
        for client in watchers {
            self.publish_to_client(client, EventPayload::AgentQuotas(quotas.clone()));
        }
    }

    fn publish_agent_accounts(&self) {
        let (list, watchers) = {
            let mut accounts = self.agent_accounts.lock();
            accounts
                .watchers
                .retain(|client| self.read_client(*client, |client| client.is_some()));
            let Some(list) = accounts.list.clone() else {
                return;
            };
            (list, accounts.watchers.clone())
        };
        for client in watchers {
            self.publish_to_client(client, EventPayload::AgentAccounts(list.clone()));
        }
    }

    fn refresh_agent_accounts(self: &Arc<Self>) {
        {
            let mut accounts = self.agent_accounts.lock();
            if accounts.refreshing {
                accounts.again = true;
                return;
            }
            accounts.refreshing = true;
        }
        let shared = Arc::downgrade(&self.server_owner());
        let spawned = thread::Builder::new()
            .name("zz-agent-accounts".to_owned())
            .spawn(move || {
                loop {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    if !shared.refresh_agent_accounts_once() {
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            log::warn!(target: "zz::agent", "could not list the agent accounts: {error}");
            self.agent_accounts.lock().refreshing = false;
        }
    }

    fn refresh_agent_accounts_once(&self) -> bool {
        let search = self.provider_search();
        let located = PROVIDERS
            .into_iter()
            .filter_map(|provider| {
                providers::locate(provider, search.as_ref()).map(|program| (provider, program))
            })
            .collect::<Vec<_>>();
        let probed = thread::scope(|scope| {
            located
                .into_iter()
                .map(|(provider, program)| {
                    let search = search.as_ref();
                    (
                        provider,
                        scope.spawn(move || provider_accounts(provider, &program, search)),
                    )
                })
                .collect::<Vec<_>>()
                .into_iter()
                .flat_map(|(provider, handle)| {
                    handle
                        .join()
                        .unwrap_or_default()
                        .into_iter()
                        .map(move |(id, probe)| {
                            probe.unwrap_or_else(|| {
                                (
                                    AgentAccount {
                                        id,
                                        ..signed_out(provider)
                                    },
                                    None,
                                )
                            })
                        })
                })
                .filter(|(account, _)| account.id.is_none() || account.signed_in)
                .collect::<Vec<_>>()
        });
        let (list, quotas): (Vec<_>, Vec<_>) = probed.into_iter().unzip();
        let (changed, quotas_changed, again) = {
            let mut accounts = self.agent_accounts.lock();
            let changed = accounts.list.as_ref() != Some(&list);
            let mut quotas_changed = false;
            for (account, quota) in list.iter().zip(quotas) {
                let Some(quota) = quota else {
                    continue;
                };
                let key = (account.provider, account.id.clone());
                quotas_changed |= accounts.quotas.insert(key, quota) != Some(quota);
            }
            accounts.list = Some(list);
            let again = std::mem::take(&mut accounts.again);
            accounts.refreshing = again;
            (changed, quotas_changed, again)
        };
        if changed {
            self.publish_agent_accounts();
        }
        if quotas_changed {
            self.publish_agent_quotas();
        }
        again
    }

    pub(super) fn record_agent_account(
        self: &Arc<Self>,
        account: AgentAccount,
        quota: Option<QuotaWindows>,
    ) {
        let quota_changed = {
            let mut accounts = self.agent_accounts.lock();
            let key = (account.provider, account.id.clone());
            let quota_changed =
                quota.is_some_and(|quota| accounts.quotas.insert(key, quota) != Some(quota));
            let Some(list) = accounts.list.as_mut() else {
                drop(accounts);
                if quota_changed {
                    self.publish_agent_quotas();
                }
                self.refresh_agent_accounts();
                return;
            };
            let slot = list
                .iter()
                .position(|known| known.provider == account.provider && known.id == account.id);
            match (slot, account.id.is_none() || account.signed_in) {
                (Some(slot), true) => list[slot] = account,
                (Some(slot), false) => {
                    list.remove(slot);
                }
                (None, true) => list.push(account),
                (None, false) => {}
            }
            quota_changed
        };
        self.publish_agent_accounts();
        if quota_changed {
            self.publish_agent_quotas();
        }
    }

    fn add_agent_account(
        self: &Arc<Self>,
        provider: AgentProvider,
        choice: &AccountChoice,
        mut context: ExecutionContext,
    ) {
        use crate::agent::cli_session;

        let search = self.provider_search();
        let Some(program) = providers::locate(provider, search.as_ref()) else {
            log::warn!(
                target: "zz::agent",
                "cannot add a {} account: {} is not installed",
                provider.label(),
                providers::program(provider)
            );
            return;
        };
        let (id, home) = match pick_account(provider, &program, search.as_ref(), choice) {
            Ok(picked) => (picked.id, picked.home),
            Err(error) => {
                log::warn!(target: "zz::agent", "{error}");
                return;
            }
        };
        let login: &[&str] = match provider {
            AgentProvider::ClaudeCode => &["claude", "auth", "login"],
            AgentProvider::Codex => &["codex", "login"],
            AgentProvider::Antigravity => &["agy", "acp", "--login"],
            AgentProvider::Grok => &["grok", "login"],
            AgentProvider::Cursor => &["cursor-agent", "login"],
        };
        let login = login
            .iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>();
        let name = id.as_deref().unwrap_or(zz_protocol::DEFAULT_AGENT_ACCOUNT);
        let mut args = vec![
            "-n".to_owned(),
            format!("{} {name}", provider.label()),
            "--".to_owned(),
        ];
        args.extend(cli_session::interactive_shell(
            home.as_deref().map(|home| (provider, home)),
            &login,
        ));
        context.no_hooks = true;
        if let Err(error) = self.execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-window", args),
        ) {
            log::warn!(
                target: "zz::agent",
                "could not open the login of the {} account {name}: {error}",
                provider.label()
            );
        }
    }
}

#[cfg(all(feature = "agent", unix))]
pub(super) struct PickedAccount {
    pub(super) id: Option<String>,
    pub(super) home: Option<PathBuf>,
    pub(super) fresh: bool,
}

#[cfg(all(feature = "agent", unix))]
pub(super) fn pick_account(
    provider: AgentProvider,
    program: &Path,
    search: Option<&OsString>,
    choice: &AccountChoice,
) -> Result<PickedAccount, String> {
    use crate::agent::accounts;

    if !provider.accounts() {
        return Ok(PickedAccount {
            id: None,
            home: None,
            fresh: false,
        });
    }
    match choice {
        AccountChoice::Default => Ok(PickedAccount {
            id: None,
            home: None,
            fresh: false,
        }),
        AccountChoice::Named(id) => {
            let home = accounts::prepare(provider, &accounts::Account::resolve(provider, id))?;
            Ok(PickedAccount {
                id: Some(id.clone()),
                home: Some(home),
                fresh: false,
            })
        }
        AccountChoice::New => {
            let probed = provider_accounts(provider, program, search);
            let existing = probed
                .iter()
                .filter_map(|(id, probe)| {
                    Some((
                        id.as_deref()?,
                        probe.as_ref().is_none_or(|(account, _)| account.signed_in),
                    ))
                })
                .collect::<Vec<_>>();
            let id = login_account(&existing);
            let fresh = existing.iter().all(|(taken, _)| *taken != id);
            let home = accounts::create(provider, &id).map_err(|error| {
                format!(
                    "could not create the {} account {id}: {error}",
                    provider.label()
                )
            })?;
            Ok(PickedAccount {
                id: Some(id),
                home: Some(home),
                fresh,
            })
        }
    }
}

#[cfg(all(feature = "agent", unix))]
fn provider_accounts(
    provider: AgentProvider,
    program: &Path,
    search: Option<&OsString>,
) -> Vec<(Option<String>, Option<(AgentAccount, Option<QuotaWindows>)>)> {
    let ids = if provider.accounts() {
        std::iter::once(None)
            .chain(crate::agent::accounts::ids(provider).into_iter().map(Some))
            .collect::<Vec<_>>()
    } else {
        vec![None]
    };
    thread::scope(|scope| {
        ids.into_iter()
            .map(|id| {
                let probe = id.clone();
                (
                    id,
                    scope.spawn(move || identify(provider, program, probe, search)),
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(id, probe)| (id, probe.join().ok().flatten()))
            .collect()
    })
}

#[cfg(all(feature = "agent", unix))]
pub(super) fn identify(
    provider: AgentProvider,
    program: &Path,
    id: Option<String>,
    search: Option<&OsString>,
) -> Option<(AgentAccount, Option<QuotaWindows>)> {
    use crate::agent::accounts;

    let home = match id.as_deref() {
        None => None,
        Some(name) => {
            match accounts::prepare(provider, &accounts::Account::resolve(provider, name)) {
                Ok(home) => Some(home),
                Err(error) => {
                    log::info!(target: "zz::agent", "{error}");
                    return Some((
                        AgentAccount {
                            id,
                            ..signed_out(provider)
                        },
                        None,
                    ));
                }
            }
        }
    };
    let mut command = Command::new(program);
    providers::prepare(&mut command, search, None);
    for name in crate::PARENT_CLAUDE_SESSION_ENVIRONMENT.iter().chain(&[
        "ZZ_PANE",
        "ZZ_SESSION",
        "TMUX",
        "TMUX_PANE",
    ]) {
        command.env_remove(name);
    }
    if let Some(home) = &home {
        accounts::run_on(&mut command, provider, home);
    }
    let (account, quota) = match provider {
        AgentProvider::ClaudeCode => {
            command.args(["auth", "status", "--json"]);
            let deadline = Instant::now() + STATUS_TIMEOUT;
            let account =
                match crate::bounded_command::run_output_until(command, MAX_STATUS_BYTES, deadline)
                {
                    Ok(output) => claude_status(&output.stdout)?,
                    Err(error) => {
                        log::info!(target: "zz::agent", "claude auth status: {error}");
                        return None;
                    }
                };
            (account, None)
        }
        AgentProvider::Codex => match codex_account_read(command) {
            Ok((result, limits)) => {
                let account = codex_account(&result);
                let quota = limits.filter(|_| account.signed_in).and_then(|limits| {
                    crate::agent::codex::translate::rate_limits(&limits["rateLimits"])
                });
                (account, quota)
            }
            Err(error) => {
                log::info!(target: "zz::agent", "codex account/read: {error}");
                return None;
            }
        },
        AgentProvider::Antigravity => {
            command.arg("acp");
            match acp_probe(command, "session/list") {
                Ok(Ok(_)) => (
                    AgentAccount {
                        signed_in: true,
                        ..signed_out(AgentProvider::Antigravity)
                    },
                    None,
                ),
                Ok(Err(AcpProbeError::AuthRequired)) => {
                    (signed_out(AgentProvider::Antigravity), None)
                }
                Ok(Err(AcpProbeError::Other(error))) | Err(error) => {
                    log::info!(target: "zz::agent", "agy acp session/list: {error}");
                    return None;
                }
            }
        }
        AgentProvider::Grok => {
            command.args(["agent", "stdio"]);
            match acp_probe(command, "_x.ai/billing") {
                Ok(Ok(result)) => grok_billing(&result),
                Ok(Err(AcpProbeError::AuthRequired)) => (signed_out(AgentProvider::Grok), None),
                Ok(Err(AcpProbeError::Other(error))) | Err(error) => {
                    log::info!(target: "zz::agent", "grok agent stdio _x.ai/billing: {error}");
                    return None;
                }
            }
        }
        AgentProvider::Cursor => {
            command.args(["status", "--format", "json"]);
            let deadline = Instant::now() + STATUS_TIMEOUT;
            let account =
                match crate::bounded_command::run_output_until(command, MAX_STATUS_BYTES, deadline)
                {
                    Ok(output) => cursor_status(&output.stdout)?,
                    Err(error) => {
                        log::info!(target: "zz::agent", "cursor-agent status: {error}");
                        return None;
                    }
                };
            (account, None)
        }
    };
    Some((AgentAccount { id, ..account }, quota))
}

#[cfg(all(feature = "agent", unix))]
enum AcpProbeError {
    AuthRequired,
    Other(String),
}

#[cfg(all(feature = "agent", unix))]
fn acp_probe(
    mut command: Command,
    method: &str,
) -> Result<Result<serde_json::Value, AcpProbeError>, String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn_unmasked()
        .map_err(|error| error.to_string())?;
    let outcome = acp_exchange(&mut child, method);
    stop(&mut child);
    outcome
}

#[cfg(all(feature = "agent", unix))]
fn acp_exchange(
    child: &mut Child,
    method: &str,
) -> Result<Result<serde_json::Value, AcpProbeError>, String> {
    let mut input = child.stdin.take().ok_or("no stdin")?;
    let output = child.stdout.take().ok_or("no stdout")?;
    let (sender, replies) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if let Ok(message) = serde_json::from_str::<serde_json::Value>(&line)
                && sender.send(message).is_err()
            {
                break;
            }
        }
    });
    let deadline = Instant::now() + RPC_REPLY;
    let mut send = |message: serde_json::Value| {
        writeln!(input, "{message}")
            .and_then(|()| input.flush())
            .map_err(|error| error.to_string())
    };
    let reply = |id: u64| -> Result<Result<serde_json::Value, AcpProbeError>, String> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("no answer from the agent".to_owned());
            }
            let message = replies
                .recv_timeout(remaining)
                .map_err(|_| "no answer from the agent".to_owned())?;
            if message["id"] != id {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Ok(Err(if is_acp_auth_required(error) {
                    AcpProbeError::AuthRequired
                } else {
                    AcpProbeError::Other(error["message"].as_str().unwrap_or("refused").to_owned())
                }));
            }
            return Ok(Ok(message["result"].clone()));
        }
    };
    send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {},
            "clientInfo": { "name": "zz", "version": env!("CARGO_PKG_VERSION") },
        },
    }))?;
    match reply(1)? {
        Ok(_) => {}
        Err(AcpProbeError::AuthRequired) => return Ok(Err(AcpProbeError::AuthRequired)),
        Err(AcpProbeError::Other(error)) => return Err(error),
    }
    send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": method,
        "params": {},
    }))?;
    reply(2)
}

#[cfg(all(feature = "agent", unix))]
fn is_acp_auth_required(error: &serde_json::Value) -> bool {
    if error["code"].as_i64() == Some(ACP_AUTH_REQUIRED) {
        return true;
    }
    let message = error["message"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    message.contains("authentication required")
        || message.contains("not authenticated")
        || message.contains("unauthenticated")
}

#[cfg(all(feature = "agent", unix))]
fn codex_account_read(
    mut command: Command,
) -> Result<(serde_json::Value, Option<serde_json::Value>), String> {
    command
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn_unmasked()
        .map_err(|error| error.to_string())?;
    let replies = crate::agent_fork::codex_requests(
        &mut child,
        &[
            (
                "account/read",
                &serde_json::json!({ "refreshToken": false }),
            ),
            (
                "account/rateLimits/read",
                &serde_json::json!({ "excludeResetCreditDetails": true }),
            ),
        ],
        RPC_REPLY,
    );
    stop(&mut child);
    let mut replies = replies?.into_iter();
    let account = replies
        .next()
        .unwrap_or_else(|| Err("no account/read answer".to_owned()))?;
    let limits = replies.next().and_then(|limits| {
        limits
            .inspect_err(
                |error| log::info!(target: "zz::agent", "codex account/rateLimits/read: {error}"),
            )
            .ok()
    });
    Ok((account, limits))
}

#[cfg(all(feature = "agent", unix))]
fn stop(child: &mut Child) {
    let deadline = Instant::now() + RPC_EXIT;
    while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
        thread::sleep(Duration::from_millis(20));
    }
    providers::kill_group(child);
}

#[cfg(all(feature = "agent", unix))]
pub(super) const fn signed_out(provider: AgentProvider) -> AgentAccount {
    AgentAccount {
        provider,
        id: None,
        signed_in: false,
        email: None,
        organization: None,
        plan: None,
    }
}

#[cfg(all(feature = "agent", unix))]
fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.is_empty())
        .map(providers::bounded)
}

#[cfg(all(feature = "agent", unix))]
fn claude_status(stdout: &[u8]) -> Option<AgentAccount> {
    let status = serde_json::from_slice::<serde_json::Value>(stdout).ok()?;
    Some(AgentAccount {
        signed_in: status["loggedIn"].as_bool()?,
        email: text(&status, "email"),
        organization: text(&status, "orgName"),
        plan: text(&status, "planTier").or_else(|| text(&status, "subscriptionType")),
        ..signed_out(AgentProvider::ClaudeCode)
    })
}

#[cfg(all(feature = "agent", unix))]
fn codex_account(result: &serde_json::Value) -> AgentAccount {
    let account = &result["account"];
    AgentAccount {
        signed_in: account.is_object(),
        email: text(account, "email"),
        organization: text(account, "workspaceName"),
        plan: text(account, "planType"),
        ..signed_out(AgentProvider::Codex)
    }
}

#[cfg(all(feature = "agent", unix))]
fn cursor_status(stdout: &[u8]) -> Option<AgentAccount> {
    let start = stdout.iter().position(|byte| *byte == b'{')?;
    let status = serde_json::from_slice::<serde_json::Value>(&stdout[start..]).ok()?;
    let signed_in = status["isAuthenticated"]
        .as_bool()
        .or_else(|| status["status"].as_str().map(|s| s == "authenticated"))?;
    let user = &status["userInfo"];
    Some(AgentAccount {
        signed_in,
        email: signed_in.then(|| text(user, "email")).flatten(),
        organization: signed_in
            .then(|| text(user, "teamName").or_else(|| text(user, "organization")))
            .flatten(),
        plan: signed_in
            .then(|| text(user, "membershipType").or_else(|| text(user, "plan")))
            .flatten(),
        ..signed_out(AgentProvider::Cursor)
    })
}

#[cfg(all(feature = "agent", unix))]
fn grok_billing(result: &serde_json::Value) -> (AgentAccount, Option<QuotaWindows>) {
    let account = AgentAccount {
        signed_in: true,
        plan: text(result, "subscription_tier"),
        ..signed_out(AgentProvider::Grok)
    };
    let quota = crate::agent::grok::credit_usage(result);
    (account, quota)
}

#[cfg(all(feature = "agent", unix))]
fn login_account(accounts: &[(&str, bool)]) -> String {
    if let Some((id, _)) = accounts.iter().find(|(_, signed_in)| !signed_in) {
        return (*id).to_owned();
    }
    (1..=accounts.len() + 1)
        .map(|number| format!("a{number}"))
        .find(|id| accounts.iter().all(|(taken, _)| taken != id))
        .expect("one more number than accounts leaves an id free")
}

#[cfg(all(test, feature = "agent", unix))]
mod tests {
    use super::*;

    #[test]
    fn claude_auth_status_gives_the_login_and_its_plan() {
        let signed_in = br#"{
  "loggedIn": true,
  "authMethod": "claude.ai",
  "apiProvider": "firstParty",
  "configDirectory": "/home/me/.claude",
  "email": "me@example.com",
  "orgId": "0f1e",
  "orgName": "Example Org",
  "subscriptionType": "max",
  "storedClaudeAiLogin": true,
  "planTier": "max_20x"
}"#;
        assert_eq!(
            claude_status(signed_in),
            Some(AgentAccount {
                provider: AgentProvider::ClaudeCode,
                id: None,
                signed_in: true,
                email: Some("me@example.com".to_owned()),
                organization: Some("Example Org".to_owned()),
                plan: Some("max_20x".to_owned()),
            })
        );
        let signed_out = br#"{
  "loggedIn": false,
  "authMethod": "none",
  "apiProvider": "firstParty",
  "configDirectory": "/data/zz/accounts/claude/a2",
  "loginExpiresAt": null
}"#;
        assert_eq!(
            claude_status(signed_out),
            Some(super::signed_out(AgentProvider::ClaudeCode))
        );
    }

    #[test]
    fn a_status_the_cli_could_not_give_is_unknown() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let garbled = directory.path().join("claude");
        std::fs::write(&garbled, "#!/bin/sh\necho 'keychain is locked'\nexit 1\n")
            .expect("write the fake cli");
        std::fs::set_permissions(&garbled, std::fs::Permissions::from_mode(0o755))
            .expect("make the fake cli runnable");
        let missing = directory.path().join("missing");
        for (provider, program) in [
            (AgentProvider::ClaudeCode, &garbled),
            (AgentProvider::ClaudeCode, &missing),
            (AgentProvider::Codex, &missing),
            (AgentProvider::Antigravity, &garbled),
            (AgentProvider::Grok, &garbled),
            (AgentProvider::Cursor, &garbled),
        ] {
            assert_eq!(
                identify(provider, program, None, None),
                None,
                "{provider:?} {}",
                program.display()
            );
        }
    }

    #[test]
    fn codex_account_read_gives_the_login_and_its_plan() {
        let chatgpt = serde_json::json!({
            "account": {
                "type": "chatgpt",
                "email": "me@example.com",
                "planType": "pro",
                "workspaceName": "Example Team",
            },
            "requiresOpenaiAuth": true,
        });
        assert_eq!(
            codex_account(&chatgpt),
            AgentAccount {
                provider: AgentProvider::Codex,
                id: None,
                signed_in: true,
                email: Some("me@example.com".to_owned()),
                organization: Some("Example Team".to_owned()),
                plan: Some("pro".to_owned()),
            }
        );
        let api_key = serde_json::json!({
            "account": { "type": "apiKey" },
            "requiresOpenaiAuth": true,
        });
        assert_eq!(
            codex_account(&api_key),
            AgentAccount {
                signed_in: true,
                ..signed_out(AgentProvider::Codex)
            }
        );
        let none = serde_json::json!({ "account": null, "requiresOpenaiAuth": true });
        assert_eq!(codex_account(&none), signed_out(AgentProvider::Codex));
    }

    #[test]
    fn a_login_reuses_the_first_signed_out_account_or_takes_the_next_id() {
        assert_eq!(login_account(&[("a1", true), ("a2", false)]), "a2");
        assert_eq!(login_account(&[("a1", true), ("a2", true)]), "a3");
        assert_eq!(login_account(&[]), "a1");
    }

    #[test]
    fn cursor_status_skips_the_tip_and_reads_email_and_signed_out_state() {
        let signed_in = b"Tip: You can start the Cursor CLI with `agent` (same as `cursor-agent`).\n\
{\"status\":\"authenticated\",\"isAuthenticated\":true,\"userInfo\":{\"email\":\"me@example.com\"}}";
        assert_eq!(
            cursor_status(signed_in),
            Some(AgentAccount {
                provider: AgentProvider::Cursor,
                id: None,
                signed_in: true,
                email: Some("me@example.com".to_owned()),
                organization: None,
                plan: None,
            })
        );
        let signed_out = br#"{"status":"unauthenticated","isAuthenticated":false}"#;
        assert_eq!(
            cursor_status(signed_out),
            Some(super::signed_out(AgentProvider::Cursor))
        );
    }

    #[test]
    fn grok_billing_reads_the_plan_and_weekly_quota() {
        let billing = serde_json::json!({
            "subscription_tier": "SuperGrok Plus",
            "config": {
                "creditUsagePercent": 42.0,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "end": "2026-11-28T00:00:00Z",
                },
            },
        });
        let (account, quota) = grok_billing(&billing);
        assert_eq!(
            account,
            AgentAccount {
                provider: AgentProvider::Grok,
                id: None,
                signed_in: true,
                email: None,
                organization: None,
                plan: Some("SuperGrok Plus".to_owned()),
            }
        );
        assert!(quota.and_then(|q| q.week).is_some());
    }

    #[test]
    fn single_account_providers_accept_default_and_reject_named_accounts() {
        let parse = |args: &[&str]| {
            let args = args
                .iter()
                .map(|arg| RawText::from(*arg))
                .collect::<Vec<_>>();
            parse_agent_accounts_args(&args)
        };
        assert_eq!(
            parse(&["--add", "grok"]).unwrap().map(|add| add.provider),
            Some(AgentProvider::Grok)
        );
        assert_eq!(
            parse(&["--add", "cursor", "--account", "default"])
                .unwrap()
                .map(|add| add.provider),
            Some(AgentProvider::Cursor)
        );
        assert_eq!(
            parse(&["--add", "agy"]).unwrap().map(|add| add.provider),
            Some(AgentProvider::Antigravity)
        );
        assert!(parse(&["--add", "grok", "--account", "work"]).is_err());
    }
}
