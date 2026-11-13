use std::{rc::Rc, sync::Arc};

use ala::{
    App, ElementId, Entity, Focusable as _, FontFeatures, IntoElement, ParentElement as _,
    RenderOnce, SharedString, Styled as _, Window, div, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Disableable as _, Ground, IconName, PaintState, Sizable as _, Size, Space,
    StyledExt as _, Surface, TextStyle,
    blocks::chat::{PHONE_COMPOSER_HEIGHT, PHONE_COMPOSER_INSET, phone_composer_corner},
    components::button::{ButtonLook, ButtonVariants as _},
    components::input::{FieldLook, Input, InputState, field_chrome, field_underlay},
    h_flex, muted_on,
};

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
        .child(
            button("phone-find-previous", IconName::ChevronUp)
                .disabled(!self.found)
                .on_click(run(self.on_previous)),
        )
        .child(
            button("phone-find-next", IconName::ChevronDown)
                .disabled(!self.found)
                .on_click(run(self.on_next)),
        );
        div().relative().w_full().children(backing).child(field)
    }
}
