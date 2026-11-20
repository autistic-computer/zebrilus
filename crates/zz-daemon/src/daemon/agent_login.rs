use super::*;
use std::{
    ffi::OsString,
    io::{BufRead as _, BufReader},
    process::{Child, ChildStdin, Command},
};

use serde_json::{Value, json};
use zz_daemon_client::unmasked::SpawnUnmasked as _;
use zz_machine::Machine as _;
use zz_protocol::{
    AGENT_LOGIN_CAPABILITY, AgentLogin, AgentLoginBrowser, AgentLoginCode, AgentLoginFailure,
    AgentLoginRequest, AgentLoginStep, AgentProvider,
};

use super::accounts::{AccountChoice, AddAccount};
use crate::agent::login::{Ctx, Effect, Fact, LoginMachine, LoginState, PaneCondition, Prompt};

const STDERR_LINES: usize = 3;
const CLAUDE_INITIALIZE: &str = "zz-login-1";
const CLAUDE_AUTHENTICATE: &str = "zz-login-2";
const CLAUDE_COMPLETE: &str = "zz-login-3";
const CODEX_INITIALIZE: u64 = 1;
const CODEX_LOGIN: u64 = 2;

#[derive(Default)]
pub(super) struct Logins {
    next: u64,
    running: BTreeMap<AgentProvider, Running>,
}

struct Running {
    id: u64,
    inbox: mpsc::Sender<Input>,
    shown: Option<AgentLogin>,
}

enum Input {
    Code(AgentLoginCode),
    Cancel,
    Restart(AgentLoginBrowser),
    Line { generation: u64, line: Value },
    Closed { generation: u64 },
}

impl Shared {
    pub(super) fn client_draws_agent_logins(&self, client: ClientId) -> bool {
        self.inner
            .lock()
            .client(client)
            .and_then(|client| client.subscriber.as_ref())
            .is_some_and(|subscriber| subscriber.has_client_capability(AGENT_LOGIN_CAPABILITY))
    }

    pub(super) fn publish_agent_logins_to(&self, client: ClientId) {
        let shown = self
            .agent_logins
            .lock()
            .running
            .values()
            .filter_map(|running| running.shown.clone())
            .collect::<Vec<_>>();
        for login in shown {
            self.publish_to_client(client, EventPayload::AgentLogin(login));
        }
    }

    pub(super) fn start_agent_login(
        self: &Arc<Self>,
        client: ClientId,
        add: AddAccount,
        pane: Option<PaneId>,
    ) -> Result<(), ServerError> {
        let provider = add.provider;
        let (id, receiver, sender) = {
            let mut logins = self.agent_logins.lock();
            if let Some(running) = logins.running.get(&provider) {
                let _ = running.inbox.send(Input::Restart(add.browser));
                if let Some(login) = running.shown.clone() {
                    drop(logins);
                    self.publish_to_client(client, EventPayload::AgentLogin(login));
                }
                return Ok(());
            }
            logins.next += 1;
            let id = logins.next;
            let (sender, receiver) = mpsc::channel();
            logins.running.insert(
                provider,
                Running {
                    id,
                    inbox: sender.clone(),
                    shown: None,
                },
            );
            (id, receiver, sender)
        };
        let shared = Arc::downgrade(&self.server_owner());
        let spawned = thread::Builder::new()
            .name("zz-agent-login".to_owned())
            .spawn(move || {
                let mut driver = Driver {
                    shared,
                    login: AgentLogin {
                        id,
                        provider,
                        account: None,
                        pane,
                        browser: add.browser,
                        step: AgentLoginStep::Starting,
                    },
                    outbox: sender,
                    generation: 0,
                };
                driver.run(&add.account, &receiver);
            });
        if let Err(error) = spawned {
            self.agent_logins.lock().running.remove(&provider);
            return Err(ServerError::Internal(format!(
                "could not sign in to {}: {error}",
                provider.label()
            )));
        }
        Ok(())
    }

    pub(super) fn agent_login_request(&self, request: AgentLoginRequest) {
        let (login, input) = match request {
            AgentLoginRequest::Code { login, code } => (login, Input::Code(code)),
            AgentLoginRequest::Cancel { login } => (login, Input::Cancel),
        };
        if let Some(running) = self
            .agent_logins
            .lock()
            .running
            .values()
            .find(|running| running.id == login)
        {
            let _ = running.inbox.send(input);
        }
    }

