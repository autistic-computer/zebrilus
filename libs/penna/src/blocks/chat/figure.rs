use ala::{Hsla, Pixels, px};

use crate::{CardLook, Colorize as _, Emphasis, Ground, PaintState, Surface, Theme, Tone};

const DIAGRAM_MAX_RADIUS: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiagramPaint {
    pub ground: Hsla,
    pub group: Hsla,
    pub group_deep: Hsla,
    pub group_edge: Hsla,
    pub node: Hsla,
    pub node_edge: Hsla,
    pub radius: Pixels,
}

#[must_use]
pub fn diagram_paint(theme: &Theme) -> DiagramPaint {
    let ground = Ground {
        fill: theme.background,
        ..theme.ground()
    };
    let base = ground.fill.opaque();
    let group = Surface::from(CardLook::Activity).on(ground);
    let group_fill = base.blend(theme.layer(group, PaintState::Rest).fill);
    let group_deep = base.blend(theme.layer(group, PaintState::Hover).fill);
    let wash = theme.paint_on(ground, Tone::Accent, Emphasis::Soft);
    let group_edge = theme.foreground.faint();
    let node_edge = if theme.outlined() {
        base.blend(
            theme
                .paint_on(ground, Tone::Accent, Emphasis::Outline)
                .border,
        )
    } else {
        base.blend(wash.active)
    };
    DiagramPaint {
        ground: base,
        group: group_fill,
        group_deep,
        group_edge,
        node: base.blend(wash.fill),
        node_edge,
        radius: theme.control_radius().min(px(DIAGRAM_MAX_RADIUS)),
    }
}
