use std::rc::Rc;

use ala::{
    AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Ground, Layer, PaintState, Space, StyledExt as _, Surface,
    TextStyle,
    blocks::touch::{StatusMark, press_highlight},
    foundation::{author_name, roles},
    h_flex, v_flex,
};

use super::{card::zoom_card_ground, tile::TILE_INSET, tile::hold_listener};

pub const MACHINE_TILE_HEIGHT: f32 = 112.0;
const LINE: f32 = 28.0;
const WAITING_TINT: f32 = 0.14;
const DIMMED: f32 = 0.55;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type HoldHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[must_use]
pub fn phone_card_ground(waiting: bool, cx: &App) -> Ground {
    let ground = zoom_card_ground(cx);
    if !waiting {
        return ground;
    }
    Ground {
        fill: cx
            .theme()
            .warning
            .mix_oklab(ground.fill, WAITING_TINT)
            .opaque(),
        ..ground
    }
}

#[derive(IntoElement)]
pub struct MachineTile {
    id: ElementId,
    name: SharedString,
    face: Option<AnyElement>,
    waiting: Option<SharedString>,
    faces: Option<AnyElement>,
    state: Option<(StatusMark, SharedString)>,
    dimmed: bool,
    on_click: Option<ClickHandler>,
    on_hold: Option<HoldHandler>,
    children: Vec<AnyElement>,
}

impl MachineTile {
    pub fn new(id: impl Into<ElementId>, name: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            face: None,
            waiting: None,
            faces: None,
            state: None,
            dimmed: false,
            on_click: None,
            on_hold: None,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn face(mut self, face: impl IntoElement) -> Self {
        self.face = Some(face.into_any_element());
        self
    }

    #[must_use]
    pub fn waiting(mut self, word: impl Into<SharedString>) -> Self {
        self.waiting = Some(word.into());
        self
    }

    #[must_use]
    pub fn faces(mut self, faces: impl IntoElement) -> Self {
        self.faces = Some(faces.into_any_element());
        self
    }

    #[must_use]
    pub fn state(mut self, mark: StatusMark, word: impl Into<SharedString>) -> Self {
        self.state = Some((mark, word.into()));
        self
    }

    #[must_use]
    pub fn dimmed(mut self) -> Self {
        self.dimmed = true;
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

impl ParentElement for MachineTile {
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

impl RenderOnce for MachineTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let frame = theme.layer(Surface::new(roles::PHONE_CARD), PaintState::Rest);
        let radius = theme.phone_radius();
        let muted = frame.muted_text();
        let warn = StatusMark::Warning.word_ink(muted, cx);
        let fill = phone_card_ground(self.waiting.is_some(), cx).fill;
        let dim = if self.dimmed { DIMMED } else { 1.0 };
        let label = match (&self.waiting, &self.state) {
            (Some(word), _) | (None, Some((_, word))) => {
                SharedString::from(format!("{}, {word}", self.name))
            }
            (None, None) => self.name.clone(),
        };
        let top = h_flex()
            .flex_none()
            .items_start()
            .justify_between()
            .children(self.face.map(|face| div().opacity(dim).child(face)))
            .children(self.waiting.map(|word| {
                div()
                    .type_style_strong(TextStyle::TouchSmall, cx)
                    .text_color(warn)
                    .child(word)
            }));
        let bottom = v_flex()
            .flex_none()
            .min_w_0()
            .gap(Space::S4)
            .child(
                line(self.name)
                    .opacity(dim)
                    .type_style_strong(TextStyle::TouchBody, cx),
            )
            .child(
                h_flex()
                    .h(px(LINE))
                    .min_w_0()
                    .children(self.faces)
                    .children(self.state.map(|(mark, word)| {
                        line(word)
                            .type_style(TextStyle::TouchSmall, cx)
                            .text_color(mark.word_ink(muted, cx))
                    })),
            );
        let on_click = self.on_click;
        v_flex()
            .id(self.id.clone())
            .relative()
            .size_full()
            .justify_between()
            .overflow_hidden()
            .p(TILE_INSET)
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
            .font_family(theme.font_family.clone())
            .child(top)
            .child(bottom)
            .children(self.children)
            .children(self.on_hold.map(hold_listener))
            .child(press_highlight(self.id, None, radius))
            .when_some(on_click, |tile, on_click| {
                tile.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}
