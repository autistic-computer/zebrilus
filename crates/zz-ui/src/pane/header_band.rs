use ala::{
    AnyElement, App, Bounds, Div, Hsla, IntoElement, ParentElement as _, Pixels, Rgba, Styled as _,
    Window, canvas, div, fill, linear_color_stop, linear_gradient, point, prelude::*, px, size,
};
use penna::Ground;
use zz_protocol::PaneKindSnapshot;

use crate::{ActiveTheme as _, pane_look::PaneLook};

pub const SCRIM_TAIL: Pixels = px(20.0);
pub const SCRIM_AT_EDGE: f32 = 0.88;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeaderPlace {
    #[default]
    Stacked,
    Over,
}

impl HeaderPlace {
    #[must_use]
    pub const fn of(kind: &PaneKindSnapshot, floats: bool) -> Self {
        match kind {
            PaneKindSnapshot::Terminal | PaneKindSnapshot::Agent(_) if !floats => Self::Over,
            _ => Self::Stacked,
        }
    }

    #[must_use]
    pub fn band(self, surface: Hsla, cx: &App) -> Option<HeaderBand> {
        match self {
            Self::Stacked => None,
            Self::Over => Some(HeaderBand {
                height: cx.theme().pane_header_height(),
                ground: surface,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderBand {
    pub height: Pixels,
    pub ground: Hsla,
}

impl HeaderBand {
    #[must_use]
    pub fn scrim(self, area: Bounds<Pixels>) -> HeaderScrim {
        HeaderScrim {
            band: Bounds::new(area.origin, size(area.size.width, self.height)),
            ground: self.ground,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderScrim {
    pub band: Bounds<Pixels>,
    pub ground: Hsla,
}

#[must_use]
pub fn stacked_surface(layers: impl IntoIterator<Item = Option<Hsla>>) -> Hsla {
    let under = layers
        .into_iter()
        .flatten()
        .fold(Rgba::default(), |under, layer| {
            let top = Rgba::from(layer);
            let alpha = top.a + under.a * (1.0 - top.a);
            if alpha <= 0.0 {
                return Rgba::default();
            }
            let mix =
                |over: f32, below: f32| (over * top.a + below * under.a * (1.0 - top.a)) / alpha;
            Rgba {
                r: mix(top.r, under.r),
                g: mix(top.g, under.g),
                b: mix(top.b, under.b),
                a: alpha,
            }
        });
    Hsla::from(under)
}

#[must_use]
pub fn tiled_pane_surface(
    window: Hsla,
    shell: Option<Hsla>,
    framed: bool,
    active: bool,
    cx: &App,
) -> Hsla {
    stacked_surface([
        Some(window),
        shell,
        framed.then(|| PaneLook::global(cx).frame(cx.theme(), window, active).tint),
    ])
}

#[must_use]
pub fn tiled_pane_ground(plane: Hsla, gaps: bool, active: bool, cx: &App) -> Ground {
    if gaps {
        PaneLook::global(cx).ground(cx.theme(), plane, active)
    } else {
        cx.theme().content_ground()
    }
}

pub fn paint_header_scrim(window: &mut Window, scrim: HeaderScrim) {
    let HeaderScrim { band, ground } = scrim;
    let top = band.top();
    let edge = band.bottom();
    let end = edge + SCRIM_TAIL;
    let tail = Bounds::new(point(band.left(), edge), size(band.size.width, SCRIM_TAIL));
    let band_clear = top + (edge - top) / (1.0 - SCRIM_AT_EDGE);
    let tail_solid = end - SCRIM_TAIL / SCRIM_AT_EDGE;
    if window.paint_fade(band, ground, band_clear, top)
        && window.paint_fade(tail, ground, end, tail_solid)
    {
        return;
    }
    let at_edge = ground.alpha(ground.a * SCRIM_AT_EDGE);
    window.paint_quad(fill(
        band,
        linear_gradient(
            180.0,
            linear_color_stop(ground, 0.0),
            linear_color_stop(at_edge, 1.0),
        ),
    ));
    window.paint_quad(fill(
        tail,
        linear_gradient(
            180.0,
            linear_color_stop(at_edge, 0.0),
            linear_color_stop(ground.alpha(0.0), 1.0),
        ),
    ));
}

#[must_use]
pub fn header_scrim(band: HeaderBand) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| paint_header_scrim(window, band.scrim(bounds)),
    )
    .absolute()
    .top_0()
    .left_0()
    .right_0()
    .h(band.height + SCRIM_TAIL)
}

pub fn headed_pane(
    place: HeaderPlace,
    header: AnyElement,
    content: AnyElement,
    under_header: Option<AnyElement>,
    cx: &App,
) -> Div {
    let height = cx.theme().pane_header_height();
    match place {
        HeaderPlace::Stacked => div().flex().flex_col().size_full().child(header).child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(content)
                .children(under_header),
        ),
        HeaderPlace::Over => div()
            .relative()
            .size_full()
            .child(div().absolute().inset_0().child(content))
            .children(under_header.map(|element| {
                div()
                    .absolute()
                    .top(height)
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(element)
            }))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(height)
                    .block_mouse_except_scroll()
                    .child(header),
            ),
    }
}

#[cfg(test)]
mod tests {
    use ala::{Hsla, Rgba};

    use super::stacked_surface;

    #[test]
    fn the_scrim_colour_is_the_layers_composited_like_the_renderer() {
        let layer = |level: f32| {
            Some(Hsla::from(Rgba {
                r: level,
                g: level,
                b: level,
                a: 0.5,
            }))
        };
        let surface = Rgba::from(stacked_surface([layer(0.0), None, layer(1.0)]));
        assert!((surface.a - 0.75).abs() < 1e-4, "alpha {}", surface.a);
        assert!((surface.r - 2.0 / 3.0).abs() < 1e-4, "red {}", surface.r);
    }
}
