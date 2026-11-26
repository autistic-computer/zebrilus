use super::*;

pub(super) fn mode_requests(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    job_needs: FormatNeeds,
) -> Vec<crate::status::ModeRequest> {
    let copy = inner
        .client(client)
        .and_then(|c| c.copy_session.as_ref())
        .filter(|copy| !copy.exiting)
        .and_then(|copy| Some((copy.pane, inner.terminals.get(&copy.pane)?, false)));
    let view = inner
        .client(client)
        .and_then(|c| c.command_output.as_ref())
        .map(|output| (output.pane, &output.terminal, true));
    let mut shown = copy
        .into_iter()
        .chain(view)
        .filter_map(|(pane, terminal, view)| {
            mode_request_position(client, terminal, view).map(|position| (pane, view, position))
        })
        .peekable();
    if shown.peek().is_none() {
        return Vec::new();
    }
    let needs = inner
        .engine
        .cached_format_needs(crate::status::MODE_FORMATS)
        | job_needs;
    let contexts = inner
        .engine
        .format_context_snapshot(FormatClient::Attached(session));
    shown
        .filter_map(|(pane, view, position)| {
            mode_request(
                inner,
                &contexts,
                needs,
                !job_needs.is_empty(),
                session,
                pane,
                view,
                position,
            )
        })
        .collect()
}

pub(super) fn mode_request(
    inner: &ServerState,
    contexts: &zz_mux::FormatContextSnapshot<'_>,
    needs: FormatNeeds,
    dynamic_jobs: bool,
    session: SessionId,
    pane: PaneId,
    view: bool,
    shown: ModeShown,
) -> Option<crate::status::ModeRequest> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let context = contexts.status_context(Some(session), Some(window), Some(pane));
    let context = if dynamic_jobs {
        context.detach(needs)
    } else {
        context.detach_with_templates(needs, crate::status::MODE_FORMATS)
    };
    Some(crate::status::ModeRequest {
        pane,
        view,
        context,
        position: shown.position,
        limit: shown.limit,
        line_numbers: shown.line_numbers,
        hide_position: shown.hide_position,
        rows: shown.rows,
        vi_keys: inner
            .engine
            .copy_mode_table_for_pane(pane)
            .is_ok_and(|table| table == "copy-mode-vi"),
    })
}

pub(super) const fn mode_kind(mode: TerminalMode) -> u8 {
    match mode {
        TerminalMode::Live => 0,
        TerminalMode::Copy {
            hide_position: false,
            ..
        } => 1,
        TerminalMode::Copy {
            hide_position: true,
            ..
        } => 2,
        TerminalMode::View { .. } => 3,
    }
}

#[derive(Clone, Copy)]
pub(super) struct ModeShown {
    pub(super) position: u32,
    pub(super) limit: u32,
    pub(super) line_numbers: u8,
    pub(super) hide_position: bool,
    pub(super) rows: u32,
}

fn mode_request_position(
    client: ClientId,
    terminal: &TerminalSession,
    view: bool,
) -> Option<ModeShown> {
    let viewport = terminal.latest_viewport_for(TerminalViewId(client.0))?;
    let line_numbers = if view {
        0
    } else {
        terminal
            .copy_mode_facts(TerminalViewId(client.0))
            .map_or(0, |facts| facts.line_numbers)
    };
    let (shown, hide_position) = match viewport.mode {
        TerminalMode::Copy { hide_position, .. } => (
            !view && (!hide_position || line_numbers != 0),
            hide_position,
        ),
        TerminalMode::View { .. } => (view, false),
        TerminalMode::Live => (false, false),
    };
    if !shown {
        return None;
    }
    let limit = viewport
        .scrollbar
        .total
        .saturating_sub(viewport.scrollbar.len);
    Some(ModeShown {
        position: limit.saturating_sub(viewport.scrollbar.offset),
        limit,
        line_numbers,
        hide_position,
        rows: viewport.scrollbar.len,
    })
}

