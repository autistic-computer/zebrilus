use std::{rc::Rc, sync::Arc};

use ala::{
    App, ClickEvent, Div, ElementId, FontFeatures, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px, relative,
};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable, Emphasis, Ground, Icon, IconName, PaintState,
    Space, StyledExt as _, Surface, TextStyle, Tone,
    blocks::touch::StatusMark,
    foundation::{
        InteractiveSurfaceExt as _, author_name, nested_radius, roles, row_radius, shared_corners,
    },
    h_flex, v_flex,
};

pub const TOUCH_ROW_HEIGHT: f32 = 52.0;
pub const TOUCH_DETAIL_ROW_HEIGHT: f32 = 64.0;
pub const TOUCH_HEADER_HEIGHT: f32 = 28.0;
const GLYPH: f32 = 20.0;
const GROUP_INSET: Space = Space::S4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Nest {
    pub outer: Pixels,
    pub inset: Pixels,
}

pub(crate) fn touch_row_frame(
    id: impl Into<ElementId>,
    surface: Surface,
    height: Pixels,
    nest: Option<Nest>,
    state: PaintState,
    cx: &App,
) -> Stateful<Div> {
    let id = id.into();
    let theme = cx.theme();
    let layer = theme.layer(surface, PaintState::Rest);
    let own = row_radius(&layer, height);
    let radius = nest.map_or(own, |nest| nested_radius(nest.outer, nest.inset, own));
    let row = h_flex().id(id.clone()).flex_none().h(height);
    let row = if state == PaintState::Disabled {
        row.text_color(layer.text)
    } else {
        row.surface_hover(surface, PaintState::Rest, cx)
            .click_cursor()
            .child(crate::blocks::touch::instant_press_highlight(
                id,
                crate::blocks::touch::over_hover(
                    theme.layer(surface, PaintState::Hover).fill,
                    theme.layer(surface, PaintState::Active).fill,
                ),
                crate::blocks::touch::PressCorners::Row(radius, layer.smoothing),
            ))
    };
    shared_corners(row, radius, height, layer.smoothing)
        .relative()
        .w_full()
        .gap(Space::S12)
        .px(Space::S12)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TouchTrailing {
    Glyph(IconName),
    Value(SharedString),
    Check,
    State(StatusMark, SharedString),
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct TouchRow {
    id: ElementId,
    title: SharedString,
    icon: Option<IconName>,
    detail: Option<SharedString>,
    trailing: Option<TouchTrailing>,
    danger: bool,
    disabled: bool,
    nest: Option<Nest>,
    ground: Option<Ground>,
    on_click: Option<ClickHandler>,
}

impl TouchRow {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon: None,
            detail: None,
            trailing: None,
            danger: false,
            disabled: false,
            nest: None,
            ground: None,
            on_click: None,
        }
    }

    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn trailing(mut self, trailing: TouchTrailing) -> Self {
        self.trailing = Some(trailing);
        self
    }

    #[must_use]
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn height(&self) -> Pixels {
        px(if self.detail.is_some() {
            TOUCH_DETAIL_ROW_HEIGHT
        } else {
            TOUCH_ROW_HEIGHT
        })
    }

    fn nested(mut self, nest: Nest, ground: Ground) -> Self {
        self.nest = Some(nest);
        self.ground = Some(ground);
        self
    }
}

