use std::time::Duration;

use zz_gpui::{
    AnyElement, App, Div, ElementId, EntityId, FontWeight, Hsla, Pixels, SharedString, Stateful,
    Transformation, div, ease_in_out, percentage, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    pulse::pulse_phase,
    v_flex,
};

pub use zz_client::agent_config::{MAX_RENDERED_ERROR_BYTES, rendered_error};

pub fn welcome_state(cx: &App) -> Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .px_3()
        .pb(px(32.0))
        .text_center()
        .child(
            Icon::new(IconName::Zz)
                .w(px(112.0))
                .h(px(47.0))
                .mb(px(24.0))
                .text_color(cx.theme().foreground.muted()),
        )
        .child(
            div()
                .text_size(crate::rems_from_px(21.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child("What are we working on?"),
        )
        .child(
            div()
                .mt(px(10.0))
                .text_size(crate::rems_from_px(13.0))
                .text_color(cx.theme().foreground.muted())
                .child("Ask a question or describe a task."),
        )
}

pub fn error_card(error: &str, cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap(px(CARD_GAP))
        .popover_style(cx)
        .p(px(CARD_GUTTER))
        .child(
            h_flex()
                .w_full()
                .items_start()
                .gap(px(CARD_GAP))
                .text_size(crate::rems_from_px(12.0))
                .line_height(px(CARD_LINE))
                .child(card_glyph(
                    Icon::new(IconName::CircleX).text_color(cx.theme().danger),
                    CARD_LINE,
                ))
                .child(div().min_w_0().flex_1().child(rendered_error(error))),
        )
}

pub fn error_actions() -> Div {
    h_flex()
        .flex_wrap()
        .gap(px(CARD_GAP))
        .pl(px(CARD_GLYPH + CARD_GAP))
}

pub fn spinner_phase(view: EntityId, cx: &mut App) -> f32 {
    ease_in_out(pulse_phase(Duration::from_millis(800), view, cx))
}

pub fn spinner(phase: f32) -> Icon {
    Icon::new(IconName::Loader).transform(Transformation::rotate(percentage(phase)))
}

pub fn empty_state(
    message: impl Into<SharedString>,
    busy: bool,
    view: EntityId,
    cx: &mut App,
) -> Div {
    let phase = if busy { spinner_phase(view, cx) } else { 0.0 };
    v_flex()
        .w_full()
        .py(px(48.0))
        .items_center()
        .gap_2()
        .text_size(crate::rems_from_px(12.0))
        .text_color(cx.theme().foreground.muted())
        .when(busy, |column| {
            column.child(
                spinner(phase)
                    .small()
                    .text_color(cx.theme().foreground.muted()),
            )
        })
        .child(message.into())
}

pub fn permission_option(
    id: impl Into<ElementId>,
    index: usize,
    label: impl Into<SharedString>,
    danger: bool,
    highlighted: bool,
    enabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let selected = highlighted && enabled;
    card_tile(id, enabled, selected, cx)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .when(danger && enabled, |label| {
                    label.text_color(cx.theme().danger)
                })
                .child(label.into()),
        )
        .child(
            card_trailing()
                .gap(px(4.0))
                .when(selected, |trailing| {
                    trailing.child(card_keycap("↵", cx.theme().foreground, cx))
                })
                .when(index < 9, |trailing| {
                    trailing.child(card_keycap(
                        format!("{}", index + 1),
                        cx.theme().foreground,
                        cx,
                    ))
                }),
        )
}

pub fn permission_card(
    title: impl Into<SharedString>,
    counter: Option<SharedString>,
    options: Vec<AnyElement>,
    cancel: Button,
    cx: &App,
) -> Div {
    card_surface(cx)
        .child(
            card_header()
                .child(card_glyph(
                    Icon::new(IconName::TriangleAlert).text_color(cx.theme().warning),
                    CARD_TITLE_LINE,
                ))
                .child(div().min_w_0().flex_1().child(title.into()))
                .when_some(counter, |row, counter| row.child(card_chip(counter, cx)))
                .child(card_close(cancel)),
        )
        .child(card_tiles().pb(px(CARD_GUTTER)).children(options))
}

pub fn card_close(button: Button) -> Div {
    div()
        .flex_none()
        .mt(px((CARD_TITLE_LINE - CARD_CLOSE) / 2.0))
        .mb(px((CARD_TITLE_LINE - CARD_CLOSE) / 2.0))
        .mr(px(-(CARD_CLOSE - CARD_GLYPH) / 2.0))
        .child(
            button
                .icon(IconName::Xmark)
                .ghost()
                .compact()
                .size(px(CARD_CLOSE)),
        )
}

pub fn card_button(button: Button) -> Button {
    button.small()
}