    fn show_agent_login(&self, login: &AgentLogin) {
        if login.finished() {
            self.end_agent_login(login);
        } else if let Some(running) = self.agent_logins.lock().running.get_mut(&login.provider)
            && running.id == login.id
        {
            running.shown = Some(login.clone());
        }
        self.publish(EventPayload::AgentLogin(login.clone()));
    }

    fn end_agent_login(&self, login: &AgentLogin) {
        let mut logins = self.agent_logins.lock();
        if logins
            .running
            .get(&login.provider)
            .is_some_and(|running| running.id == login.id)
        {
            logins.running.remove(&login.provider);
        }
    }

    fn login_pane(&self, pane: Option<PaneId>, provider: AgentProvider) -> PaneCondition {
        use crate::agent::cli_session;

        let Some(pane) = pane else {
            return PaneCondition::Absent;
        };
        let kind = self
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .map(|state| state.kind.clone());
        match kind {
            Some(PaneKind::Agent(descriptor)) if descriptor.provider == provider => {
                let state = self
                    .open_agent_runtime()
                    .and_then(|runtime| runtime.wire_state(pane))
                    .unwrap_or_default();
                if cli_session::busy(&cli_session::chat_running(&state)).is_some() {
                    PaneCondition::Busy
                } else {
                    PaneCondition::Idle
                }
            }
            _ => PaneCondition::Absent,
        }
    }
}

struct Driver {
    shared: Weak<Shared>,
    login: AgentLogin,
    outbox: mpsc::Sender<Input>,
    generation: u64,
}

struct Account {
    program: PathBuf,
    search: Option<OsString>,
    home: Option<PathBuf>,
}

impl Driver {
    fn run(&mut self, choice: &AccountChoice, inbox: &mpsc::Receiver<Input>) {
        let now = Instant::now();
        let mut state = LoginState::new(self.login.browser, false, now);
        let account = match self.account(choice) {
            Ok((account, fresh)) => {
                state = LoginState::new(self.login.browser, fresh, now);
                Some(account)
            }
            Err(failure) => {
                if self.step(&mut state, &Fact::Failed(failure)).1 {
                    self.show(&state);
                }
                None
            }
        };
        if let Some(account) = account {
            self.show(&state);
            self.drive(&mut state, &account, inbox);
        }
        if let Some(shared) = self.shared.upgrade() {
            shared.end_agent_login(&self.login);
        }
    }

    fn account(&mut self, choice: &AccountChoice) -> Result<(Account, bool), AgentLoginFailure> {
        let provider = self.login.provider;
        let shared = self
            .shared
            .upgrade()
            .ok_or_else(|| AgentLoginFailure::Cli("zz is stopping.".to_owned()))?;
        let search = shared.provider_search();
        let program =
            providers::locate(provider, search.as_ref()).ok_or(AgentLoginFailure::NotInstalled)?;
        let picked = super::accounts::pick_account(provider, &program, search.as_ref(), choice)
            .map_err(AgentLoginFailure::Cli)?;
        self.login.account = picked.id;
        Ok((
            Account {
                program,
                search,
                home: picked.home,
            },
            picked.fresh,
        ))
    }

