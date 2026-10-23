use zz_gpui::{
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
const INK_SWITCH: f32 = 1.25;

/// The radius of a corner set `gap` inside one of `radius`, so the two
/// share a center. A full round stays full.
pub(super) fn concentric(radius: Pixels, gap: Pixels) -> Pixels {
    if radius >= FULL_CORNER_RADIUS {
        FULL_CORNER_RADIUS
    } else {
        (radius - gap).max(px(0.0))
    }
}

/// Where the floating pill sits. On a screen with rounded corners it sits in
/// from the sides and the bottom by one distance, at most 20pt so the bar
/// spans the screen, and rounds by the screen's radius less that distance,
/// following the screen's corners; elsewhere it clears the safe area and
/// keeps its role's corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PillPlacement {
    pub inset: Pixels,
    pub bottom: Pixels,
    pub radius: Option<Pixels>,
    pub inverted: Option<Hsla>,
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
                    inverted: None,
                }
            }
            None => Self {
                inset: px(PILL_INSET),
                bottom: safe_bottom + px(PILL_INSET),
                radius: None,
                inverted: None,
            },
        }
    }

    /// The height of the strip the pill floats in, from the gap above it to
    /// the bottom of the screen.
    pub fn zone(&self) -> Pixels {
        px(PILL_GAP + COMPACT_PILL_HEIGHT) + self.bottom
    }

    /// How far the pill's top sits below the top of that strip.
    pub fn pill_top(&self) -> Pixels {
        px(PILL_GAP)
    }
}

/// The strip the floating pill sits in at the bottom of a phone page. It
/// paints nothing, so the page shows around the pill; a
/// [`compact_floating_group`] added after the pill sits at its other end.
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

/// How a floating pill or group paints: [`roles::PHONE_PILL`] at rest,
/// rounded by the placement's concentric radius unless the role is square.
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
    match placement.inverted {
        Some(over) => {
            let ink = layer.fill.opaque();
            Layer {
                text: ink,
                ground: Ground {
                    fill: over,
                    ink,
                    ..layer.ground
                },
                ..layer
            }
        }
        None => layer,
    }
}

/// Whether the floating pill's glass over `backdrop`, the color under it,
/// reads better with the pill's fill as its ink than with its own ink, and
/// the color the glass then shows. `was` is the last answer: it holds until
/// the other ink reads clearly better, so a page scrolling under the pill
/// does not flicker it.
pub fn pill_inverted_over(backdrop: Hsla, was: bool, cx: &App) -> Option<Hsla> {
    let layer = cx.theme().layer(roles::PHONE_PILL, PaintState::Rest);
    let tint = layer.glass.filter(GlassMaterial::reads_backdrop)?.tint;
    let under = Rgba::from(backdrop.opaque());
    let over = Rgba::from(tint.opaque());
    let mix = |under: f32, over: f32| under + (over - under) * tint.a;
    let glass = Hsla::from(Rgba {
        r: mix(under.r, over.r),
        g: mix(under.g, over.g),
        b: mix(under.b, over.b),
        a: 1.0,
    });
    let own = contrast(layer.text, glass);
    let fill = contrast(layer.fill.opaque(), glass);
    let inverted = if was {
        fill * INK_SWITCH >= own
    } else {
        fill >= own * INK_SWITCH
    };
    inverted.then_some(glass)
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

/// The ink the floating pill's text takes at `placement`.
pub fn pill_ink(placement: &PillPlacement, cx: &App) -> Hsla {
    match placement.inverted {
        Some(_) => floating_layer(placement, cx).text,
        None => cx.theme().foreground,
    }
}

/// Buttons in a capsule beside the floating pill, painted and rounded as the
/// pill is, so it runs concentric with the screen's other corner.
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

/// A button on the phone's bars, drawn as [`roles::PHONE_BUTTON`]: back,
/// close and settings.
pub fn compact_bar_button(id: impl Into<ElementId>, icon: IconName) -> Button {
    Button::new(id)
        .ghost()
        .compact()
        .tab_stop(false)
        .surface_role(roles::PHONE_BUTTON)
        .size(rems_from_px(BUTTON))
        .child(Icon::new(icon).size(rems_from_px(BUTTON_ICON)))
}

/// A 44pt bar across the top of a phone page, drawn as [`roles::PHONE_BAR`]:
/// the settings bar.
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

/// What a phone pill says: its icon, title and the line under it, amber
/// when the page needs attention.
pub struct PillLabel {
    pub icon: IconName,
    pub title: SharedString,
    pub meta: SharedString,
    pub warning: bool,
}

/// The pill floating at the bottom of a phone page, drawn as
/// [`roles::PHONE_PILL`], in a [`compact_floating_bar`] with the same
/// placement.
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

/// What the floating pill holds, without the pill: the icon tile, the title
/// and the line under it, laid out as the pill lays them out.
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
        concentric(pill.radius, px(TILE_INSET))
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
    use zz_gpui::px;

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

    #[test]
    fn inner_corners_share_the_outer_center() {
        assert_eq!(concentric(px(28.0), px(8.0)), px(20.0));
        assert_eq!(concentric(px(4.0), px(8.0)), px(0.0));
        assert_eq!(concentric(FULL_CORNER_RADIUS, px(8.0)), FULL_CORNER_RADIUS);
    }
}
