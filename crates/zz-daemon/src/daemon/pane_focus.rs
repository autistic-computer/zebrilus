use super::*;

#[derive(Default)]
pub(super) struct PaneFocusProbe {
    pub(super) window_active_panes: BTreeMap<WindowId, PaneId>,
    pub(super) session_windows: BTreeMap<SessionId, WindowId>,
    client_sessions: BTreeMap<ClientId, SessionId>,
    client_focused: BTreeMap<ClientId, bool>,
    client_overlays: BTreeSet<ClientId>,
}

#[derive(Default)]
pub(super) struct PaneFocusEvents {
    leading: Vec<PendingHookEvent>,
    after_window_pane_changed: Vec<(WindowId, PendingHookEvent)>,
}

impl PaneFocusEvents {
    pub(super) fn is_empty(&self) -> bool {
        self.leading.is_empty() && self.after_window_pane_changed.is_empty()
    }
}

pub(super) fn capture_pane_focus_probe(inner: &ServerState) -> PaneFocusProbe {
    PaneFocusProbe {
        window_active_panes: inner
            .engine
            .state
            .windows
            .iter()
            .map(|(window, state)| (*window, state.active_pane))
            .collect(),
        session_windows: inner
            .engine
            .state
            .sessions
            .iter()
            .map(|(session, state)| (*session, state.active_window))
            .collect(),
        ..capture_client_focus_probe(inner)
    }
}

pub(super) fn capture_client_focus_probe(inner: &ServerState) -> PaneFocusProbe {
    PaneFocusProbe {
        window_active_panes: BTreeMap::new(),
        session_windows: BTreeMap::new(),
        client_sessions: inner
            .attachment
            .attached
            .iter()
            .flat_map(|(session, clients)| clients.iter().map(|client| (*client, *session)))
            .collect(),
        client_focused: inner
            .clients
            .iter()
            .filter_map(|(id, client)| client.focused.map(|focused| (*id, focused)))
            .collect(),
        client_overlays: inner
            .clients
            .iter()
            .filter_map(|(id, client)| client.kind.map(|_| *id))
            .filter(|client| any_overlay_present(inner, *client))
            .collect(),
    }
}

fn pane_is_focused(inner: &ServerState, pane: PaneId) -> bool {
    let Some(window) = inner.engine.state.window_for_pane(pane) else {
        return false;
    };
    if inner
        .engine
        .state
        .windows
        .get(&window)
        .map(|state| state.active_pane)
        != Some(pane)
    {
        return false;
    }
    inner
        .clients
        .iter()
        .filter_map(|(id, client)| (client.focused == Some(true)).then_some(*id))
        .filter(|client| !any_overlay_present(inner, *client))
        .any(|client| client_focused_window_for_attachment(inner, client) == Some(window))
}

fn pane_focus_transition(inner: &mut ServerState, pane: PaneId) -> Option<PendingHookEvent> {
    if inner.engine.state.pane(pane).is_none_or(|state| state.dead) {
        return None;
    }
    let focused = pane_is_focused(inner, pane);
    if focused == inner.attachment.pane_focus.contains(&pane) {
        return None;
    }
    let name = if focused {
        inner.attachment.pane_focus.insert(pane);
        "pane-focus-in"
    } else {
        inner.attachment.pane_focus.remove(&pane);
        "pane-focus-out"
    };
    PendingHookEvent::live_pane(name, pane, &inner.engine)
}

fn session_active_pane(inner: &ServerState, session: SessionId) -> Option<PaneId> {
    let window = inner.engine.state.sessions.get(&session)?.active_window;
    Some(inner.engine.state.windows.get(&window)?.active_pane)
}

pub(super) fn client_active_pane(inner: &ServerState, client: ClientId) -> Option<PaneId> {
    let window = client_focused_window_for_attachment(inner, client)?;
    Some(inner.engine.state.windows.get(&window)?.active_pane)
}

