use ala::{
    App, ClickEvent, Div, ElementId, RenderOnce, Role, SharedString, Toggled, Window, div,
    prelude::*, px,
};

use super::composer::{COMPOSER_BUTTON, COMPOSER_FOOTER_INSET};
use crate::{
    ActiveTheme as _, Disableable as _, Edge, Emphasis, Ground, Icon, IconName, Label, Row,
    RowLook, RuleLook, Sizable as _, StyledExt as _, Surface, TextStyle, Tone, Well,
    components::button::{Button, ButtonVariants as _},
    components::icon::cap_center_drop,
    components::kbd::Keycap,
    h_flex, muted_on, rems_from_px, row_state, v_flex,
};

pub const ASK_RADIUS: f32 = 16.0;
pub const ASK_GUTTER: f32 = 14.0;
pub const ASK_GAP: f32 = 8.0;
pub(super) const ASK_ROW_PADDING: f32 = 10.0;
const ASK_HEADER_TOP: f32 = 8.0;
const ASK_HEADER_HEIGHT: f32 = 22.0;
const ASK_QUESTION_INSET: f32 = (ASK_HEADER_HEIGHT - TextStyle::Body.line_height_px()) / 2.0;
const ASK_QUESTION_BELOW: f32 = 8.0;
const ASK_GLYPH: f32 = 14.0;
const ASK_CONTENT_INSET: f32 = 10.0;
const ASK_OPTIONS_BOTTOM: f32 = 5.0;
const ASK_OPTION_GAP: f32 = 2.0;
const ASK_DIVIDER_GAP: f32 = 4.0;
const ASK_OPTION_RADIUS: f32 = 10.0;
const ASK_OPTION_PARTS: f32 = 10.0;
const ASK_INSET_RADIUS: f32 = 9.0;
const ASK_BUTTON_RADIUS: f32 = 9.0;
const ASK_HINT_GAP: f32 = 14.0;
const ASK_RETURN: f32 = 12.0;

pub fn ask_header(icon: IconName, title: impl Into<SharedString>, ground: Ground, cx: &App) -> Div {
    let size = TextStyle::Small.design_px();
    h_flex()
        .debug_selector(|| "chat-ask-header".to_owned())
        .w_full()
        .flex_none()
        .min_h(px(ASK_HEADER_HEIGHT))
        .mt(px(ASK_HEADER_TOP))
        .px(px(ASK_GUTTER))
        .items_center()
        .gap(px(ASK_GAP))
        .type_style(TextStyle::Small, cx)
        .text_color(muted_on(ground, cx))
        .child(
            div()
                .flex_none()
                .relative()
                .top(cap_center_drop(px(size), px(ASK_GLYPH), cx))
                .child(Icon::new(icon).size(px(ASK_GLYPH))),
        )
        .child(
            Label::new(TextStyle::Small, title)
                .strong()
                .on(ground)
                .flex_1()
                .min_w_0()
                .truncate(),
        )
}

pub fn ask_question(question: impl Into<SharedString>, ground: Ground, cx: &App) -> Div {
    let size = TextStyle::Body.design_px();
    h_flex()
        .debug_selector(|| "chat-ask-question".to_owned())
        .w_full()
        .flex_none()
        .items_start()
        .mt(px(ASK_HEADER_TOP))
        .pb(px(ASK_QUESTION_BELOW))
        .px(px(ASK_GUTTER))
        .gap(px(ASK_GAP))
        .type_style(TextStyle::Small, cx)
        .text_color(muted_on(ground, cx))
        .child(
            h_flex().flex_none().h(px(ASK_HEADER_HEIGHT)).child(
                div()
                    .relative()
                    .top(cap_center_drop(px(size), px(ASK_GLYPH), cx))
                    .child(Icon::new(IconName::Chat).size(px(ASK_GLYPH))),
            ),
        )
        .child(
            Label::new(TextStyle::Body, question)
                .strong()
                .tone(Tone::Neutral)
                .on(ground)
                .flex_1()
                .min_w_0()
                .py(px(ASK_QUESTION_INSET)),
        )
}

