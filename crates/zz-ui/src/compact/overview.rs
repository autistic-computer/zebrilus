use zz_gpui::{
    AnyElement, App, Div, ElementId, ParentElement as _, SharedString, Stateful, Styled as _, div,
    prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Layer, PaintState, StyledExt as _, roles,
    touch::press_highlight,
};

pub const OVERVIEW_CARD_WIDTH: f32 = 108.0;
pub const OVERVIEW_CARD_HEIGHT: f32 = 192.0;
pub const OVERVIEW_CARD_GAP: f32 = 10.0;
const CARD_STRIP: f32 = 20.0;

/// `n` of `noun`, as the overview counts panes and windows.
pub fn overview_count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// A pane's card in the phone overview, drawn as [`roles::PHONE_CARD`]: a
/// strip with its icon and title over `preview`, on `background`, lit when
/// it is the `current` page and dotted when it asks for `attention`.
pub fn overview_card(
    id: impl Into<ElementId>,
    press: impl Into<ElementId>,
    icon: IconName,
    title: impl Into<SharedString>,
    preview: Option<AnyElement>,
    current: bool,
    attention: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let muted = theme.foreground.muted();
    let frame = theme.layer(
        roles::PHONE_CARD,
        if current {
            PaintState::Selected
        } else {
            PaintState::Rest
        },
    );
    let radius = frame.radius;
    div()
        .id(id)
        .relative()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(OVERVIEW_CARD_WIDTH))
        .h(px(OVERVIEW_CARD_HEIGHT))
        .overflow_hidden()
        .layer(&Layer {
            glass: None,
            ..frame
        })
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(4.0))
                .h(px(CARD_STRIP))
                .px(px(6.0))
                .text_size(px(11.0))
                .text_color(muted)
                .child(Icon::new(icon).size(px(11.0)))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title.into()),
                ),
        )
        .child(div().flex_1().min_h_0().overflow_hidden().children(preview))
        .when(attention && !current, |card| {
            card.child(
                div()
                    .absolute()
                    .top(px(7.0))
                    .right(px(6.0))
                    .size(px(6.0))
                    .rounded_full()
                    .bg(theme.warning),
            )
        })
        .child(press_highlight(press, None, radius))
}

/// The New pane tile that ends a window's row of cards, as big as a card
/// and drawn as [`roles::PHONE_ADD`].
pub fn overview_add_tile(
    id: impl Into<ElementId>,
    press: impl Into<ElementId>,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let tile = theme.layer(roles::PHONE_ADD, PaintState::Rest);
    let pressed = theme.layer(roles::PHONE_ADD, PaintState::Active).fill;
    let radius = tile.radius;
    div()
        .id(id)
        .relative()
        .flex()
        .flex_col()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .w(px(OVERVIEW_CARD_WIDTH))
        .h(px(OVERVIEW_CARD_HEIGHT))
        .layer(&tile)
        .text_size(px(12.0))
        .text_color(theme.foreground.muted())
        .child(Icon::new(IconName::Plus).size(px(16.0)))
        .child("New pane")
        .child(press_highlight(press, pressed, radius))
}

/// The line over a window's cards: its title and how many panes it holds.
pub fn overview_section_heading(title: &str, panes: usize, cx: &App) -> Div {
    let muted = cx.theme().foreground.muted();
    div()
        .flex()
        .items_baseline()
        .gap(px(8.0))
        .px(px(16.0))
        .pt(px(14.0))
        .pb(px(8.0))
        .text_size(px(13.0))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_medium()
                .child(title.to_owned()),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(12.0))
                .text_color(muted)
                .child(overview_count(panes, "pane")),
        )
}
