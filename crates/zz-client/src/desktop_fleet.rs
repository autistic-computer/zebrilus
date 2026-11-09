use zz_protocol::{MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatusState, SessionId, WindowId};
use crate::{navigation::{SessionLabel, session_label, ordered_panes, pane_index, pane_label}, program::Program, window_summary::pane_program, desktop_nav_tree::{NavNode, active_for_session}};

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
            label: pane_label(pane),
            cwd: pane.summary.cwd.clone(),
            kind,
            bell: pane.bell,
            status: pane.status.as_ref().map(|status| status.state),
            program: pane_program(pane),
        }
    }
}

#[must_use]
pub fn sessions<Host: Copy + Eq>(mut active_target: Option<NavNode<Host>>, host: Host, attached_host: Host, attached: Option<SessionId>, snapshot: Option<&MuxSnapshot>) -> (Vec<FleetSession>, Option<NavNode<Host>>) {
    let sessions = snapshot.into_iter().flat_map(|snapshot| &snapshot.sessions).map(|session| {
        let active = host == attached_host && Some(session.id) == attached;
        let focused = snapshot.map_or(session.active_window, |snapshot| snapshot.focused_window_for(session));
        active_target = active_for_session(active_target, host, attached_host, session.id, attached, focused, session.windows.iter().map(|window| (window.id, window.active_pane, window.panes.values().map(|pane| pane.id))));
        let windows = session.windows.iter().map(|window| FleetWindow {
            id: window.id, index: window.index, name: window.name.clone(), active_pane: window.active_pane,
            active: active && window.id == focused,
            panes: ordered_panes(window).into_iter().map(|pane| FleetPane { index: pane_index(window, pane.id), ..FleetPane::from_snapshot(pane) }).collect(),
        }).collect();
        FleetSession { id: session.id, name: session.name.clone(), label: session_label(session), active, windows }
    }).collect();
    (sessions, active_target)
}