pub(crate) const CARD_GUTTER: f32 = 12.0;
pub(crate) const CARD_GAP: f32 = 8.0;
pub(crate) const CARD_GLYPH: f32 = 14.0;
pub(crate) const CARD_LINE: f32 = 16.0;
pub(crate) const CARD_TITLE_LINE: f32 = 18.0;
pub(crate) const CARD_OPTICAL_DROP: f32 = 0.5;
const CARD_TITLE_TOP: f32 = 8.0;
const CARD_TITLE_BOTTOM: f32 = 10.0;
const CARD_TILE_GAP: f32 = 6.0;
const CARD_TILE_PADDING_X: f32 = 12.0;
const CARD_TILE_PADDING_Y: f32 = 9.0;
const CARD_LABEL_LINE: f32 = 18.0;
const CARD_DETAIL_LINE: f32 = 16.0;
const CARD_KEYCAP: f32 = 18.0;
const CARD_CLOSE: f32 = 24.0;

pub(crate) fn card_surface(cx: &App) -> Div {
    v_flex().w_full().overflow_hidden().popover_style(cx)
}

pub(crate) fn card_header() -> Div {
    h_flex()
        .w_full()
        .items_start()
        .gap(px(CARD_GAP))
        .px(px(CARD_GUTTER))
        .pt(px(CARD_TITLE_TOP))
        .pb(px(CARD_TITLE_BOTTOM))
        .text_size(crate::rems_from_px(13.0))
        .line_height(px(CARD_TITLE_LINE))
        .font_weight(FontWeight::MEDIUM)
}

pub(crate) fn card_glyph(icon: Icon, line: f32) -> Div {
    h_flex().flex_none().w(px(CARD_GLYPH)).h(px(line)).child(
        div()
            .relative()
            .top(px(CARD_OPTICAL_DROP))
            .child(icon.size(px(CARD_GLYPH))),
    )
}

pub(crate) fn card_chip(text: impl Into<SharedString>, cx: &App) -> Div {
    h_flex().flex_none().h(px(CARD_TITLE_LINE)).child(
        div()
            .rounded(cx.theme().menu_radius())
            .bg(cx.theme().background.washed(2))
            .px(px(6.0))
            .text_size(crate::rems_from_px(10.0))
            .line_height(px(14.0))
            .font_weight(FontWeight::NORMAL)
            .text_color(cx.theme().foreground.muted())
            .child(text.into()),
    )
}

pub(crate) fn card_divider(cx: &App) -> Div {
    div()
        .mx(px(CARD_GUTTER))
        .my(px(CARD_GAP))
        .h(px(0.5))
        .bg(cx.theme().foreground.opacity(0.1 * cx.theme().divider))
}

pub(crate) fn card_tiles() -> Div {
    v_flex().w_full().gap(px(CARD_TILE_GAP)).px(px(CARD_GUTTER))
}

fn card_tile_radius(cx: &App) -> Pixels {
    let radius = cx.theme().radius;
    (radius - px(CARD_GUTTER)).max(radius.min(px(6.0)))
}

pub(crate) fn card_tile(
    id: impl Into<ElementId>,
    enabled: bool,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let rest = theme.control_background(theme.background.washed(1));
    let hover = theme.control_background(theme.background.washed(2));
    let chosen = theme.control_background(theme.background.washed(3));
    let fill = if selected { chosen } else { rest };
    h_flex()
        .id(id)
        .w_full()
        .items_start()
        .gap(px(CARD_GAP))
        .px(px(CARD_TILE_PADDING_X))
        .py(cx.theme().dense(px(CARD_TILE_PADDING_Y)))
        .rounded(card_tile_radius(cx))
        .bg(fill)
        .text_size(crate::rems_from_px(13.0))
        .line_height(px(CARD_LABEL_LINE))
        .when(enabled, |tile| {
            tile.cursor_pointer()
                .hover(move |style| style.bg(if selected { chosen } else { hover }))
        })
        .when(!enabled, |tile| tile.text_color(theme.foreground.muted()))
}

pub(crate) fn card_detail(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(crate::rems_from_px(12.0))
        .line_height(px(CARD_DETAIL_LINE))
        .text_color(cx.theme().foreground.muted())
        .child(text.into())
}

pub(crate) fn card_trailing() -> Div {
    h_flex().flex_none().h(px(CARD_LABEL_LINE))
}

pub(crate) fn card_keycap(text: impl Into<SharedString>, ink: Hsla, cx: &App) -> Div {
    h_flex()
        .flex_none()
        .justify_center()
        .h(px(CARD_KEYCAP))
        .min_w(px(CARD_KEYCAP))
        .px(px(4.0))
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(ink.opacity(0.25))
        .text_size(crate::rems_from_px(11.0))
        .line_height(px(CARD_KEYCAP - 2.0))
        .font_weight(FontWeight::NORMAL)
        .text_color(ink.opacity(0.7))
        .child(text.into())
}

pub(crate) fn card_footer() -> Div {
    h_flex()
        .w_full()
        .items_center()
        .justify_end()
        .gap(px(CARD_GAP))
        .px(px(CARD_GUTTER))
        .pt(px(CARD_GUTTER))
        .pb(px(CARD_GUTTER))
}