pub(super) fn pane_focus_candidates(
    inner: &ServerState,
    before: &PaneFocusProbe,
) -> (Vec<PaneId>, Vec<(WindowId, PaneId)>) {
    if no_pane_can_hold_focus(inner) {
        return (Vec::new(), Vec::new());
    }
    let gated = inner.engine.focus_events();
    let mut leading = Vec::new();
    let mut removals = Vec::new();
    for client in before
        .client_overlays
        .iter()
        .copied()
        .chain(
            inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.kind.map(|_| *id))
                .filter(|client| any_overlay_present(inner, *client)),
        )
        .collect::<BTreeSet<_>>()
    {
        if before.client_overlays.contains(&client) != any_overlay_present(inner, client) {
            leading.extend(client_active_pane(inner, client));
        }
    }
    for (client, session) in inner
        .attachment
        .attached
        .iter()
        .flat_map(|(session, clients)| clients.iter().map(|client| (*client, Some(*session))))
        .chain(before.client_sessions.keys().map(|client| (*client, None)))
        .collect::<BTreeMap<_, _>>()
    {
        let previous = before.client_sessions.get(&client).copied();
        let session = session.or_else(|| client_attached_session(inner, client));
        if previous == session {
            continue;
        }
        leading.extend(previous.and_then(|previous| session_active_pane(inner, previous)));
        leading.extend(session.and_then(|session| session_active_pane(inner, session)));
    }
    for (client, focused) in inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.focused.as_ref().map(|focused| (id, focused)))
    {
        if before.client_focused.get(client) != Some(focused) {
            leading.extend(client_active_pane(inner, *client));
        }
    }
    if gated {
        for (session, window) in &before.session_windows {
            let Some(current) = inner.engine.state.sessions.get(session) else {
                continue;
            };
            if current.active_window == *window {
                continue;
            }
            leading.extend(
                inner
                    .engine
                    .state
                    .windows
                    .get(window)
                    .map(|state| state.active_pane),
            );
            leading.extend(session_active_pane(inner, *session));
        }
    }
    for (window, pane) in &before.window_active_panes {
        let Some(current) = inner.engine.state.windows.get(window) else {
            continue;
        };
        if current.active_pane == *pane {
            continue;
        }
        if inner.engine.state.pane(*pane).is_some() {
            if gated {
                leading.push(*pane);
                leading.push(current.active_pane);
            }
        } else {
            removals.push((*window, current.active_pane));
        }
    }
    (leading, removals)
}

fn no_pane_can_hold_focus(inner: &ServerState) -> bool {
    inner.attachment.pane_focus.is_empty()
        && !inner
            .attachment
            .attached
            .values()
            .flatten()
            .any(|client| inner.client(*client).and_then(|c| c.focused.as_ref()) == Some(&true))
}

pub(super) fn pane_focus_hook_events(
    inner: &mut ServerState,
    before: &PaneFocusProbe,
) -> PaneFocusEvents {
    inner
        .attachment
        .pane_focus
        .retain(|pane| inner.engine.state.pane(*pane).is_some());
    let (leading, removals) = pane_focus_candidates(inner, before);
    let mut events = PaneFocusEvents::default();
    for pane in leading {
        events.leading.extend(pane_focus_transition(inner, pane));
    }
    for (window, pane) in removals {
        events
            .after_window_pane_changed
            .extend(pane_focus_transition(inner, pane).map(|event| (window, event)));
    }
    events
}

pub(super) fn splice_pane_focus_events(
    events: &mut Vec<PendingHookEvent>,
    at: usize,
    focus: PaneFocusEvents,
) {
    if focus.is_empty() {
        return;
    }
    let leading = focus.leading.len();
    events.splice(at..at, focus.leading);
    for (window, event) in focus.after_window_pane_changed {
        let anchor = events
            .iter()
            .rposition(|candidate| {
                candidate.name == "window-pane-changed"
                    && candidate.variables.get(HOOK_WINDOW_CONTEXT_FORMAT)
                        == Some(&window.to_string())
            })
            .map_or(at + leading, |index| index + 1);
        events.insert(anchor, event);
    }
}

pub(super) fn client_focused_window_for_attachment(
    inner: &ServerState,
    client: ClientId,
) -> Option<WindowId> {
    client_attached_session(inner, client)
        .and_then(|session| inner.engine.state.sessions.get(&session))
        .map(|session| client_focused_window(inner, client, session))
}

pub(super) fn client_hook_event_for_window(
    inner: &ServerState,
    name: &'static str,
    client: ClientId,
    window: WindowId,
) -> Option<PendingHookEvent> {
    let session = client_attached_session(inner, client)?;
    let window_state = inner.engine.state.windows.get(&window)?;
    if window_state.session != session {
        return None;
    }
    let pane = window_state.active_pane;
    let client_name = client_format_name(inner, client);
    Some(PendingHookEvent::client(
        name,
        ExecutionContext::new(Some(session), Some(window), Some(pane)),
        client,
        Some(client_name.as_str()),
    ))
}

pub(super) fn client_hook_event(
    inner: &ServerState,
    name: &'static str,
    client: ClientId,
) -> Option<PendingHookEvent> {
    let window = client_focused_window_for_attachment(inner, client)?;
    client_hook_event_for_window(inner, name, client, window)
}
