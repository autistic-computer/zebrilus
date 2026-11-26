use super::*;

pub(super) fn best_client_on_session(inner: &ServerState, session: SessionId) -> Option<ClientId> {
    inner
        .attachment
        .attached
        .get(&session)?
        .iter()
        .copied()
        .max_by_key(|client| {
            (
                inner
                    .client(*client)
                    .and_then(|c| c.activity)
                    .unwrap_or_default(),
                Reverse(client.0),
            )
        })
}

pub(super) fn best_attached_client(inner: &ServerState) -> Option<ClientId> {
    inner
        .attachment
        .attached
        .values()
        .flatten()
        .copied()
        .max_by_key(|client| {
            (
                inner
                    .client(*client)
                    .and_then(|c| c.activity)
                    .unwrap_or_default(),
                Reverse(client.0),
            )
        })
}

pub(super) fn best_attached_session(inner: &ServerState) -> Option<SessionId> {
    inner
        .attachment
        .attached
        .iter()
        .filter(|(session, clients)| {
            !clients.is_empty() && inner.engine.state.sessions.contains_key(session)
        })
        .map(|(session, _)| *session)
        .max_by_key(|session| {
            let state = &inner.engine.state.sessions[session];
            (state.sort_activity, state.sort_created, state.name.clone())
        })
}

pub(super) fn resolve_client_target(
    inner: &ServerState,
    invoking_client: ClientId,
    kind: ClientKind,
    target: Option<&str>,
) -> Result<ClientId, ServerError> {
    if let Some(target) = target {
        return find_attached_client(inner, target).ok_or_else(|| client_target_error(target));
    }
    if matches!(kind, ClientKind::Interactive | ClientKind::Control)
        && client_attached_session(inner, invoking_client).is_some()
    {
        return Ok(invoking_client);
    }
    current_client(inner, invoking_client)
        .ok_or_else(|| ServerError::InvalidCommand("no current client".to_owned()))
}

pub(super) fn current_client(inner: &ServerState, invoking_client: ClientId) -> Option<ClientId> {
    if let Some(session) = inner
        .client(invoking_client)
        .and_then(|c| c.origin.as_ref())
        .and_then(|pane| inner.engine.state.window_for_pane(*pane))
        .and_then(|window| inner.engine.state.windows.get(&window))
        .map(|window| window.session)
        && let Some(client) = best_client_on_session(inner, session)
    {
        return Some(client);
    }
    best_attached_session(inner).and_then(|session| best_client_on_session(inner, session))
}

fn normalized_client_target(target: &str) -> &str {
    target.strip_suffix(':').unwrap_or(target)
}

fn attached_client_matches(
    inner: &ServerState,
    client: ClientId,
    target: &str,
    native_aliases: bool,
) -> bool {
    inner
        .client(client)
        .and_then(|c| c.name.as_ref())
        .is_some_and(|name| name == target)
        || client_format_name(inner, client) == target
        || format!("device-{}", client.0) == target
        || inner
            .client(client)
            .and_then(|c| c.tty.as_ref())
            .is_some_and(|tty| {
                tty == target
                    || tty
                        .strip_prefix("/dev/")
                        .is_some_and(|device| device == target)
            })
        || native_aliases
            && (target == client.0.to_string() || target == format!("client-{}", client.0))
}

pub(super) fn find_attached_client_with_aliases(
    inner: &ServerState,
    target: &str,
    native_aliases: bool,
) -> Option<ClientId> {
    let target = normalized_client_target(target);
    inner
        .attachment
        .attached
        .values()
        .flatten()
        .copied()
        .filter(|client| attached_client_matches(inner, *client, target, native_aliases))
        .min()
}

pub(super) fn find_attached_client(inner: &ServerState, target: &str) -> Option<ClientId> {
    find_attached_client_with_aliases(inner, target, false)
}

pub(super) fn client_target_error(target: &str) -> ServerError {
    ServerError::InvalidCommand(format!(
        "can't find client: {}",
        normalized_client_target(target)
    ))
}

