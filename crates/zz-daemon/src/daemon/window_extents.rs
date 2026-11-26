use super::*;

fn attached_client_extents(
    inner: &ServerState,
    session: SessionId,
    window: WindowId,
    mode: WindowSize,
) -> (Option<(u16, u16)>, Vec<(u16, u16)>) {
    let suppress_ignored = unignored_attached_sizing_client_exists(inner);
    let mut candidates = Vec::new();
    let mut ceilings = Vec::new();
    for client in inner
        .attachment
        .attached
        .get(&session)
        .into_iter()
        .flatten()
        .copied()
    {
        if !client_is_sizing_candidate(inner, client, suppress_ignored) {
            continue;
        }
        let extent = match inner.client(client).and_then(|c| c.kind) {
            Some(ClientKind::Interactive) => {
                interactive_client_window_extent(inner, client, session, window)
            }
            Some(ClientKind::Control) => {
                let output = inner.client(client).and_then(|c| c.control_output.as_ref());
                if let Some(geometry) =
                    output.and_then(|output| output.window_geometries.get(&window).copied())
                {
                    ceilings.push((geometry.columns, geometry.rows));
                    Some((geometry.columns, geometry.rows))
                } else {
                    output
                        .and_then(|output| output.geometry)
                        .map(|geometry| (geometry.columns, geometry.rows))
                }
            }
            Some(ClientKind::Command) | None => None,
        };
        if let Some(extent) = extent {
            candidates.push(extent);
        }
    }
    let measured = candidates.into_iter().reduce(|left, right| {
        if mode == WindowSize::Smallest {
            (left.0.min(right.0), left.1.min(right.1))
        } else {
            (left.0.max(right.0), left.1.max(right.1))
        }
    });
    (measured, ceilings)
}

fn clamped_window_extent(extent: (u16, u16), ceilings: &[(u16, u16)]) -> (u16, u16) {
    let (mut columns, mut rows) = extent;
    for (ceiling_columns, ceiling_rows) in ceilings {
        columns = columns.min(*ceiling_columns);
        rows = rows.min(*ceiling_rows);
    }
    (
        columns.clamp(1, WINDOW_SIZE_MAXIMUM),
        rows.clamp(1, WINDOW_SIZE_MAXIMUM),
    )
}

fn measured_client_window_extent(
    inner: &ServerState,
    window: WindowId,
    mode: WindowSize,
) -> Option<(u16, u16)> {
    let session = inner.engine.state.windows.get(&window)?.session;
    let (measured, ceilings) = attached_client_extents(inner, session, window, mode);
    Some(clamped_window_extent(measured?, &ceilings))
}

pub(super) fn attached_client_window_extent(
    inner: &ServerState,
    window: WindowId,
    mode: WindowSize,
) -> Option<(u16, u16)> {
    let session = inner.engine.state.windows.get(&window)?.session;
    let (measured, ceilings) = attached_client_extents(inner, session, window, mode);
    let extent = measured.unwrap_or_else(|| inner.engine.default_size_for_session(session));
    Some(clamped_window_extent(extent, &ceilings))
}

pub(super) fn recalculate_window_extents(
    inner: &mut ServerState,
    session: SessionId,
) -> BTreeSet<WindowId> {
    let windows = inner
        .engine
        .state
        .sessions
        .get(&session)
        .map(|state| state.windows.clone())
        .unwrap_or_default();
    let mut resized = BTreeSet::new();
    for window in windows {
        let mode = inner.engine.window_size(window);
        let Some((columns, rows)) = measured_client_window_extent(inner, window, mode) else {
            continue;
        };
        if inner.engine.resize_window_to_extent(window, columns, rows) {
            resized.insert(window);
        }
    }
    resized
}
