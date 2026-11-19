use std::{rc::Rc, sync::Arc};

use ala::{
    AnyElement, App, ClickEvent, Div, ElementId, Entity, Focusable as _, FontFeatures, IntoElement,
    ParentElement as _, RenderOnce, SharedString, Stateful, Styled as _, Window, div, prelude::*,
    px,
};

use crate::{
    ActiveTheme as _, Disableable as _, Ground, IconName, PaintState, Sizable as _, Size, Space,
    StyledExt as _, Surface, TextStyle,
    blocks::chat::{PHONE_COMPOSER_HEIGHT, PHONE_COMPOSER_INSET, phone_composer_corner},
    blocks::touch::{Nest, TouchRow},
    components::button::{ButtonLook, ButtonVariants as _},
    components::input::{FieldLook, Input, InputState, field_chrome, field_underlay},
    h_flex, muted_on, v_flex,
};

const SUGGESTION_INSET: f32 = 4.0;
const REST_GLYPH: f32 = 20.0;

pub fn phone_suggestions(rows: Vec<TouchRow>, ground: Ground, cx: &App) -> Option<AnyElement> {
    if rows.is_empty() {
        return None;
    }
    let outer = cx.theme().phone_radius();
    let inside = cx
        .theme()
        .ground_of(Surface::from(FieldLook::Composer).on(ground));
    let nest = Nest {
        outer,
        inset: px(SUGGESTION_INSET),
    };
    Some(
        field_chrome(
            v_flex()
                .debug_selector(|| "phone-suggestions".to_owned())
                .w_full()
                .p(px(SUGGESTION_INSET))
                .gap(Space::S2),
            FieldLook::Composer,
            Some(ground),
            PaintState::Rest,
            cx,
        )
        .rounded(outer)
        .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        .children(rows.into_iter().map(|row| row.nested(nest, inside)))
        .into_any_element(),
    )
}

pub fn phone_field_at_rest(
    id: impl Into<ElementId>,
    icon: IconName,
    placeholder: impl Into<SharedString>,
    ground: Ground,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let inside = cx
        .theme()
        .ground_of(Surface::from(FieldLook::Composer).on(ground));
    let muted = muted_on(inside, cx);
    field_chrome(
        h_flex()
            .id(id)
            .debug_selector(|| "phone-field-at-rest".to_owned())
            .w_full()
            .h(px(PHONE_COMPOSER_HEIGHT))
            .items_center()
            .gap(Space::S2)
            .p(px(PHONE_COMPOSER_INSET)),
        FieldLook::Composer,
        Some(ground),
        PaintState::Rest,
        cx,
    )
    .rounded(phone_composer_corner(cx))
    .corner_radius_mode(ala::CornerRadiusMode::Fixed)
    .click_cursor()
    .text_color(muted)
    .child(
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(crate::blocks::chat::PHONE_COMPOSER_BUTTON))
            .child(crate::Icon::new(icon).size(px(REST_GLYPH))),
    )
    .child(
        div()
            .flex_1()
            .min_w_0()
            .pl(Space::S4)
            .truncate()
            .type_style(TextStyle::TouchBody, cx)
            .child(placeholder.into()),
    )
    .on_click(on_click)
}

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PhoneFind {
    id: ElementId,
    input: Entity<InputState>,
    count: Option<SharedString>,
    found: bool,
    ground: Ground,
    on_close: Option<Handler>,
    on_previous: Option<Handler>,
    on_next: Option<Handler>,
    on_clear: Option<Handler>,
}

impl PhoneFind {
    pub fn new(id: impl Into<ElementId>, input: &Entity<InputState>, ground: Ground) -> Self {
        Self {
            id: id.into(),
            input: input.clone(),
            count: None,
            found: false,
            ground,
            on_close: None,
            on_previous: None,
            on_next: None,
            on_clear: None,
        }
    }

    #[must_use]
    pub fn count(mut self, count: impl Into<SharedString>) -> Self {
        let count = count.into();
        self.count = (!count.is_empty()).then_some(count);
        self
    }

    #[must_use]
    pub fn found(mut self, found: bool) -> Self {
        self.found = found;
        self
    }

    #[must_use]
    pub fn on_close(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn on_previous(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_previous = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn on_next(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_next = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn on_clear(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_clear = Some(Rc::new(handler));
        self
    }
}

fn run(handler: Option<Handler>) -> impl Fn(&ala::ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        if let Some(handler) = &handler {
            handler(window, cx);
        }
        cx.stop_propagation();
    }
}

impl RenderOnce for PhoneFind {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = if self.input.read(cx).focus_handle(cx).is_focused(window) {
            PaintState::Focused
        } else {
            PaintState::Rest
        };
        let inside = cx
            .theme()
            .ground_at(Surface::from(FieldLook::Composer).on(self.ground), state);
        let typed = !self.input.read(cx).value().is_empty();
        let radius = phone_composer_corner(cx);
        let inset = px(1.0);
        let backing = field_underlay(FieldLook::Composer, inset, cx).map(|underlay| {
            underlay
                .rounded(radius - inset)
                .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        });
        let button = |id: &'static str, icon: IconName| {
            crate::blocks::chat::phone_button(id, icon)
                .ghost()
                .look(ButtonLook::Quiet)
                .on(inside)
        };
        let field = field_chrome(
            h_flex()
                .id(self.id)
                .debug_selector(|| "phone-find".to_owned())
                .w_full()
                .h(px(PHONE_COMPOSER_HEIGHT))
                .items_center()
                .gap(Space::S2)
                .p(px(PHONE_COMPOSER_INSET)),
            FieldLook::Composer,
            Some(self.ground),
            state,
            cx,
        )
        .rounded(radius)
        .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(
            button("phone-find-close", IconName::Xmark)
                .debug_selector(|| "phone-find-close".to_owned())
                .on_click(run(self.on_close)),
        )
        .child(
            div().flex_1().min_w_0().pl(Space::S4).child(
                Input::new(&self.input)
                    .with_size(Size::TouchLarge)
                    .w_full()
                    .min_w_0()
                    .px(px(0.0))
                    .on(inside)
                    .appearance(false)
                    .bordered(false),
            ),
        )
        .children(self.count.map(|count| {
            div()
                .debug_selector(|| "phone-find-count".to_owned())
                .flex_none()
                .px(Space::S6)
                .whitespace_nowrap()
                .type_style(TextStyle::TouchSmall, cx)
                .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
                .text_color(muted_on(inside, cx))
                .child(count)
        }))
        .when_some(self.on_previous, |field, handler| {
            field.child(
                button("phone-find-previous", IconName::ChevronUp)
                    .disabled(!self.found)
                    .on_click(run(Some(handler))),
            )
        })
        .when_some(self.on_next, |field, handler| {
            field.child(
                button("phone-find-next", IconName::ChevronDown)
                    .disabled(!self.found)
                    .on_click(run(Some(handler))),
            )
        })
        .when_some(self.on_clear.filter(|_| typed), |field, handler| {
            field.child(
                button("phone-find-clear", IconName::CircleX)
                    .debug_selector(|| "phone-find-clear".to_owned())
                    .on_click(run(Some(handler))),
            )
        });
        div().relative().w_full().children(backing).child(field)
    }
}
