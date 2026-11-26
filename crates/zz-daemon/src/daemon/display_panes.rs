use super::*;

impl Shared {
    pub(super) fn finish_panes_mode(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        host: PaneId,
        mode: &PanesMode,
        chosen: Option<PaneId>,
    ) {
        let events = {
            let mut inner = self.inner.lock();
            if chosen.is_some()
                && let Some(window) = inner.engine.state.window_for_pane(host)
                && let Some(zoomed) = inner.engine.state.windows[&window].zoomed_pane
            {
                inner.modes.pane_mode_zooms.remove(&host);
                let _ = inner.engine.state.toggle_zoom(zoomed);
            }
            pop_pane_mode(&mut inner, host);
            take_pane_mode_hook_events(&mut inner)
        };
        self.run_event_hooks(events);
        self.reap_pane_mode_kill_panes(client, ClientKind::Interactive, context);
        if let Some(target) = chosen {
            self.submit_template(
                client,
                ClientKind::Interactive,
                context,
                &CommandPromptSubmission {
                    inputs: vec![target.to_string()],
                    template: Some(mode.template.clone().unwrap_or_else(|| {
                        CommandPromptTemplate::String(DISPLAY_PANES_DEFAULT_TEMPLATE.to_owned())
                    })),
                    source: mode.source.clone(),
                },
                "display-panes",
            );
        }
        self.publish_snapshot();
    }

    pub(super) fn end_display_panes(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
    ) {
        if !dismiss_display_panes(&mut self.inner.lock(), client) {
            return;
        }
        self.publish_to_client(client, EventPayload::DisplayPanes { state: None });
        self.settle_pane_modes(client, context);
        self.publish_snapshot();
    }

    pub(super) fn expire_panes_mode(self: &Arc<Self>, pane: PaneId, token: u64) {
        let events = {
            let mut inner = self.inner.lock();
            let due = matches!(
                inner.modes.pane_modes.get(&pane).and_then(|modes| modes.last()),
                Some(PaneModeRequest::Panes(mode)) if mode.token == token
            );
            if !(due && pop_pane_mode(&mut inner, pane)) {
                return;
            }
            take_pane_mode_hook_events(&mut inner)
        };
        self.run_event_hooks(events);
        let mut context = ExecutionContext::default();
        self.reap_pane_mode_kill_panes(ClientId(u64::MAX), ClientKind::Command, &mut context);
        self.publish_snapshot();
    }

