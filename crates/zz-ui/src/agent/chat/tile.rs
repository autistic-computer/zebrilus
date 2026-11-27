use super::*;

pub(super) fn agent_title(item: &zz_protocol::chat::ItemView) -> String {
    if item.body.title.is_empty() {
        "Subagent".to_owned()
    } else {
        item.body.title.clone()
    }
}

pub(super) fn agent_attention(state: Option<&AgentState>) -> Attention {
    match state {
        Some(AgentState::Working { .. }) => Attention::Working,
        Some(AgentState::Waiting) => Attention::NeedsYou,
        Some(AgentState::Done { .. }) => Attention::Done,
        Some(AgentState::Failed { .. }) => Attention::Failed,
        Some(AgentState::Stopped) | None => Attention::None,
    }
}

pub(super) fn agent_status(state: Option<&AgentState>) -> Option<&'static str> {
    Some(match state? {
        AgentState::Working { .. } => "Working",
        AgentState::Waiting => "Waiting for you",
        AgentState::Done { .. } => "Done",
        AgentState::Failed { .. } => "Failed",
        AgentState::Stopped => "Stopped",
    })
}

pub(super) fn agent_tile(
    vendor: Option<&'static Program>,
    attention: Attention,
    ground: penna::Ground,
    size: f32,
    cx: &App,
) -> Div {
    tile(vendor, IconName::Bot, attention, ground, size, cx)
}

pub(super) fn tile(
    vendor: Option<&'static Program>,
    glyph: IconName,
    attention: Attention,
    ground: penna::Ground,
    tile: f32,
    cx: &App,
) -> Div {
    let size = px(tile);
    let mark = px((tile * TILE_MARK_SHARE).round());
    let tile = match vendor.filter(|_| crate::program::program_colors(cx)) {
        Some(program) => crate::program::program_tile(program, size, 0.0, cx),
        None => div()
            .flex_none()
            .size(size)
            .flex()
            .items_center()
            .justify_center()
            .rounded(size * TILE_CORNER_SHARE)
            .bg(cx.theme().paint(Tone::Neutral, Emphasis::Soft).fill)
            .child(
                vendor
                    .and_then(|program| {
                        crate::program::program_mark(program, mark, cx.theme().foreground, None)
                    })
                    .unwrap_or_else(|| Icon::new(glyph).size(mark).into_any_element()),
            ),
    };
    tile.relative()
        .children(status_dot(attention, size, ground.fill, cx))
}
