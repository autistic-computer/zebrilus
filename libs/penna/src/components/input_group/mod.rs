use ala::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder as _,
};

use crate::{
    ActiveTheme as _, Colorize as _, Ground, Kind, Space, StyledExt as _, Surface, TextStyle,
    components::separator::Separator, foundation::RuleLook, h_flex,
};

#[derive(IntoElement)]
pub struct InputGroup {
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    ground: Option<Ground>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl InputGroup {
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefix: None,
            suffix: None,
            ground: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn prefix_text(mut self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.prefix = Some(div().px(Space::S8).child(text).into_any_element());
        self
    }

    #[must_use]
    pub fn prefix(mut self, element: impl IntoElement) -> Self {
        self.prefix = Some(element.into_any_element());
        self
    }

    #[must_use]
    pub fn suffix_text(mut self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.suffix = Some(div().px(Space::S8).child(text).into_any_element());
        self
    }

    #[must_use]
    pub fn suffix(mut self, element: impl IntoElement) -> Self {
        self.suffix = Some(element.into_any_element());
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }
}

impl Default for InputGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for InputGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for InputGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for InputGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let ground = self.ground.unwrap_or_else(|| theme.ground());
        let surface = Surface::new(Kind::Field.role()).on(ground);
        let muted = theme.foreground.muted();

        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .surface(surface, cx)
            .refine_style(&self.style)
            .when_some(self.prefix, |row, prefix| {
                row.child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .type_style(TextStyle::Small, cx)
                        .text_color(muted)
                        .child(prefix),
                )
                .child(
                    div()
                        .h(Space::S16)
                        .child(Separator::vertical().look(RuleLook::List)),
                )
            })
            .child(h_flex().flex_1().min_w_0().children(self.children))
            .when_some(self.suffix, |row, suffix| {
                row.child(
                    div()
                        .h(Space::S16)
                        .child(Separator::vertical().look(RuleLook::List)),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .type_style(TextStyle::Small, cx)
                        .text_color(muted)
                        .child(suffix),
                )
            })
    }
}
