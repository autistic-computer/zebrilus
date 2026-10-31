use std::{
    collections::BTreeSet,
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use ala::{
    AnyElement, App, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use penna::{menu::PopupMenuItem, scroll::ScrollableElement as _};
use zz_client::{
    new_chat::open_picker_commands,
    window_summary::{age_label, summarize_chats},
};
use zz_protocol::{PaneKindSnapshot, SessionId, WindowId};
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
    awaiting: Option<(HostId, BTreeSet<WindowId>)>,
    moving: Option<(HostId, WindowId, SessionId)>,
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
            FlatEvent::NewChat(host) => self.new_chat(host.into(), cx),
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

    fn new_chat(&mut self, host: HostId, cx: &mut Context<Self>) {
        let mux = self.mux.read(cx);
        let Some(snapshot) = mux.host_snapshot(host) else {
            return;
        };
        let attached = (host == mux.attached_host())
            .then(|| mux.attached_session())
            .flatten();
        let known = snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .map(|window| window.id)
            .collect();
        for command in open_picker_commands(snapshot, attached) {
            mux.execute_on_host(host, command);
        }
        self.flat_view.awaiting = Some((host, known));
    }

    pub(crate) fn follow_moved_window(&mut self, host: HostId, window: WindowId, from: SessionId) {
        self.flat_view.moving = Some((host, window, from));
    }

    pub(super) fn follow_chats(&mut self, cx: &mut Context<Self>) {
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
        let target = if let Some((host, known)) = &self.flat_view.awaiting {
            mux.host_snapshot(*host)
                .and_then(|snapshot| {
                    snapshot
                        .sessions
                        .iter()
                        .flat_map(|session| &session.windows)
                        .find(|window| {
                            !known.contains(&window.id)
                                && !window.panes.is_empty()
                                && window
                                    .panes
                                    .values()
                                    .all(|pane| matches!(pane.kind, PaneKindSnapshot::Picker))
                        })
                })
                .map(|window| (*host, window.id))
        } else if let Some((host, window, from)) = self.flat_view.moving {
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
        self.flat_view.awaiting = None;
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
        cx.emit(super::SidebarModeChanged);
        cx.notify();
    }
}
