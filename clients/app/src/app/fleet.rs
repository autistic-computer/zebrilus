use ala::{AppContext as _, Context, Entity, Window};
use zz_protocol::{PaneId, SessionId};

use super::{AppShell, compact, floating, sidebar, watch_connection};
use crate::connection::Connection;

impl AppShell {
    pub(super) fn reconnect_saved_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut attached = self.connection.read(cx).endpoint().map(ToOwned::to_owned);
        for endpoint in self.hosts.saved.connected().to_vec() {
            if attached.as_deref() == Some(endpoint.as_str()) {
                continue;
            }
            if attached.is_none() {
                attached = Some(endpoint.clone());
                self.connection
                    .update(cx, |connection, cx| connection.connect_to(endpoint, cx));
            } else {
                self.add_background_host(endpoint, window, cx);
            }
        }
    }

    pub(super) fn host_connection(
        &self,
        endpoint: &str,
        cx: &Context<Self>,
    ) -> Option<Entity<Connection>> {
        self.host_connections()
            .into_iter()
            .find(|connection| connection.read(cx).endpoint() == Some(endpoint))
    }

    pub(super) fn is_attached_host(&self, endpoint: &str, cx: &Context<Self>) -> bool {
        self.connection.read(cx).endpoint() == Some(endpoint)
    }

    pub(super) fn connect_host(
        &mut self,
        endpoint: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(connection) = self.host_connection(&endpoint, cx) {
            let idle = {
                let connection = connection.read(cx);
                !connection.connected && !connection.busy()
            };
            if idle {
                connection.update(cx, Connection::reconnect);
            }
        } else if self.connection.read(cx).endpoint().is_none() {
            self.hosts.saved.mark_connected(&endpoint, true);
            self.connection
                .update(cx, |connection, cx| connection.connect_to(endpoint, cx));
        } else {
            self.hosts.saved.mark_connected(&endpoint, false);
            self.add_background_host(endpoint, window, cx);
        }
        self.hosts.saved.save();
        cx.notify();
    }

    fn add_background_host(
        &mut self,
        endpoint: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let connection = cx.new(Connection::new);
        let focused = window.is_window_active();
        connection.update(cx, |connection, cx| {
            connection.set_focused(focused, cx);
            connection.connect_background(endpoint, cx);
        });
        let observer = cx.observe(&connection, |_, _, cx| cx.notify());
        self.fleet.push((connection, observer));
    }

    pub(super) fn disconnect_host(
        &mut self,
        endpoint: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hosts.saved.mark_disconnected(endpoint);
        self.hosts.saved.save();
        if self.is_attached_host(endpoint, cx) {
            let next = self
                .fleet
                .iter()
                .find(|(connection, _)| connection.read(cx).connected)
                .and_then(|(connection, _)| connection.read(cx).endpoint().map(ToOwned::to_owned));
            match next {
                Some(next) => {
                    self.switch_host(&next, None, None, window, cx);
                    self.fleet
                        .retain(|(connection, _)| connection.read(cx).endpoint() != Some(endpoint));
                }
                None => self.connection.update(cx, Connection::disconnect),
            }
        } else {
            self.fleet
                .retain(|(connection, _)| connection.read(cx).endpoint() != Some(endpoint));
        }
        cx.notify();
    }

    pub(super) fn switch_host(
        &mut self,
        endpoint: &str,
        session: Option<SessionId>,
        pane: Option<PaneId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_attached_host(endpoint, cx) {
            let Some(index) = self
                .fleet
                .iter()
                .position(|(connection, _)| connection.read(cx).endpoint() == Some(endpoint))
            else {
                return;
            };
            let (next, _) = self.fleet.remove(index);
            let previous = std::mem::replace(&mut self.connection, next);
            previous.update(cx, Connection::send_to_background);
            let observer = cx.observe(&previous, |_, _, cx| cx.notify());
            self.fleet.insert(index, (previous, observer));
            self.reset_host_state();
            self._connection_subscriptions = watch_connection(&self.connection, window, cx);
            let connection = self.connection.read(cx);
            self.connected = connection.connected;
            self.connection_status.clone_from(&connection.status);
            self.sync_idle_guard(cx);
            self.hosts.saved.mark_connected(endpoint, true);
            self.hosts.saved.save();
            self.preferences.apply(&self.connection, window, cx);
            self.connection
                .update(cx, |connection, cx| connection.bring_forward(session, cx));
        } else if let Some(session) = session
            && self.connection.read(cx).core.attached_session() != Some(session)
        {
            self.connection
                .update(cx, |connection, cx| connection.attach(session, cx));
        }
        if let Some(pane) = pane {
            let target = pane.to_string();
            self.command("select-window", vec!["-t".into(), target.clone()], cx);
            self.command("select-pane", vec!["-Z".into(), "-t".into(), target], cx);
        }
        cx.notify();
    }

    fn reset_host_state(&mut self) {
        self.terminals.clear();
        self.lenses.clear();
        self.browsers.clear();
        self.pickers.clear();
        self.waiting_panes.clear();
        self.unseen_agents.clear();
        self.collapsed_tree.clear();
        self.sidebar_selection = None;
        self.sidebar_active = None;
        self.flat = sidebar::Flat::default();
        self.menu_selection = None;
        self.focused_pane = None;
        self.output_terminal = None;
        self.set_split_drag(None);
        self.pane_drag = None;
        self.pane_layout_override = None;
        self.pip = floating::PipState::default();
        self.pane_bounds.borrow_mut().clear();
        self.pane_arrivals = zz_ui::pane::PaneArrivals::default();
        self.prompt_revision = self.prompt_revision.wrapping_add(1).max(1);
        self.chooser_revision = self.chooser_revision.wrapping_add(1).max(1);
        self.compact.reset_host();
    }
}

impl compact::CompactState {
    fn reset_host(&mut self) {
        self.reset_attachment();
        self.forget_panes();
    }
}
