mod example;
mod header;
mod nav;

pub use example::{Example, ExampleCode};
pub use header::DocsHeader;
pub use nav::{DocsNav, NavGroup};

use crate::Space;
use crate::{
    Edge, StyledExt as _, Surface, TextStyle, foundation::RoleRuleExt as _, foundation::roles,
    h_flex, v_flex,
};
use ala::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Styled as _, Window, div,
    prelude::*, px,
};

pub const DOCS_NAV_WIDTH: f32 = 248.0;
pub const DOCS_CONTENT_WIDTH: f32 = 760.0;

#[derive(IntoElement)]
pub struct DocsLayout {
    id: ElementId,
    header: Option<AnyElement>,
    nav: Option<AnyElement>,
    content: Vec<AnyElement>,
}

impl DocsLayout {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            header: None,
            nav: None,
            content: Vec::new(),
        }
    }

    #[must_use]
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    #[must_use]
    pub fn nav(mut self, nav: impl IntoElement) -> Self {
        self.nav = Some(nav.into_any_element());
        self
    }
}

impl ParentElement for DocsLayout {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

impl RenderOnce for DocsLayout {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id;
        v_flex()
            .id(id.clone())
            .size_full()
            .min_h_0()
            .children(self.header)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .when_some(self.nav, |this, nav| {
                        this.child(
                            div()
                                .id((id.clone(), "nav"))
                                .flex_none()
                                .w(px(DOCS_NAV_WIDTH))
                                .h_full()
                                .overflow_y_scroll()
                                .surface(Surface::new(roles::SIDEBAR), cx)
                                .rule_on(roles::SIDEBAR_RULE, Edge::Right, cx)
                                .child(nav),
                        )
                    })
                    .child(
                        div()
                            .id((id, "main"))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(DOCS_CONTENT_WIDTH))
                                    .mx_auto()
                                    .px(Space::INSET_PAGE)
                                    .pt(Space::S40)
                                    .pb(Space::S64)
                                    .type_style(TextStyle::ReadBody, cx)
                                    .children(self.content),
                            ),
                    ),
            )
    }
}
