use std::rc::Rc;

use zz_gpui::{
    Bounds, Hsla, IntoElement, Pixels, Styled, canvas, fill, linear_color_stop, linear_gradient,
    point, size,
};

use super::ScrollbarHandle;

/// A fade over the bottom edge of a scroll area, toward `ground`, while there
/// is content below. It weakens over the last `height` of scroll so reaching
/// the end clears it smoothly, and it takes no pointer events.
pub fn bottom_fade<H: ScrollbarHandle>(
    handle: Rc<H>,
    ground: Hsla,
    height: Pixels,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let below = handle.content_size().height - bounds.size.height + handle.offset().y;
            let strength = (below / height).clamp(0.0, 1.0);
            if strength <= 0.0 {
                return;
            }
            let strip = Bounds::new(
                point(bounds.left(), bounds.bottom() - height),
                size(bounds.size.width, height),
            );
            window.paint_quad(fill(
                strip,
                linear_gradient(
                    180.0,
                    linear_color_stop(ground.alpha(0.0), 0.0),
                    linear_color_stop(ground.alpha(ground.a * strength), 1.0),
                ),
            ));
        },
    )
    .absolute()
    .inset_0()
}