pub(super) fn nested_attach_refusal(inner: &ServerState, client: ClientId) -> Option<ServerError> {
    if !inner.client(client).is_some_and(|c| c.nested) {
        return None;
    }
    let tty = inner.client(client).and_then(|c| c.tty.as_ref())?;
    if inner.engine.state.sessions.is_empty() {
        return None;
    }
    inner
        .terminals
        .iter()
        .filter(|(pane, _)| !inner.panes.popup_panes.contains(pane))
        .any(|(_, terminal)| {
            terminal
                .tty()
                .is_some_and(|pane_tty| pane_tty.as_os_str() == tty.as_str())
        })
        .then(|| {
            ServerError::InvalidCommand(
                "sessions should be nested with care, unset $TMUX to force".to_owned(),
            )
        })
}

pub(super) fn client_ignores_size(inner: &ServerState, client: ClientId) -> bool {
    client_ignores_size_from_source(&ClientFormatSource::from_inner(inner), client)
}

pub(super) fn client_ignores_size_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> bool {
    inner.client_flags.get(client).ignore_size
}

pub(super) fn unignored_attached_sizing_client_exists(inner: &ServerState) -> bool {
    inner.attachment.attached.values().flatten().any(|client| {
        matches!(
            inner.client(*client).and_then(|c| c.kind),
            Some(ClientKind::Interactive | ClientKind::Control)
        ) && !client_ignores_size(inner, *client)
    })
}

pub(super) fn client_is_sizing_candidate(
    inner: &ServerState,
    client: ClientId,
    suppress_ignored: bool,
) -> bool {
    matches!(
        inner.client(client).and_then(|c| c.kind),
        Some(ClientKind::Interactive | ClientKind::Control)
    ) && (!suppress_ignored || !client_ignores_size(inner, client))
}

pub(super) fn interactive_client_window_extent(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    window: WindowId,
) -> Option<(u16, u16)> {
    interactive_client_window_extent_from_source(
        &ClientFormatSource::from_inner(inner),
        client,
        session,
        window,
    )
}

pub(super) fn interactive_client_window_extent_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: SessionId,
    window: WindowId,
) -> Option<(u16, u16)> {
    if let Some((columns, rows)) = inner.clients.get(&client).and_then(|c| c.size) {
        let lines = u16::from(inner.engine.status_rows_for_session(Some(session)));
        let status_rows = if rows <= lines { 0 } else { lines };
        return Some((columns.max(1), rows.saturating_sub(status_rows).max(1)));
    }
    let window_state = inner.engine.state.windows.get(&window)?;
    std::iter::once(window_state.active_pane)
        .chain(
            window_state
                .panes
                .keys()
                .copied()
                .filter(|pane| *pane != window_state.active_pane),
        )
        .find_map(|pane| {
            let geometry = inner.terminal_geometries.get(&pane)?.get(&client)?;
            inner
                .engine
                .window_extent_for_pane_geometry(pane, geometry.columns, geometry.rows)
        })
}

pub(super) fn control_client_geometry(
    inner: &ServerState,
    client: ClientId,
    window: WindowId,
) -> Option<TerminalGeometry> {
    control_client_geometry_from_source(&ClientFormatSource::from_inner(inner), client, window)
}

pub(super) fn control_client_geometry_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    window: WindowId,
) -> Option<TerminalGeometry> {
    let output = inner
        .clients
        .get(&client)
        .and_then(|c| c.control_output.as_ref())?;
    output
        .window_geometries
        .get(&window)
        .copied()
        .or(output.geometry)
}

pub(super) fn control_client_sized_panes(
    inner: &ServerState,
    client: ClientId,
) -> BTreeSet<PaneId> {
    let Some(output) = inner.client(client).and_then(|c| c.control_output.as_ref()) else {
        return BTreeSet::new();
    };
    let Some(session) = client_attached_session(inner, client)
        .and_then(|session| inner.engine.state.sessions.get(&session))
    else {
        return BTreeSet::new();
    };
    session
        .windows
        .iter()
        .filter(|window| output.geometry.is_some() || output.window_geometries.contains_key(window))
        .filter_map(|window| inner.engine.state.windows.get(window))
        .flat_map(|window| window.panes.keys().copied())
        .collect()
}

pub(super) fn client_focused_window(
    inner: &ServerState,
    client: ClientId,
    session: &zz_mux::Session,
) -> WindowId {
    client_focused_window_from_source(&ClientFormatSource::from_inner(inner), client, session)
}

pub(super) fn client_focused_window_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: &zz_mux::Session,
) -> WindowId {
    inner
        .clients
        .get(&client)
        .and_then(|c| c.focused_window.as_ref())
        .copied()
        .filter(|focused| session.windows.contains(focused))
        .unwrap_or(session.active_window)
}
