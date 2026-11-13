use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use ala::{
    AnyElement, App, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use penna::{components::menu::PopupMenuItem, components::scroll::ScrollableElement as _};
use zz_client::{
    new_chat::leave_picker_command,
    sidebar::SidebarTarget,
    window_summary::{
        ChatHints, ChatMachine, ChatParent, ChatRow, chat_view_lists, number_chat_rows,
        summarize_chat, summarize_chat_machine,
    },
};
use zz_protocol::{MuxSnapshot, SessionId, WindowId, WindowSnapshot};
use zz_ui::{
    command::PaletteHostId,
    navigation::{
        WORKSPACE_SIDEBAR_FADE_HEIGHT,
        flat::{
            FlatEvent, FlatHandler, FlatHost, FlatMenu, FlatState, HostConnection, flat_sidebar,
        },
        workspace_sidebar_fade,
    },
};

use super::{WorkspaceSidebar, activate_sidebar};
use crate::mux::{
    client::MuxClient,
    hosts::{HostId, HostState},
    nav::{MuxTreeHost, TreeNode, TreeTarget, kill_target_command},
};

#[derive(Default)]
pub(super) struct FlatView {
    state: FlatState,
    folded_hosts: BTreeSet<HostId>,
    scroll: ScrollHandle,
    moving: Option<(HostId, WindowId, SessionId)>,
    last_chat: Option<(HostId, WindowId)>,
    landing: bool,
    landing_from: Option<(HostId, WindowId)>,
    landing_known: BTreeSet<(HostId, WindowId)>,
    entered: bool,
    hints: BTreeMap<HostId, ChatHints>,
    open: BTreeSet<(HostId, WindowId)>,
    piles: BTreeSet<HostId>,
    servers: BTreeMap<HostId, u64>,
}

fn connection(state: &HostState, loaded: bool) -> HostConnection {
    match state {
        HostState::Connected if loaded => HostConnection::Connected,
        HostState::Connected | HostState::Connecting | HostState::Installing => {
            HostConnection::Connecting
        }
        HostState::Reconnecting { .. } => HostConnection::Reconnecting,
        HostState::Disconnected => HostConnection::Disconnected,
        HostState::Unreachable { reason } => HostConnection::Unreachable(reason.clone().into()),
        HostState::Missing => HostConnection::Unreachable("zz is not installed".into()),
        HostState::ClientTooOld { .. } => {
            HostConnection::Unreachable("it runs a newer zz, update zz here".into())
        }
        HostState::DaemonTooOld { daemon, .. } => {
            HostConnection::Incompatible(format!("revision {daemon}").into())
        }
        HostState::Incompatible { remote, .. } => HostConnection::Incompatible(
            remote
                .map_or_else(
                    || "an older protocol".to_owned(),
                    |remote| format!("protocol v{remote}"),
                )
                .into(),
        ),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn current_window(mux: &MuxClient) -> Option<(PaletteHostId, WindowId)> {
    let host = mux.attached_host();
    let snapshot = mux.host_snapshot(host)?;
    let session = mux.attached_session()?;
    let session = snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session)?;
    Some((host.into(), snapshot.focused_window_for(session)))
}

fn focused_window(mux: &MuxClient) -> Option<(HostId, &WindowSnapshot)> {
    let (host, window) = current_window(mux)?;
    let host = HostId::from(host);
    mux.host_snapshot(host)?
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .find(|candidate| candidate.id == window)
        .map(|window| (host, window))
}

fn on_row(mux: &MuxClient) -> Option<(HostId, WindowId)> {
    focused_window(mux)
        .filter(|(_, window)| chat_view_lists(window))
        .map(|(host, window)| (host, window.id))
}

fn is_row(mux: &MuxClient, (host, window): (HostId, WindowId)) -> bool {
    mux.host_snapshot(host).is_some_and(|snapshot| {
        snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .any(|candidate| candidate.id == window && chat_view_lists(candidate))
    })
}

fn snapshots(mux: &MuxClient) -> impl Iterator<Item = (HostId, bool, &MuxSnapshot)> {
    mux.fleet_hosts().filter_map(|(host, _, state, snapshot)| {
        let snapshot = snapshot?;
        Some((
            host,
            connection(state, true) == HostConnection::Connected,
            snapshot,
        ))
    })
}

fn chat_machines<'a>(
    hosts: impl IntoIterator<Item = (HostId, bool, &'a MuxSnapshot)>,
    hints: &BTreeMap<HostId, ChatHints>,
    open: impl Fn(HostId, ChatParent) -> bool,
) -> Vec<(HostId, ChatMachine)> {
    let none = ChatHints::default();
    let mut machines = hosts
        .into_iter()
        .map(|(host, live, snapshot)| {
            let hints = hints.get(&host).unwrap_or(&none);
            (
                host,
                live,
                summarize_chat_machine(&snapshot.sessions, hints, |parent| open(host, parent)),
            )
        })
        .collect::<Vec<_>>();
    number_chat_rows(
        machines
            .iter_mut()
            .filter(|(_, live, _)| *live)
            .map(|(_, _, machine)| machine),
    );
    machines
        .into_iter()
        .map(|(host, _, machine)| (host, machine))
        .collect()
}

fn chat_shortcut_windows(machines: &[(HostId, ChatMachine)]) -> Vec<(HostId, WindowId)> {
    machines
        .iter()
        .flat_map(|(host, machine)| {
            machine.rows.iter().filter_map(move |row| match row {
                ChatRow::Window(row) => row.shortcut.map(|_| (*host, row.summary.window)),
                ChatRow::Pile(_) => None,
            })
        })
        .collect()
}

fn latest_chat(mux: &MuxClient, only: Option<HostId>) -> Option<(HostId, WindowId)> {
    mux.fleet_hosts()
        .filter(|(host, ..)| only.is_none_or(|only| only == *host))
        .filter_map(|(host, _, _, snapshot)| Some((host, snapshot?)))
        .flat_map(|(host, snapshot)| {
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .filter_map(move |window| summarize_chat(window).map(|chat| (host, chat)))
        })
        .max_by_key(|(_, chat)| chat.changed_at)
        .map(|(host, chat)| (host, chat.window))
}

fn known_windows(mux: &MuxClient) -> BTreeSet<(HostId, WindowId)> {
    mux.fleet_hosts()
        .filter_map(|(host, _, _, snapshot)| Some((host, snapshot?)))
        .flat_map(|(host, snapshot)| {
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .map(move |window| (host, window.id))
        })
        .collect()
}

fn landing_follows(
    from: Option<(HostId, WindowId)>,
    (host, window): (HostId, &WindowSnapshot),
    known: &BTreeSet<(HostId, WindowId)>,
) -> Option<bool> {
    let now = (host, window.id);
    if from == Some(now) || !chat_view_lists(window) {
        return None;
    }
    let moved = from.is_some_and(|from| from.0 == now.0 && from.1 != now.1);
    Some(!moved && known.contains(&now))
}

fn window_menu(sidebar: &Entity<WorkspaceSidebar>) -> FlatMenu {
    let sidebar = sidebar.clone();
    Rc::new(move |host, window, menu, _, cx| {
        let host = HostId::from(host);
        let this = sidebar.read(cx);
        let mux = this.mux.clone();
        let attached_host = mux.read(cx).attached_host();
        let connected = this
            .tree_model
            .host(host)
            .is_some_and(MuxTreeHost::connected);
        let rename = this.tree_model.rename_activation_for_node(
            TreeNode::Target(host, TreeTarget::Window(window)),
            attached_host,
        );
        let menu = match rename {
            Some((label, activation)) => {
                let mux = mux.clone();
                menu.item(
                    PopupMenuItem::new(label)
                        .on_click(move |_, _, cx| activate_sidebar(&mux, activation.clone(), cx)),
                )
            }
            None => menu,
        };
        menu.item(
            PopupMenuItem::new("Close window")
                .disabled(!connected)
                .on_click(move |_, _, cx| {
                    mux.read(cx)
                        .execute_on_host(host, kill_target_command(TreeTarget::Window(window)));
                }),
        )
    })
}

impl WorkspaceSidebar {
    pub(super) fn chat_shortcut_targets(&self, cx: &App) -> Vec<(HostId, WindowId)> {
        chat_shortcut_windows(&chat_machines(
            snapshots(self.mux.read(cx)),
            &self.flat_view.hints,
            |_, _| false,
        ))
    }

    pub(super) fn render_flat_navigation(&self, sidebar: &Entity<Self>, cx: &App) -> AnyElement {
        let mux = self.mux.read(cx);
        let now = unix_now();
        let view = &self.flat_view;
        let mut machines =
            chat_machines(snapshots(mux), &view.hints, |host, parent| match parent {
                ChatParent::Window(window) => view.open.contains(&(host, window)),
                ChatParent::Pile => view.piles.contains(&host),
            })
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let current = focused_window(mux).and_then(|(host, window)| {
            let at = machines
                .get(&host)?
                .current(window.id, window.active_pane)?;
            Some((PaletteHostId::from(host), at))
        });
        let hosts = mux
            .fleet_hosts()
            .map(|(id, name, state, snapshot)| {
                let mut host = FlatHost::new(
                    id.into(),
                    if id == HostId::LOCAL {
                        self.local_hostname.clone()
                    } else {
                        SharedString::from(name.to_owned())
                    },
                    id != HostId::LOCAL,
                    connection(state, snapshot.is_some()),
                    machines.remove(&id).unwrap_or_default(),
                    now,
                )
                .shortcuts(|position| self.shortcut_key(position, cx));
                host.folded = self.flat_view.folded_hosts.contains(&id);
                host.platform = mux.host_platform(id);
                host
            })
            .collect::<Vec<_>>();
        let state = FlatState {
            current,
            ..self.flat_view.state
        };
        let handler: FlatHandler = {
            let sidebar = sidebar.clone();
            Rc::new(move |event, window, cx| {
                sidebar.update(cx, |sidebar, cx| sidebar.on_flat_event(event, window, cx));
            })
        };
        let scroll = &self.flat_view.scroll;
        div()
            .relative()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .id("workspace-flat-sidebar")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(scroll)
                    .child(flat_sidebar(
                        &hosts,
                        state,
                        &handler,
                        &window_menu(sidebar),
                        self.content_ground(cx),
                        cx,
                    )),
            )
            .bottom_fade(
                scroll,
                workspace_sidebar_fade(self.ground(cx), cx),
                WORKSPACE_SIDEBAR_FADE_HEIGHT,
            )
            .vertical_scrollbar(scroll)
            .into_any_element()
    }

    fn on_flat_event(&mut self, event: FlatEvent, _: &mut Window, cx: &mut Context<Self>) {
        let (attached_host, attached) = {
            let mux = self.mux.read(cx);
            (mux.attached_host(), mux.attached_session())
        };
        let activate = |node: TreeNode, sidebar: &mut Self, cx: &mut Context<Self>| {
            if let Some(activation) =
                sidebar
                    .tree_model
                    .activation_for_node(node, attached_host, attached)
            {
                sidebar.dismiss_slideover(cx);
                activate_sidebar(&sidebar.mux, activation, cx);
            }
        };
        let leaving = match event {
            FlatEvent::Open(_, window) => Some(SidebarTarget::Window(window)),
            FlatEvent::OpenPane(_, _, pane) => Some(SidebarTarget::Pane(pane)),
            _ => None,
        };
        if let Some(target) = leaving {
            let mux = self.mux.read(cx);
            if let Some(command) = mux
                .host_snapshot(attached_host)
                .and_then(|snapshot| leave_picker_command(snapshot, attached, target))
            {
                mux.execute_on_host(attached_host, command);
            }
            self.flat_view.landing = false;
            cx.emit(super::SidebarModeChanged);
        }
        match event {
            FlatEvent::Open(host, window) => activate(
                TreeNode::Target(host.into(), TreeTarget::Window(window)),
                self,
                cx,
            ),
            FlatEvent::OpenPane(host, _, pane) => {
                activate(
                    TreeNode::Target(host.into(), TreeTarget::Pane(pane)),
                    self,
                    cx,
                );
            }
            FlatEvent::FoldHost(host) => {
                let host = HostId::from(host);
                if !self.flat_view.folded_hosts.remove(&host) {
                    self.flat_view.folded_hosts.insert(host);
                }
                cx.notify();
            }
            FlatEvent::ToggleWindow(host, window) => {
                let key = (HostId::from(host), window);
                if !self.flat_view.open.remove(&key) {
                    self.flat_view.open.insert(key);
                }
                cx.notify();
            }
            FlatEvent::TogglePile(host) => {
                let host = HostId::from(host);
                if !self.flat_view.piles.remove(&host) {
                    self.flat_view.piles.insert(host);
                }
                cx.notify();
            }
            FlatEvent::Connect(host) | FlatEvent::Upgrade(host) => {
                self.mux
                    .update(cx, |mux, cx| mux.install_or_retry_host(host.into(), cx));
            }
            FlatEvent::Hover(target, hovered) => {
                if self.flat_view.state.hover(target, hovered) {
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn new_chat(&mut self, host: HostId, cx: &mut Context<Self>) {
        let switch = {
            let mux = self.mux.read(cx);
            let switch = host != mux.attached_host();
            if switch && mux.host_snapshot(host).is_none() {
                return;
            }
            if !self.flat_view.landing {
                self.flat_view.landing_from = on_row(mux);
            }
            self.flat_view.landing_known = known_windows(mux);
            switch
        };
        if !self.flat_view.landing {
            self.flat_view.landing = true;
            cx.emit(super::SidebarModeChanged);
            cx.notify();
        }
        if switch {
            self.mux
                .update(cx, |mux, cx| mux.attach_to_host_default(host, cx));
        }
    }

    fn open_window(&mut self, host: HostId, window: WindowId, cx: &mut Context<Self>) {
        let (attached_host, attached) = {
            let mux = self.mux.read(cx);
            (mux.attached_host(), mux.attached_session())
        };
        if let Some(activation) = self.tree_model.activation_for_node(
            TreeNode::Target(host, TreeTarget::Window(window)),
            attached_host,
            attached,
        ) {
            activate_sidebar(&self.mux, activation, cx);
        }
    }

    fn enter_chat_view(&mut self, cx: &mut Context<Self>) {
        self.flat_view.entered = true;
        let target = {
            let mux = self.mux.read(cx);
            if on_row(mux).is_some() {
                return;
            }
            self.flat_view
                .last_chat
                .filter(|last| is_row(mux, *last))
                .or_else(|| latest_chat(mux, Some(mux.attached_host())))
                .or_else(|| latest_chat(mux, None))
        };
        if let Some((host, window)) = target {
            self.open_window(host, window, cx);
        } else {
            self.flat_view.landing = true;
            self.flat_view.landing_from = None;
            self.flat_view.landing_known = known_windows(self.mux.read(cx));
        }
    }

    fn leave_chat_view(&mut self, cx: &App) {
        if !self.flat_view.landing {
            self.flat_view.last_chat = on_row(self.mux.read(cx));
        }
        self.flat_view.landing = false;
    }

    pub(crate) const fn chat_landing(&self) -> bool {
        self.flat && self.flat_view.landing
    }

    pub(crate) fn follow_moved_window(&mut self, host: HostId, window: WindowId, from: SessionId) {
        self.flat_view.moving = Some((host, window, from));
    }

    pub(super) fn follow_chats(&mut self, cx: &mut Context<Self>) {
        if self.flat && !self.flat_view.entered {
            let loaded = {
                let mux = self.mux.read(cx);
                mux.attached_session().is_some()
                    && mux
                        .host_snapshot(mux.attached_host())
                        .is_some_and(|snapshot| !snapshot.sessions.is_empty())
            };
            if loaded {
                self.enter_chat_view(cx);
                cx.emit(super::SidebarModeChanged);
                cx.notify();
            }
        }
        self.forget_gone_windows(cx);
        if let Some((host, window)) = focused_window(self.mux.read(cx)) {
            let step = self.flat_view.landing.then(|| {
                landing_follows(
                    self.flat_view.landing_from,
                    (host, window),
                    &self.flat_view.landing_known,
                )
            });
            let id = window.id;
            self.flat_view
                .hints
                .entry(host)
                .or_default()
                .observe(window);
            match step.flatten() {
                Some(true) => self.flat_view.landing_from = Some((host, id)),
                Some(false) => {
                    self.flat_view.landing = false;
                    cx.emit(super::SidebarModeChanged);
                }
                None => {}
            }
        }
        let mux = self.mux.read(cx);
        let session_of = |host: HostId, window: WindowId| {
            mux.host_snapshot(host).and_then(|snapshot| {
                snapshot
                    .sessions
                    .iter()
                    .find(|session| {
                        session
                            .windows
                            .iter()
                            .any(|candidate| candidate.id == window)
                    })
                    .map(|session| session.id)
            })
        };
        let target = if let Some((host, window, from)) = self.flat_view.moving {
            match session_of(host, window) {
                Some(session) if session == from => None,
                Some(_) => Some((host, window)),
                None => {
                    self.flat_view.moving = None;
                    None
                }
            }
        } else {
            None
        };
        let Some((host, window)) = target else {
            return;
        };
        let (attached_host, attached) = (mux.attached_host(), mux.attached_session());
        self.flat_view.moving = None;
        if let Some(activation) = self.tree_model.activation_for_node(
            TreeNode::Target(host, TreeTarget::Window(window)),
            attached_host,
            attached,
        ) {
            activate_sidebar(&self.mux, activation, cx);
        }
    }

    fn forget_gone_windows(&mut self, cx: &App) {
        let mux = self.mux.read(cx);
        let view = &mut self.flat_view;
        for (host, _, _, snapshot) in mux.fleet_hosts() {
            if let Some(server) = mux.host_server_id(host)
                && view
                    .servers
                    .insert(host, server)
                    .is_some_and(|known| known != server)
            {
                view.hints.remove(&host);
                view.open.retain(|(open, _)| *open != host);
                view.piles.remove(&host);
            }
            let Some(snapshot) = snapshot else {
                continue;
            };
            let live = snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .map(|window| window.id)
                .collect::<BTreeSet<_>>();
            if let Some(hints) = view.hints.get_mut(&host) {
                hints.retain(|window| live.contains(&window));
            }
            view.open
                .retain(|(open, window)| *open != host || live.contains(window));
        }
    }

    pub(crate) const fn is_flat(&self) -> bool {
        self.flat
    }

    pub(crate) fn set_flat(&mut self, flat: bool, cx: &mut Context<Self>) {
        if self.flat != flat {
            self.toggle_flat(cx);
        }
    }

    pub(crate) const fn chat_mode(&self) -> bool {
        self.flat
    }

    pub(super) fn toggle_flat(&mut self, cx: &mut Context<Self>) {
        if self.flat {
            self.leave_chat_view(cx);
        }
        self.flat = !self.flat;
        let value = if self.flat { "true" } else { "false" };
        if let Err(error) =
            crate::config::set_config_key(crate::config::ConfigKey::FlatSidebar, value)
        {
            log::warn!(target: "zz::config", "could not save flat-sidebar: {error}");
        }
        let mut config = crate::config::resolved_config(cx);
        config.0.flat_sidebar.value = self.flat;
        cx.set_global(config);
        if self.flat {
            self.enter_chat_view(cx);
        }
        cx.emit(super::SidebarModeChanged);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use zz_protocol::{
        LayoutNode, PaneId, PaneKindSnapshot, PaneSnapshot, PaneSummary, SessionSnapshot,
    };

    use super::*;

    fn pane(id: u64, kind: PaneKindSnapshot, process: &str) -> PaneSnapshot {
        PaneSnapshot {
            id: PaneId(id),
            title: String::new(),
            kind,
            synchronized_input: false,
            bell: false,
            dead: false,
            dead_status: None,
            border_colour: None,
            active_border_colour: None,
            border_status_text: String::new(),
            mode: None,
            status: None,
            summary: PaneSummary {
                process: vec![process.to_owned()],
                ..PaneSummary::default()
            },
        }
    }

    fn window(id: u64, kind: PaneKindSnapshot, process: &str) -> WindowSnapshot {
        let pane = pane(id * 10, kind, process);
        WindowSnapshot {
            id: WindowId(id),
            index: 0,
            name: "zsh".to_owned(),
            automatic_rename: true,
            active_pane: pane.id,
            zoomed_pane: None,
            layout: LayoutNode::Pane(pane.id),
            panes: BTreeMap::from([(pane.id, pane)]),
            layout_dump: String::new(),
            visible_layout_dump: String::new(),
            status_label: String::new(),
            activity: false,
            silence: false,
            pane_border_status: zz_protocol::PaneBorderStatus::Off,
            pane_border_lines: zz_protocol::PaneBorderLines::Single,
            pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
            pane_order: Vec::new(),
            pane_z_order: Vec::new(),
            floating: Vec::new(),
            modal: None,
            sx: 80,
            sy: 24,
            pane_base_index: 0,
        }
    }

    fn chat(id: u64) -> WindowSnapshot {
        window(id, PaneKindSnapshot::Terminal, "claude")
    }

    fn shell(id: u64) -> WindowSnapshot {
        window(id, PaneKindSnapshot::Terminal, "zsh")
    }

    fn snapshot(sessions: Vec<Vec<WindowSnapshot>>) -> MuxSnapshot {
        MuxSnapshot {
            sessions: sessions
                .into_iter()
                .enumerate()
                .map(|(index, windows)| SessionSnapshot {
                    id: SessionId(index as u64),
                    name: index.to_string(),
                    active_window: windows[0].id,
                    windows,
                    viewers: Vec::new(),
                })
                .collect(),
            ..MuxSnapshot::default()
        }
    }

    #[test]
    fn landing_survives_a_machine_switch_and_ends_on_a_new_chat() {
        let local = (HostId::LOCAL, WindowId(4));
        let remote = HostId::from(PaletteHostId(1));
        let known = BTreeSet::from([local, (remote, WindowId(4)), (remote, WindowId(9))]);
        let (plain, fresh, picker) = (shell(9), chat(12), window(13, PaneKindSnapshot::Picker, ""));

        assert_eq!(
            landing_follows(Some(local), (remote, &plain), &known),
            Some(true)
        );
        assert_eq!(landing_follows(None, (remote, &plain), &known), Some(true));
        assert_eq!(
            landing_follows(Some(local), (remote, &fresh), &known),
            Some(false)
        );
        assert_eq!(landing_follows(None, (remote, &fresh), &known), Some(false));
        assert_eq!(
            landing_follows(Some((remote, WindowId(9))), (remote, &chat(4)), &known),
            Some(false)
        );
        assert_eq!(
            landing_follows(Some((remote, WindowId(9))), (remote, &plain), &known),
            None
        );
        assert_eq!(
            landing_follows(Some(local), (remote, &picker), &known),
            None
        );
    }

    #[test]
    fn shortcut_keys_in_chat_view_hit_the_chat_rows_across_machines() {
        let remote = HostId::from(PaletteHostId(1));
        let offline = HostId::from(PaletteHostId(2));
        let local = snapshot(vec![
            vec![shell(1), chat(2), shell(3)],
            vec![chat(4), window(5, PaneKindSnapshot::Picker, "")],
        ]);
        let studio = snapshot(vec![vec![shell(6), chat(7), chat(8)]]);
        let attic = snapshot(vec![vec![chat(9)]]);
        let machines = chat_machines(
            [
                (HostId::LOCAL, true, &local),
                (offline, false, &attic),
                (remote, true, &studio),
            ],
            &BTreeMap::new(),
            |_, _| false,
        );
        assert_eq!(
            chat_shortcut_windows(&machines),
            [
                (HostId::LOCAL, WindowId(2)),
                (HostId::LOCAL, WindowId(4)),
                (remote, WindowId(7)),
                (remote, WindowId(8)),
            ]
        );
    }
}
