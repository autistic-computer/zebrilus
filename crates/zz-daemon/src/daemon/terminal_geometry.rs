use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalGeometry {
    pub(super) columns: u16,
    pub(super) rows: u16,
    pub(super) cell_width_px: u32,
    pub(super) cell_height_px: u32,
}

pub(super) fn terminal_geometry_owner(inner: &ServerState, pane: PaneId) -> Option<ClientId> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let session = inner.engine.state.windows.get(&window)?.session;
    inner
        .attachment
        .attached
        .get(&session)?
        .iter()
        .filter(|client| {
            inner.client(**client).and_then(|c| c.kind) == Some(ClientKind::Control)
                || inner
                    .client(**client)
                    .and_then(|c| c.visible_terminals.as_ref())
                    .is_some_and(|visible| visible.contains(&pane))
        })
        .filter(|client| client_terminal_geometry(inner, **client, pane).is_some())
        .min_by_key(|client| {
            (
                Reverse(
                    inner
                        .client(**client)
                        .and_then(|c| c.terminal_input_sequence.as_ref())
                        .copied()
                        .unwrap_or_default(),
                ),
                client.0,
            )
        })
        .copied()
}

pub(super) fn window_geometry_owner(
    inner: &ServerState,
    window: &zz_mux::Window,
) -> Option<(PaneId, ClientId)> {
    window
        .panes
        .keys()
        .find_map(|pane| Some((*pane, terminal_geometry_owner(inner, *pane)?)))
}

fn client_terminal_geometry(
    inner: &ServerState,
    client: ClientId,
    pane: PaneId,
) -> Option<TerminalGeometry> {
    if inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control) {
        return inner
            .panes
            .terminal_geometries
            .get(&pane)?
            .get(&client)
            .copied();
    }
    let window = inner.engine.state.window_for_pane(pane)?;
    let session = inner.engine.state.windows.get(&window)?.session;
    if client_attached_session(inner, client) != Some(session) {
        return None;
    }
    let geometry = control_client_geometry(inner, client, window)?;
    let (columns, rows) =
        inner
            .engine
            .pane_geometry_at_window_extent(pane, geometry.columns, geometry.rows)?;
    Some(TerminalGeometry {
        columns,
        rows,
        ..geometry
    })
}

