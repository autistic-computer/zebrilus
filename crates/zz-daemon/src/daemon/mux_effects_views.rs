use super::mux_effects::{CommandAftermath, EffectContext};
use super::*;

pub(super) fn apply_view_effect(
    inner: &mut ServerState,
    cx: EffectContext<'_>,
    after: &mut CommandAftermath,
    effect: &MuxEffect,
) -> Result<(), DaemonError> {
    let EffectContext {
        client,
        context,
        command_name,
        ..
    } = cx;
    match effect {
        MuxEffect::ArmMouseDrag(drag) => {
            if let Some(registered) = inner.client_mut(client) {
                registered.mouse_drag = Some(*drag);
            }
        }
        MuxEffect::ArmCopyModeSource { pane, source } => {
            if !inner.terminals.contains_key(source) {
                return Err(ServerError::PaneExited(*source).into());
            }
            inner.modes.pending_copy_source = Some((*pane, *source));
        }
        MuxEffect::ArmCopyModeKill { pane } => {
            if !inner
                .clients
                .values()
                .filter_map(|c| c.copy_session.as_ref())
                .any(|session| session.pane == *pane && !session.exiting)
            {
                inner.modes.pending_copy_kill = Some(*pane);
            }
        }
        MuxEffect::CopyModeRepeat {
            pane,
            count,
            target_client,
            require_mode,
        } => {
            if *require_mode && !pane_carries_a_mode_command(inner, client, *pane) {
                return Err(ServerError::InvalidCommand("not in a mode".to_owned()).into());
            }
            if *count == 0 {
                return Ok(());
            }
            let selected = selected_effect_client(inner, client, target_client.as_deref());
            let owners: Vec<ClientId> = match selected {
                Some(selected)
                    if inner
                        .client(selected)
                        .and_then(|c| c.copy_session.as_ref())
                        .is_some_and(|session| session.pane == *pane) =>
                {
                    vec![selected]
                }
                _ => inner
                    .clients
                    .iter()
                    .filter_map(|(id, client)| {
                        client.copy_session.as_ref().map(|value| (id, value))
                    })
                    .filter(|(_, session)| session.pane == *pane)
                    .map(|(owner, _)| *owner)
                    .collect(),
            };
            for owner in owners {
                inner
                    .client_entry(owner)
                    .key_engine
                    .get_or_insert_default()
                    .set_repeat_count(*count);
            }
        }
        MuxEffect::TerminalView {
            pane,
            action,
            target_client,
            require_mode,
        } => {
            if command_name == "send-keys"
                && let Some(mouse) = context.invoking_mouse().cloned()
                && !require_mode
                && consume_pane_mode_mouse(inner, *pane, Some(action))
            {
                if let Some(key) = context.invoking_key()
                    && let Some((x, y)) = mouse_pane_cell(inner, *pane, &mouse)
                {
                    after.pane_mode_pointers.push((*pane, key.to_owned(), x, y));
                }
                after.snapshot_changed = true;
                return Ok(());
            }
            if command_name == "send-keys"
                && context.invoking_mouse().is_some()
                && !require_mode
                && context
                    .invoking_key()
                    .is_some_and(zz_mux::mouse_key_is_a_replayed_double_click)
            {
                return Ok(());
            }
            if matches!(action, zz_terminal::TerminalViewAction::ClearHistory)
                && clear_pane_modes(inner, *pane)
            {
                after.snapshot_changed = true;
            }
            let armed_copy_kill = inner.modes.pending_copy_kill.take();
            let armed_copy_source = inner.modes.pending_copy_source.take();
            if *require_mode && !pane_carries_a_mode_command(inner, client, *pane) {
                return Err(ServerError::InvalidCommand("not in a mode".to_owned()).into());
            }
            let command_output = inner
                .client(client)
                .and_then(|c| c.command_output.as_ref())
                .map(|output| Arc::clone(&output.terminal));
            if let Some(terminal) = command_output {
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::ViewAction {
                        terminal,
                        view: TerminalViewId(client.0),
                        action: action.clone(),
                    });
                return Ok(());
            }
            let terminal = inner
                .terminals
                .get(pane)
                .cloned()
                .ok_or(ServerError::PaneExited(*pane))?;
            let caller_attached = client_is_attached_to_pane(inner, client, *pane);
            if inner.attachment.client_flags.contains(client) && !caller_attached {
                return Err(ServerError::InvalidCommand("client is read-only".to_owned()).into());
            }
            let selected = selected_effect_client(inner, client, target_client.as_deref());
            let targets: Vec<ClientId> = match selected {
                Some(selected) if client_is_attached_to_pane(inner, selected, *pane) => {
                    vec![selected]
                }
                _ => attached_clients_for_pane(inner, *pane)
                    .map(|clients| clients.iter().copied().collect())
                    .unwrap_or_default(),
            };
            if targets.is_empty() {
                if matches!(
                    action,
                    zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel)
                ) {
                    return Ok(());
                }
                return Err(ServerError::PaneNotAttached(*pane).into());
            }
            inner.modes.pending_copy_kill = armed_copy_kill;
            let copy_source = armed_copy_source
                .filter(|(armed, _)| armed == pane)
                .and_then(|(_, source)| inner.terminals.get(&source).cloned());
            for target in targets {
                let action = &counted_copy_mode_action(inner, target, action);
                if let Some(source) = &copy_source
                    && terminal_view_action_enters_copy_mode(action)
                {
                    after
                        .deferred_terminal_commands
                        .push(DeferredTerminalCommand::ArmCopySource {
                            terminal: Arc::clone(&terminal),
                            source: Arc::clone(source),
                        });
                }
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::ViewAction {
                        terminal: Arc::clone(&terminal),
                        view: TerminalViewId(target.0),
                        action: action.clone(),
                    });
                if terminal_view_action_enters_copy_mode(action) {
                    enter_copy_session(inner, target, *pane)?;
                    if copy_source.is_some()
                        && let Some(session) = inner
                            .client_mut(target)
                            .and_then(|c| c.copy_session.as_mut())
                    {
                        session.sourced = true;
                        session.refresh = false;
                    }
                } else if terminal_view_action_exits_copy_mode(action) {
                    begin_exit_copy_session(inner, target);
                } else if terminal_view_action_arms_scroll_exit(action)
                    && let Some(session) = inner
                        .client_mut(target)
                        .and_then(|c| c.copy_session.as_mut())
                    && session.pane == *pane
                {
                    session.scroll_exit = true;
                } else if terminal_view_action_refreshes_now(action)
                    && let Some(session) = inner
                        .client_mut(target)
                        .and_then(|c| c.copy_session.as_mut())
                    && session.pane == *pane
                    && !session.sourced
                {
                    session.unseen = false;
                } else if let Some(refresh) = terminal_view_action_refresh_request(action)
                    && let Some(session) = inner
                        .client_mut(target)
                        .and_then(|c| c.copy_session.as_mut())
                    && session.pane == *pane
                {
                    session.refresh = !session.sourced && refresh.applied(session.refresh);
                    after.refresh_armed |= session.refresh;
                }
            }
        }
        MuxEffect::TerminalUi { pane, command } => {
            if let Some(output) = inner.client(client).and_then(|c| c.command_output.as_ref()) {
                after.client_events.push(EventPayload::TerminalUiCommand {
                    pane: output.pane,
                    command: *command,
                });
            } else if !inner.terminals.contains_key(pane) {
                return Err(ServerError::PaneExited(*pane).into());
            } else {
                after.client_events.push(EventPayload::TerminalUiCommand {
                    pane: *pane,
                    command: *command,
                });
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
