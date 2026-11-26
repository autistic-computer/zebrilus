use super::*;

pub(super) enum PaneSink {
    Terminal(Arc<TerminalSession>),
    Browser(PaneId),
}

pub(super) fn resolve_input_sinks(
    inner: &ServerState,
    source: PaneId,
) -> Result<Vec<PaneSink>, ServerError> {
    if inner.engine.pane_input_off(source)? {
        return Ok(Vec::new());
    }
    let targets = inner.engine.synchronized_input_targets(source)?;
    let mut sinks = Vec::with_capacity(targets.len());
    for pane in targets {
        if inner.engine.pane_input_off(pane)? {
            continue;
        }
        if matches!(
            inner.engine.state.pane(pane).map(|pane| &pane.kind),
            Some(
                zz_mux::PaneKind::Picker { .. }
                    | zz_mux::PaneKind::Editor(_)
                    | zz_mux::PaneKind::View(_)
                    | zz_mux::PaneKind::Simulator(_)
            )
        ) {
        } else if let Some(terminal) = inner.terminals.get(&pane) {
            sinks.push(PaneSink::Terminal(Arc::clone(terminal)));
        } else if matches!(
            inner.engine.state.pane(pane).map(|pane| &pane.kind),
            Some(zz_mux::PaneKind::Browser(_))
        ) {
            ensure_browser_attached(inner, pane)?;
            sinks.push(PaneSink::Browser(pane));
        } else {
            return Err(ServerError::PaneExited(pane));
        }
    }
    Ok(sinks)
}

fn host_terminal_panes(inner: &ServerState) -> BTreeSet<PaneId> {
    inner
        .engine
        .state
        .sessions
        .values()
        .flat_map(|session| session.windows.iter())
        .filter_map(|window| inner.engine.state.windows.get(window))
        .flat_map(|window| window.panes.keys())
        .filter(|pane| inner.terminals.contains_key(pane))
        .copied()
        .collect()
}

pub(super) fn visible_terminal_panes(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
) -> BTreeSet<PaneId> {
    if inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Control) {
        return BTreeSet::new();
    }
    let Some(session) = inner.engine.state.sessions.get(&session) else {
        return BTreeSet::new();
    };
    let focused_window = client_focused_window(inner, client, session);
    let Some(window) = inner.engine.state.windows.get(&focused_window) else {
        return BTreeSet::new();
    };
    window
        .panes
        .iter()
        .filter(|(pane, _)| window.pane_is_visible(**pane) && inner.terminals.contains_key(pane))
        .map(|(pane, _)| *pane)
        .collect()
}

pub(super) fn streamed_terminal_panes(
    inner: &ServerState,
    client: ClientId,
    visible: &BTreeSet<PaneId>,
) -> BTreeMap<PaneId, TerminalStreamKind> {
    let mut streamed = visible
        .iter()
        .copied()
        .map(|pane| (pane, TerminalStreamKind::Foreground))
        .collect::<BTreeMap<_, _>>();
    if inner.client(client).is_some_and(|c| c.terminal_preview) {
        for pane in host_terminal_panes(inner) {
            streamed.entry(pane).or_insert(TerminalStreamKind::Preview);
        }
    }
    streamed
}

pub(super) fn apply_view_streams(
    inner: &ServerState,
    view: TerminalViewId,
    previous: &BTreeMap<PaneId, TerminalStreamKind>,
    next: &BTreeMap<PaneId, TerminalStreamKind>,
) {
    let client = ClientId(view.0);
    for pane in previous.keys().filter(|pane| !next.contains_key(pane)) {
        if let Some(terminal) = inner.terminals.get(pane) {
            terminal.set_view_stream(view, ViewStream::Off);
            sync_view_sink(inner, terminal, client, *pane, None);
            if !client_is_attached_to_pane(inner, client, *pane) {
                terminal.detach_view(view);
            }
        }
    }
    for (pane, kind) in next {
        let Some(terminal) = inner.terminals.get(pane) else {
            continue;
        };
        sync_view_sink(inner, terminal, client, *pane, Some(*kind));
        if previous.contains_key(pane) {
            continue;
        }
        if !client_is_attached_to_pane(inner, client, *pane) {
            terminal.attach_view(view);
        }
        sync_view_area(inner, *pane, client);
        terminal.set_view_stream(
            view,
            match kind {
                TerminalStreamKind::Foreground => ViewStream::Foreground,
                TerminalStreamKind::Preview => ViewStream::Preview,
            },
        );
    }
}

pub(super) fn sync_view_sink(
    inner: &ServerState,
    terminal: &TerminalSession,
    client: ClientId,
    pane: PaneId,
    kind: Option<TerminalStreamKind>,
) {
    let Some(sink) = shard_sink::PaneSink::of(terminal) else {
        return;
    };
    let record = inner
        .client(client)
        .filter(|_| kind == Some(TerminalStreamKind::Foreground))
        .and_then(|c| {
            let live = c
                .copy_session
                .as_ref()
                .is_none_or(|session| session.pane != pane);
            c.subscriber.clone().map(|mailbox| (mailbox, live))
        });
    sink.set_view(TerminalViewId(client.0), record);
}

