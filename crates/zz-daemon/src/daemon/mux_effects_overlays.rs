use super::mux_effects::{CommandAftermath, EffectContext};
use super::*;

impl Shared {
    pub(super) fn apply_overlay_effect(
        &self,
        inner: &mut ServerState,
        cx: EffectContext<'_>,
        after: &mut CommandAftermath,
        effect: &MuxEffect,
    ) -> Result<(), DaemonError> {
        let EffectContext {
            client,
            kind,
            command_name,
            ..
        } = cx;
        match effect {
            MuxEffect::FocusSidebar { pane } => {
                if kind != ClientKind::Interactive
                    || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                {
                    return Err(ServerError::InvalidCommand(
                        "focus-sidebar requires an interactive client".to_owned(),
                    )
                    .into());
                }
                if !client_is_attached_to_pane(inner, client, *pane) {
                    return Err(ServerError::PaneNotAttached(*pane).into());
                }
                dismiss_overlays(
                    inner,
                    client,
                    None,
                    &mut after.direct_events,
                    &mut after.retired_command_outputs,
                    &mut after.resume_client_terminals,
                );
                inner
                    .client_mut(client)
                    .and_then(|c| c.swallowed_keys.take());
                after.direct_events.push(EventPayload::FocusSidebar);
            }
            MuxEffect::ChoosePath { pane, start_dir } => {
                if kind != ClientKind::Interactive
                    || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                {
                    return Err(ServerError::InvalidCommand(
                        "choose-path requires an interactive client".to_owned(),
                    )
                    .into());
                }
                if !inner.client(client).is_some_and(|c| c.path_picker) {
                    return Err(ServerError::InvalidCommand(
                        "choose-path needs the zz desktop app".to_owned(),
                    )
                    .into());
                }
                if !client_is_attached_to_pane(inner, client, *pane)
                    || inner.engine.state.window_for_pane(*pane)
                        != client_focused_window_for_attachment(inner, client)
                {
                    return Err(ServerError::PaneNotAttached(*pane).into());
                }
                if inner.attachment.client_flags.contains(client) {
                    return Err(
                        ServerError::InvalidCommand("client is read-only".to_owned()).into(),
                    );
                }
                if inner
                    .client(client)
                    .and_then(|c| c.copy_session.as_ref())
                    .is_some_and(|session| session.pane == *pane)
                {
                    return Err(ServerError::InvalidCommand(
                        "choose-path cannot run while the pane is in copy mode".to_owned(),
                    )
                    .into());
                }
                if !inner.terminals.contains_key(pane) {
                    return Err(ServerError::InvalidCommand(
                        "choose-path needs a terminal pane".to_owned(),
                    )
                    .into());
                }
                if start_dir.as_deref().is_some_and(|start_dir| {
                    start_dir.len() > zz_protocol::MAX_PATH_LIST_TEXT_BYTES
                        || start_dir.chars().any(char::is_control)
                }) {
                    return Err(ServerError::InvalidCommand(
                        "choose-path start directory cannot be listed".to_owned(),
                    )
                    .into());
                }
                dismiss_overlays(
                    inner,
                    client,
                    None,
                    &mut after.direct_events,
                    &mut after.retired_command_outputs,
                    &mut after.resume_client_terminals,
                );
                inner
                    .client_mut(client)
                    .and_then(|c| c.swallowed_keys.take());
                inner
                    .client_mut(client)
                    .and_then(|c| c.path_list_root.take());
                after.direct_events.push(EventPayload::OpenPathPicker {
                    pane: *pane,
                    start_dir: start_dir.clone(),
                });
            }
            MuxEffect::CommandPrompt {
                steps,
                template,
                source,
                prompt_type,
                mode,
                no_freeze,
                pane,
            } => {
                if kind != ClientKind::Interactive
                    || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                {
                    return Err(ServerError::InvalidCommand(
                        "command-prompt requires an interactive client".to_owned(),
                    )
                    .into());
                }
                if inner
                    .client(client)
                    .is_some_and(|c| c.native_terminal_search)
                    && inner
                        .client(client)
                        .is_none_or(|c| c.command_prompt.is_none())
                    && let Some(direction) =
                        native_copy_search_direction(steps, template.as_ref(), *prompt_type, *mode)
                    && let Some(pane) = inner
                        .client(client)
                        .and_then(|c| c.command_output.as_ref())
                        .map(|output| output.pane)
                        .or_else(|| {
                            inner
                                .client(client)
                                .and_then(|c| c.copy_session.as_ref())
                                .map(|session| session.pane)
                        })
                {
                    after.client_events.push(EventPayload::TerminalUiCommand {
                        pane,
                        command: zz_protocol::TerminalUiCommand::BeginSearch { direction },
                    });
                    return Ok(());
                }
                dismiss_overlays(
                    inner,
                    client,
                    Some(Overlay::CommandPrompt),
                    &mut after.direct_events,
                    &mut after.retired_command_outputs,
                    &mut after.resume_client_terminals,
                );
                if inner
                    .client(client)
                    .is_none_or(|c| c.command_prompt.is_none())
                {
                    let (vi_keys, word_separators) = inner
                        .engine
                        .prompt_key_options(client_attached_session(inner, client));
                    let mut prompt = CommandPrompt::new(
                        steps.clone(),
                        template.clone(),
                        source.clone(),
                        *prompt_type,
                        *mode,
                        *no_freeze,
                        *pane,
                    )
                    .with_key_options(vi_keys, word_separators)
                    .with_cursors(prompt_cursors(
                        inner,
                        client_attached_session(inner, client),
                    ));
                    prompt.waiter = self
                        .command_item
                        .as_ref()
                        .and_then(|item| item.lock().raising_overlay.take());
                    after.prompt_cleared_message = take_client_message(inner, client);
                    let state = prompt.state(prompt_history(inner, prompt.prompt_type));
                    let fired = (prompt.mode == CommandPromptMode::Incremental).then(|| {
                        CommandPromptSubmission {
                            inputs: vec![format!("={}", prompt.input)],
                            template: prompt.template.clone(),
                            source: prompt.source.clone(),
                        }
                    });
                    inner.client_entry(client).command_prompt.replace(prompt);
                    sync_terminal_freeze(inner, client);
                    after
                        .direct_events
                        .push(EventPayload::CommandPrompt { state: Some(state) });
                    after.incremental_start = fired;
                }
            }
            MuxEffect::ChooseTree {
                pane,
                preview,
                kind: tree_kind,
                info_preview,
                sessions_only,
                filter,
                format,
                hide_source,
                kill_source,
                prompt_accept,
                sort,
                key_format,
                template,
                zoom,
            } => {
                if kind != ClientKind::Interactive
                    || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                {
                    if command_name == "find-window" {
                        return Ok(());
                    }
                    return Err(ServerError::InvalidCommand(format!(
                        "{command_name} requires an interactive client"
                    ))
                    .into());
                }
                if *tree_kind == ChooseTreeKind::Clients && inner.attachment.attached.is_empty() {
                    return Ok(());
                }
                let attached_session = client_attached_session(inner, client);
                let facts = borrowed_format_hook_facts(inner);
                let client_rows = if *tree_kind == ChooseTreeKind::Clients {
                    chooser_presentation::client_chooser_rows(
                        inner,
                        format.as_deref(),
                        filter.as_deref(),
                    )
                } else {
                    Vec::new()
                };
                let mut chooser = ChooseTreeSession::new(
                    *tree_kind,
                    *info_preview,
                    client_rows,
                    *sessions_only,
                    *pane,
                    &inner.engine,
                    attached_session,
                    &facts,
                    filter.clone(),
                    format.clone(),
                    *hide_source,
                    *kill_source,
                    *prompt_accept,
                    *sort,
                    key_format.clone(),
                )?;
                chooser.template = template.clone().or_else(|| {
                    (*tree_kind == ChooseTreeKind::Clients)
                        .then(|| CLIENT_MODE_DEFAULT_COMMAND.to_owned())
                });
                chooser.preview_size = *preview;
                if attached_session != Some(chooser.source_session) {
                    return Err(ServerError::PaneNotAttached(*pane).into());
                }
                dismiss_overlays(
                    inner,
                    client,
                    Some(Overlay::ChooseTree),
                    &mut after.direct_events,
                    &mut after.retired_command_outputs,
                    &mut after.resume_client_terminals,
                );
                inner
                    .client_mut(client)
                    .and_then(|c| c.swallowed_keys.take());
                if hold_chooser_zoom(inner, client, *pane, *zoom) {
                    after.snapshot_changed = true;
                    let facts = borrowed_format_hook_facts(inner);
                    chooser.rebuild(&inner.engine, attached_session, &facts);
                }
                let state = chooser.rendered.clone();
                inner.client_entry(client).chooser_under = chooser_under(inner, client, *pane);
                inner.client_entry(client).choose_tree.replace(chooser);
                inner.client_entry(client).published_chooser = Default::default();
                after.direct_events.push(EventPayload::ChooseTree {
                    state: chooser_shown(inner, client).then_some(state),
                });
                after.direct_events.push(EventPayload::ChooserPresentation {
                    presentation: chooser_presentation::chooser_presentation(inner, client)
                        .map(Box::new),
                });
            }
            MuxEffect::ChooseBuffer {
                pane,
                preview,
                filter,
                format,
                kill_source,
                sort,
                key_format,
                template,
                zoom,
            } => {
                if kind != ClientKind::Interactive
                    || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                {
                    return Err(ServerError::InvalidCommand(
                        "choose-buffer requires an interactive client".to_owned(),
                    )
                    .into());
                }
                let attached_session = client_attached_session(inner, client);
                let facts = borrowed_format_hook_facts(inner);
                let Some(mut chooser) = ChooseBufferSession::new(
                    *pane,
                    &inner.engine,
                    &inner.buffers.paste_buffers,
                    attached_session,
                    &facts,
                    filter.clone(),
                    format.clone(),
                    *kill_source,
                    *sort,
                    key_format.clone(),
                )?
                else {
                    return Ok(());
                };
                chooser.template.clone_from(template);
                chooser.preview_size = *preview;
                if attached_session != Some(chooser.source_session) {
                    return Err(ServerError::PaneNotAttached(*pane).into());
                }
                dismiss_overlays(
                    inner,
                    client,
                    Some(Overlay::ChooseBuffer),
                    &mut after.direct_events,
                    &mut after.retired_command_outputs,
                    &mut after.resume_client_terminals,
                );
                inner
                    .client_mut(client)
                    .and_then(|c| c.swallowed_keys.take());
                if hold_chooser_zoom(inner, client, *pane, *zoom) {
                    after.snapshot_changed = true;
                }
                let state = chooser.rendered.clone();
                inner.client_entry(client).chooser_under = chooser_under(inner, client, *pane);
                inner.client_entry(client).choose_buffer.replace(chooser);
                inner.client_entry(client).published_chooser = Default::default();
                after.direct_events.push(EventPayload::ChooseBuffer {
                    state: chooser_shown(inner, client).then_some(state),
                });
                after.direct_events.push(EventPayload::ChooserPresentation {
                    presentation: chooser_presentation::chooser_presentation(inner, client)
                        .map(Box::new),
                });
            }
            MuxEffect::DisplayPanes {
                pane,
                delay,
                selectable,
                template,
                source,
                source_window,
                source_session,
                kill_source,
                zoom,
            } => {
                if matches!(
                    inner
                        .modes
                        .pane_modes
                        .get(pane)
                        .and_then(|modes| modes.last()),
                    Some(PaneModeRequest::Panes(_))
                ) {
                    return Ok(());
                }
                let host_window = inner
                    .engine
                    .state
                    .window_for_pane(*pane)
                    .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                let duration_ms = inner
                    .engine
                    .display_panes_delay(host_window, delay.as_deref())?;
                let viewers = panes_mode_viewers(inner, host_window);
                let native_only = !viewers.is_empty() && viewers.iter().all(|(_, native)| *native);
                inner.modes.next_display_panes_token =
                    inner.modes.next_display_panes_token.wrapping_add(1).max(1);
                let token = inner.modes.next_display_panes_token;
                let mode = PanesMode {
                    source_session: *source_session,
                    source_window: *source_window,
                    duration_ms,
                    ignore_keys: !*selectable,
                    template: template.clone(),
                    source: source.clone(),
                    kill_source: *kill_source,
                    zoom: *zoom && !native_only,
                    token,
                };
                if !push_pane_mode(inner, *pane, PaneModeRequest::Panes(Box::new(mode))) {
                    return Ok(());
                }
                after.snapshot_changed = true;
                if duration_ms != 0 {
                    after.panes_mode_deadline = Some((
                        *pane,
                        token,
                        Instant::now() + Duration::from_millis(u64::from(duration_ms)),
                    ));
                }
                after
                    .panes_mode_overlays
                    .extend(sync_display_panes_overlays(inner));
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}
