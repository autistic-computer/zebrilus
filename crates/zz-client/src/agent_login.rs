use zz_protocol::{
    AGENT_LOGIN_CAPABILITY, AgentAccount, AgentLogin, AgentLoginBrowser, AgentLoginCode,
    AgentLoginFailure, AgentLoginPane, AgentLoginRequest, AgentLoginStep, AgentProvider,
    CommandInvocation, DEFAULT_AGENT_ACCOUNT, PaneId, ProtocolMessage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginAccount<'a> {
    New,
    Default,
    Named(&'a str),
}

impl<'a> LoginAccount<'a> {
    #[must_use]
    pub fn of(account: Option<&'a str>) -> Self {
        account.map_or(Self::Default, Self::Named)
    }
}

#[must_use]
pub fn daemon_signs_in(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == AGENT_LOGIN_CAPABILITY)
}

#[must_use]
pub const fn browser(local_host: bool) -> AgentLoginBrowser {
    if local_host {
        AgentLoginBrowser::Local
    } else {
        AgentLoginBrowser::Elsewhere
    }
}

#[must_use]
pub fn entry(
    provider: AgentProvider,
    account: LoginAccount<'_>,
    pane: PaneId,
    browser: AgentLoginBrowser,
    capabilities: &[String],
) -> CommandInvocation {
    if daemon_signs_in(capabilities) {
        start(provider, account, Some(pane), browser)
    } else {
        CommandInvocation::new("agent-accounts", ["--add", provider.as_str()])
    }
}

#[must_use]
pub fn start(
    provider: AgentProvider,
    account: LoginAccount<'_>,
    pane: Option<PaneId>,
    browser: AgentLoginBrowser,
) -> CommandInvocation {
    let mut args = vec!["--add".to_owned(), provider.as_str().to_owned()];
    match account {
        LoginAccount::New => {}
        LoginAccount::Default => {
            args.extend(["--account".to_owned(), DEFAULT_AGENT_ACCOUNT.to_owned()]);
        }
        LoginAccount::Named(id) => args.extend(["--account".to_owned(), id.to_owned()]),
    }
    if let Some(pane) = pane {
        args.extend(["--pane".to_owned(), pane.to_string()]);
    }
    args.extend(["--browser".to_owned(), browser.as_str().to_owned()]);
    CommandInvocation::new("agent-accounts", args)
}

#[must_use]
pub fn restart(login: &AgentLogin, browser: AgentLoginBrowser) -> CommandInvocation {
    start(
        login.provider,
        LoginAccount::of(login.account.as_deref()),
        login.pane,
        browser,
    )
}

#[must_use]
pub fn retry(
    login: &AgentLogin,
    accounts: &[AgentAccount],
    browser: AgentLoginBrowser,
) -> CommandInvocation {
    let account = match login.account.as_deref() {
        None => LoginAccount::Default,
        Some(id)
            if accounts.iter().any(|account| {
                account.provider == login.provider && account.id.as_deref() == Some(id)
            }) =>
        {
            LoginAccount::Named(id)
        }
        Some(_) => LoginAccount::New,
    };
    start(login.provider, account, login.pane, browser)
}

#[must_use]
pub fn in_terminal(login: &AgentLogin, accounts: &[AgentAccount]) -> CommandInvocation {
    let mut command = retry(login, accounts, login.browser);
    command.args.push("--terminal".into());
    command
}

#[must_use]
pub fn submit_code(login: &AgentLogin, code: &str) -> Option<ProtocolMessage> {
    if !matches!(login.step, AgentLoginStep::PasteCode { .. }) {
        return None;
    }
    let code = AgentLoginCode::new(code)?;
    Some(ProtocolMessage::AgentLogin(AgentLoginRequest::Code {
        login: login.id,
        code,
    }))
}

#[must_use]
pub fn cancel(login: &AgentLogin) -> ProtocolMessage {
    ProtocolMessage::AgentLogin(AgentLoginRequest::Cancel { login: login.id })
}

pub fn reduce(logins: &mut Vec<AgentLogin>, login: AgentLogin) -> bool {
    match logins
        .iter_mut()
        .find(|known| known.provider == login.provider)
    {
        Some(known) if *known == login => false,
        Some(known) => {
            *known = login;
            true
        }
        None => {
            logins.push(login);
            true
        }
    }
}

#[must_use]
pub fn for_pane(logins: &[AgentLogin], pane: PaneId) -> Option<&AgentLogin> {
    logins.iter().find(|login| login.pane == Some(pane))
}

