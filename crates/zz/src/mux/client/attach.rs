use super::*;

impl MuxClient {
    pub fn new_session(&self, host: HostId) {
        let Some(connection) = self.connections.get(&host) else {
            log::warn!("cannot create a session on unknown fleet host {host:?}");
            return;
        };
        let capabilities = connection.client.as_ref().map_or(&[][..], |client| {
            client.server_hello().capabilities.as_slice()
        });
        #[cfg(test)]
        let capabilities = connection
            .fake_client
            .as_ref()
            .map_or(capabilities, |client| client.hello.capabilities.as_slice());
        let snapshot = if host == self.attached_host && !self.attached_snapshot_pending {
            Some(self.core.snapshot().as_ref())
        } else {
            connection.snapshot.as_deref()
        };
        let working_directory = (host == HostId::LOCAL
            && snapshot.is_some_and(|snapshot| snapshot.sessions.is_empty()))
        .then(std::env::current_dir)
        .transpose()
        .ok()
        .flatten()
        .filter(|directory| directory.is_dir());
        for command in new_session_commands(host, capabilities, working_directory.as_deref()) {
            self.execute_on_host(host, command);
        }
    }

    pub(crate) fn attach(&mut self, session: SessionId) {
        let client = self.attached_connection().client.as_ref().map(Arc::clone);
        #[cfg(test)]
        let fake_client = self
            .attached_connection()
            .fake_client
            .as_ref()
            .map(Arc::clone);
        #[cfg(not(test))]
        let result = client.map(|client| client.attach(session.to_string()));
        #[cfg(test)]
        let result = client
            .map(|client| client.attach(session.to_string()))
            .or_else(|| fake_client.map(|client| client.attach(Some(session))));
        match result {
            Some(Ok(())) => self.begin_client_focus_attach(),
            Some(Err(error)) => log::warn!("failed to attach mux session: {error}"),
            None => {}
        }
    }

    pub fn attach_to_host(
        &mut self,
        host: HostId,
        session: SessionId,
        cx: &mut Context<Self>,
    ) -> bool {
        self.attach_to_host_target(host, Some(session), None, cx)
    }

    pub fn attach_to_host_at(
        &mut self,
        host: HostId,
        target: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.attach_to_host_target(host, None, Some(target), cx)
    }

    pub fn attach_to_host_default(&mut self, host: HostId, cx: &mut Context<Self>) -> bool {
        self.attach_to_host_target(host, None, None, cx)
    }

    pub(crate) fn land(
        &mut self,
        host: HostId,
        landing: zz_client::new_chat::Landing,
        cx: &mut Context<Self>,
    ) -> bool {
        for command in landing.commands {
            self.execute_on_host(host, command);
        }
        self.attach_to_host_at(host, landing.attach, cx)
    }

    pub(crate) fn release_host(&mut self, host: HostId, cx: &mut Context<Self>) {
        if self.attached_host == host {
            if self.registry.get(HostId::LOCAL).is_some() {
                self.attach_to_host_default(HostId::LOCAL, cx);
            } else {
                let outgoing_snapshot = Arc::clone(self.core.snapshot());
                self.cancel_reconnect(host);
                self.attached_connection_mut().snapshot = Some(outgoing_snapshot);
                if let Some(client) = &self.attached_connection().client {
                    let _ = client.detach();
                }
                self.reset_session_state(cx);
                self.clear_cross_host_state();
                self.attached_host = HostId::LOCAL;
                self.error = None;
                self.error_after_next_attach = None;
                cx.notify();
            }
        }
    }

    pub(super) fn attach_to_host_target(
        &mut self,
        host: HostId,
        session: Option<SessionId>,
        target: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if host == self.attached_host {
            if let Some(target) = target {
                self.execute_on_host(
                    host,
                    CommandInvocation::new("attach-session", ["-t", &target]),
                );
            } else if let Some(session) = session {
                self.attach(session);
            }
            return true;
        }

        let requested_name = self.registry.get(host).map(|entry| entry.name.clone());
        self.reconcile_hosts(cx);
        let Some((host, name)) = requested_name
            .as_deref()
            .and_then(|name| self.registry.get_by_name(name))
            .map(|(host, entry)| (host, entry.name.clone()))
        else {
            log::warn!("cannot attach to unknown fleet host {host:?}");
            return false;
        };
        let Some(connection) = self.connections.get(&host) else {
            log::warn!("cannot attach to fleet host {name}: no connection slot");
            return false;
        };
        if !connection.is_connected() {
            log::warn!(
                "cannot attach to fleet host {name}: host is {}",
                connection.state.label(),
            );
            return false;
        }
        let client = connection.client.as_ref().map(Arc::clone);
        #[cfg(test)]
        let fake_client = connection.fake_client.as_ref().map(Arc::clone);
        let hello = connection
            .current_hello()
            .expect("a connected host has an interactive client");

        let outgoing_snapshot = Arc::clone(self.core.snapshot());
        self.cancel_reconnect(self.attached_host);
        self.attached_connection_mut().snapshot = Some(outgoing_snapshot);
        if let Some(current) = &self.attached_connection().client {
            let _ = current.detach();
        }
        self.reset_session_state(cx);

        self.clear_cross_host_state();

        self.error = None;
        self.error_after_next_attach = None;
        self.attached_host = host;
        self.report_color_scheme();
        self.ingest_server_hello(hello, cx);
        if let Some(target) = target {
            self.execute_on_host(
                host,
                CommandInvocation::new("attach-session", ["-t", &target]),
            );
            self.reconcile_hosts(cx);
            cx.notify();
            return true;
        }
        let attach_result = if let Some(client) = client {
            client.attach(session.map_or_else(String::new, |session| session.to_string()))
        } else {
            #[cfg(test)]
            {
                fake_client
                    .expect("a connected test host has a fake client")
                    .attach(session)
            }
            #[cfg(not(test))]
            unreachable!("a connected host has an interactive client");
        };
        match attach_result {
            Ok(()) => self.begin_client_focus_attach(),
            Err(error) => {
                self.reset_client_focus_attach();
                log::warn!("failed to attach mux session on fleet host {name}: {error}");
            }
        }
        self.reconcile_hosts(cx);
        cx.notify();
        true
    }