    fn drive(&mut self, state: &mut LoginState, account: &Account, inbox: &mpsc::Receiver<Input>) {
        let mut cli = None;
        let mut code = None;
        let mut pending = VecDeque::new();
        match self.launch(state, account) {
            Ok(launched) => cli = Some(launched),
            Err(error) => pending.push_back(Fact::Failed(AgentLoginFailure::Cli(error))),
        }
        while !state.finished() {
            if let Some(fact) = pending.pop_front() {
                let (effects, changed) = self.step(state, &fact);
                if changed && !state.finished() {
                    self.show(state);
                }
                for effect in effects {
                    pending.extend(self.apply(effect, state, account, &mut cli, &mut code));
                }
                if changed && state.finished() {
                    self.show(state);
                }
                continue;
            }
            let wait = LoginMachine::wake_at(state).map_or(Duration::from_hours(1), |at| {
                at.saturating_duration_since(Instant::now())
            });
            let fact = match inbox.recv_timeout(wait) {
                Err(mpsc::RecvTimeoutError::Timeout) => Some(Fact::Tick),
                Err(mpsc::RecvTimeoutError::Disconnected) | Ok(Input::Cancel) => Some(Fact::Cancel),
                Ok(Input::Restart(browser)) => Some(Fact::Restart(browser)),
                Ok(Input::Code(submitted)) if state.awaits_code() => {
                    code = Some(submitted);
                    Some(Fact::CodeSubmitted)
                }
                Ok(Input::Code(_)) => None,
                Ok(Input::Line { generation, line }) => cli
                    .as_mut()
                    .filter(|cli| cli.generation == generation)
                    .and_then(|cli| cli.read(&line)),
                Ok(Input::Closed { generation }) => cli
                    .as_mut()
                    .filter(|cli| cli.generation == generation)
                    .map(|cli| Fact::Failed(cli.closed())),
            };
            pending.extend(fact);
            if self.shared.strong_count() == 0 {
                break;
            }
        }
        if let Some(mut cli) = cli {
            cli.stop();
        }
    }

    fn apply(
        &mut self,
        effect: Effect,
        state: &LoginState,
        account: &Account,
        cli: &mut Option<Cli>,
        code: &mut Option<AgentLoginCode>,
    ) -> Option<Fact> {
        let failed = |error| Fact::Failed(AgentLoginFailure::Cli(error));
        match effect {
            Effect::Launch => match self.launch(state, account) {
                Ok(launched) => {
                    *cli = Some(launched);
                    None
                }
                Err(error) => Some(failed(error)),
            },
            Effect::StopCli => {
                if let Some(mut stopped) = cli.take() {
                    stopped.stop();
                }
                None
            }
            Effect::SendCode => match (cli.as_mut(), code.take()) {
                (Some(cli), Some(code)) => cli.send_code(&code).err().map(failed),
                _ => None,
            },
            Effect::Probe => Some(self.probe(account)),
            Effect::SwitchPane => {
                self.switch_pane();
                None
            }
            Effect::RemoveHome => {
                remove_home(account.home.as_deref());
                None
            }
        }
    }

    fn step(&mut self, state: &mut LoginState, fact: &Fact) -> (Vec<Effect>, bool) {
        let before = (state.step.clone(), state.browser);
        let step = LoginMachine::step(
            state,
            fact,
            &Ctx {
                now: Instant::now(),
            },
        );
        self.login.browser = state.browser;
        let changed = step.moved && before != (state.step.clone(), state.browser);
        (step.effects, changed)
    }

    fn show(&mut self, state: &LoginState) {
        self.login.step = state.step.clone();
        if let Some(shared) = self.shared.upgrade() {
            shared.show_agent_login(&self.login);
        }
    }

    fn launch(&mut self, state: &LoginState, account: &Account) -> Result<Cli, String> {
        self.generation += 1;
        Cli::launch(
            self.login.provider,
            state.browser,
            account,
            self.generation,
            &self.outbox,
        )
    }

    fn probe(&self, account: &Account) -> Fact {
        let provider = self.login.provider;
        let (probed, quota) = super::accounts::identify(
            provider,
            &account.program,
            self.login.account.clone(),
            account.search.as_ref(),
        );
        let pane = match self.shared.upgrade() {
            Some(shared) => {
                shared.record_agent_account(probed.clone(), quota);
                shared.login_pane(self.login.pane, provider)
            }
            None => PaneCondition::Absent,
        };
        Fact::Verified {
            account: probed,
            pane,
        }
    }

    fn switch_pane(&self) {
        if let (Some(shared), Some(pane)) = (self.shared.upgrade(), self.login.pane) {
            shared.switch_agent_account(pane, self.login.account.clone());
        }
    }
}

fn remove_home(home: Option<&Path>) {
    let Some(home) = home else {
        return;
    };
    if let Err(error) = fs::remove_dir_all(home) {
        log::warn!(target: "zz::agent", "could not remove {}: {error}", home.display());
    }
}

struct Cli {
    provider: AgentProvider,
    browser: AgentLoginBrowser,
    generation: u64,
    process: Option<(Child, ChildStdin)>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    login: Option<String>,
    state: Option<String>,
}

