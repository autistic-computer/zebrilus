use std::rc::Rc;

use ala::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    Pixels, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Ground, IconName, Layer, PaintState, Space, StyledExt as _, Surface,
    blocks::settings::large_title,
    blocks::touch::{ACTION_BAR_BOTTOM, TOUCH_TARGET, action_bar},
    foundation::roles,
    h_flex, v_flex,
};

use super::bar::compact_bar_button;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[must_use]
pub fn zoom_card_ground(cx: &App) -> Ground {
    cx.theme().ground_of(Surface::new(roles::PHONE_CARD))
}

#[derive(IntoElement)]
pub struct ZoomCard {
    id: ElementId,
    title: SharedString,
    top: Pixels,
    bottom: Pixels,
    radius: Pixels,
    on_close: Option<ClickHandler>,
    body: Vec<AnyElement>,
    actions: Vec<AnyElement>,
}

impl ZoomCard {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            top: px(0.0),
            bottom: px(ACTION_BAR_BOTTOM),
            radius: px(0.0),
            on_close: None,
            body: Vec::new(),
            actions: Vec::new(),
        }
    }

    #[must_use]
    pub fn top_inset(mut self, top: Pixels) -> Self {
        self.top = top;
        self
    }

    #[must_use]
    pub fn bottom_inset(mut self, bottom: Pixels) -> Self {
        self.bottom = bottom;
        self
    }

    #[must_use]
    pub fn rounded(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }

    #[must_use]
    pub fn on_close(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }
}

impl ParentElement for ZoomCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for ZoomCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let frame = theme.layer(Surface::new(roles::PHONE_CARD), PaintState::Rest);
        let close = compact_bar_button("zoom-card-close", IconName::Xmark)
            .on(frame.ground)
            .tooltip("Close")
            .when_some(self.on_close, |close, on_close| {
                close.on_click(move |event, window, cx| on_close(event, window, cx))
            });
        v_flex()
            .id(self.id)
            .relative()
            .size_full()
            .overflow_hidden()
            .layer(&Layer {
                glass: None,
                radius: self.radius,
                share: None,
                ..frame
            })
            .font_family(theme.font_family.clone())
            .pt(self.top)
            .child(
                h_flex()
                    .flex_none()
                    .justify_end()
                    .h(px(TOUCH_TARGET))
                    .px(Space::INSET_PANE)
                    .child(close),
            )
            .child(large_title(self.title, cx))
            .child(
                v_flex()
                    .id("zoom-card-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(Space::GAP_GROUP)
                    .pt(Space::GAP_GROUP)
                    .px(Space::INSET_PANE)
                    .children(self.body),
            )
            .when(!self.actions.is_empty(), |card| {
                card.child(action_bar(self.bottom).children(self.actions))
            })
    }
}
