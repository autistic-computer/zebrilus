use std::rc::Rc;

use ala::{
    AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Layer, PaintState, Space, StyledExt as _,
    Surface, TextStyle,
    blocks::touch::{StatusMark, TOUCH_DETAIL_ROW_HEIGHT, press_highlight},
    components::separator::Separator,
    foundation::{author_name, roles},
    h_flex, v_flex,
};

use super::{
    machine_tile::phone_card_ground,
    tile::{TILE_INSET, hold_listener},
};

pub const TAIL_LINES: u16 = 4;
const HEADER: f32 = 24.0;
const FACE_SLOT: f32 = 28.0;
const CHEVRON: f32 = 20.0;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type HoldHandler = Rc<dyn Fn(&mut Window, &mut App)>;

fn line(text: SharedString) -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text)
}

#[derive(IntoElement)]
pub struct WindowTail {
    id: ElementId,
    text: AnyElement,
}

impl WindowTail {
    pub fn new(id: impl Into<ElementId>, text: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            text: text.into_any_element(),
        }
    }
}

impl RenderOnce for WindowTail {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let height = TextStyle::Mono.line_height_px() * f32::from(TAIL_LINES);
        div().id(self.id).flex_1().min_w_0().child(
            v_flex()
                .h(crate::rems_from_px(height))
                .justify_end()
                .overflow_hidden()
                .whitespace_nowrap()
                .type_style(TextStyle::Mono, cx)
                .text_color(theme.foreground.muted())
                .child(self.text),
        )
    }
}

#[derive(IntoElement)]
pub struct WindowCard {
    id: ElementId,
    title: SharedString,
    faces: Option<AnyElement>,
    line: SharedString,
    state: Option<(StatusMark, SharedString)>,
    working: bool,
    waiting: bool,
    tails: Vec<WindowTail>,
    on_click: Option<ClickHandler>,
    on_hold: Option<HoldHandler>,
}

impl WindowCard {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            faces: None,
            line: SharedString::default(),
            state: None,
            working: false,
            waiting: false,
            tails: Vec::new(),
            on_click: None,
            on_hold: None,
        }
    }

    #[must_use]
    pub fn faces(mut self, faces: impl IntoElement) -> Self {
        self.faces = Some(faces.into_any_element());
        self
    }

    #[must_use]
    pub fn line(mut self, line: impl Into<SharedString>) -> Self {
        self.line = line.into();
        self
    }

    #[must_use]
    pub fn state(mut self, mark: StatusMark, word: impl Into<SharedString>) -> Self {
        self.state = Some((mark, word.into()));
        self
    }

    #[must_use]
    pub fn working(mut self, working: bool) -> Self {
        self.working = working;
        self
    }

    #[must_use]
    pub fn waiting(mut self, waiting: bool) -> Self {
        self.waiting = waiting;
        self
    }

    #[must_use]
    pub fn tail(mut self, tail: WindowTail) -> Self {
        self.tails.push(tail);
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

impl RenderOnce for WindowCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let frame = theme.layer(Surface::new(roles::PHONE_CARD), PaintState::Rest);
        let radius = theme.phone_radius();
        let muted = frame.muted_text();
        let fill = phone_card_ground(self.waiting, cx).fill;
        let label = match (&self.state, self.line.is_empty()) {
            (Some((_, word)), _) => SharedString::from(format!("{}, {word}", self.title)),
            (None, false) => SharedString::from(format!("{}, {}", self.title, self.line)),
            (None, true) => self.title.clone(),
        };
        let card = div()
            .id(self.id.clone())
            .relative()
            .w_full()
            .overflow_hidden()
            .click_cursor()
            .role(Role::Button)
            .accessibility_id(author_name(&self.id))
            .aria_label(label)
            .layer(&Layer {
                fill,
                glass: None,
                radius,
                share: None,
                ..frame
            })
            .font_family(theme.font_family.clone());
        let faces = self
            .faces
            .map(|faces| h_flex().flex_none().min_w(px(FACE_SLOT)).child(faces));
        let card = if self.tails.is_empty() {
            let state = self.state.map(|(mark, word)| {
                div()
                    .flex_none()
                    .text_color(mark.word_ink(muted, cx))
                    .child(word)
            });
            let detail = h_flex()
                .min_w_0()
                .gap(Space::S6)
                .type_style(TextStyle::TouchSmall, cx)
                .text_color(muted)
                .when(self.working, |row| row.child(StatusMark::Spinner))
                .children(state)
                .when(!self.line.is_empty(), |row| row.child(line(self.line)));
            card.child(
                h_flex()
                    .min_h(px(TOUCH_DETAIL_ROW_HEIGHT))
                    .px(TILE_INSET)
                    .gap(Space::S12)
                    .children(faces)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                line(self.title)
                                    .type_style_strong(TextStyle::TouchBody, cx)
                                    .text_color(theme.foreground),
                            )
                            .child(detail),
                    )
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(CHEVRON))
                            .flex_none()
                            .text_color(muted),
                    ),
            )
        } else {
            let mut tails = h_flex().items_stretch().gap(Space::S6).min_w_0();
            for (index, tail) in self.tails.into_iter().enumerate() {
                if index > 0 {
                    tails = tails.child(Separator::vertical());
                }
                tails = tails.child(tail);
            }
            let state = self.state.map(|(mark, word)| {
                div()
                    .flex_none()
                    .type_style_strong(TextStyle::TouchSmall, cx)
                    .text_color(mark.word_ink(muted, cx))
                    .child(word)
            });
            card.child(
                v_flex()
                    .p(TILE_INSET)
                    .gap(Space::GAP_RELATED)
                    .child(
                        h_flex()
                            .h(px(HEADER))
                            .gap(Space::S8)
                            .children(faces)
                            .child(
                                line(self.title)
                                    .flex_1()
                                    .type_style_strong(TextStyle::TouchBody, cx)
                                    .text_color(theme.foreground),
                            )
                            .when(self.working, |header| header.child(StatusMark::Spinner))
                            .children(state),
                    )
                    .child(tails),
            )
        };
        let on_click = self.on_click;
        card.children(self.on_hold.map(hold_listener))
            .child(press_highlight(self.id, None, radius))
            .when_some(on_click, |card, on_click| {
                card.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}
