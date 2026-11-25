use std::rc::Rc;

use ala::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Icon, IconName, Kind, PaintState,
    Sizable as _, Space, StyledExt as _, Surface, SurfaceRole, TextStyle, components::chip::Chip,
    components::icon::cap_center_drop, h_flex, v_flex,
};

type ChangeHandler = Rc<dyn Fn(&[SharedString], &mut Window, &mut App)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComboboxOption {
    pub value: SharedString,
    pub label: SharedString,
    pub detail: Option<SharedString>,
}

impl ComboboxOption {
    #[must_use]
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            detail: None,
        }
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComboboxState {
    pub query: SharedString,
    pub selected: Vec<SharedString>,
    pub open: bool,
    pub multiple: bool,
}

impl ComboboxState {
    #[must_use]
    pub fn single() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn multiple() -> Self {
        Self {
            multiple: true,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn is_selected(&self, value: &str) -> bool {
        self.selected.iter().any(|item| item.as_ref() == value)
    }

    pub fn toggle(&mut self, value: impl Into<SharedString>) {
        let value = value.into();
        if let Some(pos) = self.selected.iter().position(|item| item == &value) {
            self.selected.remove(pos);
        } else if self.multiple {
            self.selected.push(value);
        } else {
            self.selected.clear();
            self.selected.push(value);
            self.open = false;
        }
    }

    pub fn remove_last(&mut self) -> Option<SharedString> {
        self.selected.pop()
    }

    #[must_use]
    pub fn filter<'a>(&self, options: &'a [ComboboxOption]) -> Vec<&'a ComboboxOption> {
        let needle = self.query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return options.iter().collect();
        }
        options
            .iter()
            .filter(|opt| {
                opt.label.to_ascii_lowercase().contains(&needle)
                    || opt.value.to_ascii_lowercase().contains(&needle)
                    || opt
                        .detail
                        .as_ref()
                        .is_some_and(|d| d.to_ascii_lowercase().contains(&needle))
            })
            .collect()
    }
}

#[derive(IntoElement)]
pub struct Combobox {
    id: ElementId,
    options: Vec<ComboboxOption>,
    selected: Vec<SharedString>,
    placeholder: SharedString,
    multiple: bool,
    on_change: Option<ChangeHandler>,
    style: StyleRefinement,
}

