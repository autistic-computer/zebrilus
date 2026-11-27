use super::*;

impl MuxClient {
    pub(crate) fn dismiss_stale_daemon(&mut self, cx: &mut Context<Self>) {
        if self.stale_daemon.take().is_none() {
            return;
        }
        let message = self.error.as_deref().map_or_else(
            || "the running zz daemon is incompatible with this zz".to_owned(),
            str::to_owned,
        );
        self.error = Some(Arc::from(format!(
            "{message}\nRun 'zz kill-server' to restart it manually."
        )));
        cx.notify();
    }

    pub(crate) fn restart_daemon_for_update(&mut self, cx: &mut Context<Self>) {
        if self
            .connections
            .get(&HostId::LOCAL)
            .is_some_and(HostConnection::is_connected)
        {
            self.release_local_connection(cx);
            self.restart_local_daemon(LocalDaemonStop::KillServer, None, cx);
            return;
        }
        self.stale_daemon
            .get_or_insert(StaleDaemonInfo::Protocol { daemon: None });
        self.restart_stale_daemon(false, cx);
    }

    pub(crate) fn restart_stale_daemon(&mut self, automatic: bool, cx: &mut Context<Self>) {
        let Some(stale) = self.stale_daemon.take() else {
            return;
        };
        let notice = automatic.then(|| stale.restarted_notice());
        self.restart_local_daemon(LocalDaemonStop::Signal, notice, cx);
    }

    fn release_local_connection(&mut self, cx: &mut Context<Self>) {
        let Some(connection) = self.connections.get_mut(&HostId::LOCAL) else {
            return;
        };
        connection.clear_route();
        let mut released = HostConnection::disconnected(HostId::LOCAL);
        released.platform = connection.platform;
        self.connections.insert(HostId::LOCAL, released);
        if self.attached_host == HostId::LOCAL {
            self.error_after_next_attach = None;
            self.reset_session_state(cx);
        }
    }

    fn restart_local_daemon(
        &mut self,
        stop: LocalDaemonStop,
        notice: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let socket_path = self.registry.local_socket_path().to_path_buf();
        let color_scheme = self.color_scheme;
        self.error = Some(Arc::from(match stop {
            LocalDaemonStop::Signal => "restarting stale zz daemon…",
            LocalDaemonStop::KillServer => "restarting the zz daemon…",
        }));
        cx.notify();

        let (results, incoming) = async_channel::bounded(1);
        let restart_thread = thread::Builder::new()
            .name("zz-daemon-restart".to_owned())
            .spawn(move || {
                let result = stop.stop(&socket_path).and_then(|()| {
                    crate::connect_interactive_client(&socket_path, color_scheme)
                        .map_err(|error| error.to_string())
                });
                let _ = results.send_blocking(result);
            });
        if let Err(error) = restart_thread {
            self.error = Some(Arc::from(format!(
                "could not start the daemon restart: {error}"
            )));
            cx.notify();
            return;
        }

        cx.spawn(async move |this, cx| {
            let Ok(result) = incoming.recv().await else {
                return;
            };
            let _ = this.update(cx, |client, cx| {
                let installed = match result {
                    Ok(connection) if client.attached_host == HostId::LOCAL => client
                        .install_initial_local_connection(connection, cx)
                        .map_err(|error| {
                            format!("could not attach after restarting the zz daemon: {error}")
                        }),
                    Ok(connection) => client
                        .install_background_local_connection(connection, cx)
                        .map_err(|error| {
                            format!("could not connect after restarting the zz daemon: {error}")
                        }),
                    Err(error) => Err(format!("could not restart the zz daemon: {error}")),
                };
                match installed {
                    Ok(()) => {
                        client.error = None;
                        client.ensure_all_connected(cx);
                        if let Some(notice) = notice {
                            Self::emit_notification(ClientMessageKind::Success, notice, cx);
                        }
                    }
                    Err(error) => client.error = Some(Arc::from(error)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn install_background_local_connection(
        &mut self,
        client: InteractiveClient,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Err(error) = client.request_resync() {
            log::warn!("failed to request the restarted daemon's snapshot: {error}");
        }
        let connection = HostConnection::connected(Arc::new(client), HostId::LOCAL, cx)
            .map_err(|error| error.to_string())?;
        self.connections.insert(HostId::LOCAL, connection);
        self.report_color_scheme();
        Ok(())
    }
}
