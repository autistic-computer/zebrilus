use ala::{AnyElement, App, Div, ElementId, SharedString, div, prelude::*, px};

use crate::{
    ActiveTheme as _, Edge, Emphasis, Ground, Icon, IconName, Label, PaintState, Panel, PanelLook,
    Row, RowLook, RuleLook, StyledExt as _, Surface, TextStyle, Tone,
    blocks::picker::list_row_height, components::badge::Badge, components::button::Button,
    components::icon::cap_center_drop, components::kbd::Keycap, h_flex, row_state, v_flex,
};

pub const CARD_GUTTER: f32 = 12.0;
pub const CARD_GAP: f32 = 8.0;
pub const CARD_GLYPH: f32 = 14.0;
pub const CARD_TITLE_LINE: f32 = 18.0;
const CARD_TITLE_SIZE: f32 = 13.0;
const CARD_TITLE_TOP: f32 = 8.0;
const CARD_TITLE_BOTTOM: f32 = 10.0;
const CARD_TILE_GAP: f32 = 2.0;
const CARD_TILE_PADDING_X: f32 = 12.0;
const CARD_TILE_PADDING_Y: f32 = 8.0;
const CARD_LABEL_LINE: f32 = 16.0;
const CARD_DETAIL_LINE: f32 = 14.0;
const CARD_CLOSE: f32 = 24.0;

pub fn card_surface(id: impl Into<ElementId>) -> Panel {
    Panel::new(id, PanelLook::Inline)
        .v_flex()
        .w_full()
        .overflow_hidden()
}

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
pub fn card_divider(cx: &App) -> Div {
    div()
        .mx(px(CARD_GUTTER))
        .my(px(CARD_GAP))
        .rule(RuleLook::List, Edge::Top, cx)
}

#[must_use]
pub fn card_tiles() -> Div {
    v_flex().w_full().gap(px(CARD_TILE_GAP)).px(px(CARD_GUTTER))
}

#[must_use]
pub fn card_trailing() -> Div {
    h_flex().flex_none().h(px(CARD_LABEL_LINE))
}

#[must_use]
pub fn card_footer() -> Div {
    h_flex()
        .w_full()
        .items_center()
        .justify_end()
        .gap(px(CARD_GAP))
        .px(px(CARD_GUTTER))
        .pt(px(CARD_GUTTER))
        .pb(px(CARD_GUTTER))
}

#[must_use]
pub fn card_button(button: Button, cx: &App) -> Button {
    button.on(cx.theme().ground_of(PanelLook::Inline))
}

#[must_use]
pub fn card_close(button: Button, cx: &App) -> Div {
    div()
        .flex_none()
        .mt(px((CARD_TITLE_LINE - CARD_CLOSE) / 2.0))
        .mb(px((CARD_TITLE_LINE - CARD_CLOSE) / 2.0))
        .mr(px(-(CARD_CLOSE - CARD_GLYPH) / 2.0))
        .child(
            card_button(button, cx)
                .icon(IconName::Xmark)
                .emphasis(Emphasis::Ghost)
                .compact()
                .size(px(CARD_CLOSE)),
        )
}

#[derive(Clone, Copy)]
pub struct CardTile {
    tone: Tone,
    state: PaintState,
    ground: Ground,
}

impl CardTile {
    #[must_use]
    pub fn new(tone: Tone, enabled: bool, selected: bool, cx: &App) -> Self {
        let state = row_state(selected, enabled);
        Self {
            tone,
            state,
            ground: cx
                .theme()
                .ground_at(Surface::from(RowLook::Menu).tone(tone), state),
        }
    }

    #[must_use]
    pub fn selected(self) -> bool {
        self.state == PaintState::Selected
    }

    #[must_use]
    pub const fn ground(self) -> Ground {
        self.ground
    }

    pub fn render(self, id: impl Into<ElementId>, cx: &App) -> Row {
        let enabled = self.state != PaintState::Disabled;
        let height = list_row_height(cx);
        Row::new(id, RowLook::Menu)
            .tone(self.tone)
            .state(self.state)
            .row_height(height)
            .h_flex()
            .w_full()
            .min_h(height)
            .items_start()
            .gap(px(CARD_GAP))
            .px(px(CARD_TILE_PADDING_X))
            .py(px(CARD_TILE_PADDING_Y))
            .type_style(TextStyle::Small, cx)
            .when(enabled, crate::StyledExt::click_cursor)
    }

    pub fn detail(self, text: impl Into<SharedString>) -> Label {
        Label::new(TextStyle::Caption, text)
            .muted()
            .on(self.ground)
            .line_height(px(CARD_DETAIL_LINE))
    }

    pub fn keycap(self, text: impl Into<SharedString>) -> Keycap {
        Keycap::new(text).on(self.ground())
    }
}

pub fn permission_option(
    id: impl Into<ElementId>,
    index: usize,
    label: impl Into<SharedString>,
    danger: bool,
    highlighted: bool,
    enabled: bool,
    cx: &App,
) -> Row {
    let tone = if danger { Tone::Danger } else { Tone::Neutral };
    let tile = CardTile::new(tone, enabled, highlighted && enabled, cx);
    tile.render(id, cx)
        .child(div().min_w_0().flex_1().child(label.into()))
        .child(
            card_trailing()
                .gap(px(4.0))
                .when(tile.selected(), |trailing| trailing.child(tile.keycap("↵")))
                .when(index < 9, |trailing| {
                    trailing.child(tile.keycap(format!("{}", index + 1)))
                }),
        )
}

pub fn permission_card(
    title: impl Into<SharedString>,
    counter: Option<SharedString>,
    options: Vec<AnyElement>,
    cancel: Button,
    cx: &App,
) -> Panel {
    card_surface("chat-permission-card")
        .child(
            card_header()
                .child(card_glyph(
                    Icon::new(IconName::TriangleAlert).text_color(cx.theme().warning),
                    cx,
                ))
                .child(card_title(title))
                .when_some(counter, |row, counter| row.child(card_chip(counter)))
                .child(card_close(cancel, cx)),
        )
        .child(card_tiles().pb(px(CARD_GUTTER)).children(options))
}
