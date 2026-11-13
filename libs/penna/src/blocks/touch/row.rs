use std::{rc::Rc, sync::Arc};

use ala::{
    AnyElement, App, ClickEvent, Corners, Div, ElementId, FontFeatures, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px, relative,
};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable, Emphasis, Ground, Icon, IconName, PaintState,
    RULE_WIDTH, RuleLook, Space, StyledExt as _, Surface, TextStyle, Tone,
    blocks::settings::StackPosition,
    blocks::touch::StatusMark,
    components::separator::Separator,
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
const RULED_INSET: Space = Space::S16;
const RULED_GAP: Space = Space::S12;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Listed {
    position: StackPosition,
    corner: Pixels,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchInk {
    Quiet,
    Accent,
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct TouchRow {
    id: ElementId,
    title: SharedString,
    icon: Option<IconName>,
    slot: bool,
    checked: bool,
    detail: Option<SharedString>,
    trailing: Option<TouchTrailing>,
    more: bool,
    end: Option<AnyElement>,
    danger: bool,
    ink: Option<TouchInk>,
    disabled: bool,
    nest: Option<Nest>,
    listed: Option<Listed>,
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
            slot: false,
            checked: false,
            detail: None,
            trailing: None,
            more: false,
            end: None,
            danger: false,
            ink: None,
            disabled: false,
            nest: None,
            listed: None,
            ground: None,
            on_click: None,
        }
    }

    #[must_use]
    pub fn more(mut self) -> Self {
        self.more = true;
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn slot(mut self) -> Self {
        self.slot = true;
        self
    }

    #[must_use]
    pub fn checked(mut self) -> Self {
        self.checked = true;
        self.slot = true;
        self
    }

    #[must_use]
    pub fn end(mut self, end: impl IntoElement) -> Self {
        self.end = Some(end.into_any_element());
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
    pub fn ink(mut self, ink: TouchInk) -> Self {
        self.ink = Some(ink);
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

    #[must_use]
    pub fn in_sheet(self, cx: &App) -> Self {
        let theme = cx.theme();
        let nest = Nest {
            outer: theme.phone_radius(),
            inset: theme.row_inset,
        };
        self.nested(nest, crate::blocks::sheet::sheet_ground(cx))
    }

    pub(crate) fn nested(mut self, nest: Nest, ground: Ground) -> Self {
        self.nest = Some(nest);
        self.ground = Some(ground);
        self
    }

    fn listed(mut self, position: StackPosition, corner: Pixels, ground: Ground) -> Self {
        self.listed = Some(Listed { position, corner });
        self.ground = Some(ground);
        self
    }
}

fn listed_frame(
    id: ElementId,
    surface: Surface,
    height: Pixels,
    listed: Listed,
    state: PaintState,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let layer = theme.layer(surface, PaintState::Rest);
    let corner = |rounds: bool| if rounds { listed.corner } else { px(0.0) };
    let corners = Corners {
        top_left: corner(listed.position.rounds_top()),
        top_right: corner(listed.position.rounds_top()),
        bottom_left: corner(listed.position.rounds_bottom()),
        bottom_right: corner(listed.position.rounds_bottom()),
    };
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
                crate::blocks::touch::PressCorners::Each(corners),
            ))
    };
    row.when(listed.position.rounds_top(), |row| {
        row.rounded_t(listed.corner)
    })
    .when(listed.position.rounds_bottom(), |row| {
        row.rounded_b(listed.corner)
    })
    .corner_smoothing(layer.smoothing)
    .relative()
    .w_full()
    .gap(RULED_GAP)
    .px(RULED_INSET)
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
        let toned = match (self.danger, self.ink) {
            (true, _) => Some(Tone::Danger),
            (false, Some(TouchInk::Accent)) => Some(Tone::Accent),
            _ => None,
        };
        let toned = toned.map(|tone| theme.paint_on(layer.ground, tone, Emphasis::Ghost).text);
        let quiet = self.ink == Some(TouchInk::Quiet);
        let ink = toned.unwrap_or(if quiet {
            painted.muted_text()
        } else {
            painted.text
        });
        let muted = toned.unwrap_or_else(|| painted.muted_text());
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
                    .when(self.ink == Some(TouchInk::Accent), |title| {
                        title.font_weight(TextStyle::TouchBody.strong_weight())
                    })
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
        let led = self.icon.is_some() || self.checked || self.slot;
        let listed = self.listed;
        let frame = match listed {
            Some(listed) => listed_frame(self.id, surface, height, listed, state, cx),
            None => touch_row_frame(self.id, surface, height, self.nest, state, cx),
        };
        let rule = listed
            .filter(|listed| listed.position.rules_above())
            .map(|_| {
                let edge = if led {
                    RULED_INSET.rems() + crate::rems_from_px(GLYPH) + RULED_GAP.rems()
                } else {
                    RULED_INSET.rems()
                };
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .left(edge)
                    .child(Separator::horizontal().look(RuleLook::List).h(RULE_WIDTH))
            });
        frame
            .children(rule)
            .role(Role::Button)
            .accessibility_id(author)
            .aria_label(self.title)
            .aria_disabled(disabled)
            .type_style(TextStyle::TouchBody, cx)
            .text_color(ink)
            .map(|row| match self.icon {
                Some(icon) => row.child(glyph(icon, muted)),
                None if self.checked => row.child(glyph(IconName::Check, ink)),
                None if self.slot => row.child(div().flex_none().w(px(GLYPH))),
                None => row,
            })
            .child(lines)
            .children(trailing)
            .when(self.more && !disabled, |row| {
                row.child(glyph(IconName::ChevronRight, muted))
            })
            .when_some(self.end, |row, end| row.pr(Space::S4).child(end))
            .when_some(self.on_click.filter(|_| !disabled), |row, on_click| {
                row.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}

#[derive(Default, IntoElement)]
pub struct TouchGroup {
    rows: Vec<TouchRow>,
    ruled: bool,
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

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    #[must_use]
    pub fn ruled(mut self) -> Self {
        self.ruled = true;
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
        if self.ruled {
            let card = card.on(theme.content_ground());
            let layer = theme.layer(card, PaintState::Rest);
            let ground = layer.ground;
            let corner = (outer - px(Space::S8.design_px())).max(px(0.0));
            let last = self.rows.len().saturating_sub(1);
            return v_flex()
                .w_full()
                .items_stretch()
                .layer(&layer)
                .rounded(corner)
                .refine_style(&self.style)
                .children(self.rows.into_iter().enumerate().map(|(index, row)| {
                    row.listed(StackPosition::at(index, last), corner, ground)
                }));
        }
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
    on_hold: Option<crate::blocks::compact::HoldHandler>,
}

impl TouchHeader {
    #[must_use]
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            count: None,
            over_tiles: false,
            holdable: false,
            on_hold: None,
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

    #[must_use]
    pub fn on_hold(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.holdable = true;
        self.on_hold = Some(Rc::new(handler));
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
            .relative()
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
            .children(self.on_hold.map(crate::blocks::compact::hold_listener))
    }
}
