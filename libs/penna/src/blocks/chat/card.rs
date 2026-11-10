use ala::{App, Div, SharedString, div, prelude::*, px};

use crate::{
    ActiveTheme as _, Icon, Label, PanelLook, TextStyle, components::badge::Badge,
    components::button::Button, components::icon::cap_center_drop, h_flex,
};

pub const CARD_GUTTER: f32 = 12.0;
pub const CARD_GAP: f32 = 8.0;
pub const CARD_GLYPH: f32 = 14.0;
pub const CARD_TITLE_LINE: f32 = 18.0;
const CARD_TITLE_SIZE: f32 = 13.0;
const CARD_TITLE_TOP: f32 = 8.0;
const CARD_TITLE_BOTTOM: f32 = 10.0;

#[must_use]
pub fn card_header() -> Div {
    h_flex()
        .w_full()
        .items_start()
        .gap(px(CARD_GAP))
        .px(px(CARD_GUTTER))
        .pt(px(CARD_TITLE_TOP))
        .pb(px(CARD_TITLE_BOTTOM))
}

pub fn card_title(title: impl Into<SharedString>) -> Label {
    Label::new(TextStyle::Body, title)
        .strong()
        .min_w_0()
        .flex_1()
}

#[must_use]
pub fn card_glyph(icon: Icon, cx: &App) -> Div {
    h_flex()
        .flex_none()
        .w(px(CARD_GLYPH))
        .h(px(CARD_TITLE_LINE))
        .child(
            div()
                .relative()
                .top(cap_center_drop(px(CARD_TITLE_SIZE), px(CARD_GLYPH), cx))
                .child(icon.size(px(CARD_GLYPH))),
        )
}

pub fn card_chip(text: impl Into<SharedString>) -> Badge {
    Badge::new(text).flex_none()
}

#[must_use]
pub fn card_button(button: Button, cx: &App) -> Button {
    button.on(cx.theme().ground_of(PanelLook::Inline))
}
