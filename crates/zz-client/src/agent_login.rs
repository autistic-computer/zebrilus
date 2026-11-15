use zz_protocol::{
    AGENT_LOGIN_CAPABILITY, AgentLogin, AgentLoginBrowser, AgentLoginCode, AgentLoginRequest,
    AgentProvider, CommandInvocation, DEFAULT_AGENT_ACCOUNT, PaneId, ProtocolMessage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginAccount<'a> {
    New,
    Default,
    Named(&'a str),
}

#[must_use]
pub fn daemon_signs_in(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == AGENT_LOGIN_CAPABILITY)
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
    let account = login
        .account
        .as_deref()
        .map_or(LoginAccount::Default, LoginAccount::Named);
    start(login.provider, account, login.pane, browser)
}

#[must_use]
pub fn submit_code(login: &AgentLogin, code: &str) -> Option<ProtocolMessage> {
    if !matches!(login.step, zz_protocol::AgentLoginStep::PasteCode { .. }) {
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
