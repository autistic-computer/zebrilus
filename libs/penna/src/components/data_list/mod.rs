use ala::{
    AnyElement, App, IntoElement, Length, ParentElement as _, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Space, StyledExt as _, TextStyle,
    components::separator::Separator, foundation::RuleLook, h_flex, v_flex,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DataListLayout {
    #[default]
    Horizontal,
    Vertical,
}

pub struct DataListItem {
    label: SharedString,
    value: SharedString,
    mono: bool,
    trailing: Option<AnyElement>,
}

impl DataListItem {
    #[must_use]
    pub fn new(label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            mono: false,
            trailing: None,
        }
    }

    #[must_use]
    pub const fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    #[must_use]
    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing = Some(element.into_any_element());
        self
    }
}

#[derive(IntoElement)]
pub struct DataList {
    items: Vec<DataListItem>,
    layout: DataListLayout,
    label_width: Length,
    dividers: bool,
    style: StyleRefinement,
}

impl DataList {
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            layout: DataListLayout::Horizontal,
            label_width: px(120.0).into(),
            dividers: true,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn item(mut self, item: DataListItem) -> Self {
        self.items.push(item);
        self
    }

    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = DataListItem>) -> Self {
        self.items.extend(items);
        self
    }

    #[must_use]
    pub const fn layout(mut self, layout: DataListLayout) -> Self {
        self.layout = layout;
        self
    }

    #[must_use]
    pub fn label_width(mut self, width: impl Into<Length>) -> Self {
        self.label_width = width.into();
        self
    }

    #[must_use]
    pub const fn dividers(mut self, dividers: bool) -> Self {
        self.dividers = dividers;
        self
    }
}

impl Default for DataList {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for DataList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for DataList {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.foreground.muted();
        let mono_family = theme.mono_font_family.clone();

        let mut list = v_flex().w_full().min_w_0().refine_style(&self.style);

        for (index, item) in self.items.into_iter().enumerate() {
            if self.dividers && index > 0 {
                list = list.child(Separator::horizontal().look(RuleLook::List));
            }
            let label = div()
                .type_style(TextStyle::Small, cx)
                .text_color(muted)
                .when(self.layout == DataListLayout::Horizontal, |el| {
                    el.flex_none().w(self.label_width)
                })
                .child(item.label);

            let value = h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .justify_between()
                .gap(Space::S8)
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .when(item.mono, |el| {
                            el.font_family(mono_family.clone())
                                .type_style(TextStyle::Small, cx)
                        })
                        .when(!item.mono, |el| el.type_style(TextStyle::Small, cx))
                        .text_color(fg)
                        .child(item.value),
                )
                .children(item.trailing);

            let row = match self.layout {
                DataListLayout::Horizontal => h_flex()
                    .w_full()
                    .min_w_0()
                    .items_baseline()
                    .gap(Space::S12)
                    .py(Space::S8)
                    .child(label)
                    .child(value),
                DataListLayout::Vertical => v_flex()
                    .w_full()
                    .min_w_0()
                    .gap(Space::S2)
                    .py(Space::S8)
                    .child(label)
                    .child(value),
            };
            list = list.child(row);
        }

        list
    }
}
