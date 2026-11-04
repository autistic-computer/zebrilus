use crate::Space;
use std::rc::Rc;

use crate::{
    Edge, IconName, Selectable as _, Sizable as _, StyledExt as _, TextStyle,
    components::bar::{Bar, BarLook},
    components::button::{Button, ButtonVariants as _},
    components::icon::Icon,
    foundation::RoleRuleExt as _,
    foundation::roles,
    h_flex,
};
use ala::{
    AnyElement, App, ElementId, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled as _, Window, div, prelude::*, px,
};

pub const DOCS_HEADER_HEIGHT: f32 = 48.0;

type Pick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct DocsHeader {
    id: ElementId,
    brand: Option<(IconName, SharedString)>,
    links: Vec<SharedString>,
    current: Option<usize>,
    on_link: Option<Pick>,
    end: Vec<AnyElement>,
}

impl DocsHeader {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            brand: None,
            links: Vec::new(),
            current: None,
            on_link: None,
            end: Vec::new(),
        }
    }

    #[must_use]
    pub fn brand(mut self, icon: IconName, name: impl Into<SharedString>) -> Self {
        self.brand = Some((icon, name.into()));
        self
    }

    #[must_use]
    pub fn links(mut self, links: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.links = links.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub const fn current(mut self, link: Option<usize>) -> Self {
        self.current = link;
        self
    }

    #[must_use]
    pub fn on_link(mut self, on_link: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_link = Some(Rc::new(on_link));
        self
    }

    #[must_use]
    pub fn end(mut self, element: impl IntoElement) -> Self {
        self.end.push(element.into_any_element());
        self
    }
}

impl RenderOnce for DocsHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let links = self.links.into_iter().enumerate().map(|(index, label)| {
            let on_link = self.on_link.clone();
            Button::new(("docs-header-link", index))
                .ghost()
                .small()
                .label(label)
                .selected(self.current == Some(index))
                .on_click(move |_, window, cx| {
                    if let Some(on_link) = &on_link {
                        on_link(index, window, cx);
                    }
                })
        });
        Bar::new(self.id, BarLook::Plain)
            .flex()
            .flex_none()
            .items_center()
            .gap(Space::S4)
            .w_full()
            .h(px(DOCS_HEADER_HEIGHT))
            .px(Space::S16)
            .rule_on(roles::BAR_RULE, Edge::Bottom, cx)
            .when_some(self.brand, |this, (icon, name)| {
                this.child(
                    h_flex()
                        .flex_none()
                        .gap(Space::S8)
                        .mr(Space::S16)
                        .type_style(TextStyle::Lead, cx)
                        .font_semibold()
                        .child(Icon::new(icon).size(px(16.0)))
                        .child(name),
                )
            })
            .children(links)
            .child(div().flex_1())
            .children(self.end)
    }
}
