use ala::{App, Div, Hsla, ParentElement as _, Pixels, Styled as _, div, px};
use penna::{
    ActiveTheme as _, Ground, Icon, IconName, Space, StyledExt as _, TextStyle,
    blocks::touch::StatusMark,
};
use zz_client::{
    program::Program,
    window_summary::{Attention, PaneTile},
};

use crate::navigation::{
    flat::pane_glyph,
    pane_stack::{attention_tone, pane_face, ringed_dot, stack_radius, status_dot},
};

pub const LEAD_FACE: f32 = 24.0;
pub const PILL_GLYPH: f32 = 20.0;
pub const WINDOW_FACE: f32 = 24.0;
const MACHINE_GLYPH: f32 = 20.0;
const FACE_DOT: f32 = 8.0;
const WINDOW_FACES: usize = 5;
const WINDOW_RING: f32 = 2.0;
const WINDOW_STEP: f32 = 20.0;
const GLYPH_SCALE: f32 = crate::program::PROGRAM_GLYPH_SCALE;

fn face(tile: &PaneTile, size: f32, glyph: f32, radius: Pixels, ground: Ground, cx: &App) -> Div {
    pane_face(
        tile.program,
        pane_glyph(tile.tag),
        px(size),
        px(glyph),
        radius,
        ground,
        cx,
    )
}

#[must_use]
pub fn lead_face(tile: &PaneTile, dot: Option<Hsla>, ground: Ground, cx: &App) -> Div {
    let size = LEAD_FACE;
    face(
        tile,
        size,
        size * GLYPH_SCALE,
        stack_radius(px(size), cx),
        ground,
        cx,
    )
    .relative()
    .children(dot.map(|color| ringed_dot(color, px(FACE_DOT), ground.fill)))
}

#[must_use]
pub fn machine_face(platform: zz_protocol::HostPlatform, dot: Option<Hsla>, ground: Ground) -> Div {
    let icon = crate::program::platform_icon(platform).unwrap_or(IconName::Server);
    div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(LEAD_FACE))
        .child(Icon::new(icon).size(px(MACHINE_GLYPH)))
        .children(dot.map(|color| ringed_dot(color, px(FACE_DOT), ground.fill)))
}

#[must_use]
pub fn mark_dot(mark: Option<StatusMark>, cx: &App) -> Option<Hsla> {
    mark.and_then(|mark| mark.dot(cx))
}

#[must_use]
pub fn attention_dot(tile: &PaneTile, cx: &App) -> Option<Hsla> {
    attention_tone(tile.attention).map(|tone| cx.theme().tone(tone))
}

#[must_use]
pub fn pill_face(
    program: &'static Program,
    size: f32,
    radius: Pixels,
    ground: Ground,
    cx: &App,
) -> Div {
    pane_face(
        Some(program),
        IconName::Terminal,
        px(size),
        px(PILL_GLYPH),
        radius,
        ground,
        cx,
    )
}

#[must_use]
pub fn window_faces(windows: &[(PaneTile, Attention)], ground: Ground, cx: &App) -> Div {
    let muted = penna::muted_on(ground, cx);
    let size = WINDOW_FACE;
    let ringed = size + 2.0 * WINDOW_RING;
    let radius = stack_radius(px(size), cx);
    let ring_radius = stack_radius(px(ringed), cx);
    let shown = windows.len().min(WINDOW_FACES);
    let more = windows.len() - shown;
    let width = ringed + shown.saturating_sub(1) as f32 * WINDOW_STEP;
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(Space::S6)
        .child(
            div()
                .relative()
                .flex_none()
                .w(px(width))
                .h(px(ringed))
                .children(windows.iter().take(shown).enumerate().rev().map(
                    |(slot, (tile, attention))| {
                        div()
                            .absolute()
                            .top_0()
                            .left(px(slot as f32 * WINDOW_STEP))
                            .p(px(WINDOW_RING))
                            .rounded(ring_radius)
                            .bg(ground.fill)
                            .child(
                                face(tile, size, size * GLYPH_SCALE, radius, ground, cx)
                                    .relative()
                                    .children(status_dot(*attention, px(size), ground.fill, cx)),
                            )
                    },
                )),
        )
        .children((more > 0).then(|| {
            div()
                .flex_none()
                .type_style_strong(TextStyle::TouchSmall, cx)
                .text_color(muted)
                .child(format!("+{more}"))
        }))
}
