use super::*;
#[cfg(all(feature = "agent", unix))]
use std::{
    ffi::OsString,
    process::{Child, Command, Stdio},
};

#[cfg(all(feature = "agent", unix))]
use zz_daemon_client::unmasked::SpawnUnmasked as _;
#[cfg(all(feature = "agent", unix))]
use zz_protocol::AgentAccount;
use zz_protocol::AgentProvider;

#[cfg(all(feature = "agent", unix))]
const PROVIDERS: [AgentProvider; 2] = [AgentProvider::ClaudeCode, AgentProvider::Codex];
#[cfg(all(feature = "agent", unix))]
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(all(feature = "agent", unix))]
const MAX_STATUS_BYTES: usize = 64 * 1024;
#[cfg(all(feature = "agent", unix))]
const CODEX_REPLY: Duration = Duration::from_secs(5);
#[cfg(all(feature = "agent", unix))]
const CODEX_EXIT: Duration = Duration::from_secs(2);

#[cfg(all(feature = "agent", unix))]
#[derive(Default)]
pub(super) struct Accounts {
    list: Option<Vec<AgentAccount>>,
    watchers: BTreeSet<ClientId>,
    refreshing: bool,
    again: bool,
}

pub(super) fn parse_agent_accounts_args(
    args: &[RawText],
) -> Result<Option<AgentProvider>, ServerError> {
    let spec = zz_protocol::catalog_command_spec("agent-accounts").expect("registered command");
    let parsed = zz_protocol::parse_tmux_options(spec, args)?;
    spec.validate_positional_maximum(parsed.positionals.len())?;
    let mut add = None;
    for option in parsed.options {
        match option {
            zz_protocol::TmuxOption::Value("--add", value) => {
                add = Some(
                    value
                        .parse::<AgentProvider>()
                        .map_err(ServerError::CommandParse)?,
                );
            }
            zz_protocol::TmuxOption::Flag(name) | zz_protocol::TmuxOption::Value(name, _) => {
                return Err(ServerError::CommandParse(format!(
                    "agent-accounts: unknown option {name}"
                )));
            }
        }
    }
    Ok(add)
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
            let Some(provider) = add else {
                self.refresh_agent_accounts();
                return Ok(Execution::default());
            };
            let shared = Arc::downgrade(&self.server_owner());
            let context = ExecutionContext::new(context.session, context.window, context.pane);
            let spawned = thread::Builder::new()
                .name("zz-agent-accounts-add".to_owned())
                .spawn(move || {
                    if let Some(shared) = shared.upgrade() {
                        shared.add_agent_account(provider, context);
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

#[cfg(all(feature = "agent", unix))]
impl Shared {
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
        let list = PROVIDERS
            .into_iter()
            .filter_map(|provider| {
                providers::locate(provider, search.as_ref())
                    .map(|program| provider_accounts(provider, &program, search.as_ref()))
            })
            .flatten()
            .filter(|account| account.id.is_none() || account.signed_in)
            .collect::<Vec<_>>();
        let (changed, again) = {
            let mut accounts = self.agent_accounts.lock();
            let changed = accounts.list.as_ref() != Some(&list);
            accounts.list = Some(list);
            let again = std::mem::take(&mut accounts.again);
            accounts.refreshing = again;
            (changed, again)
        };
        if changed {
            self.publish_agent_accounts();
        }
        again
    }

    fn add_agent_account(self: &Arc<Self>, provider: AgentProvider, mut context: ExecutionContext) {
        use crate::agent::{accounts, cli_session};

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
        let probed = provider_accounts(provider, &program, search.as_ref());
        let existing = probed
            .iter()
            .filter_map(|account| Some((account.id.as_deref()?, account.signed_in)))
            .collect::<Vec<_>>();
        let id = login_account(&existing);
        let home = match accounts::create(provider, &id) {
            Ok(home) => home,
            Err(error) => {
                log::warn!(
                    target: "zz::agent",
                    "could not create the {} account {id}: {error}",
                    provider.label()
                );
                return;
            }
        };
        let login: &[&str] = match provider {
            AgentProvider::ClaudeCode => &["claude", "auth", "login"],
            AgentProvider::Codex => &["codex", "login"],
        };
        let login = login
            .iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>();
        let mut args = vec![
            "-n".to_owned(),
            format!("{} {id}", provider.label()),
            "--".to_owned(),
        ];
        args.extend(cli_session::interactive_shell(
            Some((provider, &home)),
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
                "could not open the login of the {} account {id}: {error}",
                provider.label()
            );
        }
    }
}

#[cfg(all(feature = "agent", unix))]
fn provider_accounts(
    provider: AgentProvider,
    program: &Path,
    search: Option<&OsString>,
) -> Vec<AgentAccount> {
    let ids = std::iter::once(None)
        .chain(crate::agent::accounts::ids(provider).into_iter().map(Some))
        .collect::<Vec<_>>();
    thread::scope(|scope| {
        ids.into_iter()
            .map(|id| scope.spawn(move || identify(provider, program, id, search)))
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|probe| probe.join().ok())
            .collect()
    })
}

#[cfg(all(feature = "agent", unix))]
fn identify(
    provider: AgentProvider,
    program: &Path,
    id: Option<String>,
    search: Option<&OsString>,
) -> AgentAccount {
    use crate::agent::accounts;

    let home = match id.as_deref() {
        None => None,
        Some(name) => {
            match accounts::prepare(provider, &accounts::Account::resolve(provider, name)) {
                Ok(home) => Some(home),
                Err(error) => {
                    log::info!(target: "zz::agent", "{error}");
                    return AgentAccount {
                        id,
                        ..signed_out(provider)
                    };
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
    let account = match provider {
        AgentProvider::ClaudeCode => {
            command.args(["auth", "status", "--json"]);
            let deadline = Instant::now() + STATUS_TIMEOUT;
            match crate::bounded_command::run_output_until(command, MAX_STATUS_BYTES, deadline) {
                Ok(output) => claude_status(&output.stdout),
                Err(error) => {
                    log::info!(target: "zz::agent", "claude auth status: {error}");
                    signed_out(provider)
                }
            }
        }
        AgentProvider::Codex => match codex_account_read(command) {
            Ok(result) => codex_account(&result),
            Err(error) => {
                log::info!(target: "zz::agent", "codex account/read: {error}");
                signed_out(provider)
            }
        },
    };
    AgentAccount { id, ..account }
}

#[cfg(all(feature = "agent", unix))]
fn codex_account_read(mut command: Command) -> Result<serde_json::Value, String> {
    command
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn_unmasked()
        .map_err(|error| error.to_string())?;
    let result = crate::agent_fork::codex_request(
        &mut child,
        "account/read",
        &serde_json::json!({ "refreshToken": false }),
        CODEX_REPLY,
    );
    stop(&mut child);
    result
}

#[cfg(all(feature = "agent", unix))]
fn stop(child: &mut Child) {
    let deadline = Instant::now() + CODEX_EXIT;
    while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
        thread::sleep(Duration::from_millis(20));
    }
    providers::kill_group(child);
}

#[cfg(all(feature = "agent", unix))]
const fn signed_out(provider: AgentProvider) -> AgentAccount {
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
fn claude_status(stdout: &[u8]) -> AgentAccount {
    let status = serde_json::from_slice::<serde_json::Value>(stdout).unwrap_or_default();
    AgentAccount {
        signed_in: status["loggedIn"] == true,
        email: text(&status, "email"),
        organization: text(&status, "orgName"),
        plan: text(&status, "planTier").or_else(|| text(&status, "subscriptionType")),
        ..signed_out(AgentProvider::ClaudeCode)
    }
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
            AgentAccount {
                provider: AgentProvider::ClaudeCode,
                id: None,
                signed_in: true,
                email: Some("me@example.com".to_owned()),
                organization: Some("Example Org".to_owned()),
                plan: Some("max_20x".to_owned()),
            }
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
            super::signed_out(AgentProvider::ClaudeCode)
        );
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
}