impl Disableable for TouchRow {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

fn glyph(icon: IconName, color: Hsla) -> impl IntoElement {
    Icon::new(icon)
        .size(px(GLYPH))
        .flex_none()
        .text_color(color)
}

impl RenderOnce for TouchRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let surface = Surface::new(roles::PHONE_ROW).on_maybe(self.ground);
        let height = self.height();
        let state = if self.disabled {
            PaintState::Disabled
        } else {
            PaintState::Rest
        };
        let theme = cx.theme();
        let layer = theme.layer(surface, PaintState::Rest);
        let painted = theme.layer(surface, state);
        let danger = self.danger.then(|| {
            theme
                .paint_on(layer.ground, Tone::Danger, Emphasis::Ghost)
                .text
        });
        let ink = danger.unwrap_or(painted.text);
        let muted = danger.unwrap_or_else(|| painted.muted_text());
        let title_ink = if self.disabled {
            theme.foreground.faint()
        } else {
            ink
        };
        let trailing = self
            .trailing
            .filter(|_| !self.disabled)
            .map(|trailing| match trailing {
                TouchTrailing::Glyph(icon) => glyph(icon, muted).into_any_element(),
                TouchTrailing::Check => glyph(IconName::Check, ink).into_any_element(),
                TouchTrailing::Value(value) => div()
                    .flex_initial()
                    .min_w_0()
                    .max_w(relative(0.6))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .type_style(TextStyle::TouchSmall, cx)
                    .text_color(muted)
                    .child(value)
                    .into_any_element(),
                TouchTrailing::State(mark, word) => h_flex()
                    .flex_none()
                    .gap(Space::S6)
                    .type_style(TextStyle::TouchSmall, cx)
                    .text_color(mark.word_ink(muted, cx))
                    .child(mark)
                    .child(word)
                    .into_any_element(),
            });
        let lines = v_flex()
            .flex_1()
            .min_w_0()
            .gap(Space::S2)
            .child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(title_ink)
                    .child(self.title.clone()),
            )
            .when_some(self.detail, |lines, detail| {
                lines.child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .type_style(TextStyle::TouchSmall, cx)
                        .text_color(muted)
                        .child(detail),
                )
            });
        let disabled = self.disabled;
        let author = author_name(&self.id);
        touch_row_frame(self.id, surface, height, self.nest, state, cx)
            .role(Role::Button)
            .accessibility_id(author)
            .aria_label(self.title)
            .aria_disabled(disabled)
            .type_style(TextStyle::TouchBody, cx)
            .text_color(ink)
            .when_some(self.icon, |row, icon| row.child(glyph(icon, muted)))
            .child(lines)
            .children(trailing)
            .when_some(self.on_click.filter(|_| !disabled), |row, on_click| {
                row.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}

#[derive(Default, IntoElement)]
pub struct TouchGroup {
    rows: Vec<TouchRow>,
    style: StyleRefinement,
}

impl TouchGroup {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn row(mut self, row: TouchRow) -> Self {
        self.rows.push(row);
        self
    }

    #[must_use]
    pub fn rows(mut self, rows: impl IntoIterator<Item = TouchRow>) -> Self {
        self.rows.extend(rows);
        self
    }
}

impl Styled for TouchGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TouchGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let card = Surface::new(roles::SETTINGS_CARD);
        let layer = theme.layer(card, PaintState::Rest);
        let outer = theme.phone_radius();
        let nest = Nest {
            outer,
            inset: px(GROUP_INSET.design_px()),
        };
        let ground = layer.ground;
        v_flex()
            .w_full()
            .items_stretch()
            .layer(&layer)
            .rounded(outer)
            .p(GROUP_INSET)
            .gap(Space::S2)
            .refine_style(&self.style)
            .children(self.rows.into_iter().map(|row| row.nested(nest, ground)))
    }
}

#[derive(IntoElement)]
pub struct TouchHeader {
    title: SharedString,
    count: Option<usize>,
    over_tiles: bool,
    holdable: bool,
}

impl TouchHeader {
    #[must_use]
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            count: None,
            over_tiles: false,
            holdable: false,
        }
    }

    #[must_use]
    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    #[must_use]
    pub fn over_tiles(mut self) -> Self {
        self.over_tiles = true;
        self
    }

    #[must_use]
    pub fn holdable(mut self) -> Self {
        self.holdable = true;
        self
    }
}

impl RenderOnce for TouchHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let height = if self.holdable {
            crate::blocks::touch::TOUCH_TARGET
        } else {
            TOUCH_HEADER_HEIGHT
        };
        h_flex()
            .flex_none()
            .w_full()
            .items_end()
            .gap(Space::GAP_RELATED)
            .h(px(height))
            .px(if self.over_tiles {
                Space::INSET_CARD
            } else {
                Space::INSET_PANE
            })
            .pb(Space::S4)
            .type_style_strong(TextStyle::TouchSmall, cx)
            .text_color(cx.theme().foreground.muted())
            .when(self.holdable, crate::StyledExt::click_cursor)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.title),
            )
            .when_some(self.count, |header, count| {
                header.child(
                    div()
                        .flex_none()
                        .font_weight(TextStyle::TouchSmall.weight())
                        .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
                        .child(SharedString::from(count.to_string())),
                )
            })
    }
}
