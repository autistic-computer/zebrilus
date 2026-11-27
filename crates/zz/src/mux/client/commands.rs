use super::*;

impl MuxClient {
    pub fn execute(&self, command: CommandInvocation) {
        self.execute_on_host(self.attached_host, command);
    }

    pub fn execute_tracked(&self, command: CommandInvocation) -> Option<u64> {
        let request_id = self.execute_on_host(self.attached_host, command)?;
        self.attached_connection().watch_command(request_id);
        Some(request_id)
    }

    pub fn execute_answered(&self, command: CommandInvocation) -> Option<u64> {
        let request_id = self.execute_on_host(self.attached_host, command)?;
        self.attached_connection().answer_command(request_id);
        Some(request_id)
    }

    pub fn execute_printed(&self, command: CommandInvocation) -> Option<u64> {
        let request_id = self.execute_on_host(self.attached_host, command)?;
        self.printed_commands.borrow_mut().insert(request_id);
        Some(request_id)
    }

    pub(crate) fn has_command_answers(&self) -> bool {
        !self.command_answers.is_empty()
    }

    pub(crate) fn take_command_answers(&mut self) -> Vec<(u64, Result<(), String>)> {
        std::mem::take(&mut self.command_answers)
    }

    pub(super) fn answer(
        &mut self,
        host: HostId,
        request_id: u64,
        result: Result<(), String>,
    ) -> bool {
        if request_id == 0
            || !self
                .connections
                .get(&host)
                .is_some_and(|connection| connection.take_answered(request_id))
        {
            return false;
        }
        if let Some(connection) = self.connections.get(&host) {
            connection.take_command(request_id);
            connection.settle_command(request_id);
        }
        self.command_answers.push((request_id, result));
        true
    }

    #[must_use]
    pub fn command_settled(&self, request_id: u64) -> bool {
        self.attached_connection()
            .watched_commands
            .read()
            .iter()
            .any(|&(id, settled)| id == request_id && settled)
    }

    #[cfg(test)]
    pub(crate) fn watch_command_for_test(&self, request_id: u64) {
        self.attached_connection().watch_command(request_id);
    }

    pub fn execute_on_host(&self, host: HostId, command: CommandInvocation) -> Option<u64> {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        log::trace!(
            target: "zz::diagnostics::mux",
            "execute begin host={host:?} command={command:#?}",
        );
        let Some(connection) = self.connections.get(&host) else {
            log::warn!("cannot send mux command to unknown fleet host {host:?}");
            return None;
        };
        let name = command.name.clone();
        let sent = if let Some(client) = &connection.client {
            client.execute(command)
        } else {
            #[cfg(test)]
            if let Some(client) = &connection.fake_client {
                let sent = client.execute(command);
                if let Ok(request_id) = sent {
                    connection.track_command(request_id, name);
                }
                return sent.ok();
            }
            log::trace!(
                target: "zz::diagnostics::mux",
                "execute skipped: no client for host={host:?}",
            );
            return None;
        };
        log::trace!(
            target: "zz::diagnostics::mux",
            "execute end elapsed_us={}",
            diagnostics::elapsed_us(started)
        );
        match sent {
            Ok(request_id) => {
                connection.track_command(request_id, name);
                Some(request_id)
            }
            Err(error) => {
                log::warn!("failed to send mux command: {error}");
                None
            }
        }
    }

    pub(super) fn report_command_failure(
        &mut self,
        host: HostId,
        request_id: u64,
        error: &ServerError,
        cx: &mut Context<Self>,
    ) -> bool {
        if request_id == 0 {
            return false;
        }
        if self.answer(host, request_id, Err(error.to_string())) {
            cx.notify();
            return true;
        }
        let tracked = self.connections.get(&host).and_then(|connection| {
            connection.settle_command(request_id);
            connection.take_command(request_id)
        });
        match tracked {
            Some(name)
                if matches!(
                    error,
                    ServerError::PaneExited(_) | ServerError::PaneNotAttached(_)
                ) =>
            {
                log::debug!(
                    target: "zz::diagnostics::mux",
                    "command_raced_teardown host={host:?} command={name} error={error}"
                );
            }
            Some(name) => {
                log::warn!(
                    target: "zz::diagnostics::mux",
                    "command_failed host={host:?} command={name} error={error}"
                );
                Self::emit_notification(ClientMessageKind::Error, format!("{name}: {error}"), cx);
            }
            None => log::warn!(
                target: "zz::diagnostics::mux",
                "untracked_command_failed host={host:?} request_id={request_id} error={error}"
            ),
        }
        true
    }

    pub(super) fn handle_command_response(
        &mut self,
        host: HostId,
        response: CommandResponse,
        cx: &mut Context<Self>,
    ) {
        match response {
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::MissingTarget(_) | ServerError::SessionNotFound(_),
                ..
            } if self.retry_default_after_missing_session() => {}
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::MissingTarget(_) | ServerError::SessionNotFound(_),
                ..
            } if self.core.attached_session().is_none()
                && self.core.snapshot().sessions.is_empty() =>
            {
                self.fail_client_focus_attach();
                self.error = None;
                let connection = self.attached_connection_mut();
                if !connection.resync_pending
                    && let Some(client) = &connection.client
                {
                    if let Err(error) = client.request_resync() {
                        self.error = Some(Arc::from(error.to_string()));
                    } else {
                        connection.resync_pending = true;
                    }
                }
            }
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::PaneExited(pane) | ServerError::PaneNotAttached(pane),
                ..
            } => log::debug!("ignoring stale input for detached pane {pane}"),
            CommandResponse::Error {
                request_id: 0,
                error: error @ (ServerError::MissingTarget(_) | ServerError::SessionNotFound(_)),
                ..
            } if self.client_focus_attach_pending => {
                self.fail_client_focus_attach();
                if !self.report_command_failure(host, 0, &error, cx) {
                    self.attached_connection_mut().reconnect_attach = None;
                    self.error = Some(Arc::from(error.to_string()));
                }
            }
            CommandResponse::Error {
                request_id, error, ..
            } => {
                self.printed_commands.borrow_mut().remove(&request_id);
                if !self.report_command_failure(host, request_id, &error, cx) {
                    self.attached_connection_mut().reconnect_attach = None;
                    self.error = Some(Arc::from(error.to_string()));
                }
            }
            CommandResponse::Success {
                request_id, output, ..
            } => {
                if self.printed_commands.borrow_mut().remove(&request_id) {
                    cx.emit(CommandPrinted {
                        request_id,
                        output: AsRef::<str>::as_ref(&output).to_owned(),
                    });
                }
                if self.answer(host, request_id, Ok(())) {
                    cx.notify();
                } else {
                    let connection = self.attached_connection();
                    connection.take_command(request_id);
                    if connection.settle_command(request_id) {
                        cx.notify();
                    }
                }
                self.error = None;
            }
        }
    }
}
