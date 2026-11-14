use std::rc::Rc;

use ala::{
    AnyElement, App, ClickEvent, Div, ElementId, HitboxBehavior, Hsla, InteractiveElement as _,
    IntoElement, LongPressEvent, ParentElement, Pixels, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TouchPhase, Window, canvas, div,
    prelude::FluentBuilder as _, px, relative,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Layer, PaintState, Space, StyledExt as _,
    Surface, TextStyle, Tile, TileVariant,
    blocks::touch::{StatusMark, concentric, press_highlight},
    foundation::{author_name, roles},
    h_flex, v_flex,
};

use super::bento::BentoSpan;

pub const TILE_INSET: Space = Space::INSET_CARD;
pub const MINI_GAP: f32 = 4.0;
pub const MINI_TEXT: f32 = 5.0;
pub const MINI_LINE: f32 = 6.0;
const ADD_GLYPH: f32 = 20.0;
const PAGE_BARS: [f32; 2] = [0.9, 0.7];
const PAGE_HEAD: f32 = 0.45;
const BUBBLE_BARS: [f32; 2] = [1.0, 0.55];
const REPLY_BARS: [f32; 4] = [1.0, 0.85, 0.95, 0.6];

#[must_use]
pub fn window_span(panes: usize) -> BentoSpan {
    match panes {
        0..=2 => BentoSpan::new(2, 2),
        3 => BentoSpan::new(4, 2),
        _ => BentoSpan::new(4, 3),
    }
}

#[must_use]
pub fn mini_radius(cx: &App) -> Pixels {
    concentric(cx.theme().phone_radius(), px(TILE_INSET.design_px()))
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
pub(crate) type HoldHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PhoneTile {
    id: ElementId,
    title: SharedString,
    add: bool,
    mark: Option<StatusMark>,
    lead: Option<AnyElement>,
    trail: Option<AnyElement>,
    state: Option<SharedString>,
    foot: Vec<SharedString>,
    panes: Vec<([f32; 4], MiniPane)>,
    children: Vec<AnyElement>,
    on_click: Option<ClickHandler>,
    on_hold: Option<HoldHandler>,
}

impl PhoneTile {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            add: false,
            mark: None,
            lead: None,
            trail: None,
            state: None,
            foot: Vec::new(),
            panes: Vec::new(),
            children: Vec::new(),
            on_click: None,
            on_hold: None,
        }
    }

    #[must_use]
    pub fn add(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            add: true,
            ..Self::new(id, title)
        }
    }

    #[must_use]
    pub fn mark(mut self, mark: impl Into<Option<StatusMark>>) -> Self {
        self.mark = mark.into();
        self
    }

    #[must_use]
    pub fn lead(mut self, lead: impl IntoElement) -> Self {
        self.lead = Some(lead.into_any_element());
        self
    }

    #[must_use]
    pub fn trail(mut self, trail: impl IntoElement) -> Self {
        self.trail = Some(trail.into_any_element());
        self
    }

    #[must_use]
    pub fn state(mut self, state: impl Into<SharedString>) -> Self {
        self.state = Some(state.into());
        self
    }

    #[must_use]
    pub fn foot(mut self, line: impl Into<SharedString>) -> Self {
        self.foot.push(line.into());
        self
    }

    #[must_use]
    pub fn pane(mut self, rect: [f32; 4], pane: MiniPane) -> Self {
        self.panes.push((rect, pane));
        self
    }

    #[must_use]
    pub fn panes(mut self, panes: impl IntoIterator<Item = ([f32; 4], MiniPane)>) -> Self {
        self.panes.extend(panes);
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
    pub fn on_hold(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_hold = Some(Rc::new(handler));
        self
    }
}

