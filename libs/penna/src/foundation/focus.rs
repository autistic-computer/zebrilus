use ala::{Corners, Edges, ParentElement, Pixels, StyleRefinement, Styled, Window, div};

use crate::{FOCUS_RING_WIDTH, Paint, StyledExt as _};

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
    .map(|v| *v + FOCUS_RING_WIDTH);

    let mut inner_style = StyleRefinement::default();
    inner_style.corner_radii.top_left = Some(radius.top_left.into());
    inner_style.corner_radii.top_right = Some(radius.top_right.into());
    inner_style.corner_radii.bottom_left = Some(radius.bottom_left.into());
    inner_style.corner_radii.bottom_right = Some(radius.bottom_right.into());

    let inset = FOCUS_RING_WIDTH + margins;

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