pub(super) fn sync_client_pane_sink(inner: &ServerState, client: ClientId, pane: PaneId) {
    let Some(terminal) = inner.terminals.get(&pane) else {
        return;
    };
    let kind = inner
        .client(client)
        .and_then(|c| c.streamed_terminals.as_ref())
        .and_then(|streamed| streamed.get(&pane))
        .copied();
    sync_view_sink(inner, terminal, client, pane, kind);
}

fn control_route_panes(inner: &ServerState, client: ClientId) -> BTreeSet<PaneId> {
    let Some(session) = client_attached_session(inner, client)
        .and_then(|session| inner.engine.state.sessions.get(&session))
    else {
        return BTreeSet::new();
    };
    session
        .windows
        .iter()
        .filter_map(|window| inner.engine.state.windows.get(window))
        .flat_map(|window| window.panes.iter())
        .filter(|(pane, _)| inner.terminals.contains_key(pane))
        .map(|(pane, _)| *pane)
        .collect()
}

pub(super) fn sync_control_feed(
    inner: &ServerState,
    client: ClientId,
    wake: &Arc<shard_sink::ControlWake>,
) {
    let Some(state) = inner
        .client(client)
        .filter(|c| c.kind == Some(ClientKind::Control))
    else {
        return;
    };
    let Some(mailbox) = state.subscriber.as_ref() else {
        return;
    };
    let output = state.control_output.as_ref();
    mailbox.ensure_control_feed(wake).configure(
        output.is_some_and(|output| output.no_output),
        output.and_then(|output| output.pause_after_ms),
        output
            .map(|output| {
                output
                    .panes
                    .iter()
                    .filter(|(_, pane)| pane.mode != ControlPaneOutputMode::On)
                    .map(|(pane, _)| *pane)
                    .collect()
            })
            .unwrap_or_default(),
    );
}

pub(super) fn sync_control_routes(inner: &mut ServerState, wake: &Arc<shard_sink::ControlWake>) {
    let desired = inner
        .clients
        .iter()
        .filter(|(_, state)| state.kind == Some(ClientKind::Control))
        .filter_map(|(client, state)| {
            let mailbox = Arc::clone(state.subscriber.as_ref()?);
            Some((*client, (mailbox, control_route_panes(inner, *client))))
        })
        .collect::<BTreeMap<_, _>>();
    if desired.is_empty() && inner.attachment.control_routes.is_empty() {
        return;
    }
    let previous = std::mem::take(&mut inner.attachment.control_routes);
    let sink = |pane: &PaneId| {
        inner
            .terminals
            .get(pane)
            .and_then(|terminal| shard_sink::PaneSink::of(terminal))
    };
    for (client, route) in &previous {
        let kept = desired
            .get(client)
            .filter(|(mailbox, _)| Arc::ptr_eq(mailbox, &route.mailbox));
        for pane in &route.panes {
            if kept.is_some_and(|(_, panes)| panes.contains(pane)) {
                continue;
            }
            if let Some(sink) = sink(pane) {
                sink.set_control(&route.mailbox, false);
            }
        }
    }
    for (client, (mailbox, panes)) in &desired {
        sync_control_feed(inner, *client, wake);
        let known = previous
            .get(client)
            .filter(|route| Arc::ptr_eq(&route.mailbox, mailbox));
        for pane in panes {
            if known.is_some_and(|route| route.panes.contains(pane)) {
                continue;
            }
            if let Some(sink) = sink(pane) {
                sink.set_control(mailbox, true);
            }
        }
    }
    inner.attachment.control_routes = desired
        .into_iter()
        .map(|(client, (mailbox, panes))| (client, ControlRoute { mailbox, panes }))
        .collect();
}

#[cfg(test)]
pub(super) fn output_routed(inner: &ServerState, pane: PaneId) -> bool {
    inner
        .terminals
        .get(&pane)
        .and_then(|terminal| shard_sink::PaneSink::of(terminal))
        .is_some_and(zz_terminal::TerminalFrameSink::takes_output)
}

pub(super) fn retire_terminal(terminal: &TerminalSession) {
    terminal.retire();
    if let Some(sink) = shard_sink::PaneSink::of(terminal) {
        sink.clear();
    }
}

pub(super) fn sync_terminal_freeze(inner: &ServerState, client: ClientId) {
    if let Some(subscriber) = inner.client(client).and_then(|c| c.subscriber.as_ref()) {
        subscriber.terminals_frozen.store(
            client_terminal_publication_frozen(inner, client),
            Ordering::Release,
        );
    }
}

