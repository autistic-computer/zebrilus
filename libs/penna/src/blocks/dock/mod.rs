use crate::{Edge, Label, RuleLook, Space, StyledExt as _, TextStyle, h_flex, v_flex};
use ala::{
    AnyElement, App, IntoElement, ParentElement, Pixels, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};

#[derive(IntoElement)]
pub struct Dock {
    title: SharedString,
    subtitle: Option<SharedString>,
    action: Option<AnyElement>,
    header_height: Option<Pixels>,
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Dock {
    #[must_use]
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            action: None,
            header_height: None,
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
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    #[must_use]
    pub fn header_height(mut self, height: Pixels) -> Self {
        self.header_height = Some(height);
        self
    }
}

impl ParentElement for Dock {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Dock {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Dock {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .rule(RuleLook::Bar, Edge::Top, cx)
            .refine_style(&self.refinement)
            .child(
                h_flex()
                    .when_some(self.header_height, Styled::h)
                    .px(Space::S8)
                    .gap(Space::S6)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(Label::new(TextStyle::Body, self.title).strong())
                            .children(
                                self.subtitle
                                    .map(|subtitle| Label::new(TextStyle::Small, subtitle).muted()),
                            ),
                    )
                    .children(self.action),
            )
            .children(self.children)
    }
}