    pub(super) fn give_up_removed_attached_host(&mut self, name: &str, cx: &mut Context<Self>) {
        debug_assert_eq!(self.attached_host, HostId::LOCAL);
        self.reset_session_state(cx);
        self.clear_cross_host_state();
        let banner = Arc::<str>::from(format!("fleet host {name} disconnected"));
        self.error = Some(Arc::clone(&banner));
        self.error_after_next_attach = None;

        let connection = self.attached_connection();
        if !connection.is_connected() {
            return;
        }
        let client = connection.client.as_ref().map(Arc::clone);
        #[cfg(test)]
        let fake_client = connection.fake_client.as_ref().map(Arc::clone);
        let hello = connection
            .current_hello()
            .expect("a connected host has an interactive client");
        self.ingest_server_hello(hello, cx);
        let result = if let Some(client) = client {
            client.attach("")
        } else {
            #[cfg(test)]
            {
                fake_client
                    .expect("a connected test host has a fake client")
                    .attach(None)
            }
            #[cfg(not(test))]
            unreachable!("a connected host has an interactive client");
        };
        match result {
            Ok(()) => {
                self.begin_client_focus_attach();
                self.error_after_next_attach = Some(banner);
            }
            Err(error) => {
                self.reset_client_focus_attach();
                log::warn!("failed to fall back after removing fleet host {name}: {error}");
            }
        }
    }

    fn clear_cross_host_state(&mut self) {
        self.reset_client_focus_attach();
        self.core.clear_attachment();
        self.attached_snapshot_pending = true;
        self.viewports.clear();
        self.clear_all_kitty_images();
        self.browser_commands.clear();
        self.clear_agent_streams();
        self.screenshot_requests.clear();
        self.simulator_commands.clear();
        self.terminal_commands.clear();
        self.clear_all_pasted_images();
        self.pending_commands_revision = self.pending_commands_revision.wrapping_add(1).max(1);
        self.command_output = None;
        self.command_output_diff.invalidate();
        self.menu_revision = self.menu_revision.wrapping_add(1).max(1);
        self.confirm_revision = self.confirm_revision.wrapping_add(1).max(1);
    }

    pub(super) fn reset_session_state(&mut self, cx: &mut Context<Self>) {
        let key_table_changed = self.core.key_table().is_some();
        cx.emit(PathPickerUpdate::Close);
        self.reset_client_focus_attach();
        let connection = self.attached_connection_mut();
        connection.resync_pending = false;
        connection.full_requests_pending.clear();
        connection.history.clear();
        if self.next_prefix_cancel_request != 0 {
            self.prefix_cancelled_request = self
                .prefix_cancelled_request
                .max(Some(self.next_prefix_cancel_request));
        }
        self.core.reset_session();
        self.command_prompt_revision = self.command_prompt_revision.wrapping_add(1).max(1);
        self.command_output = None;
        self.choose_tree_revision = self.choose_tree_revision.wrapping_add(1).max(1);
        self.choose_tree_closed_revision = self.choose_tree_revision;
        self.choose_buffer_revision = self.choose_buffer_revision.wrapping_add(1).max(1);
        self.display_panes_revision = self.display_panes_revision.wrapping_add(1).max(1);
        self.menu_revision = self.menu_revision.wrapping_add(1).max(1);
        self.confirm_revision = self.confirm_revision.wrapping_add(1).max(1);
        self.clear_all_kitty_images();
        self.clear_all_pasted_images();
        for fetch in std::mem::take(&mut self.artifact_fetches).into_values() {
            fetch.request.respond(Err(
                "zz lost this page's daemon connection; reload to try again".to_owned(),
            ));
        }
        if key_table_changed {
            cx.emit(KeyTableChanged);
        }
    }

    pub(super) fn finish_attach(
        &mut self,
        host: HostId,
        session: SessionId,
        cx: &mut Context<Self>,
    ) {
        self.clear_all_kitty_images();
        self.clear_all_pasted_images();
        self.clear_agent_streams();
        self.attached_snapshot_pending = false;
        let connection = self.attached_connection_mut();
        connection.last_attached_session = Some(session);
        let reconnected = connection.reconnect_attach.take().is_some();
        connection.resync_pending = false;
        connection.full_requests_pending.clear();
        connection.history.clear();
        self.error = self.error_after_next_attach.take();
        self.client_focus_attach_ready = true;
        self.client_focus_attach_pending = false;
        self.client_focus_attach_recoverable = false;
        self.sent_client_window_focus = None;
        self.flush_client_window_focus();
        if reconnected {
            let name = self
                .registry
                .get(host)
                .map_or_else(|| format!("{host:?}"), |entry| entry.name.clone());
            Self::emit_keyed_notification(
                ClientMessageKind::Success,
                format!("reconnected to {name}"),
                Some(format!("connection-{name}")),
                cx,
            );
        }
    }
}