#[cfg(test)]
pub(super) fn aggressive_terminal_geometry(
    inner: &ServerState,
    pane: PaneId,
) -> Option<TerminalGeometry> {
    terminal_geometry_for_mode(inner, pane, true, WindowSize::Smallest)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GeometrySource {
    ClientReport,
    LaidOut,
}

#[cfg(test)]
pub(super) fn terminal_geometry_for_mode(
    inner: &ServerState,
    pane: PaneId,
    aggressive: bool,
    mode: WindowSize,
) -> Option<TerminalGeometry> {
    terminal_geometry_for_mode_from(inner, pane, aggressive, mode, GeometrySource::ClientReport)
}

fn terminal_geometry_for_mode_from(
    inner: &ServerState,
    pane: PaneId,
    aggressive: bool,
    mode: WindowSize,
    source: GeometrySource,
) -> Option<TerminalGeometry> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let window_state = inner.engine.state.windows.get(&window)?;
    let floating = window_state.is_floating(pane);
    if floating && (source == GeometrySource::LaidOut || window_state.zoomed_pane != Some(pane)) {
        return floating_terminal_geometry(inner, window_state, pane);
    }
    if window_state
        .zoomed_pane
        .is_some_and(|zoomed| zoomed != pane)
    {
        return None;
    }
    let session = window_state.session;
    let suppress_ignored = unignored_attached_sizing_client_exists(inner);
    let place = (source == GeometrySource::ClientReport)
        .then(|| pane_place(inner, pane))
        .flatten();
    let drawn = !inner.terminals.contains_key(&pane);
    let candidates = inner
        .attachment
        .attached
        .get(&session)?
        .iter()
        .filter(|client| client_is_sizing_candidate(inner, **client, suppress_ignored))
        .filter(|client| {
            if aggressive || drawn {
                client_focused_window_for_attachment(inner, **client) == Some(window)
            } else {
                inner.client(**client).and_then(|c| c.kind) == Some(ClientKind::Control)
                    || inner
                        .client(**client)
                        .and_then(|c| c.visible_terminals.as_ref())
                        .is_some_and(|visible| visible.contains(&pane))
            }
        })
        .filter_map(|client| {
            let geometry = client_terminal_geometry(inner, *client, pane)?;
            let reported = inner
                .panes
                .reported_pane_places
                .get(&pane)
                .and_then(|places| places.get(client));
            if floating && reported.map(|reported| &reported.place) != place.as_ref() {
                return None;
            }
            let Some((columns, rows)) = place.as_ref().and_then(|place| {
                let reported = reported?;
                (reported.place != *place).then_some(reported.extent)
            }) else {
                return Some((*client, geometry));
            };
            let (columns, rows) = inner
                .engine
                .pane_geometry_at_window_extent(pane, columns, rows)?;
            Some((
                *client,
                TerminalGeometry {
                    columns,
                    rows,
                    ..geometry
                },
            ))
        })
        .collect::<Vec<_>>();
    let owner = candidates.iter().min_by_key(|(client, _)| {
        (
            Reverse(
                inner
                    .client(*client)
                    .and_then(|c| c.terminal_input_sequence.as_ref())
                    .copied()
                    .unwrap_or_default(),
            ),
            client.0,
        )
    })?;
    let mut geometry = if mode == WindowSize::Manual || source == GeometrySource::LaidOut {
        let (columns, rows) = inner.engine.pane_geometry(pane)?;
        TerminalGeometry {
            columns,
            rows,
            ..owner.1
        }
    } else if mode == WindowSize::Latest {
        owner.1
    } else {
        let first_geometry = candidates.first()?.1;
        let mut columns = first_geometry.columns;
        let mut rows = first_geometry.rows;
        for (_, geometry) in &candidates[1..] {
            if mode == WindowSize::Largest {
                columns = columns.max(geometry.columns);
                rows = rows.max(geometry.rows);
            } else {
                columns = columns.min(geometry.columns);
                rows = rows.min(geometry.rows);
            }
        }
        TerminalGeometry {
            columns,
            rows,
            ..owner.1
        }
    };
    if mode != WindowSize::Manual {
        for (client, _) in &candidates {
            let Some(ceiling) = inner
                .client(*client)
                .and_then(|c| c.control_output.as_ref())
                .and_then(|output| output.window_geometries.get(&window))
            else {
                continue;
            };
            let (columns, rows) =
                inner
                    .engine
                    .pane_geometry_at_window_extent(pane, ceiling.columns, ceiling.rows)?;
            geometry.columns = geometry.columns.min(columns);
            geometry.rows = geometry.rows.min(rows);
        }
    }
    Some(geometry)
}

fn floating_terminal_geometry(
    inner: &ServerState,
    window: &zz_mux::Window,
    pane: PaneId,
) -> Option<TerminalGeometry> {
    if !window.pane_is_visible(pane) {
        return None;
    }
    let (columns, rows) = inner.engine.pane_geometry(pane)?;
    let (cell_width_px, cell_height_px) = window_geometry_owner(inner, window)
        .and_then(|(reported, owner)| client_terminal_geometry(inner, owner, reported))
        .map(|geometry| (geometry.cell_width_px, geometry.cell_height_px))
        .or_else(|| {
            inner
                .attachment
                .attached
                .get(&window.session)
                .into_iter()
                .flatten()
                .find_map(|client| inner.client(*client).and_then(|c| c.cell_pixels))
        })
        .unwrap_or_default();
    Some(TerminalGeometry {
        columns,
        rows,
        cell_width_px,
        cell_height_px,
    })
}

pub(super) fn terminal_resize_for_pane(
    inner: &ServerState,
    pane: PaneId,
) -> Option<(Arc<TerminalSession>, TerminalGeometry)> {
    let geometry = pane_geometry_from(inner, pane, GeometrySource::LaidOut)?;
    let terminal = inner.terminals.get(&pane).cloned()?;
    Some((terminal, geometry))
}

pub(super) fn pane_geometry_from(
    inner: &ServerState,
    pane: PaneId,
    source: GeometrySource,
) -> Option<TerminalGeometry> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let aggressive = inner.engine.state.window_aggressive_resize(window).ok()?;
    let mode = inner.engine.window_size(window);
    terminal_geometry_for_mode_from(inner, pane, aggressive, mode, source)
}

