use super::*;

impl MuxClient {
    pub(super) fn finish_initial_connection(
        &mut self,
        result: Result<InteractiveClient, DaemonError>,
        cx: &mut Context<Self>,
    ) {
        if self.shutting_down
            || !self
                .connections
                .get(&HostId::LOCAL)
                .is_some_and(|connection| connection.state == HostState::Connecting)
        {
            return;
        }
        if self.attached_host != HostId::LOCAL {
            self.handle_connect_result(HostId::LOCAL, result, cx);
            self.ensure_all_connected(cx);
            cx.emit(InitialConnectionFinished);
            return;
        }
        match result {
            Ok(client) => {
                if let Err(error) = self.install_initial_local_connection(client, cx) {
                    self.connections.get_mut(&HostId::LOCAL).unwrap().state =
                        HostState::Unreachable {
                            reason: error.clone(),
                        };
                    self.error = Some(Arc::from(error));
                } else {
                    self.error = None;
                    self.ensure_all_connected(cx);
                }
            }
            Err(error) => {
                self.stale_daemon = StaleDaemonInfo::from_error(&error);
                self.connections.get_mut(&HostId::LOCAL).unwrap().state =
                    connect_error_state(&error);
                self.error = Some(Arc::from(local_connect_error_text(&error)));
            }
        }
        cx.notify();
        cx.emit(InitialConnectionFinished);
    }

    pub(super) fn install_initial_local_connection(
        &mut self,
        client: InteractiveClient,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.ingest_server_hello(client.server_hello().clone(), cx);
        let client = Arc::new(client);
        crate::config::register_config_override_client(&client, cx);
        if !client.is_initially_attached() {
            match std::env::current_dir()
                .ok()
                .filter(|directory| directory.is_dir())
            {
                Some(directory) => client.attach_default_in(&directory),
                None => client.attach(""),
            }
            .map_err(|error| error.to_string())?;
        }
        let connection = HostConnection::connected(client, HostId::LOCAL, cx)
            .map_err(|error| error.to_string())?;
        self.connections.insert(HostId::LOCAL, connection);
        self.report_color_scheme();
        self.begin_client_focus_attach();
        Ok(())
    }

    pub(super) fn reconcile_hosts(&mut self, cx: &mut Context<Self>) {
        let configured = crate::config::known_hosts(cx);
        let has_retained = self
            .registry
            .iter()
            .any(|(host, _)| self.registry.is_retained(host));
        if self.registry.configured() == configured.as_slice()
            && (!has_retained || self.registry.is_retained(self.attached_host))
        {
            return;
        }

        let old_attached_host = self.attached_host;
        let attached_entry = self.registry.get(old_attached_host).cloned();
        let local_socket_path = self.registry.local_socket_path().to_path_buf();
        let mut next_registry = HostRegistry::new(
            local_socket_path,
            &configured,
            crate::profile::profile(cx).local_host,
        );
        let attached_remote_vanished = old_attached_host != HostId::LOCAL
            && attached_entry
                .as_ref()
                .is_some_and(|attached| !configured.iter().any(|host| host.name == attached.name));
        let removed_active_reconnect = attached_remote_vanished
            && self
                .connections
                .get(&old_attached_host)
                .is_some_and(HostConnection::reconnect_active);
        if attached_remote_vanished
            && !removed_active_reconnect
            && let Some(attached) = &attached_entry
        {
            log::warn!(
                "attached fleet host {} was removed from configuration; keeping its live connection until detach",
                attached.name,
            );
            next_registry.push_retained(attached.clone());
        }

        let old_registry = std::mem::replace(&mut self.registry, next_registry);
        let mut old_connections = std::mem::take(&mut self.connections);
        let mut next_connections = HashMap::with_capacity(self.registry.iter().count());
        for (host, entry) in self.registry.iter() {
            let connection = if host == HostId::LOCAL {
                old_connections
                    .remove(&HostId::LOCAL)
                    .unwrap_or_else(|| HostConnection::disconnected(host))
            } else if let Some((old_host, old_entry)) = old_registry.get_by_name(&entry.name)
                && old_entry == entry
            {
                old_connections
                    .remove(&old_host)
                    .unwrap_or_else(|| HostConnection::disconnected(host))
            } else {
                HostConnection::disconnected(host)
            };
            connection.reroute(host);
            next_connections.insert(host, connection);
        }
        if self.registry.get(HostId::LOCAL).is_none() {
            let connection = old_connections
                .remove(&HostId::LOCAL)
                .unwrap_or_else(|| HostConnection::disconnected(HostId::LOCAL));
            next_connections.insert(HostId::LOCAL, connection);
        }
        for connection in old_connections.values_mut() {
            connection.clear_route();
        }
        self.connections = next_connections;
        self.attached_host = if old_attached_host == HostId::LOCAL {
            HostId::LOCAL
        } else {
            attached_entry
                .as_ref()
                .and_then(|attached| self.registry.get_by_name(&attached.name))
                .map_or(HostId::LOCAL, |(host, _)| host)
        };
        if removed_active_reconnect {
            self.give_up_removed_attached_host(
                attached_entry.as_ref().map_or("remote", |host| &host.name),
                cx,
            );
        }
        cx.notify();
        self.ensure_all_connected(cx);
    }

