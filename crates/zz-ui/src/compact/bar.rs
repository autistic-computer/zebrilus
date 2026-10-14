use zz_gpui::{
    App, Div, ElementId, IntoElement, MouseButton, ParentElement as _, SharedString, Stateful,
    Styled as _, div, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Icon, IconName, Layer, PaintState, StyledExt as _,
    Tone,
    button::{Button, ButtonVariants as _},
    rems_from_px, roles,
};

pub const COMPACT_BAR_HEIGHT: f32 = 60.0;
pub const COMPACT_NAV_BAR_HEIGHT: f32 = 44.0;

const BUTTON: f32 = 44.0;
const BUTTON_ICON: f32 = 20.0;
const PADDING_X: f32 = 8.0;
const PADDING_TOP: f32 = 12.0;
const GRABBER_TOP: f32 = 5.0;
const GRABBER_WIDTH: f32 = 36.0;
const GRABBER_HEIGHT: f32 = 4.0;
const TILE: f32 = 28.0;

pub fn compact_bar(
    pill: impl IntoElement,
    trailing: impl IntoElement,
    held: bool,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    div()
        .debug_selector(|| "compact-bar".to_owned())
        .relative()
        .flex()
        .flex_none()
        .items_start()
        .w_full()
        .h(rems_from_px(COMPACT_BAR_HEIGHT))
        .px(rems_from_px(PADDING_X))
        .pt(rems_from_px(PADDING_TOP))
        .gap(rems_from_px(8.0))
        .font_family(theme.font_family.clone())
        .text_color(theme.foreground)
        .child(
            div()
                .absolute()
                .top(rems_from_px(GRABBER_TOP))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(grabber(held, cx)),
        )
        .child(div().flex().flex_1().min_w_0().child(pill))
        .child(div().flex().flex_none().child(trailing))
}

/// The handle on the bar, drawn as [`roles::GRABBER`] in ink: faint, and
/// brighter while the bar is held.
fn grabber(held: bool, cx: &App) -> Div {
    let theme = cx.theme();
    let layer = theme.layer(roles::GRABBER, PaintState::Rest);
    let ink = if held {
        theme.foreground.muted()
    } else {
        theme.foreground.faint()
    };
    div()
        .w(rems_from_px(GRABBER_WIDTH))
        .h(rems_from_px(GRABBER_HEIGHT))
        .layer(&Layer {
            fill: ink,
            glass: None,
            ..layer
        })
}

/// A button on the phone's bars, drawn as [`roles::PHONE_BUTTON`]: back,
/// close, settings, the keyboard and the session switcher, which adds its
/// name after the icon.
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
/// the settings and overview bars.
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

pub fn compact_bar_pill(
    id: impl Into<ElementId>,
    icon: IconName,
    title: impl Into<SharedString>,
    meta: impl Into<SharedString>,
    warning: bool,
    dots: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let pill = theme.layer(roles::PHONE_PILL, PaintState::Rest);
    let pressed = theme.layer(roles::PHONE_PILL, PaintState::Active).fill;
    let badge = theme.paint_on(
        pill.ground,
        if warning {
            Tone::Warning
        } else {
            Tone::Neutral
        },
        Emphasis::Soft,
    );
    let meta_color = if warning {
        badge.text
    } else {
        theme
            .paint_on(pill.ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest)
    };
    let inset = px((BUTTON - TILE) / 2.0);
    let tile_radius = (pill.radius - inset).max(px(0.0));
    let radius = pill.radius;
    div()
        .id(id)
        .debug_selector(|| "compact-bar-pill".to_owned())
        .relative()
        .flex()
        .flex_1()
        .items_center()
        .min_w_0()
        .h(rems_from_px(BUTTON))
        .pl(rems_from_px(8.0))
        .pr(rems_from_px(12.0))
        .gap(rems_from_px(10.0))
        .surface(roles::PHONE_PILL, cx)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(rems_from_px(TILE))
                .rounded(tile_radius)
                .bg(badge.fill)
                .text_color(if warning {
                    badge.text
                } else {
                    badge.muted_at(PaintState::Rest)
                })
                .child(Icon::new(icon).size(rems_from_px(16.0))),
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
                        .child(title.into()),
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
                                .child(meta.into()),
                        )
                        .child(dots),
                ),
        )
        .child(crate::touch::press_highlight(
            "compact-bar-pill-press",
            pressed,
            radius,
        ))
}