impl Cli {
    fn launch(
        provider: AgentProvider,
        browser: AgentLoginBrowser,
        account: &Account,
        generation: u64,
        outbox: &mpsc::Sender<Input>,
    ) -> Result<Self, String> {
        use crate::agent::accounts;

        let mut command = Command::new(&account.program);
        providers::prepare(&mut command, account.search.as_ref(), None);
        for name in crate::PARENT_CLAUDE_SESSION_ENVIRONMENT
            .iter()
            .chain(&["ZZ_PANE", "ZZ_SESSION", "TMUX", "TMUX_PANE"])
            .chain(accounts::login_overrides(provider))
        {
            command.env_remove(name);
        }
        if let Some(home) = &account.home {
            accounts::run_on(&mut command, provider, home);
        }
        match provider {
            AgentProvider::ClaudeCode => command.args([
                "-p",
                "--output-format",
                "stream-json",
                "--input-format",
                "stream-json",
                "--verbose",
                "--strict-mcp-config",
                "--no-session-persistence",
            ]),
            AgentProvider::Codex => command.arg("app-server"),
            AgentProvider::Antigravity => {
                return Err("Sign in to Antigravity by running agy in a terminal.".to_owned());
            }
            AgentProvider::Grok => {
                return Err("Sign in to Grok by running grok login in a terminal.".to_owned());
            }
        };
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn_unmasked().map_err(|error| {
            format!("Could not run {}: {error}", command.get_program().display())
        })?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            providers::kill_group(&mut child);
            return Err(format!("{} gave no pipes", provider.label()));
        };
        let lines = outbox.clone();
        let read = thread::Builder::new()
            .name("zz-agent-login-out".to_owned())
            .spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Ok(line) = serde_json::from_str::<Value>(&line)
                        && lines.send(Input::Line { generation, line }).is_err()
                    {
                        return;
                    }
                }
                let _ = lines.send(Input::Closed { generation });
            });
        let tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_LINES)));
        let errors = Arc::clone(&tail);
        let errs = thread::Builder::new()
            .name("zz-agent-login-err".to_owned())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let line = providers::bounded(line.trim());
                    if line.is_empty() {
                        continue;
                    }
                    let mut tail = errors.lock();
                    if tail.len() == STDERR_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            });
        if let Err(error) = read.and(errs) {
            providers::kill_group(&mut child);
            return Err(error.to_string());
        }
        let mut cli = Self {
            provider,
            browser,
            generation,
            process: Some((child, stdin)),
            stderr: tail,
            login: None,
            state: None,
        };
        let first = match provider {
            AgentProvider::ClaudeCode => {
                claude_request(CLAUDE_INITIALIZE, &json!({ "subtype": "initialize" }))
            }
            AgentProvider::Codex => json!({
                "id": CODEX_INITIALIZE,
                "method": "initialize",
                "params": { "clientInfo": { "name": "zz", "version": env!("CARGO_PKG_VERSION") } },
            }),
            AgentProvider::Antigravity | AgentProvider::Grok => Value::Null,
        };
        if let Err(error) = cli.send(&first) {
            cli.stop();
            return Err(error);
        }
        Ok(cli)
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        let Some((_, stdin)) = self.process.as_mut() else {
            return Err(format!("{} is not running", self.provider.label()));
        };
        let mut line = message.to_string();
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|error| format!("could not write to {}: {error}", self.provider.label()))
    }

    fn read(&mut self, line: &Value) -> Option<Fact> {
        let fact = match self.provider {
            AgentProvider::ClaudeCode => self.read_claude(line),
            AgentProvider::Codex => self.read_codex(line),
            AgentProvider::Antigravity | AgentProvider::Grok => Ok(None),
        };
        match fact {
            Ok(fact) => fact,
            Err(error) => Some(Fact::Failed(AgentLoginFailure::Cli(error))),
        }
    }

    fn read_claude(&mut self, line: &Value) -> Result<Option<Fact>, String> {
        if line["type"] != "control_response" {
            return Ok(None);
        }
        let response = &line["response"];
        let request = response["request_id"].as_str().unwrap_or_default();
        if response["subtype"] != "success" {
            if ![CLAUDE_INITIALIZE, CLAUDE_AUTHENTICATE, CLAUDE_COMPLETE].contains(&request) {
                return Ok(None);
            }
            let error = response["error"]
                .as_str()
                .filter(|error| !error.is_empty())
                .unwrap_or("Claude Code refused the sign-in.");
            return Ok(Some(Fact::Failed(AgentLoginFailure::Cli(
                providers::bounded(error),
            ))));
        }
        let body = &response["response"];
        match request {
            CLAUDE_INITIALIZE => {
                self.send(&claude_request(
                    CLAUDE_AUTHENTICATE,
                    &json!({ "subtype": "claude_authenticate", "loginWithClaudeAi": true }),
                ))?;
                Ok(None)
            }
            CLAUDE_AUTHENTICATE => {
                let (Some(manual), Some(automatic)) =
                    (body["manualUrl"].as_str(), body["automaticUrl"].as_str())
                else {
                    return Err("Claude Code gave no sign-in link.".to_owned());
                };
                match self.browser {
                    AgentLoginBrowser::Local => {
                        self.send(&claude_request(
                            CLAUDE_COMPLETE,
                            &json!({ "subtype": "claude_oauth_wait_for_completion" }),
                        ))?;
                        Ok(Some(Fact::Prompted(Prompt::Browser {
                            url: automatic.to_owned(),
                        })))
                    }
                    AgentLoginBrowser::Elsewhere => {
                        self.state = query_value(manual, "state").map(str::to_owned);
                        Ok(Some(Fact::Prompted(Prompt::PasteCode {
                            url: manual.to_owned(),
                        })))
                    }
                }
            }
            CLAUDE_COMPLETE => Ok(Some(Fact::SignedIn)),
            _ => Ok(None),
        }
    }

    fn send_code(&mut self, code: &AgentLoginCode) -> Result<(), String> {
        let (authorization, state) = match code.expose().split_once('#') {
            Some((authorization, state)) => (authorization, Some(state)),
            None => (code.expose(), self.state.as_deref()),
        };
        let request = claude_request(
            CLAUDE_COMPLETE,
            &json!({
                "subtype": "claude_oauth_callback",
                "authorizationCode": authorization,
                "state": state,
            }),
        );
        self.send(&request)
    }

    fn read_codex(&mut self, line: &Value) -> Result<Option<Fact>, String> {
        if line["method"] == "account/login/completed" {
            let params = &line["params"];
            if self.login.is_none() || params["loginId"].as_str() != self.login.as_deref() {
                return Ok(None);
            }
            if params["success"] == true {
                return Ok(Some(Fact::SignedIn));
            }
            let error = params["error"]
                .as_str()
                .filter(|error| !error.is_empty())
                .unwrap_or("Codex could not finish the sign-in.");
            return Ok(Some(Fact::Failed(codex_failure(error))));
        }
        let id = line["id"].as_u64();
        if id != Some(CODEX_INITIALIZE) && id != Some(CODEX_LOGIN) {
            return Ok(None);
        }
        if let Some(error) = line.get("error") {
            let error = error["message"]
                .as_str()
                .filter(|error| !error.is_empty())
                .unwrap_or("Codex refused the sign-in.");
            return Ok(Some(Fact::Failed(codex_failure(error))));
        }
        let result = &line["result"];
        if id == Some(CODEX_INITIALIZE) {
            self.send(&json!({ "method": "initialized" }))?;
            let kind = match self.browser {
                AgentLoginBrowser::Local => "chatgpt",
                AgentLoginBrowser::Elsewhere => "chatgptDeviceCode",
            };
            self.send(&json!({
                "id": CODEX_LOGIN,
                "method": "account/login/start",
                "params": { "type": kind },
            }))?;
            return Ok(None);
        }
        self.login = result["loginId"].as_str().map(str::to_owned);
        let prompt = match (
            result["authUrl"].as_str(),
            result["verificationUrl"].as_str(),
            result["userCode"].as_str(),
        ) {
            (Some(url), _, _) => Prompt::Browser {
                url: url.to_owned(),
            },
            (None, Some(url), Some(code)) => Prompt::DeviceCode {
                url: url.to_owned(),
                code: code.to_owned(),
            },
            _ => return Err("Codex gave no sign-in link.".to_owned()),
        };
        Ok(Some(Fact::Prompted(prompt)))
    }

    fn closed(&mut self) -> AgentLoginFailure {
        if let Some((mut child, _)) = self.process.take() {
            providers::kill_group(&mut child);
        }
        let tail = Vec::from(std::mem::take(&mut *self.stderr.lock())).join("\n");
        AgentLoginFailure::Cli(if tail.is_empty() {
            format!(
                "{} quit before the sign-in finished.",
                self.provider.label()
            )
        } else {
            tail
        })
    }

    fn stop(&mut self) {
        if let Some((mut child, stdin)) = self.process.take() {
            drop(stdin);
            providers::kill_group(&mut child);
        }
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        self.stop();
    }
}

