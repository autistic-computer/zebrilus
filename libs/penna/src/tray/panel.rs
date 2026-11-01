use ala::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, StyleRefinement, Styled, Window,
};

use crate::{Space, StyledExt as _, SurfaceRole, v_flex};

#[derive(IntoElement, Default)]
pub struct TrayPanel {
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl TrayPanel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for TrayPanel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for TrayPanel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for TrayPanel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .size_full()
            .px(Space::S8)
            .pt(Space::S6)
            .pb(Space::S4)
            .gap(Space::S6)
            .surface(SurfaceRole::POPOVER, cx)
            .refine_style(&self.refinement)
            .children(self.children)
    }
}
