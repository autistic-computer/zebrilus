use ala::{
    App, Axis, ElementId, Hsla, IntoElement, ParentElement as _, RenderOnce, Role,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, accesskit::Orientation, div,
    prelude::FluentBuilder as _,
};

use crate::{AuthoredElement as _, Edge, RuleLook, StyledExt as _};

#[derive(IntoElement)]
pub struct Separator {
    style: StyleRefinement,
    color: Option<Hsla>,
    look: RuleLook,
    axis: Axis,
    id: Option<ElementId>,
}

impl Separator {
    #[must_use]
    pub fn vertical() -> Self {
        Self {
            style: StyleRefinement::default(),
            color: None,
            look: RuleLook::Plain,
            axis: Axis::Vertical,
            id: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub fn horizontal() -> Self {
        Self {
            axis: Axis::Horizontal,
            ..Self::vertical()
        }
    }

    #[must_use]
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    #[must_use]
    pub fn look(mut self, look: RuleLook) -> Self {
        self.look = look;
        self
    }
}

impl Styled for Separator {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Separator {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let look = self.look;
        let vertical = self.axis == Axis::Vertical;

        let rule = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .map(|this| {
                if vertical {
                    this.h_full()
                } else {
                    this.w_full()
                }
            })
            .refine_style(&self.style)
            .child(
                div()
                    .absolute()
                    .map(|this| {
                        if vertical {
                            this.h_full().rule(look, Edge::Left, cx)
                        } else {
                            this.w_full().rule(look, Edge::Top, cx)
                        }
                    })
                    .when_some(self.color, Styled::border_color),
            );
        let orientation = if vertical {
            Orientation::Vertical
        } else {
            Orientation::Horizontal
        };
        match self.id {
            Some(id) => rule
                .author_id(id)
                .role(Role::Splitter)
                .aria_orientation(orientation)
                .into_any_element(),
            None => rule.into_any_element(),
        }
    }
}