fn claude_request(id: &str, request: &Value) -> Value {
    json!({ "type": "control_request", "request_id": id, "request": request })
}

fn codex_failure(error: &str) -> AgentLoginFailure {
    let lower = error.to_ascii_lowercase();
    if lower.contains("device code") && lower.contains("not enabled") {
        AgentLoginFailure::DeviceCodeDisabled
    } else if lower.contains("timed out") {
        AgentLoginFailure::TimedOut
    } else {
        AgentLoginFailure::Cli(providers::bounded(error))
    }
}

fn query_value<'url>(url: &'url str, key: &str) -> Option<&'url str> {
    let (_, query) = url.split_once('?')?;
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(name, value)| (name == key).then_some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use zz_protocol::{AgentLoginPane, Event};

    const CODE: &str = "pasted-4f1c9e";
    const CLAUDE: &str = r#"
printf '%s\n' "$*" >> "$LOG/claude.args"
env >> "$LOG/claude.env"
if [ "$1" = auth ]; then
  printf '%s\n' '{"loggedIn":true,"email":"me@example.com","orgName":"Example Org","subscriptionType":"max"}'
  exit 0
fi
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$LOG/claude.in"
  case "$line" in
    *'"subtype":"initialize"'*) printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-login-1","response":{"commands":[]}}}' ;;
    *'"subtype":"claude_authenticate"'*) printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-login-2","response":{"manualUrl":"https://claude.example/authorize?code=true&state=st-1","automaticUrl":"https://claude.example/authorize?redirect_uri=local&state=st-1"}}}' ;;
    *'"subtype":"claude_oauth_callback"'*) printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-login-3","response":{"account":{"email":"me@example.com"}}}}' ;;
  esac