pub fn ask_count(text: impl Into<SharedString>, ground: Ground) -> Div {
    h_flex()
        .flex_none()
        .h(px(ASK_HEADER_HEIGHT))
        .child(Label::new(TextStyle::Small, text).muted().on(ground))
}

pub fn ask_step(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: impl Into<SharedString>,
    ground: Ground,
) -> Button {
    Button::compact_icon(id, icon)
        .size(rems_from_px(ASK_HEADER_HEIGHT))
        .on(ground)
        .tooltip(tooltip)
}

pub fn ask_body() -> Div {
    v_flex()
        .debug_selector(|| "chat-ask-body".to_owned())
        .w_full()
        .min_h_0()
        .px(px(ASK_GUTTER))
        .pt(px(8.0))
        .pb(px(10.0))
        .gap(px(4.0))
}

pub fn ask_inset(cx: &App) -> Well {
    Well::new()
        .w_full()
        .min_w_0()
        .rounded(px(ASK_INSET_RADIUS))
        .px(px(ASK_CONTENT_INSET))
        .py(px(7.0))
        .type_style_mono(TextStyle::Small, cx)
        .line_height(px(18.0))
        .text_color(Well::ground(cx).ink)
}

pub fn ask_sheet(cx: &App) -> Well {
    Well::new()
        .flex_none()
        .min_w_0()
        .mx(px(ASK_GUTTER))
        .mt(px(8.0))
        .mb(px(10.0))
        .rounded(px(ASK_INSET_RADIUS))
        .px(px(ASK_CONTENT_INSET))
        .py(px(10.0))
        .text_color(Well::ground(cx).ink)
}

pub fn ask_note(text: impl Into<SharedString>, ground: Ground) -> Label {
    Label::new(TextStyle::Caption, text)
        .muted()
        .on(ground)
        .truncate()
}

pub fn ask_options() -> Div {
    v_flex()
        .debug_selector(|| "chat-ask-options".to_owned())
        .w_full()
        .flex_none()
        .px(px(ASK_GUTTER))
        .pb(px(ASK_OPTIONS_BOTTOM))
        .gap(px(ASK_OPTION_GAP))
}

pub fn ask_divider(cx: &App) -> Div {
    div()
        .w_full()
        .flex_none()
        .my(px(ASK_DIVIDER_GAP - ASK_OPTION_GAP))
        .rule(RuleLook::List, Edge::Top, cx)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskDetail {
    Beside,
    Under,
}

type AskClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct AskOption {
    id: ElementId,
    label: SharedString,
    ground: Ground,
    detail: Option<SharedString>,
    place: AskDetail,
    key: Option<SharedString>,
    picked: Option<bool>,
    highlighted: bool,
    enabled: bool,
    on_click: Option<AskClick>,
}

impl AskOption {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, ground: Ground) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            ground,
            detail: None,
            place: AskDetail::Beside,
            key: None,
            picked: None,
            highlighted: false,
            enabled: true,
            on_click: None,
        }
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>, place: AskDetail) -> Self {
        self.detail = Some(detail.into());
        self.place = place;
        self
    }

    #[must_use]
    pub fn key(mut self, key: impl Into<SharedString>) -> Self {
        self.key = Some(key.into());
        self
    }

    #[must_use]
    pub const fn picked(mut self, picked: bool) -> Self {
        self.picked = Some(picked);
        self
    }

    #[must_use]
    pub const fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }

    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(on_click));
        self
    }
}