pub(super) fn resolve_popup_client(
    inner: &ServerState,
    invoking_client: ClientId,
    context: &ExecutionContext,
    target: Option<&str>,
) -> Result<ClientId, ServerError> {
    let eligible = inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.subscriber.as_ref().map(|_| id))
        .copied()
        .filter(|client| client_attached_session(inner, *client).is_some())
        .collect::<Vec<_>>();
    if let Some(target) = target {
        return find_attached_client_with_aliases(inner, target, true)
            .ok_or_else(|| client_target_error(target));
    }
    if eligible.contains(&invoking_client) {
        return Ok(invoking_client);
    }
    if let Some(owner) = context.pane.and_then(|pane| {
        terminal_geometry_owner(inner, pane).or_else(|| {
            let window = inner.engine.state.window_for_pane(pane)?;
            window_geometry_owner(inner, inner.engine.state.windows.get(&window)?)
                .map(|(_, owner)| owner)
        })
    }) && eligible.contains(&owner)
    {
        return Ok(owner);
    }
    eligible
        .into_iter()
        .next()
        .ok_or_else(|| ServerError::InvalidCommand("no current client".to_owned()))
}

pub(super) const POPUP_CENTRE_X_CONTEXT_FORMAT: &str = "popup_centre_x";

pub(super) const POPUP_CENTRE_Y_CONTEXT_FORMAT: &str = "popup_centre_y";

pub(super) const POPUP_HEIGHT_CONTEXT_FORMAT: &str = "popup_height";

pub(super) const POPUP_LAST_X_CONTEXT_FORMAT: &str = "popup_last_x";

pub(super) const POPUP_LAST_Y_CONTEXT_FORMAT: &str = "popup_last_y";

pub(super) const POPUP_MOUSE_BOTTOM_CONTEXT_FORMAT: &str = "popup_mouse_bottom";

pub(super) const POPUP_MOUSE_CENTRE_X_CONTEXT_FORMAT: &str = "popup_mouse_centre_x";

pub(super) const POPUP_MOUSE_CENTRE_Y_CONTEXT_FORMAT: &str = "popup_mouse_centre_y";

pub(super) const POPUP_MOUSE_TOP_CONTEXT_FORMAT: &str = "popup_mouse_top";

pub(super) const POPUP_MOUSE_X_CONTEXT_FORMAT: &str = "popup_mouse_x";

pub(super) const POPUP_MOUSE_Y_CONTEXT_FORMAT: &str = "popup_mouse_y";

pub(super) const POPUP_PANE_BOTTOM_CONTEXT_FORMAT: &str = "popup_pane_bottom";

pub(super) const POPUP_PANE_LEFT_CONTEXT_FORMAT: &str = "popup_pane_left";

pub(super) const POPUP_PANE_RIGHT_CONTEXT_FORMAT: &str = "popup_pane_right";

pub(super) const POPUP_PANE_TOP_CONTEXT_FORMAT: &str = "popup_pane_top";

pub(super) const POPUP_STATUS_LINE_Y_CONTEXT_FORMAT: &str = "popup_status_line_y";

pub(super) const POPUP_WIDTH_CONTEXT_FORMAT: &str = "popup_width";

pub(super) const POPUP_WINDOW_STATUS_LINE_X_CONTEXT_FORMAT: &str = "popup_window_status_line_x";

pub(super) const POPUP_WINDOW_STATUS_LINE_Y_CONTEXT_FORMAT: &str = "popup_window_status_line_y";

#[cfg(test)]
pub(super) const POPUP_POSITION_CONTEXT_FORMATS: [&str; 19] = [
    POPUP_CENTRE_X_CONTEXT_FORMAT,
    POPUP_CENTRE_Y_CONTEXT_FORMAT,
    POPUP_HEIGHT_CONTEXT_FORMAT,
    POPUP_LAST_X_CONTEXT_FORMAT,
    POPUP_LAST_Y_CONTEXT_FORMAT,
    POPUP_MOUSE_BOTTOM_CONTEXT_FORMAT,
    POPUP_MOUSE_CENTRE_X_CONTEXT_FORMAT,
    POPUP_MOUSE_CENTRE_Y_CONTEXT_FORMAT,
    POPUP_MOUSE_TOP_CONTEXT_FORMAT,
    POPUP_MOUSE_X_CONTEXT_FORMAT,
    POPUP_MOUSE_Y_CONTEXT_FORMAT,
    POPUP_PANE_BOTTOM_CONTEXT_FORMAT,
    POPUP_PANE_LEFT_CONTEXT_FORMAT,
    POPUP_PANE_RIGHT_CONTEXT_FORMAT,
    POPUP_PANE_TOP_CONTEXT_FORMAT,
    POPUP_STATUS_LINE_Y_CONTEXT_FORMAT,
    POPUP_WIDTH_CONTEXT_FORMAT,
    POPUP_WINDOW_STATUS_LINE_X_CONTEXT_FORMAT,
    POPUP_WINDOW_STATUS_LINE_Y_CONTEXT_FORMAT,
];