#[must_use]
pub const fn service(provider: AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => "Claude",
        AgentProvider::Codex => "ChatGPT",
        AgentProvider::Antigravity => "Google",
    }
}

#[must_use]
pub fn failure_line(failure: &AgentLoginFailure, provider: AgentProvider) -> String {
    match failure {
        AgentLoginFailure::NotInstalled => format!("{} is not installed.", provider.label()),
        AgentLoginFailure::DeviceCodeDisabled => {
            "Device code sign-in is off for this account.".to_owned()
        }
        AgentLoginFailure::TimedOut => "The sign-in timed out.".to_owned(),
        AgentLoginFailure::Cli(text) => text.trim().to_owned(),
    }
}

#[must_use]
pub fn shown_url(url: &str) -> &str {
    url.split_once("://").map_or(url, |(_, rest)| rest)
}

#[must_use]
pub fn shown_site(url: &str) -> &str {
    let shown = shown_url(url);
    match shown.split_once('?') {
        Some((path, _)) => path.split('/').next().unwrap_or(path),
        None => shown,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardGlyph {
    Working,
    Failed,
    Waiting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardAction {
    UseCode,
    Terminal,
    Retry,
    Continue,
    SwitchWhenIdle,
    SwitchNow,
}

impl CardAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UseCode => "Use a code instead",
            Self::Terminal => "Sign in in a terminal",
            Self::Retry => "Retry",
            Self::Continue => "Continue",
            Self::SwitchWhenIdle => crate::agent_flip::WHEN_IDLE_LABEL,
            Self::SwitchNow => crate::agent_flip::NOW_LABEL,
        }
    }

    #[must_use]
    pub const fn primary(self) -> bool {
        matches!(self, Self::Retry | Self::Continue | Self::SwitchNow)
    }

    #[must_use]
    pub const fn leads(self) -> bool {
        matches!(self, Self::UseCode | Self::Terminal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardClose {
    Cancel,
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginCard {
    pub title: String,
    pub glyph: CardGlyph,
    pub detail: Option<String>,
    pub url: Option<String>,
    pub code: Option<String>,
    pub paste: bool,
    pub actions: Vec<CardAction>,
    pub close: CardClose,
}

pub const BUSY_DETAIL: &str = "what is running";

#[must_use]
pub fn card(login: &AgentLogin) -> Option<LoginCard> {
    let service = service(login.provider);
    let working = |title: String| LoginCard {
        title,
        glyph: CardGlyph::Working,
        detail: None,
        url: None,
        code: None,
        paste: false,
        actions: Vec::new(),
        close: CardClose::Cancel,
    };
    let sign_in = format!("Sign in to {service}");
    Some(match &login.step {
        AgentLoginStep::Starting | AgentLoginStep::Verifying => working(sign_in),
        AgentLoginStep::OpenUrl { url } => LoginCard {
            url: Some(url.clone()),
            actions: vec![CardAction::UseCode],
            ..working(format!("{sign_in} in your browser"))
        },
        AgentLoginStep::PasteCode { url } => LoginCard {
            url: Some(url.clone()),
            paste: true,
            actions: vec![CardAction::Continue],
            ..working(sign_in)
        },
        AgentLoginStep::DeviceCode { url, code } => LoginCard {
            url: Some(url.clone()),
            code: Some(code.clone()),
            ..working(sign_in)
        },
        AgentLoginStep::Failed(failure) => LoginCard {
            title: format!("Couldn\u{2019}t sign in to {service}"),
            glyph: CardGlyph::Failed,
            detail: Some(failure_line(failure, login.provider)),
            actions: vec![CardAction::Terminal, CardAction::Retry],
            close: CardClose::Dismiss,
            ..working(String::new())
        },
        AgentLoginStep::Done {
            email,
            pane: Some(AgentLoginPane::Busy),
            ..
        } if login.pane.is_some() => LoginCard {
            title: email.as_ref().map_or_else(
                || "Added the account".to_owned(),
                |email| format!("Added {email}"),
            ),
            glyph: CardGlyph::Waiting,
            detail: Some(crate::agent_flip::busy_detail(BUSY_DETAIL)),
            actions: vec![CardAction::SwitchWhenIdle, CardAction::SwitchNow],
            close: CardClose::Dismiss,
            ..working(String::new())
        },
        AgentLoginStep::Done { .. } | AgentLoginStep::Cancelled => return None,
    })
}

#[must_use]
pub fn settled(login: &AgentLogin) -> bool {
    match &login.step {
        AgentLoginStep::Cancelled => true,
        AgentLoginStep::Done { pane, .. } => {
            login.pane.is_none() || *pane != Some(AgentLoginPane::Busy)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANE: PaneId = PaneId(4);

    fn args(command: &CommandInvocation) -> Vec<String> {
        std::iter::once(command.name.clone())
            .chain(command.args.iter().map(|arg| arg.as_str().to_owned()))
            .collect()
    }

    #[test]
    fn each_entry_point_names_the_account_the_pane_and_where_the_browser_is() {
        let signs_in = vec![AGENT_LOGIN_CAPABILITY.to_owned()];
        let desktop_local = browser(true);
        let desktop_remote = browser(false);
        let thin = AgentLoginBrowser::Elsewhere;
        assert_eq!(
            args(&entry(
                AgentProvider::ClaudeCode,
                LoginAccount::New,
                PANE,
                desktop_local,
                &signs_in
            )),
            [
                "agent-accounts",
                "--add",
                "claude-code",
                "--pane",
                "%4",
                "--browser",
                "local"
            ]
        );
        assert_eq!(
            args(&entry(
                AgentProvider::Codex,
                LoginAccount::of(Some("work")),
                PANE,
                desktop_remote,
                &signs_in
            )),
            [
                "agent-accounts",
                "--add",
                "codex",
                "--account",
                "work",
                "--pane",
                "%4",
                "--browser",
                "elsewhere"
            ]
        );
        assert_eq!(
            args(&entry(
                AgentProvider::ClaudeCode,
                LoginAccount::of(None),
                PANE,
                thin,
                &signs_in
            )),
            [
                "agent-accounts",
                "--add",
                "claude-code",
                "--account",
                "default",
                "--pane",
                "%4",
                "--browser",
                "elsewhere"
            ]
        );
        assert_eq!(
            args(&entry(
                AgentProvider::ClaudeCode,
                LoginAccount::of(Some("work")),
                PANE,
                desktop_local,
                &[]
            )),
            ["agent-accounts", "--add", "claude-code"]
        );
    }

    fn login(step: AgentLoginStep) -> AgentLogin {
        AgentLogin {
            id: 9,
            provider: AgentProvider::Codex,
            account: Some("a2".to_owned()),
            pane: Some(PANE),
            browser: AgentLoginBrowser::Elsewhere,
            step,
        }
    }

    #[test]
    fn retry_and_the_terminal_keep_the_account_and_a_code_goes_only_into_its_own_message() {
        let failed = login(AgentLoginStep::Failed(AgentLoginFailure::TimedOut));
        let listed = [AgentAccount {
            provider: AgentProvider::Codex,
            id: Some("a2".to_owned()),
            signed_in: false,
            email: None,
            organization: None,
            plan: None,
        }];
        assert_eq!(
            args(&retry(&failed, &[], AgentLoginBrowser::Elsewhere)),
            [
                "agent-accounts",
                "--add",
                "codex",
                "--pane",
                "%4",
                "--browser",
                "elsewhere"
            ]
        );
        assert_eq!(
            args(&in_terminal(&failed, &listed)),
            [
                "agent-accounts",
                "--add",
                "codex",
                "--account",
                "a2",
                "--pane",
                "%4",
                "--browser",
                "elsewhere",
                "--terminal"
            ]
        );
        assert_eq!(submit_code(&failed, "abc"), None);
        let paste = login(AgentLoginStep::PasteCode {
            url: "https://claude.ai/oauth/authorize?code=true".to_owned(),
        });
        assert_eq!(
            submit_code(&paste, "  abc#def \n"),
            Some(ProtocolMessage::AgentLogin(AgentLoginRequest::Code {
                login: 9,
                code: AgentLoginCode::new("abc#def").expect("a code"),
            }))
        );
        assert_eq!(submit_code(&paste, "   "), None);
    }

    #[test]
    fn links_show_without_their_scheme_and_sites_without_a_query() {
        let claude = "https://claude.ai/oauth/authorize?code=true&client_id=9d1c25";
        assert_eq!(
            shown_url(claude),
            "claude.ai/oauth/authorize?code=true&client_id=9d1c25"
        );
        assert_eq!(shown_site(claude), "claude.ai");
        assert_eq!(
            shown_site("https://auth.openai.com/codex/device"),
            "auth.openai.com/codex/device"
        );
    }
}
