use std::rc::Rc;

use ala::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Space, StyledExt as _, TextStyle,
    components::icon::cap_center_drop, h_flex,
};

type ClickHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BreadcrumbSeparator {
    #[default]
    Slash,
    Chevron,
}

pub struct BreadcrumbItem {
    label: SharedString,
    icon: Option<IconName>,
    on_click: Option<ClickHandler>,
}

impl BreadcrumbItem {
    #[must_use]
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            on_click: None,
        }
    }

    #[must_use]
    pub const fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

#[derive(IntoElement)]
pub struct Breadcrumb {
    id: ElementId,
    items: Vec<BreadcrumbItem>,
    separator: BreadcrumbSeparator,
    style: StyleRefinement,
}

impl Breadcrumb {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            separator: BreadcrumbSeparator::Slash,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn item(mut self, item: BreadcrumbItem) -> Self {
        self.items.push(item);
        self
    }

    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = BreadcrumbItem>) -> Self {
        self.items.extend(items);
        self
    }

    #[must_use]
    pub const fn separator(mut self, separator: BreadcrumbSeparator) -> Self {
        self.separator = separator;
        self
    }
}

impl Styled for Breadcrumb {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Breadcrumb {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.foreground.muted();
        let faint = theme.foreground.faint();
        let font_px = TextStyle::Small.design_px();
        let icon_px = 12.0;
        let drop = cap_center_drop(px(font_px), px(icon_px), cx);
        let last = self.items.len().saturating_sub(1);

        let mut row = h_flex()
            .id(self.id.clone())
            .role(Role::Navigation)
            .aria_label("Breadcrumb")
            .items_center()
            .gap(Space::S6)
            .min_w_0()
            .refine_style(&self.style);

        for (index, item) in self.items.into_iter().enumerate() {
            if index > 0 {
                let sep = match self.separator {
                    BreadcrumbSeparator::Slash => div()
                        .flex_none()
                        .type_style(TextStyle::Small, cx)
                        .text_color(faint)
                        .child("/"),
                    BreadcrumbSeparator::Chevron => div()
                        .flex_none()
                        .relative()
                        .top(drop)
                        .text_color(faint)
                        .child(Icon::new(IconName::ChevronRight).size(px(icon_px))),
                };
                row = row.child(sep);
            }

            let is_last = index == last;
            let base = h_flex()
                .id((self.id.clone(), format!("crumb-{index}")))
                .items_center()
                .gap(Space::S4)
                .min_w_0()
                .type_style(TextStyle::Small, cx)
                .when(is_last, |el| {
                    el.type_style_strong(TextStyle::Small, cx).text_color(fg)
                })
                .when(!is_last, |el| el.text_color(muted))
                .when_some(item.on_click, |el, handler| {
                    el.role(Role::Link)
                        .cursor_pointer()
                        .hover(move |style| style.text_color(fg))
                        .on_click(move |_, window, cx| handler(window, cx))
                })
                .when_some(item.icon, |el, icon| {
                    el.child(
                        div()
                            .flex_none()
                            .relative()
                            .top(drop)
                            .child(Icon::new(icon).size(px(icon_px))),
                    )
                })
                .child(div().truncate().child(item.label));

            row = row.child(base);
        }

        row
    }
}
