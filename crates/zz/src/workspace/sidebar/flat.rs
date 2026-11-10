use std::{
    collections::BTreeSet,
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
    window_summary::{age_label, summarize_chat, summarize_chats},
};
use zz_protocol::{SessionId, WindowId, WindowSnapshot};
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

fn on_chat(mux: &MuxClient) -> Option<(HostId, WindowId)> {
    focused_window(mux)
        .filter(|(_, window)| summarize_chat(window).is_some())
        .map(|(host, window)| (host, window.id))
}

fn is_chat(mux: &MuxClient, (host, window): (HostId, WindowId)) -> bool {
    mux.host_snapshot(host).is_some_and(|snapshot| {
        snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .any(|candidate| candidate.id == window && summarize_chat(candidate).is_some())
    })
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
    now: (HostId, WindowId),
    known: &BTreeSet<(HostId, WindowId)>,
) -> bool {
    let moved = from.is_some_and(|from| from.0 == now.0 && from.1 != now.1);
    !moved && known.contains(&now)
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
    pub(super) fn render_flat_navigation(&self, sidebar: &Entity<Self>, cx: &App) -> AnyElement {
        let mux = self.mux.read(cx);
        let now = unix_now();
        let shortcuts = self.window_shortcut_keys(cx);
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
                    snapshot
                        .map(|snapshot| summarize_chats(snapshot.sessions.iter()))
                        .unwrap_or_default(),
                    |changed| age_label(changed, now),
                );
                host.folded = self.flat_view.folded_hosts.contains(&id);
                host.platform = mux.host_platform(id);
                for chat in &mut host.chats {
                    chat.shortcut = shortcuts
                        .iter()
                        .find(|(host, window_id, _)| {
                            (*host, *window_id) == (id, chat.summary.window)
                        })
                        .map(|(.., stroke)| stroke.clone());
                }
                host
            })
            .collect::<Vec<_>>();
        let state = FlatState {
            current: current_window(mux),
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
                self.flat_view.landing_from = on_chat(mux);
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
            if on_chat(mux).is_some() {
                return;
            }
            self.flat_view
                .last_chat
                .filter(|last| is_chat(mux, *last))
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
            self.flat_view.last_chat = on_chat(self.mux.read(cx));
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
        if self.flat_view.landing
            && let Some(chat) =
                on_chat(self.mux.read(cx)).filter(|chat| Some(*chat) != self.flat_view.landing_from)
        {
            if landing_follows(
                self.flat_view.landing_from,
                chat,
                &self.flat_view.landing_known,
            ) {
                self.flat_view.landing_from = Some(chat);
            } else {
                self.flat_view.landing = false;
                cx.emit(super::SidebarModeChanged);
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
    use super::*;

    #[test]
    fn landing_survives_a_machine_switch_and_ends_on_a_new_chat() {
        let local = (HostId::LOCAL, WindowId(4));
        let remote = HostId::from(PaletteHostId(1));
        let known = BTreeSet::from([local, (remote, WindowId(4)), (remote, WindowId(9))]);

        assert!(landing_follows(Some(local), (remote, WindowId(9)), &known));
        assert!(landing_follows(None, (remote, WindowId(9)), &known));
        assert!(!landing_follows(
            Some(local),
            (remote, WindowId(12)),
            &known
        ));
        assert!(!landing_follows(None, (remote, WindowId(12)), &known));
        assert!(!landing_follows(
            Some((remote, WindowId(9))),
            (remote, WindowId(4)),
            &known
        ));
    }
}