done
"#;
    const CODEX: &str = r#"
printf '%s\n' "$*" >> "$LOG/codex.args"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$LOG/codex.in"
  case "$line" in
    *'"method":"initialize"'*) printf '%s\n' '{"id":1,"result":{}}' ;;
    *'"method":"account/login/start"'*)
      printf '%s\n' '{"id":2,"result":{"type":"chatgptDeviceCode","loginId":"l-1","verificationUrl":"https://auth.example/device","userCode":"ABCD-1234"}}'
      sleep 0.2
      printf '%s\n' '{"method":"account/login/completed","params":{"loginId":"l-1","success":true,"error":null}}' ;;
    *'"method":"account/read"'*) printf '%s\n' '{"id":2,"result":{"account":{"type":"chatgpt","email":"me@example.com","planType":"pro","workspaceName":"Example Team"}}}' ;;
    *'"method":"account/rateLimits/read"'*) printf '%s\n' '{"id":3,"error":{"message":"no limits"}}' ;;
  esac
done
"#;

    fn script(path: &Path, log: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\nLOG={}\n{body}", log.display())).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn steps(mailbox: &OutboundMailbox, seen: &mut Vec<ProtocolMessage>) -> Vec<AgentLogin> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut logins = Vec::new();
        loop {
            for message in super::super::tests::take_reliable_messages(mailbox) {
                if let ProtocolMessage::Event(Event {
                    payload: EventPayload::AgentLogin(login),
                    ..
                }) = &message
                {
                    logins.push(login.clone());
                }
                seen.push(message);
            }
            if logins.last().is_some_and(|login| {
                login.finished() || matches!(login.step, AgentLoginStep::PasteCode { .. })
            }) {
                return logins;
            }
            assert!(
                Instant::now() < deadline,
                "no finished login in {logins:#?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_login_drives_the_vendor_cli_and_hands_the_pasted_code_only_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let log = dir.path().join("log");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&log).unwrap();
        script(&bin.join("claude"), &log, CLAUDE);
        script(&bin.join("codex"), &log, CODEX);
        for tool in ["sh", "sleep", "env"] {
            let source = ["/bin", "/usr/bin"]
                .iter()
                .map(|root| Path::new(root).join(tool))
                .find(|path| path.exists())
                .unwrap();
            std::os::unix::fs::symlink(source, bin.join(tool)).unwrap();
        }
        let shared = Arc::new(Shared::new(1));
        shared.providers.lock().search = Some(bin.clone().into_os_string());
        shared.agent_accounts.lock().list = Some(Vec::new());
        let mailbox = OutboundMailbox::new();
        mailbox.accept_client_capabilities(&[AGENT_LOGIN_CAPABILITY.to_owned()]);
        let (client, _) =
            shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
        let start = |provider: &str| {
            shared
                .execute(
                    client,
                    ClientKind::Interactive,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new(
                        "agent-accounts",
                        [
                            "--add",
                            provider,
                            "--account",
                            "default",
                            "--browser",
                            "elsewhere",
                        ],
                    ),
                )
                .unwrap();
        };
        let login = |id, provider, step| AgentLogin {
            id,
            provider,
            account: None,
            pane: None,
            browser: AgentLoginBrowser::Elsewhere,
            step,
        };
        let mut seen = Vec::new();

        start("claude");
        let claude = steps(&mailbox, &mut seen);
        let paste = AgentLoginStep::PasteCode {
            url: "https://claude.example/authorize?code=true&state=st-1".to_owned(),
        };
        assert_eq!(
            claude,
            [
                login(1, AgentProvider::ClaudeCode, AgentLoginStep::Starting),
                login(1, AgentProvider::ClaudeCode, paste),
            ]
        );
        shared.agent_login_request(AgentLoginRequest::Code {
            login: 1,
            code: AgentLoginCode::new(CODE).unwrap(),
        });
        assert_eq!(
            steps(&mailbox, &mut seen),
            [
                login(1, AgentProvider::ClaudeCode, AgentLoginStep::Verifying),
                login(
                    1,
                    AgentProvider::ClaudeCode,
                    AgentLoginStep::Done {
                        email: Some("me@example.com".to_owned()),
                        organization: Some("Example Org".to_owned()),
                        plan: Some("max".to_owned()),
                        pane: None,
                    }
                ),
            ]
        );

        start("codex");
        assert_eq!(
            steps(&mailbox, &mut seen),
            [
                login(2, AgentProvider::Codex, AgentLoginStep::Starting),
                login(
                    2,
                    AgentProvider::Codex,
                    AgentLoginStep::DeviceCode {
                        url: "https://auth.example/device".to_owned(),
                        code: "ABCD-1234".to_owned(),
                    }
                ),
                login(2, AgentProvider::Codex, AgentLoginStep::Verifying),
                login(
                    2,
                    AgentProvider::Codex,
                    AgentLoginStep::Done {
                        email: Some("me@example.com".to_owned()),
                        organization: Some("Example Team".to_owned()),
                        plan: Some("pro".to_owned()),
                        pane: None::<AgentLoginPane>,
                    }
                ),
            ]
        );

        let read = |name: &str| fs::read_to_string(log.join(name)).unwrap_or_default();
        let callback = read("claude.in")
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|line| line["request"]["subtype"] == "claude_oauth_callback")
            .unwrap();
        assert_eq!(
            callback["request"],
            json!({
                "subtype": "claude_oauth_callback",
                "authorizationCode": CODE,
                "state": "st-1",
            })
        );
        assert!(read("codex.in").contains(r#""params":{"type":"chatgptDeviceCode"}"#));
        for elsewhere in [read("claude.args"), read("claude.env"), read("codex.in")] {
            assert!(!elsewhere.contains(CODE));
        }
        assert!(!format!("{seen:?}").contains(CODE));
        assert!(
            !shared
                .agent_accounts
                .lock()
                .list
                .iter()
                .any(|list| format!("{list:?}").contains(CODE))
        );
        assert!(shared.agent_logins.lock().running.is_empty());
    }
}
