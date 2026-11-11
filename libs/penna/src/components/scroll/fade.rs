use std::rc::Rc;

use ala::{
    Bounds, Hsla, IntoElement, Pixels, Styled, Window, canvas, fill, linear_color_stop,
    linear_gradient, point, size,
};

use super::ScrollbarHandle;

const SOLID_FROM: f32 = 0.7;
const REACH: f32 = 2.0;

pub fn bottom_fade<H: ScrollbarHandle>(
    handle: Rc<H>,
    ground: Hsla,
    height: Pixels,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let below = handle.content_size().height - bounds.size.height + handle.offset().y;
            let height = height.min(below * REACH);
            if height <= Pixels::ZERO {
                return;
            }
            let strip = Bounds::new(
                point(bounds.left(), bounds.bottom() - height),
                size(bounds.size.width, height),
            );
            let solid = strip.top() + height * SOLID_FROM;
            paint_fade(window, strip, ground, (strip.top(), solid), 180.0);
        },
    )
    .absolute()
    .inset_0()
}

pub fn top_fade<H: ScrollbarHandle>(
    handle: Rc<H>,
    ground: Hsla,
    height: Pixels,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let above = -handle.offset().y;
            let height = height.min(above * REACH);
            if height <= Pixels::ZERO {
                return;
            }
            let strip = Bounds::new(bounds.origin, size(bounds.size.width, height));
            let solid = strip.bottom() - height * SOLID_FROM;
            paint_fade(window, strip, ground, (strip.bottom(), solid), 0.0);
        },
    )
    .absolute()
    .inset_0()
}

fn paint_fade(
    window: &mut Window,
    strip: Bounds<Pixels>,
    ground: Hsla,
    (clear, solid): (Pixels, Pixels),
    angle: f32,
) {
    if window.paint_fade(strip, ground, clear, solid) {
        return;
    }
    window.paint_quad(fill(
        strip,
        linear_gradient(
            angle,
            linear_color_stop(ground.alpha(0.0), 0.0),
            linear_color_stop(ground, SOLID_FROM),
        ),
    ));
}
