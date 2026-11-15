use std::{
    io::{self, BufRead as _},
    path::Path,
    process::ExitCode,
    sync::{Arc, mpsc},
    thread,
};

use zz_client::agent_login::{self, LoginAccount};
use zz_daemon_client::InteractiveClient;
use zz_protocol::{
    AGENT_LOGIN_CAPABILITY, AgentLogin, AgentLoginBrowser, AgentLoginStep, AgentProvider,
    CommandResponse, EventPayload, PaneId, ProtocolMessage, RawText,
};
use zz_terminal::TerminalColorScheme;

const USAGE: &str = "usage: zz tools --agent-login <claude|codex> [--browser local|elsewhere] [--account id|default] [--pane %N]\nThen type a pasted code, local or elsewhere to restart in that mode, or cancel (end of input cancels too).";

struct Start {
    provider: AgentProvider,
    browser: AgentLoginBrowser,
    account: Option<String>,
    pane: Option<PaneId>,
}

enum Note {
    Daemon(Result<Box<ProtocolMessage>, String>),
    Line(Option<String>),
}

pub(crate) fn run(socket_path: &Path, arguments: &[RawText]) -> ExitCode {
    let start = match parse(arguments) {
        Ok(start) => start,
        Err(message) => {
            eprintln!("zz tools --agent-login: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match drive(socket_path, &start) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("zz tools --agent-login: {message}");
            ExitCode::FAILURE
        }
    }
}

fn parse(arguments: &[RawText]) -> Result<Start, String> {
    let mut words = arguments.iter().map(RawText::as_str);
    let provider = words
        .next()
        .ok_or("name a provider")?
        .parse::<AgentProvider>()?;
    let mut start = Start {
        provider,
        browser: AgentLoginBrowser::Elsewhere,
        account: None,
        pane: None,
    };
    while let Some(flag) = words.next() {
        let value = words
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag {
            "--browser" => start.browser = value.parse()?,
            "--account" => start.account = Some(value.to_owned()),
            "--pane" => {
                start.pane = Some(
                    value
                        .parse()
                        .map_err(|_| format!("`{value}` is not a pane id"))?,
                );
            }
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(start)
}

fn drive(socket_path: &Path, start: &Start) -> Result<ExitCode, String> {
    let client = InteractiveClient::connect_with_capabilities(
        socket_path,
        TerminalColorScheme::Dark,
        false,
        &[AGENT_LOGIN_CAPABILITY],
    )
    .map_err(|error| error.to_string())?;
    if !agent_login::daemon_signs_in(&client.server_hello().capabilities) {
        return Err("this daemon does not drive agent sign-ins; restart it on this build".into());
    }
    let client = Arc::new(client);
    let (notes, received) = mpsc::channel();
    let reader = Arc::clone(&client);
    let daemon = notes.clone();
    thread::Builder::new()
        .name("zz-agent-login-daemon".to_owned())
        .spawn(move || {
            loop {
                let message = reader
                    .recv()
                    .map(Box::new)
                    .map_err(|error| error.to_string());
                let failed = message.is_err();
                if daemon.send(Note::Daemon(message)).is_err() || failed {
                    return;
                }
            }
        })
        .map_err(|error| error.to_string())?;
    thread::Builder::new()
        .name("zz-agent-login-input".to_owned())
        .spawn(move || {
            for line in io::stdin().lock().lines() {
                let Ok(line) = line else {
                    break;
                };
                if notes.send(Note::Line(Some(line))).is_err() {
                    return;
                }
            }
            let _ = notes.send(Note::Line(None));
        })
        .map_err(|error| error.to_string())?;
    let account = match start.account.as_deref() {
        None => LoginAccount::New,
        Some(zz_protocol::DEFAULT_AGENT_ACCOUNT) => LoginAccount::Default,
        Some(id) => LoginAccount::Named(id),
    };
    let request = client
        .execute(agent_login::start(
            start.provider,
            account,
            start.pane,
            start.browser,
        ))
        .map_err(|error| error.to_string())?;
    let mut login: Option<AgentLogin> = None;
    let mut cancel = false;
    let mut shown = None;
    for note in received {
        match note {
            Note::Daemon(Err(error)) => return Err(error),
            Note::Daemon(Ok(message)) => match *message {
                ProtocolMessage::CommandResponse(CommandResponse::Error {
                    request_id,
                    error,
                    ..
                }) if request_id == request || request_id == 0 => {
                    return Err(error.to_string());
                }
                ProtocolMessage::Event(zz_protocol::Event {
                    payload: EventPayload::AgentLogin(update),
                    ..
                }) if update.provider == start.provider => {
                    if shown.as_ref() != Some(&(update.step.clone(), update.browser)) {
                        println!("{}", describe(&update));
                        shown = Some((update.step.clone(), update.browser));
                    }
                    if update.finished() {
                        return Ok(match update.step {
                            AgentLoginStep::Done { .. } => ExitCode::SUCCESS,
                            _ => ExitCode::FAILURE,
                        });
                    }
                    if cancel && !matches!(update.step, AgentLoginStep::Starting) {
                        send(&client, &agent_login::cancel(&update))?;
                    }
                    login = Some(update);
                }
                _ => {}
            },
            Note::Line(line) => {
                let line = line.as_deref().map(str::trim);
                let Some(current) = login.as_ref() else {
                    cancel |= line.is_none_or(|line| line == "cancel");
                    continue;
                };
                match line {
                    None | Some("cancel") => send(&client, &agent_login::cancel(current))?,
                    Some("") => {}
                    Some(mode @ ("local" | "elsewhere")) => {
                        let browser = mode.parse().map_err(|error: String| error)?;
                        client
                            .execute(agent_login::restart(current, browser))
                            .map_err(|error| error.to_string())?;
                    }
                    Some(code) => match agent_login::submit_code(current, code) {
                        Some(message) => send(&client, &message)?,
                        None => eprintln!("not waiting for a code"),
                    },
                }
            }
        }
    }
    Err("the daemon closed the connection".into())
}

fn send(client: &InteractiveClient, message: &ProtocolMessage) -> Result<(), String> {
    client.send(message).map_err(|error| error.to_string())
}

fn describe(login: &AgentLogin) -> String {
    let account = login
        .account
        .as_deref()
        .unwrap_or(zz_protocol::DEFAULT_AGENT_ACCOUNT);
    let head = format!(
        "login {} {} account {account} browser {}",
        login.id,
        login.provider.as_str(),
        login.browser.as_str()
    );
    let step = match &login.step {
        AgentLoginStep::Starting => "starting".to_owned(),
        AgentLoginStep::OpenUrl { url } => format!("open {url} and wait"),
        AgentLoginStep::DeviceCode { url, code } => format!("open {url} and enter {code}"),
        AgentLoginStep::PasteCode { url } => format!("open {url} and paste the code here"),
        AgentLoginStep::Verifying => "verifying".to_owned(),
        AgentLoginStep::Done {
            email,
            organization,
            plan,
            pane,
        } => format!(
            "done: {} {} {} pane {pane:?}",
            email.as_deref().unwrap_or("-"),
            organization.as_deref().unwrap_or("-"),
            plan.as_deref().unwrap_or("-"),
        ),
        AgentLoginStep::Failed(failure) => format!("failed: {}", failure.message(login.provider)),
        AgentLoginStep::Cancelled => "cancelled".to_owned(),
    };
    format!("{head}: {step}")
}