    pub(super) fn input_display_panes(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        action: DisplayPanesAction,
        text_follows: bool,
    ) -> Result<(), DaemonError> {
        if kind != ClientKind::Interactive {
            return Err(ServerError::InvalidCommand(
                "display-panes input requires an interactive client".to_owned(),
            )
            .into());
        }
        if matches!(
            &action,
            DisplayPanesAction::Key(input) if input.action == zz_terminal::KeyAction::Release
        ) {
            return Ok(());
        }
        if matches!(action, DisplayPanesAction::Dismiss) {
            self.end_display_panes(client, context);
            return Ok(());
        }

        let (source_pane, read_only, source_alive) = {
            let inner = self.inner.lock();
            let Some(overlay) = inner.client(client).and_then(|c| c.display_panes.as_ref()) else {
                return Ok(());
            };
            (
                overlay.source_pane,
                inner.attachment.client_flags.contains(client),
                inner
                    .engine
                    .state
                    .window_for_pane(overlay.source_pane)
                    .is_some(),
            )
        };
        if read_only {
            match action {
                DisplayPanesAction::Key(input) if source_alive => {
                    self.input_key(client, kind, context, source_pane, input, text_follows)?;
                }
                DisplayPanesAction::Close if source_alive => {
                    self.input_key(
                        client,
                        kind,
                        context,
                        source_pane,
                        display_panes_escape_key(),
                        false,
                    )?;
                }
                DisplayPanesAction::Select(_) => {
                    self.note_terminal_input_without_bell(client, source_pane);
                }
                DisplayPanesAction::Key(_)
                | DisplayPanesAction::Close
                | DisplayPanesAction::Dismiss => {}
            }
            return Ok(());
        }

        let linked = self
            .inner
            .lock()
            .client(client)
            .and_then(|c| c.display_panes.as_ref())
            .and_then(|overlay| overlay.mode);
        if let Some(host) = linked {
            match action {
                DisplayPanesAction::Dismiss => {}
                DisplayPanesAction::Close => {
                    self.pane_mode_key_name(client, context, host, "Escape");
                }
                DisplayPanesAction::Key(input) => {
                    self.pane_mode_key_name(client, context, host, &input_key_name(&input));
                }
                DisplayPanesAction::Select(target) => {
                    let mode = match self
                        .inner
                        .lock()
                        .modes
                        .pane_modes
                        .get(&host)
                        .and_then(|modes| modes.last())
                    {
                        Some(PaneModeRequest::Panes(mode)) => Some(mode.clone()),
                        _ => None,
                    };
                    if let Some(mode) = mode
                        && !mode.ignore_keys
                        && self.inner.lock().engine.state.window_for_pane(target)
                            == Some(mode.source_window)
                    {
                        self.finish_panes_mode(client, context, host, &mode, Some(target));
                    }
                }
            }
            self.publish_snapshot();
            return Ok(());
        }
        let Some(overlay) = ({
            let mut inner = self.inner.lock();
            take_display_panes(&mut inner, client)
        }) else {
            return Ok(());
        };
        self.publish_to_client(client, EventPayload::DisplayPanes { state: None });

        let attached = client_attached_session(&self.inner.lock(), client);
        if attached != Some(overlay.source_session) {
            return Ok(());
        }
        match action {
            DisplayPanesAction::Dismiss => {}
            DisplayPanesAction::Close => {
                self.input_key(
                    client,
                    kind,
                    context,
                    overlay.source_pane,
                    display_panes_escape_key(),
                    false,
                )?;
            }
            DisplayPanesAction::Select(pane) => {
                if overlay.selectable
                    && overlay
                        .state
                        .indicators
                        .iter()
                        .any(|indicator| indicator.pane == pane)
                {
                    self.select_display_pane(
                        client,
                        kind,
                        context,
                        pane,
                        overlay.template.clone(),
                        overlay.source.clone(),
                    )?;
                }
            }
            DisplayPanesAction::Key(input) => {
                let selected = overlay
                    .selectable
                    .then(|| {
                        display_panes_selection_key(&input).and_then(|key| {
                            overlay
                                .state
                                .indicators
                                .iter()
                                .find(|indicator| indicator.select_key == key)
                                .map(|indicator| indicator.pane)
                        })
                    })
                    .flatten();
                if let Some(pane) = selected {
                    self.suppress_committed_character(
                        client,
                        overlay.source_pane,
                        CommittedTextLane::Terminal,
                        &input,
                        text_follows,
                    );
                    self.inner
                        .lock()
                        .client_entry(client)
                        .swallowed_keys
                        .get_or_insert_default()
                        .insert(input_key_name(&input).into_string());
                    self.select_display_pane(
                        client,
                        kind,
                        context,
                        pane,
                        overlay.template.clone(),
                        overlay.source.clone(),
                    )?;
                } else if self
                    .inner
                    .lock()
                    .engine
                    .state
                    .window_for_pane(overlay.source_pane)
                    .is_some()
                {
                    self.input_key(
                        client,
                        kind,
                        context,
                        overlay.source_pane,
                        input,
                        text_follows,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn select_display_pane(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        pane: PaneId,
        template: Option<CommandPromptTemplate>,
        source: Option<SourceSpan>,
    ) -> Result<(), DaemonError> {
        let zoomed = {
            let inner = self.inner.lock();
            let window = inner
                .engine
                .state
                .window_for_pane(pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            inner.engine.state.windows[&window].zoomed_pane.is_some()
        };
        if zoomed {
            self.execute_gesture(
                client,
                kind,
                context,
                "display_panes_select",
                &CommandInvocation::new("resize-pane", ["-Z", "-t", &pane.to_string()]),
            )?;
        }
        self.submit_template(
            client,
            kind,
            context,
            &CommandPromptSubmission {
                inputs: vec![pane.to_string()],
                template: Some(template.unwrap_or_else(|| {
                    CommandPromptTemplate::String(DISPLAY_PANES_DEFAULT_TEMPLATE.to_owned())
                })),
                source,
            },
            "display-panes",
        );
        Ok(())
    }

    pub(super) fn refresh_display_panes(&self) {
        let updates = sync_display_panes_overlays(&mut self.inner.lock());
        for (client, state) in updates {
            self.publish_to_client(client, EventPayload::DisplayPanes { state });
        }
    }
}

const DISPLAY_PANES_DEFAULT_TEMPLATE: &str = "select-pane -t \"%%%\"";

fn display_panes_selection_key(input: &zz_terminal::KeyInput) -> Option<u8> {
    if input.action != zz_terminal::KeyAction::Press
        || input.modifiers != zz_terminal::Modifiers::default()
    {
        return None;
    }
    let character = input
        .text
        .as_deref()
        .and_then(|text| {
            let mut characters = text.chars();
            let character = characters.next()?;
            characters.next().is_none().then_some(character)
        })
        .or(match input.key {
            zz_terminal::KeyCode::Character(character) => Some(character),
            _ => None,
        })?;
    character
        .is_ascii()
        .then(|| u8::try_from(u32::from(character)).expect("ASCII character fits in u8"))
}

pub(super) fn display_panes_escape_key() -> zz_terminal::KeyInput {
    zz_terminal::KeyInput {
        action: zz_terminal::KeyAction::Press,
        key: zz_terminal::KeyCode::Escape,
        modifiers: zz_terminal::Modifiers::default(),
        text: None,
        unshifted_codepoint: None,
    }
}
