use super::*;

impl Shared {
    pub(super) fn deliver_clipboard_write(
        self: &Arc<Self>,
        pane: PaneId,
        target: ClipboardTarget,
        text: String,
    ) {
        let set_clipboard = self
            .inner
            .lock()
            .engine
            .mux_option_value(MuxOptionKey::SetClipboard);
        if set_clipboard != "on" {
            return;
        }
        self.store_copy_buffer(text.clone(), PasteBufferAction::Create { prefix: None });
        self.publish_for_pane(
            pane,
            &EventPayload::Clipboard {
                pane,
                request_id: 0,
                target,
                text,
                producer: ClipboardProducer::Application,
            },
        );
        self.raise_pane_set_clipboard(pane);
    }

    pub(super) fn client_exit_action(
        &self,
        session: SessionId,
        exit: &DetachExit,
    ) -> ClientExitAction {
        if let Some(command) = exit.exec.clone() {
            let shell = {
                let inner = self.inner.lock();
                terminal_shell_for_session(&inner.engine, session)
                    .unwrap_or_else(|_| inner.engine.global_default_shell().to_owned())
            };
            return ClientExitAction::Exec { command, shell };
        }
        if exit.parent_hangup {
            return ClientExitAction::ParentHangup;
        }
        ClientExitAction::Exit
    }

    fn raise_pane_set_clipboard(self: &Arc<Self>, pane: PaneId) {
        let event = {
            let inner = self.inner.lock();
            PendingHookEvent::live_pane("pane-set-clipboard", pane, &inner.engine)
        };
        if let Some(event) = event {
            self.run_event_hooks(vec![event]);
        }
    }

    pub(super) fn raise_copy_mode_set_clipboard(self: &Arc<Self>, pane: PaneId) {
        if self
            .inner
            .lock()
            .engine
            .mux_option_value(MuxOptionKey::SetClipboard)
            == "off"
        {
            return;
        }
        self.raise_pane_set_clipboard(pane);
    }

    pub(super) fn query_client_clipboard(&self, client: ClientId) {
        let now = Instant::now();
        {
            let mut inner = self.inner.lock();
            let Some(registered) = inner.client_mut(client) else {
                return;
            };
            if registered.kind != Some(ClientKind::Interactive)
                || registered
                    .clipboard_query
                    .is_some_and(|deadline| deadline > now)
            {
                return;
            }
            registered.clipboard_query = Some(now + CLIPBOARD_QUERY_TIMEOUT);
        }
        self.publish_to_client(client, EventPayload::ClipboardQuery);
    }

    pub(super) fn store_clipboard_reply(self: &Arc<Self>, client: ClientId, data: Vec<u8>) {
        let events = {
            let mut inner = self.inner.lock();
            let pending = inner
                .client_mut(client)
                .and_then(|registered| registered.clipboard_query.take())
                .is_some_and(|deadline| deadline > Instant::now());
            if !pending {
                return;
            }
            match insert_paste_buffer(&mut inner, None, "buffer", data, true) {
                Ok(events) => events,
                Err(error) => {
                    log::warn!("could not store the client clipboard: {error}");
                    return;
                }
            }
        };
        self.run_event_hooks(events);
        self.refresh_choose_buffers();
    }
}

const CLIPBOARD_QUERY_TIMEOUT: Duration = Duration::from_secs(5);
