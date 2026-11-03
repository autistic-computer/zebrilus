use crate::{ActiveTheme as _, Ground, StyledExt as _, Surface, SurfaceRole};
use ala::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, StyleRefinement, Styled, Window, div,
};

#[derive(IntoElement, Default)]
pub struct Well {
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Well {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn ground(cx: &App) -> Ground {
        cx.theme().ground_of(Surface::new(SurfaceRole::WELL))
    }
}

impl ParentElement for Well {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Well {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Well {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .surface(Surface::new(SurfaceRole::WELL), cx)
            .refine_style(&self.refinement)
            .children(self.children)
    }
}
