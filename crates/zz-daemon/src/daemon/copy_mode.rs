use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CopySession {
    pub(super) pane: PaneId,
    pub(super) observed: bool,
    pub(super) scroll_exit: bool,
    pub(super) kill: bool,
    pub(super) refresh: bool,
    pub(super) unseen: bool,
    pub(super) sourced: bool,
    pub(super) exiting: bool,
}

pub(super) fn enter_copy_session(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
) -> Result<(), ServerError> {
    let table = inner.engine.copy_mode_table_for_pane(pane)?.to_owned();
    if matches!(
        inner
            .modes
            .pane_modes
            .get(&pane)
            .and_then(|modes| modes.last()),
        Some(PaneModeRequest::Panes(_))
    ) {
        pop_pane_mode(inner, pane);
    }
    inner
        .client_entry(client)
        .key_engine
        .get_or_insert_default()
        .switch_table(Some(table));
    let kill = inner.modes.pending_copy_kill.take() == Some(pane)
        || inner
            .client(client)
            .and_then(|c| c.copy_session.as_ref())
            .is_some_and(|session| session.pane == pane && !session.exiting && session.kill);
    let previous = inner
        .client_entry(client)
        .copy_session
        .replace(CopySession {
            pane,
            observed: false,
            scroll_exit: false,
            kill,
            refresh: false,
            unseen: false,
            sourced: false,
            exiting: false,
        });
    sync_client_pane_sink(inner, client, pane);
    if let Some(previous) = previous.filter(|previous| previous.pane != pane) {
        sync_client_pane_sink(inner, client, previous.pane);
    }
    Ok(())
}

pub(super) enum ChooserKey {
    Pass,
    Key(zz_terminal::KeyInput),
    Claimed,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ChooserUnder {
    copy: bool,
    modes: usize,
}

pub(super) fn chooser_under(inner: &ServerState, client: ClientId, pane: PaneId) -> ChooserUnder {
    ChooserUnder {
        copy: client_in_copy_mode(inner, client),
        modes: inner.modes.pane_modes.get(&pane).map_or(0, Vec::len),
    }
}

fn client_in_copy_mode(inner: &ServerState, client: ClientId) -> bool {
    inner
        .client(client)
        .and_then(|c| c.copy_session.as_ref())
        .is_some_and(|session| !session.exiting)
}

pub(super) fn chooser_shown(inner: &ServerState, client: ClientId) -> bool {
    let Some(state) = inner.client(client) else {
        return false;
    };
    let Some(pane) = state
        .choose_tree
        .as_ref()
        .map(|chooser| chooser.source_pane)
        .or_else(|| {
            state
                .choose_buffer
                .as_ref()
                .map(|chooser| chooser.source_pane)
        })
    else {
        return false;
    };
    client_focused_window_for_attachment(inner, client)
        .and_then(|window| inner.engine.state.windows.get(&window))
        .is_some_and(|window| window.active_pane == pane)
        && (state.chooser_under.copy || !client_in_copy_mode(inner, client))
        && state
            .command_output
            .as_ref()
            .is_none_or(|output| output.parked)
        && inner.modes.pane_modes.get(&pane).map_or(0, Vec::len) <= state.chooser_under.modes
}

pub(super) fn chooser_takes_keys(inner: &ServerState, client: ClientId) -> bool {
    chooser_shown(inner, client)
        && inner
            .client(client)
            .is_some_and(|c| c.display_panes.is_none() && c.command_prompt.is_none())
}

pub(super) fn command_output_owns_pane(
    inner: &ServerState,
    client: ClientId,
    pane: PaneId,
) -> bool {
    inner
        .client(client)
        .and_then(|c| c.command_output.as_ref())
        .is_some_and(|output| output.pane == pane)
}

#[cfg(test)]
pub(super) fn prefix_published(inner: &ServerState, client: ClientId) -> bool {
    inner
        .client(client)
        .and_then(|c| c.published_key_table.as_ref())
        .is_some_and(|(table, _)| table.as_deref() == Some("prefix"))
}

pub(super) fn follow_command_output_focus(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
) -> bool {
    let Some(output) = inner.client(client).and_then(|c| c.command_output.as_ref()) else {
        return false;
    };
    let focused = output.pane == pane;
    if focused != output.parked {
        return false;
    }
    let output_pane = output.pane;
    let previous = output.previous_key_table.clone();
    let current = inner
        .client(client)
        .and_then(|c| c.key_engine.as_ref())
        .and_then(|engine| engine.active_table())
        .map(str::to_owned);
    if focused {
        if current != previous {
            return false;
        }
        let Ok(table) = inner.engine.copy_mode_table_for_pane(output_pane) else {
            return false;
        };
        let table = table.to_owned();
        inner
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(Some(table));
    } else if matches!(current.as_deref(), Some("copy-mode" | "copy-mode-vi")) {
        inner
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(previous);
    }
    if let Some(output) = inner
        .client_mut(client)
        .and_then(|c| c.command_output.as_mut())
    {
        output.parked = !focused;
    }
    true
}

pub(super) fn copy_mode_key_owners(
    inner: &ServerState,
    client: ClientId,
    pane: PaneId,
) -> Vec<ClientId> {
    let live = |session: &CopySession| session.pane == pane && !session.exiting;
    if inner
        .client(client)
        .and_then(|c| c.copy_session.as_ref())
        .is_some_and(live)
    {
        return vec![client];
    }
    inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|value| (id, value)))
        .filter(|(_, session)| live(session))
        .map(|(owner, _)| *owner)
        .collect()
}

