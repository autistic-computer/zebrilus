use ala::{Corners, Edges, Hsla, ParentElement, Pixels, StyleRefinement, Styled, Window, div, px};

use super::color::oklab::rgb_to_oklab;
use crate::{FOCUS_RING_WIDTH, Paint, StyledExt as _};

const SAME_INK: f32 = 0.1;
const SOLID: f32 = 0.5;

pub(crate) fn ring_gap(fill: Hsla, ring: Hsla) -> Pixels {
    if fill.a < SOLID || ring.a <= 0.0 {
        return px(0.);
    }
    let (fl, fa, fb) = rgb_to_oklab(Hsla { a: 1.0, ..fill }.into());
    let (rl, ra, rb) = rgb_to_oklab(Hsla { a: 1.0, ..ring }.into());
    let distance = ((fl - rl).powi(2) + (fa - ra).powi(2) + (fb - rb).powi(2)).sqrt();
    if distance < SAME_INK {
        FOCUS_RING_WIDTH
    } else {
        px(0.)
    }
}

pub(crate) fn with_focus_ring<T: ParentElement + Styled + Sized>(
    mut this: T,
    is_focused: bool,
    margins: Pixels,
    paint: &Paint,
    smoothing: f32,
    window: &Window,
) -> T {
    if !is_focused {
        return this;
    }

    let rem_size = window.rem_size();
    let style = this.style();
    let fill = style
        .background
        .as_ref()
        .and_then(|fill| fill.color())
        .and_then(|fill| fill.as_solid());
    let inset =
        FOCUS_RING_WIDTH + margins + fill.map_or(px(0.), |fill| ring_gap(fill, paint.focus));

    let border_widths = Edges::<Pixels> {
        top: style
            .border_widths
            .top
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        bottom: style
            .border_widths
            .bottom
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        left: style
            .border_widths
            .left
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        right: style
            .border_widths
            .right
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
    };

    let radius = Corners::<Pixels> {
        top_left: style
            .corner_radii
            .top_left
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        top_right: style
            .corner_radii
            .top_right
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        bottom_left: style
            .corner_radii
            .bottom_left
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        bottom_right: style
            .corner_radii
            .bottom_right
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
    }
    .map(|v| *v + inset);

    let mut inner_style = StyleRefinement::default();
    inner_style.corner_share = style.corner_share;
    inner_style.corner_radii.top_left = Some(radius.top_left.into());
    inner_style.corner_radii.top_right = Some(radius.top_right.into());
    inner_style.corner_radii.bottom_left = Some(radius.bottom_left.into());
    inner_style.corner_radii.bottom_right = Some(radius.bottom_right.into());

    this.child(
        div()
            .flex_none()
            .absolute()
            .top(-(inset + border_widths.top))
            .left(-(inset + border_widths.left))
            .right(-(inset + border_widths.right))
            .bottom(-(inset + border_widths.bottom))
            .focus_ring(paint)
            .corner_smoothing(smoothing)
            .refine_style(&inner_style),
    )
}
