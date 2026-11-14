use ala::{App, Div, Hsla, IntoElement as _, ParentElement as _, Pixels, Styled as _, div, px};
use penna::{
    ActiveTheme as _, Ground, IconName, Space, StyledExt as _, TextStyle, blocks::touch::StatusMark,
};
use zz_client::{program::Program, window_summary::PaneTile};

use crate::navigation::{
    flat::pane_glyph,
    pane_stack::{attention_tone, pane_face, ringed_dot, stack_radius},
};

pub const LEAD_FACE: f32 = 24.0;
pub const TRAIL_FACE: f32 = 16.0;
pub const PILL_GLYPH: f32 = 20.0;
const FACE_DOT: f32 = 8.0;
const TRAILING: usize = 3;
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
pub const fn trailing_split(panes: usize) -> (usize, usize) {
    let shown = if panes < TRAILING { panes } else { TRAILING };
    (shown, panes - shown)
}

#[must_use]
pub fn trailing_faces(tiles: &[PaneTile], ground: Ground, cx: &App) -> Div {
    let muted = penna::muted_on(ground, cx);
    let (shown, more) = trailing_split(tiles.len());
    let size = TRAIL_FACE;
    let radius = stack_radius(px(size), cx);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(Space::S4)
        .children(
            tiles
                .iter()
                .take(shown)
                .map(|tile| face(tile, size, size * GLYPH_SCALE, radius, ground, cx)),
        )
        .children((more > 0).then(|| {
            div()
                .flex_none()
                .type_style_strong(TextStyle::TouchSmall, cx)
                .text_color(muted)
                .child(format!("+{more}"))
                .into_any_element()
        }))
}

#[cfg(test)]
mod tests {
    use super::trailing_split;

    #[test]
    fn a_window_shows_three_faces_then_counts_the_rest() {
        assert_eq!(trailing_split(2), (2, 0));
        assert_eq!(trailing_split(3), (3, 0));
        assert_eq!(trailing_split(5), (3, 2));
    }
}