impl Combobox {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, options: Vec<ComboboxOption>) -> Self {
        Self {
            id: id.into(),
            options,
            selected: Vec::new(),
            placeholder: "Filter…".into(),
            multiple: true,
            on_change: None,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn selected(mut self, values: impl IntoIterator<Item = SharedString>) -> Self {
        self.selected = values.into_iter().collect();
        self
    }

    #[must_use]
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    #[must_use]
    pub const fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    #[must_use]
    pub fn on_change(
        mut self,
        handler: impl Fn(&[SharedString], &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl Styled for Combobox {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Combobox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let initial_selected = self.selected.clone();
        let multiple = self.multiple;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ComboboxState {
            query: SharedString::default(),
            selected: initial_selected,
            open: false,
            multiple,
        });
        let snapshot = state.read(cx).clone();
        let theme = cx.theme();
        let field_surface = Surface::new(Kind::Field.role());
        let field_ground = theme.ground_of(field_surface);
        let pop_surface = Surface::new(SurfaceRole::POPOVER);
        let pop_ground = theme.ground_of(pop_surface);
        let fg = theme.foreground;
        let muted = theme.foreground.muted();
        let drop = cap_center_drop(px(TextStyle::Small.design_px()), px(12.0), cx);
        let on_change = self.on_change;
        let toggle_state = state.clone();

        let chips = snapshot
            .selected
            .iter()
            .enumerate()
            .map(|(index, val)| {
                let label = self
                    .options
                    .iter()
                    .find(|opt| opt.value == *val)
                    .map_or_else(|| val.clone(), |opt| opt.label.clone());
                Chip::new((self.id.clone(), format!("chip-{index}")), label)
                    .xsmall()
                    .on(field_ground)
            })
            .collect::<Vec<_>>();

        let filtered = snapshot.filter(&self.options);
        let list_rows =
            filtered
                .into_iter()
                .enumerate()
                .map(|(index, opt)| {
                    let active = snapshot.is_selected(&opt.value);
                    let val = opt.value.clone();
                    let row_state = state.clone();
                    let handler = on_change.clone();
                    let item_surface = Surface::new(SurfaceRole::MENU_ITEM).on(pop_ground);
                    let paint_state = if active {
                        PaintState::Selected
                    } else {
                        PaintState::Rest
                    };
                    let layer = theme.layer(item_surface, paint_state);
                    h_flex()
                        .id((self.id.clone(), format!("opt-{index}")))
                        .role(Role::ListBoxOption)
                        .aria_selected(active)
                        .w_full()
                        .h(px(28.0))
                        .px(Space::S8)
                        .gap(Space::S8)
                        .items_center()
                        .justify_between()
                        .cursor_pointer()
                        .layer(&layer)
                        .on_click(move |_, window, cx| {
                            let next = row_state.update(cx, |st, cx| {
                                st.toggle(val.clone());
                                cx.notify();
                                st.selected.clone()
                            });
                            if let Some(cb) = &handler {
                                cb(&next, window, cx);
                            }
                        })
                        .child(
                            h_flex()
                                .items_center()
                                .gap(Space::S6)
                                .min_w_0()
                                .child(
                                    div().flex_none().w(px(14.0)).relative().top(drop).when(
                                        active,
                                        |slot| {
                                            slot.child(Icon::new(IconName::Check).size(px(12.0)))
                                        },
                                    ),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .type_style(TextStyle::Small, cx)
                                        .text_color(fg)
                                        .child(opt.label.clone()),
                                ),
                        )
                        .when_some(opt.detail.clone(), |row, detail| {
                            row.child(
                                div()
                                    .flex_none()
                                    .type_style(TextStyle::Caption, cx)
                                    .text_color(muted)
                                    .child(detail),
                            )
                        })
                })
                .collect::<Vec<_>>();

        v_flex()
            .w_full()
            .min_w_0()
            .gap(Space::S4)
            .refine_style(&self.style)
            .child(
                h_flex()
                    .author_id((self.id.clone(), "trigger"))
                    .role(Role::ComboBox)
                    .aria_expanded(snapshot.open)
                    .w_full()
                    .min_h(px(30.0))
                    .px(Space::S8)
                    .py(Space::S4)
                    .gap(Space::S6)
                    .flex_wrap()
                    .items_center()
                    .cursor_pointer()
                    .surface(field_surface, cx)
                    .on_click(move |_, _, cx| {
                        toggle_state.update(cx, |st, cx| {
                            st.open = !st.open;
                            cx.notify();
                        });
                    })
                    .children(chips)
                    .when(snapshot.selected.is_empty(), |row| {
                        row.child(
                            div()
                                .type_style(TextStyle::Small, cx)
                                .text_color(muted)
                                .child(self.placeholder),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex_none()
                            .relative()
                            .top(drop)
                            .text_color(muted)
                            .child(Icon::new(IconName::ChevronDown).size(px(12.0))),
                    ),
            )
            .when(snapshot.open, |host| {
                host.child(
                    v_flex()
                        .w_full()
                        .p(Space::S4)
                        .gap(Space::S2)
                        .surface(pop_surface, cx)
                        .children(list_rows),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combobox_filters_options_and_toggles_multi_and_single_tokens() {
        let options = vec![
            ComboboxOption::new("host:ubuntu", "ubuntu").detail("Linux"),
            ComboboxOption::new("host:macbook", "macbook").detail("macOS"),
            ComboboxOption::new("status:open", "Open").detail("Active"),
        ];

        let mut multi = ComboboxState::multiple();
        multi.query = "linux".into();
        let filtered = multi.filter(&options);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value.as_ref(), "host:ubuntu");

        multi.toggle("host:ubuntu");
        multi.toggle("status:open");
        assert_eq!(multi.selected.len(), 2);
        assert_eq!(multi.remove_last().as_deref(), Some("status:open"));
        assert!(multi.is_selected("host:ubuntu"));

        let mut single = ComboboxState::single();
        single.open = true;
        single.toggle("host:macbook");
        assert_eq!(single.selected.len(), 1);
        assert!(!single.open);
        single.toggle("host:ubuntu");
        assert_eq!(single.selected.len(), 1);
        assert_eq!(single.selected[0].as_ref(), "host:ubuntu");
    }
}
