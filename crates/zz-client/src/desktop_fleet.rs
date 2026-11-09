use zz_protocol::{
    MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatusState, SessionId, WindowId,
};

use crate::{
    desktop_host::{self, HostState},
    desktop_nav::{self, NavActivation, NavTarget},
    desktop_nav_tree::{self, NavNode, NavOwner, active_for_session},
    navigation::{
        RenameTarget, SessionLabel, ordered_panes, pane_index, rename_prompt_command, session_label,
    },
    program::Program,
    window_summary::{pane_heading, pane_program},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FleetPaneKind {
    Picker,
    Terminal,
    Browser,
    Editor,
    Agent(zz_protocol::AgentProvider),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetSession {
    pub id: SessionId,
    pub name: String,
    pub label: SessionLabel,
    pub active: bool,
    pub windows: Vec<FleetWindow>,
}

impl FleetSession {
    pub fn has_pending_bell(&self) -> bool {
        self.windows.iter().any(FleetWindow::has_pending_bell)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetWindow {
    pub id: WindowId,
    pub index: u32,
    pub name: String,
    pub active_pane: PaneId,
    pub active: bool,
    pub panes: Vec<FleetPane>,
}

impl FleetWindow {
    #[must_use]
    pub fn label(&self) -> String {
        self.name.clone()
    }

    pub fn has_pending_bell(&self) -> bool {
        self.panes.iter().any(|pane| pane.bell)
    }

    #[must_use]
    pub fn program(&self) -> Option<&'static Program> {
        self.panes
            .iter()
            .find(|pane| pane.id == self.active_pane)
            .and_then(|pane| pane.program)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetPane {
    pub id: PaneId,
    pub index: u32,
    pub label: String,
    pub cwd: String,
    pub kind: FleetPaneKind,
    pub bell: bool,
    pub status: Option<PaneStatusState>,
    pub program: Option<&'static Program>,
}

impl FleetPane {
    #[must_use]
    pub fn from_snapshot(pane: &PaneSnapshot) -> Self {
        let kind = match pane.kind {
            PaneKindSnapshot::Picker => FleetPaneKind::Picker,
            PaneKindSnapshot::Terminal => FleetPaneKind::Terminal,
            PaneKindSnapshot::Browser(_) => FleetPaneKind::Browser,
            PaneKindSnapshot::Editor(_) => FleetPaneKind::Editor,
            PaneKindSnapshot::Agent(ref agent) => FleetPaneKind::Agent(agent.provider),
        };
        Self {
            id: pane.id,
            index: 0,
            label: pane_heading(pane).title,
            cwd: pane.summary.cwd.clone(),
            kind,
            bell: pane.bell,
            status: pane.status.as_ref().map(|status| status.state),
            program: pane_program(pane),
        }
    }
}

#[must_use]
pub fn sessions<Host: Copy + Eq>(
    mut active_target: Option<NavNode<Host>>,
    host: Host,
    attached_host: Host,
    attached: Option<SessionId>,
    snapshot: Option<&MuxSnapshot>,
) -> (Vec<FleetSession>, Option<NavNode<Host>>) {
    let sessions = snapshot
        .into_iter()
        .flat_map(|snapshot| &snapshot.sessions)
        .map(|session| {
            let active = host == attached_host && Some(session.id) == attached;
            let focused = snapshot.map_or(session.active_window, |snapshot| {
                snapshot.focused_window_for(session)
            });
            active_target = active_for_session(
                active_target,
                host,
                attached_host,
                session.id,
                attached,
                focused,
                session.windows.iter().map(|window| {
                    (
                        window.id,
                        window.active_pane,
                        window.panes.values().map(|pane| pane.id),
                    )
                }),
            );
            let windows = session
                .windows
                .iter()
                .map(|window| FleetWindow {
                    id: window.id,
                    index: window.index,
                    name: window.name.clone(),
                    active_pane: window.active_pane,
                    active: active && window.id == focused,
                    panes: ordered_panes(window)
                        .into_iter()
                        .map(|pane| FleetPane {
                            index: pane_index(window, pane.id),
                            ..FleetPane::from_snapshot(pane)
                        })
                        .collect(),
                })
                .collect();
            FleetSession {
                id: session.id,
                name: session.name.clone(),
                label: session_label(session),
                active,
                windows,
            }
        })
        .collect();
    (sessions, active_target)
}

#[must_use]
pub fn target_owner(sessions: &[FleetSession], target: NavTarget) -> Option<NavOwner> {
    desktop_nav_tree::target_owner(
        target,
        sessions.iter().flat_map(|session| {
            session.windows.iter().map(move |window| {
                (
                    session.id,
                    window.id,
                    window.panes.iter().map(|pane| pane.id),
                )
            })
        }),
    )
}

#[must_use]
pub fn session_for_target(sessions: &[FleetSession], target: NavTarget) -> Option<SessionId> {
    match target {
        NavTarget::Session(session) => Some(session),
        NavTarget::Window(_) | NavTarget::Pane(_) => {
            target_owner(sessions, target).map(|owner| owner.session)
        }
    }
}

#[must_use]
pub fn renameable_name(sessions: &[FleetSession], target: NavTarget) -> Option<&str> {
    desktop_nav_tree::renameable_name(
        target,
        sessions.iter().map(|session| {
            (
                session.id,
                session.name.as_str(),
                session
                    .windows
                    .iter()
                    .map(|window| (window.id, window.name.as_str())),
            )
        }),
    )
}

#[must_use]
pub fn expandable(sessions: &[FleetSession], target: NavTarget) -> bool {
    desktop_nav_tree::target_expandable(
        target,
        sessions.iter().map(|session| {
            (
                session.id,
                session
                    .windows
                    .iter()
                    .map(|window| (window.id, window.panes.iter().map(|pane| pane.id))),
            )
        }),
    )
}

#[must_use]
pub fn rename_target(sessions: &[FleetSession], target: NavTarget) -> Option<NavTarget> {
    match target {
        NavTarget::Session(_) | NavTarget::Window(_) => Some(target),
        NavTarget::Pane(_) => {
            target_owner(sessions, target).map(|owner| NavTarget::Window(owner.window))
        }
    }
}

#[must_use]
pub fn rename_activation<Host: Copy + Eq>(
    host: Host,
    state: &HostState,
    sessions: &[FleetSession],
    target: NavTarget,
    attached_host: Host,
) -> Option<(&'static str, NavActivation<Host>)> {
    let target = rename_target(sessions, target)?;
    if *state != HostState::Connected {
        return None;
    }
    let name = renameable_name(sessions, target)?;
    let (label, command) = match target {
        NavTarget::Session(id) => rename_prompt_command(RenameTarget::Session(id), name),
        NavTarget::Window(id) => rename_prompt_command(RenameTarget::Window(id), name),
        NavTarget::Pane(_) => return None,
    };
    let activation = if host == attached_host {
        NavActivation::Execute {
            host,
            commands: vec![command],
        }
    } else {
        NavActivation::AttachThenExecute {
            host,
            session: session_for_target(sessions, target)?,
            commands: vec![command],
        }
    };
    Some((label, activation))
}

#[must_use]
pub fn activation<Host: Copy + Eq>(
    node: NavNode<Host>,
    state: &HostState,
    sessions: &[FleetSession],
    attached_host: Host,
    attached_session: Option<SessionId>,
) -> Option<NavActivation<Host>> {
    match node {
        NavNode::Host(host) => desktop_host::activation(host, attached_host, state),
        NavNode::Target(host, target) => desktop_nav::activation_for_target(
            host,
            target,
            session_for_target(sessions, target),
            target_owner(sessions, target).map(|owner| owner.window),
            attached_host,
            attached_session,
            *state == HostState::Connected,
        ),
    }
}

#[must_use]
pub fn pending_bell(sessions: &[FleetSession], target: Option<NavTarget>) -> bool {
    match target {
        None => sessions.iter().any(FleetSession::has_pending_bell),
        Some(NavTarget::Session(id)) => sessions
            .iter()
            .find(|session| session.id == id)
            .is_some_and(FleetSession::has_pending_bell),
        Some(NavTarget::Window(id)) => sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|window| window.id == id)
            .is_some_and(FleetWindow::has_pending_bell),
        Some(NavTarget::Pane(id)) => sessions
            .iter()
            .flat_map(|session| &session.windows)
            .flat_map(|window| &window.panes)
            .find(|pane| pane.id == id)
            .is_some_and(|pane| pane.bell),
    }
}

pub fn shortcut_windows<'a, Host: Copy + 'a>(
    hosts: impl IntoIterator<Item = (Host, &'a HostState, &'a [FleetSession])>,
) -> impl Iterator<Item = (Host, WindowId)> {
    desktop_nav_tree::shortcut_windows(hosts.into_iter().map(|(host, state, sessions)| {
        (
            host,
            *state == HostState::Connected,
            sessions
                .iter()
                .map(|session| session.windows.iter().map(|window| window.id)),
        )
    }))
}
