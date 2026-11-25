use ala::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, StyleRefinement, Styled, Window, div,
};

use crate::{
    ActiveTheme as _, Ground, Kind, Space, StyledExt as _, Surface,
    components::separator::Separator, foundation::RuleLook, h_flex, v_flex,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonGroupOrientation {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(IntoElement)]
pub struct ButtonGroup {
    orientation: ButtonGroupOrientation,
    ground: Option<Ground>,
    dividers: bool,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl ButtonGroup {
    #[must_use]
    pub fn new() -> Self {
        Self {
            orientation: ButtonGroupOrientation::Horizontal,
            ground: None,
            dividers: true,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub const fn vertical(mut self) -> Self {
        self.orientation = ButtonGroupOrientation::Vertical;
        self
    }

    #[must_use]
    pub const fn orientation(mut self, orientation: ButtonGroupOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }

    #[must_use]
    pub const fn dividers(mut self, dividers: bool) -> Self {
        self.dividers = dividers;
        self
    }
}

impl Default for ButtonGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for ButtonGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for ButtonGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ButtonGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let ground = self.ground.unwrap_or_else(|| theme.ground());
        let surface = Surface::new(Kind::Field.role()).on(ground);
        let vertical = self.orientation == ButtonGroupOrientation::Vertical;

        let mut group = if vertical {
            v_flex().items_stretch()
        } else {
            h_flex().items_center()
        }
        .flex_none()
        .p(Space::S2)
        .gap(Space::S2)
        .surface(surface, cx)
        .refine_style(&self.style);

        for (index, child) in self.children.into_iter().enumerate() {
            if self.dividers && index > 0 {
                let sep = if vertical {
                    Separator::horizontal()
                        .look(RuleLook::List)
                        .into_any_element()
                } else {
                    div()
                        .h(Space::S12)
                        .child(Separator::vertical().look(RuleLook::List))
                        .into_any_element()
                };
                group = group.child(sep);
            }
            group = group.child(child);
        }

        group
    }
}