impl RenderOnce for AskOption {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let base = self.ground;
        let state = row_state(self.highlighted && self.enabled, self.enabled);
        let ground: Ground = cx
            .theme()
            .ground_at(Surface::from(RowLook::Menu).on(base), state);
        let title = Label::new(TextStyle::Body, self.label).strong().on(ground);
        let detail = self
            .detail
            .map(|detail| Label::new(TextStyle::Small, detail).muted().on(ground));
        let text = match self.place {
            AskDetail::Under => v_flex().flex_1().min_w_0().child(title).children(detail),
            AskDetail::Beside => h_flex()
                .flex_1()
                .min_w_0()
                .items_baseline()
                .gap(px(ASK_GAP))
                .child(title.flex_none())
                .children(detail.map(|detail| detail.min_w_0().truncate())),
        };
        let padding = match self.place {
            AskDetail::Under => 6.0,
            AskDetail::Beside => 7.0,
        };
        let enabled = self.enabled;
        let cap = match (self.key, self.picked) {
            (key, Some(true)) => Some(ask_picked_key(key, ground)),
            (Some(key), _) => Some(ask_key(key, ground)),
            (None, _) => None,
        };
        Row::new(self.id, RowLook::Menu)
            .on(base)
            .state(state)
            .radius(px(ASK_OPTION_RADIUS))
            .h_flex()
            .w_full()
            .flex_none()
            .items_center()
            .gap(px(ASK_OPTION_PARTS))
            .px(px(ASK_CONTENT_INSET))
            .py(px(padding))
            .when(enabled, crate::StyledExt::click_cursor)
            .when_some(self.picked, |row, picked| {
                row.role(Role::CheckBox).aria_toggled(if picked {
                    Toggled::True
                } else {
                    Toggled::False
                })
            })
            .child(text)
            .children(cap.map(|cap| div().flex_none().child(cap.disabled(!enabled))))
            .when_some(self.on_click.filter(|_| enabled), |row, on_click| {
                row.on_click(on_click)
            })
    }
}

pub fn ask_leading(child: impl IntoElement) -> Div {
    h_flex()
        .debug_selector(|| "chat-ask-leading".to_owned())
        .flex_none()
        .h(px(COMPOSER_BUTTON))
        .pl(px(ASK_CONTENT_INSET))
        .child(child)
}

pub fn ask_key(key: impl Into<SharedString>, ground: Ground) -> Keycap {
    Keycap::new(key).small().on(ground)
}

pub fn ask_picked_key(key: Option<SharedString>, ground: Ground) -> Keycap {
    match key {
        Some(key) => Keycap::new(key),
        None => Keycap::icon(IconName::Check),
    }
    .small()
    .on(ground)
    .tone(Tone::Accent)
    .emphasis(Emphasis::Solid)
}

pub fn ask_send(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    ground: Ground,
) -> Button {
    Button::new(id)
        .primary()
        .label(label)
        .on(ground)
        .rounded(px(ASK_BUTTON_RADIUS))
        .h(px(COMPOSER_BUTTON))
        .flex_none()
        .pl(px(12.0))
        .pr(px(8.0))
        .gap(px(6.0))
        .child(Icon::new(IconName::KeyReturn).size(px(ASK_RETURN)))
}

pub fn ask_skip(id: impl Into<ElementId>, ground: Ground) -> Button {
    Button::new(id)
        .ghost()
        .label("Skip")
        .on(ground)
        .rounded(px(ASK_BUTTON_RADIUS))
        .h(px(COMPOSER_BUTTON))
        .flex_none()
        .px(px(10.0))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskHint {
    text: SharedString,
    strong: bool,
}

impl AskHint {
    #[must_use]
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            strong: false,
        }
    }

    #[must_use]
    pub fn strong(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            strong: true,
        }
    }
}

impl From<&'static str> for AskHint {
    fn from(text: &'static str) -> Self {
        Self::new(text)
    }
}

impl From<String> for AskHint {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<SharedString> for AskHint {
    fn from(text: SharedString) -> Self {
        Self::new(text)
    }
}

pub fn ask_hints<T: Into<AskHint>>(items: impl IntoIterator<Item = T>, ground: Ground) -> Div {
    h_flex()
        .debug_selector(|| "chat-ask-hints".to_owned())
        .min_w_0()
        .gap(px(ASK_HINT_GAP))
        .px(px(ASK_GUTTER - COMPOSER_FOOTER_INSET))
        .overflow_hidden()
        .children(items.into_iter().map(|item| {
            let hint: AskHint = item.into();
            let label = Label::new(TextStyle::Caption, hint.text).on(ground);
            if hint.strong {
                label.strong().tone(Tone::Neutral)
            } else {
                label.muted()
            }
            .flex_none()
        }))
}