fn chooser_preview_panes(inner: &ServerState) -> BTreeSet<PaneId> {
    let state = &inner.engine.state;
    let mut panes = BTreeSet::new();
    for chooser in inner
        .clients
        .values()
        .filter_map(|c| c.choose_tree.as_ref())
        .filter(|chooser| chooser.preview_size != ChooserPreviewSize::Off)
    {
        let Some(item) = usize::try_from(chooser.rendered.selected)
            .ok()
            .and_then(|selected| chooser.rendered.items.get(selected))
        else {
            continue;
        };
        match item.target {
            ChooseTreeTarget::Session(session) => panes.extend(
                state
                    .sessions
                    .get(&session)
                    .into_iter()
                    .flat_map(|session| session.windows.iter())
                    .filter_map(|window| state.windows.get(window))
                    .map(|window| window.active_pane),
            ),
            ChooseTreeTarget::Window(window) => panes.extend(
                state
                    .windows
                    .get(&window)
                    .into_iter()
                    .flat_map(|window| window.panes.keys().copied()),
            ),
            ChooseTreeTarget::Pane(pane) => {
                panes.insert(pane);
            }
            ChooseTreeTarget::Client(client) if !chooser.info_preview => panes.extend(
                chooser
                    .clients
                    .iter()
                    .filter(|row| row.client == client)
                    .filter_map(|row| row.pane)
                    .filter(|pane| !(chooser.hide_source && *pane == chooser.source_pane)),
            ),
            ChooseTreeTarget::Client(_) => {}
        }
    }
    panes.retain(|pane| inner.terminals.contains_key(pane));
    panes
}

pub(super) fn refresh_preview_watches(inner: &mut ServerState) {
    let next = chooser_preview_panes(inner);
    if next == inner.panes.preview_watched {
        return;
    }
    for pane in inner.panes.preview_watched.symmetric_difference(&next) {
        if let Some(terminal) = inner.terminals.get(pane) {
            terminal.set_preview_watch(next.contains(pane));
        }
    }
    inner.panes.preview_watched = next;
}

#[cfg(feature = "agent")]
pub(super) fn visible_agent_panes(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
) -> BTreeSet<PaneId> {
    let Some(session) = inner.engine.state.sessions.get(&session) else {
        return BTreeSet::new();
    };
    let focused_window = client_focused_window(inner, client, session);
    let Some(window) = inner.engine.state.windows.get(&focused_window) else {
        return BTreeSet::new();
    };
    window
        .panes
        .keys()
        .filter(|pane| {
            window.zoomed_pane.is_none_or(|zoomed| **pane == zoomed)
                && inner.agents.agent_panes.contains(*pane)
        })
        .copied()
        .collect()
}

pub(super) fn attached_clients_for_pane(
    inner: &ServerState,
    pane: PaneId,
) -> Option<&BTreeSet<ClientId>> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let session = inner.engine.state.windows.get(&window)?.session;
    inner.attachment.attached.get(&session)
}

pub(super) fn client_is_attached_to_pane(
    inner: &ServerState,
    client: ClientId,
    pane: PaneId,
) -> bool {
    attached_clients_for_pane(inner, pane).is_some_and(|clients| clients.contains(&client))
}

pub(super) fn sync_view_area(inner: &ServerState, pane: PaneId, client: ClientId) {
    if let Some(terminal) = inner.terminals.get(&pane) {
        terminal.set_view_area(
            TerminalViewId(client.0),
            client_display_area(inner, client, pane),
        );
    }
}

pub(super) fn sync_client_view_areas(inner: &ServerState, client: ClientId) {
    for pane in inner
        .client(client)
        .and_then(|c| c.streamed_terminals.as_ref())
        .into_iter()
        .flat_map(BTreeMap::keys)
    {
        sync_view_area(inner, *pane, client);
    }
}

fn client_display_area(inner: &ServerState, client: ClientId, pane: PaneId) -> Option<(u16, u16)> {
    if let Some(size) = inner.client(client)?.size {
        return Some(size);
    }
    let window = inner.engine.state.window_for_pane(pane)?;
    inner
        .panes
        .terminal_geometries
        .iter()
        .filter(|(reported, _)| inner.engine.state.window_for_pane(**reported) == Some(window))
        .filter_map(|(reported, geometries)| {
            let geometry = geometries.get(&client)?;
            inner
                .engine
                .window_extent_for_pane_geometry(*reported, geometry.columns, geometry.rows)
        })
        .reduce(|(columns, rows), (other_columns, other_rows)| {
            (columns.max(other_columns), rows.max(other_rows))
        })
}

fn ensure_browser_attached(inner: &ServerState, pane: PaneId) -> Result<(), ServerError> {
    let window = inner
        .engine
        .state
        .window_for_pane(pane)
        .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
    let session = inner.engine.state.windows[&window].session;
    gui_client(inner, session)
        .map(|_| ())
        .ok_or(ServerError::PaneNotAttached(pane))
}