    pub(crate) fn host_states<'a>(
        &'a mut self,
        cx: &mut Context<Self>,
    ) -> impl Iterator<Item = (HostId, &'a str, &'a HostState)> + 'a {
        self.reconcile_hosts(cx);
        let connections = &self.connections;
        self.registry.iter().map(move |(host, entry)| {
            let state = &connections
                .get(&host)
                .expect("every registered host has a connection slot")
                .state;
            (host, entry.name.as_str(), state)
        })
    }

    #[must_use]
    pub fn first_host(&self) -> Option<HostId> {
        self.registry.iter().next().map(|(host, _)| host)
    }

    #[must_use]
    pub fn has_hosts(&self) -> bool {
        self.first_host().is_some()
    }

    pub fn fleet_hosts(
        &self,
    ) -> impl Iterator<Item = (HostId, &str, &HostState, Option<&MuxSnapshot>)> {
        self.registry.iter().map(move |(host, entry)| {
            let connection = self
                .connections
                .get(&host)
                .expect("every registered host has a connection slot");
            (
                host,
                entry.name.as_str(),
                &connection.state,
                self.host_snapshot(host).map(Arc::as_ref),
            )
        })
    }

    pub fn host_platform(&self, host: HostId) -> zz_protocol::HostPlatform {
        let reported = self
            .connections
            .get(&host)
            .map_or_default(|connection| connection.platform);
        if reported == zz_protocol::HostPlatform::Unknown
            && let Some(Endpoint::Wsl(wsl)) = self.registry.get(host).map(|entry| &entry.endpoint)
        {
            let lower = wsl.distro.to_ascii_lowercase();
            let distro = if lower.contains("ubuntu") {
                zz_protocol::LinuxDistro::Ubuntu
            } else if lower.contains("debian") {
                zz_protocol::LinuxDistro::Debian
            } else if lower.contains("arch") {
                zz_protocol::LinuxDistro::Arch
            } else if lower.contains("fedora") {
                zz_protocol::LinuxDistro::Fedora
            } else {
                zz_protocol::LinuxDistro::Other
            };
            return zz_protocol::HostPlatform::Linux(distro);
        }
        reported
    }

    pub(crate) fn host_snapshot(&self, host: HostId) -> Option<&Arc<MuxSnapshot>> {
        if host == self.attached_host && !self.attached_snapshot_pending {
            Some(self.core.snapshot())
        } else {
            self.connections
                .get(&host)
                .and_then(|connection| connection.snapshot.as_ref())
        }
    }

    pub(crate) fn ensure_connected(&mut self, host: HostId, cx: &mut Context<Self>) {
        let requested_name = self.registry.get(host).map(|entry| entry.name.clone());
        self.reconcile_hosts(cx);
        let Some((host, endpoint)) = requested_name
            .as_deref()
            .and_then(|name| self.registry.get_by_name(name))
            .map(|(host, entry)| (host, entry.endpoint.clone()))
        else {
            log::warn!("cannot connect unknown fleet host {host:?}");
            return;
        };
        let connection = self
            .connections
            .get_mut(&host)
            .expect("every registered host has a connection slot");
        if matches!(
            connection.state,
            HostState::Connecting | HostState::Connected | HostState::Installing
        ) {
            return;
        }
        if connection.ssh_auth_declined {
            return;
        }
        let reconnect_attempt = match connection.state {
            HostState::Reconnecting { attempt } => {
                connection.bump_reconnect_generation();
                Some(attempt)
            }
            _ => None,
        };
        connection.client = None;
        connection.color_scheme_sent = None;
        connection.reader_thread = None;
        connection.resync_pending = false;
        connection.full_requests_pending.clear();
        connection.history.clear();
        if host != HostId::LOCAL {
            connection.snapshot = None;
        }
        connection.state = HostState::Connecting;
        connection.reconnect_attempt_in_flight = reconnect_attempt;
        connection.reconnect_attach = None;
        let result_route = Arc::clone(&connection.route);
        cx.notify();
        if cfg!(test) && !matches!(endpoint, Endpoint::Local(_)) {
            return;
        }

        let color_scheme = self.color_scheme;
        let prompts = self.ssh_prompts(host, &endpoint, cx);
        let (results, incoming) = async_channel::bounded(1);
        let connect_thread = thread::Builder::new()
            .name("zz-host-connect".to_owned())
            .spawn(move || {
                let result = InteractiveClient::connect_endpoint_with_prompts_and_capabilities(
                    &endpoint,
                    color_scheme,
                    prompts,
                    &[
                        zz_protocol::ClientHello::CLIENT_PATH_PICKER_CAPABILITY,
                        zz_protocol::AGENT_LOGIN_CAPABILITY,
                    ],
                );
                let _ = results.send_blocking(result);
            });
        if let Err(error) = connect_thread {
            self.handle_connect_result(
                host,
                Err(DaemonError::Thread(format!(
                    "failed to start host connection thread: {error}"
                ))),
                cx,
            );
            return;
        }
        cx.spawn(async move |this, cx| {
            let Ok(result) = incoming.recv().await else {
                return;
            };
            let _ = this.update(cx, |client, cx| {
                if let Some(host) = *result_route.read() {
                    client.handle_connect_result(host, result, cx);
                }
            });
        })
        .detach();
    }

    pub fn retry_host_now(&mut self, host: HostId, cx: &mut Context<Self>) {
        self.auto_installed.remove(&host);
        if let Some(connection) = self.connections.get_mut(&host) {
            connection.ssh_auth_declined = false;
            if matches!(connection.state, HostState::Unreachable { .. }) {
                connection.state = HostState::Reconnecting { attempt: 1 };
            }
        }
        self.ensure_connected(host, cx);
    }

    pub(crate) fn install_or_retry_host(&mut self, host: HostId, cx: &mut Context<Self>) {
        if self
            .connections
            .get(&host)
            .is_some_and(|connection| connection.state == HostState::Missing)
        {
            self.install_zz_on_host(host, cx);
        } else {
            self.retry_host_now(host, cx);
        }
    }

    pub(crate) fn note_ssh_auth_declined(&mut self, host: HostId, cx: &mut Context<Self>) {
        let Some(connection) = self.connections.get_mut(&host) else {
            return;
        };
        connection.ssh_auth_declined = true;
        cx.notify();
    }

    pub(super) fn ssh_prompts(
        &self,
        host: HostId,
        endpoint: &Endpoint,
        cx: &mut Context<Self>,
    ) -> Option<zz_daemon_client::SshPrompts> {
        let Endpoint::Ssh(ssh) = endpoint else {
            return None;
        };
        let helper = std::env::current_exe().ok()?;
        let label = ssh_destination_label(ssh);
        let route = Arc::clone(&self.connections.get(&host)?.route);

        let (requests, incoming) =
            async_channel::bounded::<(AskpassPrompt, async_channel::Sender<AskpassReply>)>(1);
        cx.spawn(async move |this, cx| {
            while let Ok((prompt, reply)) = incoming.recv().await {
                let emitted = this.update(cx, |client, cx| {
                    let Some(host) = *route.read() else {
                        return false;
                    };
                    if let Some(connection) = client.connections.get_mut(&host) {
                        connection.asked_to_sign_in = prompt.kind() != AskpassPromptKind::KeyAdded;
                    }
                    cx.emit(SshPromptRequest {
                        host,
                        label: label.clone(),
                        prompt,
                        reply,
                    });
                    true
                });
                if !matches!(emitted, Ok(true)) {
                    break;
                }
            }
        })
        .detach();

        Some(zz_daemon_client::SshPrompts::new(
            helper,
            move |prompt: &AskpassPrompt| {
                let (reply, answers) = async_channel::bounded(1);
                if requests.send_blocking((prompt.clone(), reply)).is_err()
                    || prompt.kind().is_notice()
                {
                    return AskpassReply::Cancel;
                }
                answers.recv_blocking().unwrap_or(AskpassReply::Cancel)
            },
        ))
    }

    pub(crate) fn ensure_all_connected(&mut self, cx: &mut Context<Self>) {
        let hosts = self
            .host_states(cx)
            .filter_map(|(host, _, state)| {
                (host != HostId::LOCAL && !matches!(state, HostState::Reconnecting { .. }))
                    .then_some(host)
            })
            .collect::<Vec<_>>();
        for host in hosts {
            self.ensure_connected(host, cx);
        }
    }

    fn arm_reconnect(&mut self, host: HostId, attempt: u32, cx: &mut Context<Self>) {
        let Some(connection) = self.connections.get_mut(&host) else {
            return;
        };
        if connection.state != (HostState::Reconnecting { attempt }) {
            return;
        }

        let generation = connection.bump_reconnect_generation();
        let route = Arc::clone(&connection.route);
        let delay = reconnect_delay(attempt);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |client, cx| {
                let routed_host = *route.read();
                let Some(host) = routed_host else {
                    return;
                };
                let Some(connection) = client.connections.get(&host) else {
                    return;
                };
                if !reconnect_timer_is_current(connection, generation, attempt) {
                    return;
                }
                client.ensure_connected(host, cx);
            });
        })
        .detach();
    }

    pub(super) fn cancel_reconnect(&mut self, host: HostId) {
        let Some(connection) = self.connections.get_mut(&host) else {
            return;
        };
        if !connection.reconnect_active() {
            return;
        }
        connection.bump_reconnect_generation();
        connection.reconnect_attempt_in_flight = None;
        connection.reconnect_attach = None;
        if !connection.is_connected() {
            connection.state = HostState::Unreachable {
                reason: "connection lost".to_owned(),
            };
        }
    }

    pub(super) fn handle_connect_result(
        &mut self,
        host: HostId,
        result: Result<InteractiveClient, DaemonError>,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self.connections.get(&host) else {
            log::debug!("discarding connection result for removed fleet host {host:?}");
            return;
        };
        if connection.state != HostState::Connecting {
            log::debug!("discarding stale connection result for fleet host {host:?}");
            return;
        }

        let route = Arc::clone(&connection.route);
        let last_attached_session = connection.last_attached_session;
        let snapshot = connection.snapshot.as_ref().map(Arc::clone);
        let reconnect_generation = connection.reconnect_generation;
        let reconnect_attempt = connection.reconnect_attempt_in_flight;
        let asked_to_sign_in = connection.asked_to_sign_in;
        match result {
            Ok(client) => {
                if let Err(error) = client.request_resync() {
                    log::warn!("failed to request initial fleet snapshot: {error}");
                }
                match HostConnection::connected_with_route(Arc::new(client), Arc::clone(&route), cx)
                {
                    Ok(mut connection) => {
                        connection.last_attached_session = last_attached_session;
                        connection.snapshot = snapshot;
                        connection.reconnect_generation = reconnect_generation;
                        connection.asked_to_sign_in = asked_to_sign_in;
                        if host == HostId::LOCAL
                            && let Some(client) = &connection.client
                        {
                            crate::config::register_config_override_client(client, cx);
                        }
                        self.connections.insert(host, connection);
                        self.fetch_host_facts(host, cx);
                        self.report_color_scheme();
                        if reconnect_attempt.is_some() && self.attached_host == host {
                            self.reattach_after_reconnect(host, cx);
                        }
                        cx.notify();
                    }
                    Err(error) => {
                        self.finish_connect_failure(
                            host,
                            HostState::Unreachable {
                                reason: error.to_string(),
                            },
                            route,
                            last_attached_session,
                            snapshot,
                            reconnect_generation,
                            reconnect_attempt,
                            cx,
                        );
                    }
                }
            }
            Err(error) => {
                let state = connect_error_state(&error);
                log::warn!(
                    "fleet host {name} is {label}: {error}",
                    name = self
                        .registry
                        .get(host)
                        .map_or("<unknown>", |entry| entry.name.as_str()),
                    label = state.label(),
                );
                self.finish_connect_failure(
                    host,
                    state,
                    route,
                    last_attached_session,
                    snapshot,
                    reconnect_generation,
                    reconnect_attempt,
                    cx,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_connect_failure(
        &mut self,
        host: HostId,
        state: HostState,
        route: Arc<RwLock<Option<HostId>>>,
        last_attached_session: Option<SessionId>,
        snapshot: Option<Arc<MuxSnapshot>>,
        reconnect_generation: u64,
        reconnect_attempt: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        let (declined, asked_to_sign_in) = self
            .connections
            .get(&host)
            .map_or((false, false), |connection| {
                (connection.ssh_auth_declined, connection.asked_to_sign_in)
            });
        let platform = self.host_platform(host);
        let mut connection = HostConnection::disconnected_with_route(route);
        connection.platform = platform;
        connection.last_attached_session = last_attached_session;
        connection.snapshot = snapshot;
        connection.reconnect_generation = reconnect_generation;
        connection.ssh_auth_declined = declined;
        connection.asked_to_sign_in = asked_to_sign_in;
        let too_old = client_too_old_text(&state);
        let next_attempt = reconnect_attempt
            .filter(|_| !declined && too_old.is_none())
            .and_then(|attempt| {
                (self.attached_host == host
                    || !asked_to_sign_in
                    || attempt < MAX_SIGN_IN_RECONNECT_ATTEMPTS)
                    .then(|| attempt.saturating_add(1))
            });
        if declined {
            connection.state = HostState::Unreachable {
                reason: AUTH_DECLINED_REASON.to_owned(),
            };
        } else if let Some(attempt) = next_attempt {
            connection.state = HostState::Reconnecting { attempt };
        } else {
            connection.state = state;
        }
        let missing = connection.state == HostState::Missing;
        self.connections.insert(host, connection);
        if let Some(text) = too_old
            && self.attached_host == host
        {
            self.error = Some(Arc::from(text));
        }
        if let Some(attempt) = next_attempt {
            self.arm_reconnect(host, attempt, cx);
        }
        if missing
            && self
                .registry
                .get(host)
                .is_some_and(|entry| matches!(entry.endpoint, Endpoint::Ssh(_)))
            && self.auto_installed.insert(host)
            && !zz_protocol::app_identity::DEVELOPMENT
        {
            self.install_zz_on_host(host, cx);
        }
        cx.notify();
    }

    #[must_use]
    pub(crate) fn host_supports_install(&self, host: HostId) -> bool {
        self.registry
            .get(host)
            .is_some_and(|entry| matches!(entry.endpoint, Endpoint::Ssh(_) | Endpoint::Wsl(_)))
    }

    pub(crate) fn install_zz_on_host(&mut self, host: HostId, cx: &mut Context<Self>) {
        let Some(entry) = self.registry.get(host) else {
            return;
        };
        let endpoint = entry.endpoint.clone();
        let name = entry.name.clone();
        if !matches!(endpoint, Endpoint::Ssh(_) | Endpoint::Wsl(_)) {
            return;
        }
        let Some(connection) = self.connections.get_mut(&host) else {
            return;
        };
        if connection.state != HostState::Missing {
            return;
        }
        let previous = Some(std::mem::replace(
            &mut connection.state,
            HostState::Installing,
        ));
        let route = Arc::clone(&connection.route);
        cx.notify();
        #[cfg(test)]
        if let Some(requests) = self.install_requests.as_mut() {
            requests.push(host);
            return;
        }

        let prompts = self.ssh_prompts(host, &endpoint, cx);
        let (results, incoming) = async_channel::bounded(1);
        let install_thread = thread::Builder::new()
            .name("zz-host-install".to_owned())
            .spawn(move || {
                let result = match &endpoint {
                    Endpoint::Ssh(ssh) => zz_daemon_client::install_remote(ssh, prompts, false),
                    Endpoint::Wsl(wsl) => zz_daemon_client::install_wsl(wsl, false),
                    Endpoint::Local(_) | Endpoint::Tailnet(_) => return,
                }
                .map_err(|error| error.ssh_reason().unwrap_or_else(|| error.to_string()));
                let _ = results.send_blocking(result);
            });
        if let Err(error) = install_thread {
            self.finish_install(
                host,
                previous,
                &name,
                Err(format!("could not start the install: {error}")),
                cx,
            );
            return;
        }
        cx.spawn(async move |this, cx| {
            let Ok(result) = incoming.recv().await else {
                return;
            };
            let _ = this.update(cx, |client, cx| {
                if let Some(host) = *route.read() {
                    client.finish_install(host, previous, &name, result, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn finish_install(
        &mut self,
        host: HostId,
        previous: Option<HostState>,
        name: &str,
        result: Result<RemoteInstall, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(previous) = previous else {
            let (kind, text) = match &result {
                Ok(outcome) => outcome.notice(name),
                Err(reason) => (ClientMessageKind::Error, reason.clone()),
            };
            Self::emit_notification(kind, text, cx);
            return;
        };
        let Some(connection) = self.connections.get_mut(&host) else {
            return;
        };
        if connection.state != HostState::Installing {
            return;
        }
        match result {
            Ok(outcome) => {
                connection.state = HostState::Disconnected;
                let (kind, text) = outcome.notice(name);
                Self::emit_notification(kind, text, cx);
                self.ensure_connected(host, cx);
            }
            Err(reason) => {
                connection.state = previous;
                Self::emit_notification(ClientMessageKind::Error, reason, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn reattach_after_reconnect(&mut self, host: HostId, cx: &mut Context<Self>) {
        if host != self.attached_host {
            return;
        }
        let Some(connection) = self.connections.get(&host) else {
            return;
        };
        let client = connection.client.as_ref().map(Arc::clone);
        #[cfg(test)]
        let fake_client = connection.fake_client.as_ref().map(Arc::clone);
        let hello = connection
            .current_hello()
            .expect("a connected host has an interactive client");
        let session = connection.last_attached_session;

        self.ingest_server_hello(hello, cx);
        self.error = None;
        self.error_after_next_attach = None;
        self.connections
            .get_mut(&host)
            .expect("reconnected host still has a connection slot")
            .reconnect_attach = Some(if session.is_some() {
            ReconnectAttachState::RememberedSession
        } else {
            ReconnectAttachState::DefaultSession
        });
        let result = if let Some(client) = client {
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
        if result.is_ok() {
            self.begin_client_focus_attach();
        } else if let Err(error) = result {
            self.reset_client_focus_attach();
            self.connections
                .get_mut(&host)
                .expect("reconnected host still has a connection slot")
                .reconnect_attach = None;
            log::warn!("failed to re-attach after reconnect: {error}");
        }
    }

    pub(super) fn retry_default_after_missing_session(&mut self) -> bool {
        let host = self.attached_host;
        let Some(connection) = self.connections.get(&host) else {
            return false;
        };
        if connection.reconnect_attach != Some(ReconnectAttachState::RememberedSession) {
            return false;
        }
        let client = connection.client.as_ref().map(Arc::clone);
        #[cfg(test)]
        let fake_client = connection.fake_client.as_ref().map(Arc::clone);
        self.connections
            .get_mut(&host)
            .expect("attached host still has a connection slot")
            .reconnect_attach = Some(ReconnectAttachState::DefaultSession);
        self.error = None;
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
        if result.is_ok() {
            self.begin_client_focus_attach();
        } else if let Err(error) = result {
            self.reset_client_focus_attach();
            log::warn!("failed to fall back to the default session after reconnect: {error}");
            if let Some(connection) = self.connections.get_mut(&host) {
                connection.reconnect_attach = None;
            }
        }
        true
    }

    pub(super) fn handle_host_disconnected(&mut self, host: HostId, cx: &mut Context<Self>) {
        let Some(connection) = self.connections.get(&host) else {
            log::debug!("discarding disconnect for removed fleet host {host:?}");
            return;
        };
        let route = Arc::clone(&connection.route);
        let name = self
            .registry
            .get(host)
            .map_or_else(|| format!("{host:?}"), |entry| entry.name.clone());
        let was_attached = host == self.attached_host;
        let last_attached_session = if was_attached {
            self.core
                .attached_session()
                .or(connection.last_attached_session)
        } else {
            connection.last_attached_session
        };
        let reconnect_generation = connection.reconnect_generation;
        let mut disconnected = HostConnection::disconnected_with_route(route);
        disconnected.last_attached_session = last_attached_session;
        disconnected.reconnect_generation = reconnect_generation;
        disconnected.platform = connection.platform;
        disconnected.asked_to_sign_in = connection.asked_to_sign_in;

        if self.shutting_down {
            disconnected.state = HostState::Unreachable {
                reason: "connection lost".to_owned(),
            };
            self.connections.insert(host, disconnected);
            cx.notify();
            return;
        }

        if host == HostId::LOCAL {
            disconnected.snapshot = connection.snapshot.as_ref().map(Arc::clone);
        }

        crate::agent::sound::play(zz_client::attention::Chime::Disconnected, cx);
        disconnected.state = HostState::Reconnecting { attempt: 1 };
        self.connections.insert(host, disconnected);
        if was_attached {
            self.error_after_next_attach = None;
            self.reset_session_state(cx);
            if host == HostId::LOCAL {
                self.error = Some(Arc::from(format!("fleet host {name} disconnected")));
            } else {
                self.error = None;
                Self::emit_keyed_notification(
                    ClientMessageKind::Warning,
                    format!("connection to {name} lost: reconnecting…"),
                    Some(format!("connection-{name}")),
                    cx,
                );
            }
        }
        self.arm_reconnect(host, 1, cx);
        cx.notify();
    }
}
