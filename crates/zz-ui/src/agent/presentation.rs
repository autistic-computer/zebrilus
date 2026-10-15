use std::time::Duration;

use zz_gpui::{
    AnyElement, App, Div, ElementId, EntityId, FontWeight, SharedString, Stateful, Transformation,
    div, ease_in_out, percentage, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Edge, Emphasis, Ground, Icon, IconName, PaintState,
    StyledExt as _, Surface, Tone, badge::Badge, button::Button, feedback::inline_alert, h_flex,
    kbd::Keycap, picker::muted_on, pulse::pulse_phase, roles, v_flex,
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

pub fn error_card(error: &str, stopped: bool, cx: &App) -> Div {
    inline_alert(
        stopped.then(|| "The agent stopped".into()),
        rendered_error(error),
        cx,
    )
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
                    .medium()
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
) -> Div {
    card_surface(cx)
        .child(
            card_header()
                .child(card_glyph(
                    Icon::new(IconName::TriangleAlert).text_color(cx.theme().warning),
                    cx,
                ))
                .child(div().min_w_0().flex_1().child(title.into()))
                .when_some(counter, |row, counter| row.child(card_chip(counter)))
                .child(card_close(cancel, cx)),
        )
        .child(card_tiles().pb(px(CARD_GUTTER)).children(options))
}

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

/// `button` on a card, drawn on the card's surface.
pub fn card_button(button: Button, cx: &App) -> Button {
    button.on(cx.theme().ground_of(roles::INLINE_PANEL))
}

pub(crate) const CARD_GUTTER: f32 = 12.0;
pub(crate) const CARD_GAP: f32 = 8.0;
pub(crate) const CARD_GLYPH: f32 = 14.0;
pub(crate) const CARD_TITLE_LINE: f32 = 18.0;
const CARD_TITLE_SIZE: f32 = 13.0;
const CARD_TITLE_TOP: f32 = 8.0;
const CARD_TITLE_BOTTOM: f32 = 10.0;
const CARD_TILE_GAP: f32 = 2.0;
const CARD_TILE_PADDING_X: f32 = 12.0;
const CARD_TILE_PADDING_Y: f32 = 8.0;
const CARD_LABEL_LINE: f32 = 16.0;
const CARD_DETAIL_LINE: f32 = 14.0;
const CARD_CLOSE: f32 = 24.0;

pub(crate) fn card_surface(cx: &App) -> Div {
    v_flex()
        .w_full()
        .overflow_hidden()
        .surface(roles::INLINE_PANEL, cx)
}

pub(crate) fn card_header() -> Div {
    h_flex()
        .w_full()
        .items_start()
        .gap(px(CARD_GAP))
        .px(px(CARD_GUTTER))
        .pt(px(CARD_TITLE_TOP))
        .pb(px(CARD_TITLE_BOTTOM))
        .text_size(crate::rems_from_px(CARD_TITLE_SIZE))
        .line_height(px(CARD_TITLE_LINE))
        .font_weight(FontWeight::MEDIUM)
}

pub(crate) fn card_glyph(icon: Icon, cx: &App) -> Div {
    h_flex()
        .flex_none()
        .w(px(CARD_GLYPH))
        .h(px(CARD_TITLE_LINE))
        .child(
            div()
                .relative()
                .top(crate::icon::cap_center_drop(
                    px(CARD_TITLE_SIZE),
                    px(CARD_GLYPH),
                    cx,
                ))
                .child(icon.size(px(CARD_GLYPH))),
        )
}

pub(crate) fn card_chip(text: impl Into<SharedString>) -> Badge {
    Badge::new(text).flex_none()
}

pub(crate) fn card_divider(cx: &App) -> Div {
    div()
        .mx(px(CARD_GUTTER))
        .my(px(CARD_GAP))
        .rule_on(roles::LIST_RULE, Edge::Top, cx)
}

pub(crate) fn card_tiles() -> Div {
    v_flex().w_full().gap(px(CARD_TILE_GAP)).px(px(CARD_GUTTER))
}

/// An option tile in a card: neutral, or danger for a destructive choice,
/// selectable. What sits inside it (its description, keycaps, checks) paints
/// on the tile's ground.
#[derive(Clone, Copy)]
pub(crate) struct CardTile {
    surface: Surface,
    state: PaintState,
    ground: Ground,
    muted: zz_gpui::Hsla,
}

impl CardTile {
    pub(crate) fn new(tone: Tone, enabled: bool, selected: bool, cx: &App) -> Self {
        let surface = Surface::new(roles::AGENT_OPTION).tone(tone);
        let state = crate::picker::row_state(selected, enabled);
        let layer = cx.theme().layer(surface, state);
        Self {
            surface,
            state,
            ground: layer.ground,
            muted: muted_on(&layer, cx),
        }
    }

    pub(crate) fn selected(self) -> bool {
        self.state == PaintState::Selected
    }

    pub(crate) fn ground(self) -> Ground {
        self.ground
    }

    pub(crate) fn render(self, id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
        let enabled = self.state != PaintState::Disabled;
        let height = crate::picker::LIST_ROW_HEIGHT;
        crate::picker::row_surface(h_flex().id(id), self.surface, self.state, height, cx)
            .w_full()
            .min_h(height)
            .items_start()
            .gap(px(CARD_GAP))
            .px(px(CARD_TILE_PADDING_X))
            .py(px(CARD_TILE_PADDING_Y))
            .text_size(crate::rems_from_px(12.0))
            .line_height(px(CARD_LABEL_LINE))
            .when(enabled, zz_gpui::Styled::cursor_pointer)
    }

    pub(crate) fn detail(self, text: impl Into<SharedString>) -> Div {
        div()
            .text_size(crate::rems_from_px(11.0))
            .line_height(px(CARD_DETAIL_LINE))
            .text_color(self.muted)
            .child(text.into())
    }

    pub(crate) fn keycap(self, text: impl Into<SharedString>) -> Keycap {
        Keycap::new(text).on(self.ground())
    }
}

pub(crate) fn card_trailing() -> Div {
    h_flex().flex_none().h(px(CARD_LABEL_LINE))
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
