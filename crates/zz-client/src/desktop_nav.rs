use std::path::Path;
use zz_protocol::{CommandInvocation, NEW_SESSION_ATTACH_CAPABILITY, PaneId, SessionId, WindowId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NavTarget {
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavActivation<Host> {
    AttachHost(Host),
    Attach {
        host: Host,
        session: SessionId,
    },
    AttachAt {
        host: Host,
        target: String,
    },
    Execute {
        host: Host,
        commands: Vec<CommandInvocation>,
    },
    AttachThenExecute {
        host: Host,
        session: SessionId,
        commands: Vec<CommandInvocation>,
    },
    Reconnect(Host),
}

#[must_use]
pub fn activation_for_target<Host: Copy + Eq>(
    host: Host,
    target: NavTarget,
    owner_session: Option<SessionId>,
    owner_window: Option<WindowId>,
    attached_host: Host,
    attached_session: Option<SessionId>,
    connected: bool,
) -> Option<NavActivation<Host>> {
    if !connected {
        return None;
    }
    if !matches!(target, NavTarget::Session(_))
        && let Some(session) = owner_session
        && (host != attached_host || Some(session) != attached_session)
    {
        let window = owner_window?;
        return Some(NavActivation::AttachAt {
            host,
            target: match target {
                NavTarget::Pane(pane) => format!("{session}:{window}.{pane}"),
                _ => format!("{session}:{window}"),
            },
        });
    }
    match target {
        NavTarget::Session(session) => Some(NavActivation::Attach { host, session }),
        NavTarget::Window(window) => Some(NavActivation::Execute {
            host,
            commands: vec![select_window_command(window)],
        }),
        NavTarget::Pane(pane) => Some(NavActivation::Execute {
            host,
            commands: select_pane_commands(pane),
        }),
    }
}

#[must_use]
pub fn new_window_command(session: SessionId) -> CommandInvocation {
    CommandInvocation::new("new-window", vec!["-t".to_owned(), session.to_string()])
}

#[must_use]
pub fn select_window_command(window: WindowId) -> CommandInvocation {
    CommandInvocation::new("select-window", vec!["-t".to_owned(), window.to_string()])
}

#[must_use]
pub fn select_pane_commands(pane: PaneId) -> Vec<CommandInvocation> {
    vec![
        CommandInvocation::new("select-window", ["-t".to_owned(), pane.to_string()]),
        CommandInvocation::new("select-pane", ["-t".to_owned(), pane.to_string()]),
    ]
}

#[must_use]
pub fn kill_target_command(target: NavTarget) -> CommandInvocation {
    match target {
        NavTarget::Session(session) => {
            CommandInvocation::new("kill-session", ["-t".to_owned(), session.to_string()])
        }
        NavTarget::Window(window) => {
            CommandInvocation::new("kill-window", ["-t".to_owned(), window.to_string()])
        }
        NavTarget::Pane(pane) => {
            CommandInvocation::new("kill-pane", ["-t".to_owned(), pane.to_string()])
        }
    }
}

#[must_use]
pub fn new_session_commands(
    local: bool,
    capabilities: &[String],
    working_directory: Option<&Path>,
) -> Vec<CommandInvocation> {
    if !local {
        return vec![CommandInvocation::new("new-session", ["-d"])];
    }
    let arguments = working_directory.map_or_else(Vec::new, |directory| {
        vec!["-c".to_owned(), directory.to_string_lossy().into_owned()]
    });
    let mut commands = vec![CommandInvocation::new("new-session", arguments)];
    if !capabilities
        .iter()
        .any(|capability| capability == NEW_SESSION_ATTACH_CAPABILITY)
    {
        commands.push(CommandInvocation::new("attach-session", [] as [&str; 0]));
    }
    commands
}

#[must_use]
pub fn detach_command() -> CommandInvocation {
    CommandInvocation::new("detach-client", [] as [&str; 0])
}
