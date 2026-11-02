use ala::{
    App, Axis, Hsla, IntoElement, ParentElement as _, RenderOnce, StyleRefinement, Styled, Window,
    div, prelude::FluentBuilder as _,
};

use crate::{Edge, RuleLook, StyledExt as _};

#[derive(IntoElement)]
pub struct Separator {
    style: StyleRefinement,
    color: Option<Hsla>,
    look: RuleLook,
    axis: Axis,
}

impl Separator {
    #[must_use]
    pub fn vertical() -> Self {
        Self {
            style: StyleRefinement::default(),
            color: None,
            look: RuleLook::Plain,
            axis: Axis::Vertical,
        }
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

        div()
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
            )
    }
}