#[derive(Clone, Debug, PartialEq)]
struct PanePlace {
    path: Vec<(SplitId, bool)>,
    zoomed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ReportedPlace {
    place: PanePlace,
    extent: (u16, u16),
}

pub(super) fn record_report_place(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
    extent: Option<(u16, u16)>,
) {
    let reported = pane_place(inner, pane)
        .zip(extent)
        .map(|(place, extent)| ReportedPlace { place, extent });
    let places = inner.panes.reported_pane_places.entry(pane).or_default();
    match reported {
        Some(reported) => {
            places.insert(client, reported);
        }
        None => {
            places.remove(&client);
        }
    }
}

fn pane_place(inner: &ServerState, pane: PaneId) -> Option<PanePlace> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let window = inner.engine.state.windows.get(&window)?;
    if window.is_floating(pane) {
        return Some(PanePlace {
            path: Vec::new(),
            zoomed: window.zoomed_pane == Some(pane),
        });
    }
    let mut node = window.layout.project();
    let mut path = Vec::new();
    loop {
        match node {
            zz_protocol::LayoutNode::Pane(found) => {
                return (found == pane).then(|| PanePlace {
                    path,
                    zoomed: window.zoomed_pane == Some(pane),
                });
            }
            zz_protocol::LayoutNode::Split {
                id, first, second, ..
            } => {
                let in_first = first.contains(pane);
                path.push((id, in_first));
                node = if in_first { *first } else { *second };
            }
            zz_protocol::LayoutNode::Empty => return None,
        }
    }
}

pub(super) fn write_back_terminal_geometries(
    inner: &mut ServerState,
    panes: &BTreeSet<PaneId>,
) -> bool {
    let measurements = panes
        .iter()
        .filter_map(|pane| {
            if !inner.terminals.contains_key(pane) {
                return None;
            }
            pane_geometry_from(inner, *pane, GeometrySource::ClientReport)
                .map(|geometry| (*pane, geometry.columns, geometry.rows))
        })
        .collect::<Vec<_>>();
    measurements
        .into_iter()
        .fold(false, |changed, (pane, columns, rows)| {
            inner.engine.set_pane_geometry(pane, columns, rows) || changed
        })
}

pub(super) fn mark_client_terminal_latest(inner: &mut ServerState, client: ClientId) {
    let sequence = inner
        .panes
        .terminal_input_sequence
        .checked_add(1)
        .expect("terminal input sequence exhausted");
    inner.panes.terminal_input_sequence = sequence;
    inner
        .client_entry(client)
        .terminal_input_sequence
        .replace(sequence);
}

pub(super) fn mouse_pane_cell(
    inner: &ServerState,
    pane: PaneId,
    mouse: &MouseEventTarget,
) -> Option<(usize, usize)> {
    let geometry = inner.engine.pane_format_geometry(None, None, Some(pane));
    let (left, top) = (geometry.pane_left?, geometry.pane_top?);
    let (x, y) = (mouse.column.checked_sub(left)?, mouse.row.checked_sub(top)?);
    if geometry.pane_right.is_none_or(|right| mouse.column > right)
        || geometry.pane_bottom.is_none_or(|bottom| mouse.row > bottom)
    {
        return None;
    }
    Some((usize::from(x), usize::from(y)))
}

#[cfg(all(test, unix))]
pub(super) fn wait_for_terminal_identity(terminal: &TerminalSession) {
    terminal.wait_for_identity(Duration::from_secs(2));
}

pub(super) fn terminal_resizes_for_panes(
    inner: &ServerState,
    panes: &BTreeSet<PaneId>,
) -> Vec<(Arc<TerminalSession>, TerminalGeometry)> {
    panes
        .iter()
        .filter_map(|pane| terminal_resize_for_pane(inner, *pane))
        .collect()
}

pub(super) fn apply_terminal_resizes(resizes: Vec<(Arc<TerminalSession>, TerminalGeometry)>) {
    for (terminal, geometry) in resizes {
        terminal.resize(
            geometry.columns,
            geometry.rows,
            geometry.cell_width_px,
            geometry.cell_height_px,
        );
    }
}
