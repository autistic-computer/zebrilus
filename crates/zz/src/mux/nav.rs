use std::collections::BTreeSet;

use ala::{App, Entity, SharedString};
use zz_protocol::{
    Axis, CommandInvocation, MuxSnapshot, PaneId,
    SessionId, WindowId,
};

use super::{
    client::MuxClient,
    hosts::{HostId, HostState},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TreeTarget {
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TreeNode {
    Host(HostId),
    Target(HostId, TreeTarget),
}

impl TreeNode {
    #[must_use]
    pub fn tree_id(self) -> SharedString {
        match self {
            Self::Host(host) => format!("host:{host:?}").into(),
            Self::Target(host, target) => format!("host:{host:?}:{}", target.tree_id()).into(),
        }
    }

    #[must_use]
    pub const fn host(self) -> HostId {
        match self {
            Self::Host(host) | Self::Target(host, _) => host,
        }
    }
}

impl TreeTarget {
    #[must_use]
    pub fn tree_id(self) -> SharedString {
        match self {
            Self::Session(id) => format!("session:{id}").into(),
            Self::Window(id) => format!("window:{id}").into(),
            Self::Pane(id) => format!("pane:{id}").into(),
        }
    }
}

pub use zz_client::desktop_fleet::FleetPane as MuxTreePane;
pub use zz_client::desktop_fleet::{FleetPaneKind as MuxTreePaneKind, FleetSession as MuxTreeSession, FleetWindow as MuxTreeWindow};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MuxTreeModel {
    pub hosts: Vec<MuxTreeHost>,
    pub active_target: Option<TreeNode>,
}

impl MuxTreeModel {
    #[must_use]
    pub fn from_mux(mux: &MuxClient) -> Self {
        Self::from_hosts(
            mux.attached_host(),
            mux.attached_session(),
            mux.fleet_hosts(),
        )
    }

    #[must_use]
    pub fn from_hosts<'a>(
        attached_host: HostId,
        attached: Option<SessionId>,
        hosts: impl IntoIterator<Item = (HostId, &'a str, &'a HostState, Option<&'a MuxSnapshot>)>,
    ) -> Self {
        let mut active_target = None;
        let hosts = hosts
            .into_iter()
            .map(|(host, name, state, snapshot)| {
                let snapshot_loaded = snapshot.is_some();
                let (sessions, next_active) = zz_client::desktop_fleet::sessions(active_target.map(to_desktop_nav_node), host, attached_host, attached, snapshot);
                active_target = next_active.map(from_desktop_nav_node);
                MuxTreeHost {
                    id: host,
                    name: name.to_owned(),
                    state: state.clone(),
                    snapshot_loaded,
                    sessions,
                }
            })
            .collect();

        Self {
            hosts,
            active_target,
        }
    }

    #[must_use]
    pub fn is_expandable(&self, node: TreeNode) -> bool {
        match node {
            TreeNode::Host(host) => self.host(host).is_some(),
            TreeNode::Target(host, target) => self.host(host).is_some_and(|host| {
                zz_client::desktop_nav_tree::target_expandable(
                    desktop_nav_target(target),
                    host.sessions.iter().map(|session| {
                        (
                            session.id,
                            session
                                .windows
                                .iter()
                                .map(|window| (window.id, window.panes.iter().map(|pane| pane.id))),
                        )
                    }),
                )
            }),
        }
    }

    #[must_use]
    pub fn host(&self, id: HostId) -> Option<&MuxTreeHost> {
        self.hosts.iter().find(|host| host.id == id)
    }

    #[must_use]
    pub fn session_count(&self) -> usize {
        self.hosts.iter().map(|host| host.sessions.len()).sum()
    }

    fn target_owner(
        &self,
        host: HostId,
        target: TreeTarget,
    ) -> Option<zz_client::desktop_nav_tree::NavOwner> {
        zz_client::desktop_nav_tree::target_owner(
            desktop_nav_target(target),
            self.host(host)?.sessions.iter().flat_map(|session| {
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
    pub fn session_for_target(&self, host: HostId, target: TreeTarget) -> Option<SessionId> {
        self.host(host)?;
        match target {
            TreeTarget::Session(session) => Some(session),
            _ => self.target_owner(host, target).map(|owner| owner.session),
        }
    }

    fn window_for_target(&self, host: HostId, target: TreeTarget) -> Option<WindowId> {
        self.target_owner(host, target).map(|owner| owner.window)
    }

    #[must_use]
    pub fn renameable_name(&self, host: HostId, target: TreeTarget) -> Option<&str> {
        zz_client::desktop_nav_tree::renameable_name(
            desktop_nav_target(target),
            self.host(host)?.sessions.iter().map(|session| {
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
    pub fn rename_target_for_node(&self, node: TreeNode) -> Option<(HostId, TreeTarget)> {
        match node {
            TreeNode::Target(host, target @ (TreeTarget::Session(_) | TreeTarget::Window(_))) => {
                Some((host, target))
            }
            TreeNode::Target(host, target @ TreeTarget::Pane(_)) => self
                .target_owner(host, target)
                .map(|owner| (host, TreeTarget::Window(owner.window))),
            TreeNode::Host(_) => None,
        }
    }

    #[must_use]
    pub fn rename_activation_for_node(
        &self,
        node: TreeNode,
        attached_host: HostId,
    ) -> Option<(&'static str, NavActivation)> {
        let (host, target) = self.rename_target_for_node(node)?;
        if !self.host(host)?.connected() {
            return None;
        }
        let (label, command) = rename_prompt_command(target, self.renameable_name(host, target)?)?;
        let activation = if host == attached_host {
            NavActivation::Execute {
                host,
                commands: vec![command],
            }
        } else {
            NavActivation::AttachThenExecute {
                host,
                session: self.session_for_target(host, target)?,
                commands: vec![command],
            }
        };
        Some((label, activation))
    }

    #[must_use]
    pub fn has_pending_bell(&self, node: TreeNode) -> bool {
        let Some(host) = self.host(node.host()) else {
            return false;
        };
        match node {
            TreeNode::Host(_) => host.sessions.iter().any(MuxTreeSession::has_pending_bell),
            TreeNode::Target(_, TreeTarget::Session(id)) => host
                .sessions
                .iter()
                .find(|session| session.id == id)
                .is_some_and(MuxTreeSession::has_pending_bell),
            TreeNode::Target(_, TreeTarget::Window(id)) => host
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .find(|window| window.id == id)
                .is_some_and(MuxTreeWindow::has_pending_bell),
            TreeNode::Target(_, TreeTarget::Pane(id)) => host
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .flat_map(|window| &window.panes)
                .find(|pane| pane.id == id)
                .is_some_and(|pane| pane.bell),
        }
    }

    #[must_use]
    pub fn activation_for_node(
        &self,
        node: TreeNode,
        attached_host: HostId,
        attached_session: Option<SessionId>,
    ) -> Option<NavActivation> {
        let host = self.host(node.host())?;
        match node {
            TreeNode::Host(id) => zz_client::desktop_host::activation(id, attached_host, &host.state),
            TreeNode::Target(id, target) => activation_for_target(
                id,
                target,
                self.session_for_target(id, target),
                self.window_for_target(id, target),
                attached_host,
                attached_session,
                host.connected(),
            ),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn max_depth(&self) -> usize {
        if self.hosts.is_empty() {
            0
        } else if self
            .hosts
            .iter()
            .flat_map(|host| &host.sessions)
            .any(|session| {
                session
                    .windows
                    .iter()
                    .any(|window| !window.panes.is_empty())
            })
        {
            3
        } else if self
            .hosts
            .iter()
            .flat_map(|host| &host.sessions)
            .any(|session| !session.windows.is_empty())
        {
            2
        } else {
            usize::from(self.hosts.iter().any(|host| !host.sessions.is_empty()))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MuxTreeHost {
    pub id: HostId,
    pub name: String,
    pub state: HostState,
    pub snapshot_loaded: bool,
    pub sessions: Vec<MuxTreeSession>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostIndicator {
    Connecting,
    Failed { detail: Option<SharedString> },
}

impl MuxTreeHost {
    #[must_use]
    pub const fn node(&self) -> TreeNode {
        TreeNode::Host(self.id)
    }

    #[must_use]
    pub fn connected(&self) -> bool {
        self.state == HostState::Connected
    }

    #[must_use]
    pub fn indicator(&self) -> Option<HostIndicator> {
        zz_client::desktop_host::indicator(&self.state, self.snapshot_loaded).map(|indicator| match indicator {
            zz_client::desktop_host::HostIndicator::Connecting => HostIndicator::Connecting,
            zz_client::desktop_host::HostIndicator::Failed { detail } => HostIndicator::Failed { detail: detail.map(SharedString::from) },
        })
    }
}



#[derive(Clone, Debug)]
pub enum TreeNodeKind {
    Host,
    Session,
    Window,
    Pane { kind: MuxTreePaneKind },
}

#[must_use]
pub fn active_tree_target(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
) -> Option<TreeTarget> {
    let session = snapshot
        .sessions
        .iter()
        .find(|session| Some(session.id) == attached)?;
    let Some(window) = session
        .windows
        .iter()
        .find(|window| window.id == snapshot.focused_window_for(session))
    else {
        return Some(TreeTarget::Session(session.id));
    };
    if window.panes.contains_key(&window.active_pane) && window.layout.contains(window.active_pane)
    {
        Some(TreeTarget::Pane(window.active_pane))
    } else {
        Some(TreeTarget::Window(window.id))
    }
}



pub fn expand_path_to(expanded: &mut BTreeSet<TreeNode>, model: &MuxTreeModel, node: TreeNode) {
    let owner = match node {
        TreeNode::Target(host, target) => model.target_owner(host, target),
        TreeNode::Host(_) => None,
    };
    expanded.extend(
        zz_client::desktop_nav_tree::path_ancestors(
            to_desktop_nav_node(node),
            model.host(node.host()).is_some(),
            owner,
        )
        .into_iter()
        .map(from_desktop_nav_node),
    );
}

pub(crate) fn to_desktop_nav_node(node: TreeNode) -> zz_client::desktop_nav_tree::NavNode<HostId> {
    match node {
        TreeNode::Host(host) => zz_client::desktop_nav_tree::NavNode::Host(host),
        TreeNode::Target(host, target) => {
            zz_client::desktop_nav_tree::NavNode::Target(host, desktop_nav_target(target))
        }
    }
}

pub(crate) fn from_desktop_nav_node(
    node: zz_client::desktop_nav_tree::NavNode<HostId>,
) -> TreeNode {
    match node {
        zz_client::desktop_nav_tree::NavNode::Host(host) => TreeNode::Host(host),
        zz_client::desktop_nav_tree::NavNode::Target(host, target) => TreeNode::Target(
            host,
            match target {
                zz_client::desktop_nav::NavTarget::Session(session) => TreeTarget::Session(session),
                zz_client::desktop_nav::NavTarget::Window(window) => TreeTarget::Window(window),
                zz_client::desktop_nav::NavTarget::Pane(pane) => TreeTarget::Pane(pane),
            },
        ),
    }
}

pub type NavActivation = zz_client::desktop_nav::NavActivation<HostId>;

fn desktop_nav_target(target: TreeTarget) -> zz_client::desktop_nav::NavTarget {
    match target {
        TreeTarget::Session(session) => zz_client::desktop_nav::NavTarget::Session(session),
        TreeTarget::Window(window) => zz_client::desktop_nav::NavTarget::Window(window),
        TreeTarget::Pane(pane) => zz_client::desktop_nav::NavTarget::Pane(pane),
    }
}

#[must_use]
pub fn activation_for_target(
    host: HostId,
    target: TreeTarget,
    owner_session: Option<SessionId>,
    owner_window: Option<WindowId>,
    attached_host: HostId,
    attached_session: Option<SessionId>,
    connected: bool,
) -> Option<NavActivation> {
    zz_client::desktop_nav::activation_for_target(
        host,
        desktop_nav_target(target),
        owner_session,
        owner_window,
        attached_host,
        attached_session,
        connected,
    )
}

#[must_use]
pub fn pane_activation(mux: &MuxClient, host: HostId, pane: PaneId) -> Option<NavActivation> {
    let (session, window) = mux
        .fleet_hosts()
        .find(|(fleet_host, ..)| *fleet_host == host)
        .and_then(|(.., snapshot)| snapshot)?
        .sessions
        .iter()
        .find_map(|session| {
            session
                .windows
                .iter()
                .find(|window| window.panes.contains_key(&pane))
                .map(|window| (session.id, window.id))
        })?;
    activation_for_target(
        host,
        TreeTarget::Pane(pane),
        Some(session),
        Some(window),
        mux.attached_host(),
        mux.attached_session(),
        true,
    )
}

pub fn activate_nav(mux: &Entity<MuxClient>, activation: NavActivation, cx: &mut App) {
    mux.update(cx, |mux, cx| match activation {
        NavActivation::AttachHost(host) => {
            mux.attach_to_host_default(host, cx);
        }
        NavActivation::Attach { host, session } => {
            mux.attach_to_host(host, session, cx);
        }
        NavActivation::AttachAt { host, target } => {
            mux.attach_to_host_at(host, target, cx);
        }
        NavActivation::Execute { host, commands } => {
            for command in commands {
                mux.execute_on_host(host, command);
            }
        }
        NavActivation::AttachThenExecute {
            host,
            session,
            commands,
        } => {
            if mux.attach_to_host(host, session, cx) {
                for command in commands {
                    mux.execute_on_host(host, command);
                }
            }
        }
        NavActivation::Reconnect(host) => mux.install_or_retry_host(host, cx),
    });
}

#[must_use]
pub fn new_window_command(session: SessionId) -> CommandInvocation {
    zz_client::desktop_nav::new_window_command(session)
}

#[must_use]
pub fn select_window_command(window: WindowId) -> CommandInvocation {
    zz_client::desktop_nav::select_window_command(window)
}

#[must_use]
pub fn select_pane_commands(pane: PaneId) -> Vec<CommandInvocation> {
    zz_client::desktop_nav::select_pane_commands(pane)
}

#[must_use]
pub fn rename_prompt_command(
    target: TreeTarget,
    current_name: &str,
) -> Option<(&'static str, CommandInvocation)> {
    let target = match target {
        TreeTarget::Session(id) => zz_client::navigation::RenameTarget::Session(id),
        TreeTarget::Window(id) => zz_client::navigation::RenameTarget::Window(id),
        TreeTarget::Pane(_) => return None,
    };
    Some(zz_client::navigation::rename_prompt_command(
        target,
        current_name,
    ))
}

#[must_use]
pub fn kill_target_command(target: TreeTarget) -> CommandInvocation {
    zz_client::desktop_nav::kill_target_command(desktop_nav_target(target))
}

#[must_use]
pub fn picker_split_command(pane: PaneId, axis: Axis) -> CommandInvocation {
    zz_client::picker_split_command(pane, axis)
}
