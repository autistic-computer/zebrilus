use ala::{
    App, Div, ElementId, FULL_CORNER_RADIUS, GlassMaterial, Hsla, IntoElement, MouseButton,
    ParentElement as _, Pixels, Rgba, SharedString, Stateful, Styled as _, div, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Ground, Icon, IconName, Layer, PaintState, Shape,
    StyledExt as _, Surface, Tone,
    button::{Button, ButtonVariants as _},
    rems_from_px, roles,
};

pub const COMPACT_NAV_BAR_HEIGHT: f32 = 44.0;
pub const COMPACT_PILL_HEIGHT: f32 = 56.0;

const BUTTON: f32 = 44.0;
const BUTTON_ICON: f32 = 20.0;
const PADDING_X: f32 = 8.0;
const TILE_INSET: f32 = 8.0;
const PILL_GAP: f32 = 8.0;
const PILL_INSET: f32 = 8.0;
const PILL_INSET_MIN: f32 = 16.0;
const PILL_INSET_MAX: f32 = 20.0;
const INK_CONTRAST: f32 = 4.5;
const TINT_MAX: f32 = 0.95;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PillPlacement {
    pub inset: Pixels,
    pub bottom: Pixels,
    pub radius: Option<Pixels>,
    pub glass: Option<PillGlass>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PillGlass {
    pub tint: Hsla,
    pub color: Hsla,
}

impl PillPlacement {
    pub fn new(screen_radius: Option<Pixels>, safe_bottom: Pixels) -> Self {
        let half = px(COMPACT_PILL_HEIGHT / 2.0);
        match screen_radius.filter(|radius| *radius > px(PILL_INSET_MIN)) {
            Some(screen) => {
                let inset = (screen - half).clamp(px(PILL_INSET_MIN), px(PILL_INSET_MAX));
                Self {
                    inset,
                    bottom: inset,
                    radius: Some(screen - inset),
                    glass: None,
                }
            }
            None => Self {
                inset: px(PILL_INSET),
                bottom: safe_bottom + px(PILL_INSET),
                radius: None,
                glass: None,
            },
        }
    }

    pub fn zone(&self) -> Pixels {
        px(PILL_GAP + COMPACT_PILL_HEIGHT) + self.bottom
    }

    pub fn pill_top(&self) -> Pixels {
        px(PILL_GAP)
    }
}

pub fn compact_floating_bar(pill: impl IntoElement, placement: &PillPlacement, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .debug_selector(|| "compact-bar".to_owned())
        .flex()
        .flex_none()
        .gap(px(PILL_GAP))
        .w_full()
        .h(placement.zone())
        .px(placement.inset)
        .pt(px(PILL_GAP))
        .pb(placement.bottom)
        .font_family(theme.font_family.clone())
        .text_color(theme.foreground)
        .child(pill)
}

pub(super) fn floating_layer(placement: &PillPlacement, cx: &App) -> Layer {
    let theme = cx.theme();
    let resting = theme.layer(roles::PHONE_PILL, PaintState::Rest);
    let radius = match placement.radius {
        Some(radius) if theme.style(roles::PHONE_PILL).shape != Shape::Square => {
            if radius * 2.0 >= px(COMPACT_PILL_HEIGHT) {
                FULL_CORNER_RADIUS
            } else {
                radius
            }
        }
        _ => resting.radius,
    };
    let layer = Layer { radius, ..resting };
    match placement.glass {
        Some(glass) => Layer {
            glass: layer.glass.map(|material| GlassMaterial {
                tint: glass.tint,
                ..material
            }),
            ground: Ground {
                fill: glass.color,
                ..layer.ground
            },
            ..layer
        },
        None => layer,
    }
}

pub fn pill_glass_over(backdrop: Hsla, cx: &App) -> Option<PillGlass> {
    let layer = cx.theme().layer(roles::PHONE_PILL, PaintState::Rest);
    let tint = layer.glass.filter(GlassMaterial::reads_backdrop)?.tint;
    let under = Rgba::from(backdrop.opaque());
    let over = Rgba::from(tint.opaque());
    let shows = |alpha: f32| {
        let mix = |under: f32, over: f32| under + (over - under) * alpha;
        Hsla::from(Rgba {
            r: mix(under.r, over.r),
            g: mix(under.g, over.g),
            b: mix(under.b, over.b),
            a: 1.0,
        })
    };
    let reads = |alpha: f32| contrast(layer.text, shows(alpha)) >= INK_CONTRAST;
    let mut alpha = tint.a;
    if !reads(alpha) {
        let (mut low, mut high) = (alpha, TINT_MAX.max(alpha));
        for _ in 0..12 {
            let middle = f32::midpoint(low, high);
            if reads(middle) {
                high = middle;
            } else {
                low = middle;
            }
        }
        alpha = high;
    }
    Some(PillGlass {
        tint: tint.alpha(alpha),
        color: shows(alpha),
    })
}

fn contrast(left: Hsla, right: Hsla) -> f32 {
    let luminance = |color: Hsla| {
        let color = Rgba::from(color);
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    };
    let (left, right) = (luminance(left), luminance(right));
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

pub fn compact_floating_group(
    buttons: impl IntoIterator<Item = Button>,
    placement: &PillPlacement,
    cx: &App,
) -> Div {
    let layer = floating_layer(placement, cx);
    let ground = layer.ground;
    div()
        .debug_selector(|| "compact-bar-group".to_owned())
        .flex()
        .flex_none()
        .items_center()
        .h(px(COMPACT_PILL_HEIGHT))
        .px(px((COMPACT_PILL_HEIGHT - BUTTON) / 2.0))
        .layer(&layer)
        .children(buttons.into_iter().map(|button| button.on(ground)))
}

pub fn compact_bar_button(id: impl Into<ElementId>, icon: IconName) -> Button {
    Button::new(id)
        .ghost()
        .compact()
        .tab_stop(false)
        .surface_role(roles::PHONE_BUTTON)
        .size(rems_from_px(BUTTON))
        .child(Icon::new(icon).size(rems_from_px(BUTTON_ICON)))
}

pub fn compact_nav_bar(cx: &App) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(rems_from_px(4.0))
        .h(rems_from_px(COMPACT_NAV_BAR_HEIGHT))
        .px(rems_from_px(PADDING_X))
        .surface(roles::PHONE_BAR, cx)
}

pub struct PillLabel {
    pub icon: IconName,
    pub title: SharedString,
    pub meta: SharedString,
    pub warning: bool,
}

pub fn compact_floating_pill(
    id: impl Into<ElementId>,
    label: PillLabel,
    dots: impl IntoElement,
    placement: &PillPlacement,
    cx: &App,
) -> Stateful<Div> {
    let pill = floating_layer(placement, cx);
    let pressed = cx.theme().layer(roles::PHONE_PILL, PaintState::Active).fill;
    let radius = pill.radius;
    div()
        .id(id)
        .debug_selector(|| "compact-bar-pill".to_owned())
        .relative()
        .flex()
        .flex_1()
        .min_w_0()
        .h(rems_from_px(COMPACT_PILL_HEIGHT))
        .layer(&pill)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(crate::touch::press_highlight(
            "compact-bar-pill-press",
            pressed,
            radius,
        ))
        .child(pill_row(label, dots, &pill, cx))
}

pub fn compact_pill_label(
    label: PillLabel,
    dots: impl IntoElement,
    placement: &PillPlacement,
    cx: &App,
) -> Div {
    let pill = floating_layer(placement, cx);
    pill_row(label, dots, &pill, cx)
        .h(rems_from_px(COMPACT_PILL_HEIGHT))
        .text_color(pill.text)
}

fn pill_row(label: PillLabel, dots: impl IntoElement, pill: &Layer, cx: &App) -> Div {
    let PillLabel {
        icon,
        title,
        meta,
        warning,
    } = label;
    let theme = cx.theme();
    let tone = if warning {
        Tone::Warning
    } else {
        Tone::Neutral
    };
    let badge = theme.paint_on(pill.ground, tone, Emphasis::Soft);
    let backing = theme.layer(
        Surface::new(roles::PHONE_PILL_ICON)
            .tone(tone)
            .on(pill.ground),
        PaintState::Rest,
    );
    let meta_color = if warning {
        badge.text
    } else {
        theme
            .paint_on(pill.ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest)
    };
    let tile = COMPACT_PILL_HEIGHT - 2.0 * TILE_INSET;
    let tile_radius = if theme.style(roles::PHONE_PILL_ICON).shape == Shape::Round {
        crate::touch::concentric(pill.radius, px(TILE_INSET))
    } else {
        backing.radius
    };
    div()
        .flex()
        .flex_1()
        .items_center()
        .min_w_0()
        .pl(rems_from_px(TILE_INSET))
        .pr(rems_from_px(12.0))
        .gap(rems_from_px(10.0))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(rems_from_px(tile))
                .layer(&Layer {
                    radius: tile_radius,
                    ..backing
                })
                .text_color(if warning {
                    badge.text
                } else {
                    badge.muted_at(PaintState::Rest)
                })
                .child(Icon::new(icon).size(rems_from_px((tile / 2.0).max(16.0)))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(rems_from_px(2.0))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(rems_from_px(15.0))
                        .line_height(rems_from_px(19.0))
                        .font_medium()
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rems_from_px(8.0))
                        .text_size(rems_from_px(12.0))
                        .line_height(rems_from_px(15.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_color(meta_color)
                                .child(meta),
                        )
                        .child(dots),
                ),
        )
}