pub fn hold_listener(on_hold: Rc<dyn Fn(&mut Window, &mut App)>) -> impl IntoElement {
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            window.on_mouse_event(move |event: &LongPressEvent, phase, window, cx| {
                if phase.bubble()
                    && event.phase == TouchPhase::Started
                    && !window.default_prevented()
                    && hitbox.is_hovered_at(event.start_position, window)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    on_hold(window, cx);
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

impl ParentElement for PhoneTile {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

fn line(text: SharedString) -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text)
}

impl RenderOnce for PhoneTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let role = if self.add {
            roles::PHONE_ADD
        } else {
            roles::PHONE_CARD
        };
        let frame = theme.layer(Surface::new(role), PaintState::Rest);
        let radius = theme.phone_radius();
        let muted = frame.muted_text();
        let label = match &self.state {
            Some(state) => SharedString::from(format!("{}, {state}", self.title)),
            None => self.title.clone(),
        };
        let tile = v_flex()
            .id(self.id.clone())
            .relative()
            .size_full()
            .overflow_hidden()
            .click_cursor()
            .role(Role::Button)
            .accessibility_id(author_name(&self.id))
            .aria_label(label)
            .layer(&Layer {
                glass: None,
                radius,
                share: None,
                ..frame
            })
            .font_family(theme.font_family.clone());
        let tile = if self.add {
            tile.items_center()
                .justify_center()
                .gap(Space::S6)
                .type_style(TextStyle::TouchBody, cx)
                .text_color(muted)
                .child(Icon::new(IconName::Plus).size(px(ADD_GLYPH)))
                .child(div().whitespace_nowrap().child(self.title))
        } else {
            let state_ink = self.mark.map_or(muted, |mark| mark.word_ink(muted, cx));
            let preview = !self.panes.is_empty();
            tile.p(TILE_INSET)
                .child(
                    h_flex()
                        .flex_none()
                        .gap(Space::S8)
                        .type_style_strong(TextStyle::TouchBody, cx)
                        .children(self.mark)
                        .children(self.lead)
                        .child(line(self.title).flex_1())
                        .children(self.trail),
                )
                .when_some(self.state, |tile, state| {
                    tile.child(
                        line(state)
                            .flex_none()
                            .mt(Space::S2)
                            .type_style(TextStyle::TouchSmall, cx)
                            .text_color(state_ink),
                    )
                })
                .when(preview, |tile| {
                    tile.child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .mt(Space::GAP_RELATED)
                            .child(mini_layout(self.panes)),
                    )
                })
                .when(!self.foot.is_empty(), |tile| {
                    tile.when(!preview, |tile| tile.child(div().flex_1()))
                        .child(
                            v_flex()
                                .flex_none()
                                .gap(Space::S2)
                                .type_style(TextStyle::TouchSmall, cx)
                                .text_color(muted)
                                .children(self.foot.into_iter().map(line)),
                        )
                })
        };
        let on_click = self.on_click;
        tile.children(self.children)
            .children(self.on_hold.map(hold_listener))
            .child(press_highlight(self.id, None, radius))
            .when_some(on_click, |tile, on_click| {
                tile.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}

fn mini_layout(panes: Vec<([f32; 4], MiniPane)>) -> Div {
    let half = px(MINI_GAP / 2.0);
    div()
        .absolute()
        .inset(-half)
        .children(panes.into_iter().map(|([x, y, width, height], pane)| {
            div()
                .absolute()
                .left(relative(x))
                .top(relative(y))
                .w(relative(width))
                .h(relative(height))
                .p(half)
                .child(pane)
        }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiniPreview {
    Blank,
    Terminal,
    Browser(SharedString),
    Chat,
}

#[derive(IntoElement)]
pub struct MiniPane {
    id: ElementId,
    preview: MiniPreview,
    content: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl MiniPane {
    #[must_use]
    pub fn blank(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            preview: MiniPreview::Blank,
            content: None,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn terminal(id: impl Into<ElementId>, text: impl IntoElement) -> Self {
        Self {
            preview: MiniPreview::Terminal,
            content: Some(text.into_any_element()),
            ..Self::blank(id)
        }
    }

    #[must_use]
    pub fn browser(id: impl Into<ElementId>, host: impl Into<SharedString>) -> Self {
        Self {
            preview: MiniPreview::Browser(host.into()),
            ..Self::blank(id)
        }
    }

    #[must_use]
    pub fn chat(id: impl Into<ElementId>) -> Self {
        Self {
            preview: MiniPreview::Chat,
            ..Self::blank(id)
        }
    }
}

impl ParentElement for MiniPane {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

fn bars(widths: &[f32], color: Hsla) -> impl Iterator<Item = Div> + '_ {
    widths.iter().map(move |width| {
        div()
            .flex_none()
            .w(relative(*width))
            .h(Space::S4)
            .rounded(Space::S2)
            .bg(color)
    })
}

#[allow(
    clippy::disallowed_methods,
    reason = "a mini pane draws its terminal's text as a picture, under the type floor"
)]
fn mini_text(cx: &App) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .pt(Space::S6)
        .px(Space::S6)
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(MINI_TEXT))
        .line_height(px(MINI_LINE))
}

impl RenderOnce for MiniPane {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let ink = theme.foreground;
        let preview = match self.preview {
            MiniPreview::Blank => None,
            MiniPreview::Terminal => Some(mini_text(cx).children(self.content).into_any_element()),
            MiniPreview::Browser(host) => Some(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        line(host)
                            .flex_none()
                            .pt(Space::S6)
                            .px(Space::S8)
                            .type_style(TextStyle::Body, cx),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .gap(Space::S4)
                            .p(Space::S8)
                            .child(
                                div()
                                    .flex_none()
                                    .w(relative(PAGE_HEAD))
                                    .h(Space::S6)
                                    .rounded(Space::S2)
                                    .bg(ink.opacity(0.22)),
                            )
                            .children(bars(&PAGE_BARS, ink.opacity(0.14)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_h(Space::S12)
                                    .max_h(px(48.0))
                                    .rounded(Space::S4)
                                    .bg(ink.opacity(0.08)),
                            ),
                    )
                    .into_any_element(),
            ),
            MiniPreview::Chat => Some(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .justify_end()
                    .gap(Space::S8)
                    .p(Space::S8)
                    .child(
                        v_flex()
                            .flex_none()
                            .self_end()
                            .w(relative(0.64))
                            .gap(Space::S4)
                            .py(Space::S6)
                            .px(Space::S8)
                            .rounded(Space::S8)
                            .bg(ink.opacity(0.10))
                            .children(bars(&BUBBLE_BARS, ink.opacity(0.22))),
                    )
                    .child(
                        v_flex()
                            .flex_none()
                            .gap(Space::S4)
                            .children(bars(&REPLY_BARS, ink.opacity(0.14))),
                    )
                    .into_any_element(),
            ),
        };
        Tile::new(self.id)
            .variant(TileVariant::Mini)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme.background)
            .rounded(mini_radius(cx))
            .text_color(theme.foreground.muted())
            .children(preview)
            .children(self.children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_grows_with_its_panes() {
        assert_eq!(window_span(1), BentoSpan::new(2, 2));
        assert_eq!(window_span(2), BentoSpan::new(2, 2));
        assert_eq!(window_span(3), BentoSpan::new(4, 2));
        assert_eq!(window_span(6), BentoSpan::new(4, 3));
    }
}
