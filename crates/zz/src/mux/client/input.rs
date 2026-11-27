use super::*;

impl MuxClient {
    pub(crate) fn send_input(&self, input: InputMessage) -> bool {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        log::trace!(target: "zz::diagnostics::mux", "send_input begin input={input:#?}");
        #[cfg(test)]
        if let Some(sink) = &self.input_sink {
            sink.borrow_mut().push(input);
            return true;
        }
        let input = match input {
            InputMessage::ResizeTerminal {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
            } => InputMessage::ResizeTerminalV2 {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
                layout_generation: self.core.layout_generation(),
            },
            InputMessage::ClientTerminalSize { columns, rows } => {
                InputMessage::ClientTerminalSizeV2 {
                    columns,
                    rows,
                    layout_generation: self.core.layout_generation(),
                }
            }
            input => input,
        };
        let sent = if let Some(client) = &self.attached_connection().client {
            if let Err(error) = client.send_input(input) {
                log::warn!("failed to send mux input: {error}");
                false
            } else {
                true
            }
        } else {
            log::trace!(target: "zz::diagnostics::mux", "send_input skipped: no client");
            false
        };
        log::trace!(
            target: "zz::diagnostics::mux",
            "send_input end elapsed_us={}",
            diagnostics::elapsed_us(started)
        );
        sent
    }

    pub(crate) fn set_client_window_focused(&mut self, focused: bool) {
        self.desired_client_window_focus = Some(focused);
        self.flush_client_window_focus();
    }

    pub(super) fn begin_client_focus_attach(&mut self) {
        if !self.client_focus_attach_pending {
            self.client_focus_attach_recoverable = self.client_focus_attach_ready;
        }
        self.client_focus_attach_ready = false;
        self.client_focus_attach_pending = true;
    }

    pub(super) fn reset_client_focus_attach(&mut self) {
        self.client_focus_attach_ready = false;
        self.client_focus_attach_pending = false;
        self.client_focus_attach_recoverable = false;
        self.sent_client_window_focus = None;
    }

    pub(super) fn fail_client_focus_attach(&mut self) {
        if !self.client_focus_attach_pending {
            return;
        }
        self.client_focus_attach_pending = false;
        let recoverable = std::mem::take(&mut self.client_focus_attach_recoverable);
        if recoverable {
            self.client_focus_attach_ready = true;
            self.flush_client_window_focus();
        } else {
            self.reset_client_focus_attach();
        }
    }

    pub(super) fn flush_client_window_focus(&mut self) {
        let Some(focused) = self.desired_client_window_focus else {
            return;
        };
        if !self.client_focus_attach_ready || self.sent_client_window_focus == Some(focused) {
            return;
        }
        if self.send_input(InputMessage::ClientFocus { focused }) {
            self.sent_client_window_focus = Some(focused);
        }
    }

    pub(crate) fn request_host_path_list(
        &mut self,
        host: HostId,
        pane: Option<PaneId>,
        dir: String,
    ) -> Option<u64> {
        #[cfg(test)]
        if let Some(sink) = &self.path_list_sink {
            let sent = sink
                .borrow()
                .iter()
                .filter(|message| matches!(message, ProtocolMessage::PathListRequest { .. }))
                .count();
            let request_id = u64::try_from(sent).unwrap_or(u64::MAX) + 1;
            sink.borrow_mut().push(ProtocolMessage::PathListRequest {
                request_id,
                pane,
                dir: Some(dir),
            });
            self.host_path_lists.insert((host, request_id));
            return Some(request_id);
        }
        let client = self.connections.get(&host)?.client.as_ref()?;
        match client.request_path_list(pane, Some(dir)) {
            Ok(request_id) => {
                self.host_path_lists.insert((host, request_id));
                Some(request_id)
            }
            Err(error) => {
                log::warn!("failed to request a path listing on {host:?}: {error}");
                None
            }
        }
    }

    pub(crate) fn cancel_host_path_list(&mut self, host: HostId, request_id: u64) {
        #[cfg(test)]
        if let Some(sink) = &self.path_list_sink {
            if self.host_path_lists.remove(&(host, request_id)) {
                sink.borrow_mut()
                    .push(ProtocolMessage::PathListCancel { request_id });
            }
            return;
        }
        if self.host_path_lists.remove(&(host, request_id))
            && let Some(client) = self
                .connections
                .get(&host)
                .and_then(|connection| connection.client.as_ref())
            && let Err(error) = client.cancel_path_list(request_id)
        {
            log::warn!("failed to cancel path listing {request_id} on {host:?}: {error}");
        }
    }

    pub(super) fn route_host_path_list(
        &mut self,
        host: HostId,
        message: ProtocolMessage,
        cx: &mut Context<Self>,
    ) -> Option<ProtocolMessage> {
        let asked = |request_id: &u64| self.host_path_lists.contains(&(host, *request_id));
        let (request_id, finished, update) = match message {
            ProtocolMessage::PathListBegin { request_id, result } if asked(&request_id) => (
                request_id,
                result.is_err(),
                PathPickerUpdate::Begin { request_id, result },
            ),
            ProtocolMessage::PathListChunk {
                request_id,
                entries,
                done,
                truncated,
            } if asked(&request_id) => (
                request_id,
                done,
                PathPickerUpdate::Chunk {
                    request_id,
                    entries: entries.into(),
                    done,
                    truncated,
                },
            ),
            ProtocolMessage::PathListGit { request_id, marks } if asked(&request_id) => (
                request_id,
                false,
                PathPickerUpdate::Git { request_id, marks },
            ),
            message => return Some(message),
        };
        if finished {
            self.host_path_lists.remove(&(host, request_id));
        }
        cx.emit(HostPathListUpdate { host, update });
        None
    }

    pub(crate) fn paste_to_pane(&self, pane: PaneId, text: String) -> bool {
        self.send_input(InputMessage::TerminalView {
            pane,
            action: TerminalViewAction::Paste(text),
        })
    }

    pub(crate) fn send_prefix_cancel(&mut self) -> Option<u64> {
        let request_id = self.next_prefix_cancel_request.saturating_add(1).max(1);
        if self.send_input(InputMessage::CancelPrefix { request_id }) {
            self.next_prefix_cancel_request = request_id;
            Some(request_id)
        } else {
            None
        }
    }
}