#[cfg(test)]
mod tests {
    use ala::px;

    use super::*;

    #[test]
    fn the_pill_runs_concentric_with_rounded_screen_corners() {
        let placement = PillPlacement::new(Some(px(44.0)), px(34.0));
        assert_eq!(placement.inset, px(16.0));
        assert_eq!(placement.bottom, placement.inset);
        assert_eq!(placement.radius, Some(px(28.0)));

        let wide = PillPlacement::new(Some(px(62.0)), px(34.0));
        assert_eq!(wide.inset, px(PILL_INSET_MAX));
        assert_eq!(wide.bottom, wide.inset);
        assert_eq!(wide.radius, Some(px(62.0 - PILL_INSET_MAX)));

        let wider = PillPlacement::new(Some(px(80.0)), px(34.0));
        assert_eq!(wider.inset, px(PILL_INSET_MAX));
        assert_eq!(wider.radius, Some(px(80.0 - PILL_INSET_MAX)));

        let square = PillPlacement::new(None, px(20.0));
        assert_eq!(square.inset, px(PILL_INSET));
        assert_eq!(square.bottom, px(20.0 + PILL_INSET));
        assert_eq!(square.radius, None);
        assert_eq!(
            square.zone(),
            px(PILL_GAP + COMPACT_PILL_HEIGHT + 20.0 + PILL_INSET)
        );
    }
}