pub(super) fn copy_mode_refresh_needed(inner: &ServerState) -> bool {
    inner.clients.values().any(|client| {
        client.copy_session.as_ref().is_some_and(|session| {
            session.refresh && !session.exiting && inner.terminals.contains_key(&session.pane)
        })
    })
}

pub(super) fn copy_mode_refresh_ticks(
    inner: &ServerState,
) -> Vec<(ClientId, Arc<TerminalSession>)> {
    inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|value| (id, value)))
        .filter(|(_, session)| session.refresh && !session.exiting)
        .filter_map(|(client, session)| {
            inner
                .terminals
                .get(&session.pane)
                .map(|terminal| (*client, Arc::clone(terminal)))
        })
        .collect()
}

pub(super) fn terminal_view_action_arms_scroll_exit(
    action: &zz_terminal::TerminalViewAction,
) -> bool {
    matches!(
        action,
        zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::PageDownScrollExit)
            | zz_terminal::TerminalViewAction::CopyModeCounted {
                action: zz_terminal::CopyModeAction::PageDownScrollExit,
                ..
            }
    )
}

pub(super) fn exit_copy_session(inner: &mut ServerState, client: ClientId) {
    if let Some(session) = inner.client_mut(client).and_then(|c| c.copy_session.take()) {
        sync_client_pane_sink(inner, client, session.pane);
    }
    inner
        .client_entry(client)
        .key_engine
        .get_or_insert_default()
        .switch_table(None);
}

pub(super) fn begin_exit_copy_session(inner: &mut ServerState, client: ClientId) {
    if let Some(session) = inner
        .client_mut(client)
        .and_then(|c| c.copy_session.as_mut())
    {
        session.exiting = true;
    }
    inner
        .client_entry(client)
        .key_engine
        .get_or_insert_default()
        .switch_table(None);
}

pub(super) fn copy_mode_format_facts(
    inner: &ServerState,
) -> BTreeMap<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>> {
    let mut panes: BTreeMap<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>> =
        BTreeMap::new();
    for (client, session) in inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|session| (id, session)))
    {
        if session.exiting {
            continue;
        }
        let Some(facts) = inner
            .terminals
            .get(&session.pane)
            .and_then(|terminal| terminal.copy_mode_facts(TerminalViewId(client.0)))
        else {
            continue;
        };
        let name = inner
            .client(*client)
            .and_then(|c| c.name.as_ref())
            .cloned()
            .unwrap_or_default();
        panes.entry(session.pane).or_default().push((name, facts));
    }
    panes
}

pub(super) fn active_copy_mode_panes(inner: &ServerState) -> BTreeSet<PaneId> {
    inner
        .clients
        .values()
        .filter_map(|c| c.copy_session.as_ref())
        .filter(|session| !session.exiting)
        .map(|session| session.pane)
        .collect()
}

pub(super) fn terminal_view_action_enters_copy_mode(
    action: &zz_terminal::TerminalViewAction,
) -> bool {
    matches!(
        action,
        zz_terminal::TerminalViewAction::EnterCopyMode
            | zz_terminal::TerminalViewAction::EnterCopyModeScrollExit
            | zz_terminal::TerminalViewAction::EnterCopyModeWith { .. }
    )
}

pub(super) fn terminal_view_action_exits_copy_mode(
    action: &zz_terminal::TerminalViewAction,
) -> bool {
    match action {
        zz_terminal::TerminalViewAction::ClearHistory
        | zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel)
        | zz_terminal::TerminalViewAction::CopyModeCounted {
            action: zz_terminal::CopyModeAction::Cancel,
            ..
        } => true,
        zz_terminal::TerminalViewAction::CopyMode(
            zz_terminal::CopyModeAction::CopySelection(copy)
            | zz_terminal::CopyModeAction::CopyEndOfLine(copy)
            | zz_terminal::CopyModeAction::CopyLine(copy),
        )
        | zz_terminal::TerminalViewAction::CopyModeCounted {
            action:
                zz_terminal::CopyModeAction::CopySelection(copy)
                | zz_terminal::CopyModeAction::CopyEndOfLine(copy)
                | zz_terminal::CopyModeAction::CopyLine(copy),
            ..
        } => copy.cancel,
        _ => false,
    }
}

pub(super) fn unfocused_copy_sessions(
    inner: &mut ServerState,
) -> Vec<(ClientId, Arc<TerminalSession>)> {
    let stale = inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|value| (id, value)))
        .filter(|(client, session)| client_context_pane(inner, **client) != Some(session.pane))
        .map(|(client, session)| (*client, session.pane))
        .collect::<Vec<_>>();
    stale
        .into_iter()
        .filter_map(|(client, pane)| {
            exit_copy_session(inner, client);
            Some((client, inner.terminals.get(&pane).cloned()?))
        })
        .collect()
}
