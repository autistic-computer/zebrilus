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

const MAX_DISPLAY_PANE_INDICATORS: usize = 4_096;

#[derive(Debug)]
pub(super) struct DisplayPanesSession {
    pub(super) token: u64,
    pub(super) source_pane: PaneId,
    source_session: SessionId,
    selectable: bool,
    pub(super) state: DisplayPanesState,
    template: Option<CommandPromptTemplate>,
    source: Option<SourceSpan>,
    pub(super) waiter: Option<OverlayWait>,
    pub(super) mode: Option<PaneId>,
}

pub(super) fn take_display_panes(
    inner: &mut ServerState,
    client: ClientId,
) -> Option<DisplayPanesSession> {
    inner
        .client_mut(client)
        .and_then(|c| c.display_panes.take())
}

pub(super) fn dismiss_display_panes(inner: &mut ServerState, client: ClientId) -> bool {
    let Some(overlay) = take_display_panes(inner, client) else {
        return false;
    };
    if let Some(host) = overlay.mode
        && matches!(
            inner.modes.pane_modes.get(&host).and_then(|modes| modes.last()),
            Some(PaneModeRequest::Panes(mode)) if mode.token == overlay.token
        )
    {
        pop_pane_mode(inner, host);
    }
    true
}

fn display_panes_overlay_mode(
    inner: &ServerState,
    client: ClientId,
) -> Option<(PaneId, PanesMode)> {
    let registered = inner.client(client)?;
    if registered.subscriber.is_none()
        || registered.kind != Some(ClientKind::Interactive)
        || registered.exits_on_detach
    {
        return None;
    }
    let window = client_focused_window_for_attachment(inner, client)?;
    let state = inner.engine.state.windows.get(&window)?;
    std::iter::once(state.active_pane)
        .chain(state.pane_order().iter().copied())
        .find_map(|pane| {
            match inner
                .modes
                .pane_modes
                .get(&pane)
                .and_then(|modes| modes.last())
            {
                Some(PaneModeRequest::Panes(mode)) => Some((pane, mode.as_ref().clone())),
                _ => None,
            }
        })
}

pub(super) fn sync_display_panes_overlays(
    inner: &mut ServerState,
) -> Vec<(ClientId, Option<DisplayPanesState>)> {
    let clients = inner.clients.keys().copied().collect::<Vec<_>>();
    let mut updates = Vec::new();
    for client in clients {
        let wanted = display_panes_overlay_mode(inner, client);
        let current = take_display_panes(inner, client);
        let shown = current.is_some();
        let built = wanted.and_then(|(host, mode)| {
            let session = client_attached_session(inner, client)?;
            let host_window = inner.engine.state.window_for_pane(host)?;
            let source_pane = if host_window == mode.source_window {
                host
            } else {
                inner
                    .engine
                    .state
                    .windows
                    .get(&mode.source_window)?
                    .active_pane
            };
            let facts = borrowed_format_hook_facts(inner);
            let (_, _, state) = build_display_panes_state(
                &inner.engine,
                &inner.config.config_files,
                &facts,
                source_pane,
                mode.duration_ms,
                session,
            )
            .ok()?;
            Some((host, mode, session, source_pane, state))
        });
        let Some((host, mode, session, source_pane, state)) = built else {
            if shown {
                updates.push((client, None));
            }
            continue;
        };
        let current = current.filter(|overlay| {
            overlay.token == mode.token
                && overlay.mode == Some(host)
                && overlay.source_session == session
        });
        if current
            .as_ref()
            .is_none_or(|overlay| overlay.state != state)
        {
            updates.push((client, Some(state.clone())));
        }
        inner
            .client_entry(client)
            .display_panes
            .replace(DisplayPanesSession {
                token: mode.token,
                source_pane,
                source_session: session,
                selectable: !mode.ignore_keys,
                state,
                template: mode.template,
                source: mode.source,
                waiter: current.and_then(|overlay| overlay.waiter),
                mode: Some(host),
            });
    }
    updates
}

pub(super) fn build_display_panes_state(
    engine: &MuxEngine,
    config_files: &str,
    facts: &dyn crate::status::FormatFactSource,
    source_pane: PaneId,
    duration_ms: u32,
    format_client_session: SessionId,
) -> Result<(SessionId, WindowId, DisplayPanesState), ServerError> {
    let state = &engine.state;
    let window_id = state
        .window_for_pane(source_pane)
        .ok_or_else(|| ServerError::MissingTarget(source_pane.to_string()))?;
    let window = state
        .windows
        .get(&window_id)
        .ok_or_else(|| ServerError::MissingTarget(window_id.to_string()))?;
    let panes = window.pane_order().to_vec();
    if panes.len() > MAX_DISPLAY_PANE_INDICATORS {
        return Err(ServerError::Internal(format!(
            "window {window_id} exceeds the pane indicator limit"
        )));
    }
    let format = engine.display_panes_format_for_window(window_id);
    let indicators = panes
        .into_iter()
        .filter(|pane| window.zoomed_pane.is_none_or(|zoomed| *pane == zoomed))
        .map(|pane| {
            let index = engine
                .pane_index(window_id, pane)
                .expect("window pane has an effective index");
            let select_key = match index {
                0..=9 => b'0' + u8::try_from(index).expect("digit pane index"),
                10..=35 => b'a' + u8::try_from(index - 10).expect("letter pane index"),
                _ => 0,
            };
            let label = if format.is_empty() {
                String::new()
            } else {
                let mut context = engine.format_status_context_for_client(
                    Some(window.session),
                    Some(window_id),
                    Some(pane),
                    format_client_session,
                );
                context.set_format_value("config_files", config_files);
                let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(engine);
                truncate_pane_indicator_label(expand_format_values(&format, &context, &mut hooks))
            };
            PaneIndicator {
                pane,
                index,
                select_key,
                flags: if pane == window.active_pane {
                    PaneIndicator::ACTIVE
                } else {
                    0
                },
                label,
            }
        })
        .collect();
    let colour_context = engine.format_status_context_for_client(
        Some(window.session),
        Some(window_id),
        Some(window.active_pane),
        format_client_session,
    );
    let colour = |name: &str| {
        let value = engine.format_option_value(&colour_context, name)?;
        zz_protocol::parse_tmux_colour(&value)
            .or_else(|| zz_protocol::parse_style(&value).and_then(|style| style.fg))
    };
    Ok((
        window.session,
        window_id,
        DisplayPanesState {
            window: window_id,
            duration_ms,
            indicators,
            colour: colour("display-panes-colour"),
            active_colour: colour("display-panes-active-colour"),
        },
    ))
}
