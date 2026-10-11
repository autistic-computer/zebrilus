use zz_gpui::{
    App, Div, ElementId, IntoElement, MouseButton, ParentElement as _, SharedString, Stateful,
    Styled as _, div, prelude::*,
};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Icon, IconName, PaintExt as _, PaintState,
    StyledExt as _, Tone,
    button::{Button, ButtonVariants as _},
    rems_from_px,
};

pub const COMPACT_BAR_HEIGHT: f32 = 60.0;

const BUTTON: f32 = 44.0;
const BUTTON_ICON: f32 = 24.0;
const PADDING_X: f32 = 8.0;
const PADDING_TOP: f32 = 12.0;
const GRABBER_TOP: f32 = 5.0;
const GRABBER_WIDTH: f32 = 36.0;
const GRABBER_HEIGHT: f32 = 4.0;
const TILE: f32 = 28.0;
const TILE_RADIUS: f32 = 4.0;

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
                .child(
                    div()
                        .w(rems_from_px(GRABBER_WIDTH))
                        .h(rems_from_px(GRABBER_HEIGHT))
                        .rounded_full()
                        .bg(if held {
                            theme.foreground.muted()
                        } else {
                            theme.foreground.faint()
                        }),
                ),
        )
        .child(div().flex().flex_1().min_w_0().child(pill))
        .child(div().flex().flex_none().child(trailing))
}

pub fn compact_bar_button(id: impl Into<ElementId>, icon: IconName) -> Button {
    Button::new(id)
        .ghost()
        .compact()
        .tab_stop(false)
        .size(rems_from_px(BUTTON))
        .child(Icon::new(icon).size(rems_from_px(BUTTON_ICON)))
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
    let pill = theme.paint(Tone::Neutral, Emphasis::Soft).opaque();
    let badge = theme.paint_on(
        pill.ground_at(PaintState::Rest),
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
        pill.muted_at(PaintState::Rest)
    };
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
        .rounded(theme.radius)
        .control_surface(cx)
        .painted(pill, PaintState::Rest)
        .surface_fill(pill.fill, cx)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(rems_from_px(TILE))
                .rounded(rems_from_px(TILE_RADIUS))
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
            pill.active,
            theme.radius,
        ))
}
