use std::rc::Rc;

use ala::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
    prelude::FluentBuilder as _,
};

use crate::{
    ActiveTheme as _, Edge, IconName, Label, Side, Space, StyledExt as _, Surface, SurfaceRole,
    TextStyle, button::Button, foundation::RoleRuleExt as _, h_flex, rems_from_px, roles, v_flex,
};

const WIDTH: f32 = 340.0;

type CloseHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Inspector {
    id: ElementId,
    title: SharedString,
    subtitle: Option<SharedString>,
    trailing: Option<AnyElement>,
    on_close: Option<CloseHandler>,
    side: Side,
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Inspector {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            trailing: None,
            on_close: None,
            side: Side::Right,
            refinement: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    #[must_use]
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
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
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }
}

impl ParentElement for Inspector {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Inspector {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Inspector {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ground = cx.theme().ground_of(SurfaceRole::INSPECTOR);
        let edge = match self.side {
            Side::Left => Edge::Right,
            Side::Right => Edge::Left,
        };
        let header =
            h_flex()
                .w_full()
                .items_start()
                .gap(Space::S4)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(Space::S1)
                        .child(Label::new(TextStyle::Heading, self.title).truncate())
                        .children(self.subtitle.map(|subtitle| {
                            Label::new(TextStyle::Small, subtitle).muted().on(ground)
                        })),
                )
                .children(self.trailing)
                .when_some(self.on_close, |header, on_close| {
                    header.child(
                        Button::compact_icon("inspector-close", IconName::Xmark)
                            .tooltip("Close")
                            .on(ground)
                            .on_click(move |event, window, cx| on_close(event, window, cx)),
                    )
                });
        v_flex()
            .id(self.id)
            .flex_none()
            .w(rems_from_px(WIDTH))
            .h_full()
            .overflow_y_scroll()
            .p(Space::S8)
            .gap(Space::S8)
            .surface(Surface::new(SurfaceRole::INSPECTOR), cx)
            .rule_on(roles::SIDEBAR_RULE, edge, cx)
            .refine_style(&self.refinement)
            .child(header)
            .children(self.children)
    }
}
