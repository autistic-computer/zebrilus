use std::{cmp::Ordering, rc::Rc};

use ala::{
    App, ElementId, InteractiveElement as _, IntoElement, Length, ParentElement as _, RenderOnce,
    Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Density, Icon, IconName, Kind,
    PaintState, Space, StyledExt as _, Surface, SurfaceRole, TextStyle,
    components::icon::cap_center_drop, components::separator::Separator, foundation::RuleLook,
    h_flex, v_flex,
};

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnAlign {
    #[default]
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SortState {
    pub column: Option<usize>,
    pub dir: Option<SortDir>,
}

impl SortState {
    pub fn toggle(&mut self, column: usize) {
        if self.column == Some(column) {
            match self.dir {
                Some(SortDir::Asc) => self.dir = Some(SortDir::Desc),
                Some(SortDir::Desc) => {
                    self.column = None;
                    self.dir = None;
                }
                None => self.dir = Some(SortDir::Asc),
            }
        } else {
            self.column = Some(column);
            self.dir = Some(SortDir::Asc);
        }
    }

    #[must_use]
    pub fn sorted_indices(&self, rows: &[TableRow]) -> Vec<usize> {
        let mut order: Vec<usize> = (0..rows.len()).collect();
        let (Some(col), Some(dir)) = (self.column, self.dir) else {
            return order;
        };
        order.sort_by(|&a, &b| {
            let av = rows[a].cells.get(col).map_or("", SharedString::as_ref);
            let bv = rows[b].cells.get(col).map_or("", SharedString::as_ref);
            let cmp = compare_cells(av, bv);
            match dir {
                SortDir::Asc => cmp,
                SortDir::Desc => cmp.reverse(),
            }
        });
        order
    }
}

fn compare_cells(a: &str, b: &str) -> Ordering {
    let parse_num = |s: &str| {
        s.trim()
            .trim_end_matches(['%', 's', 'm', 'k', 'M', 'G'])
            .trim()
            .replace(',', "")
            .parse::<f64>()
            .ok()
    };
    if let (Some(na), Some(nb)) = (parse_num(a), parse_num(b)) {
        return na.total_cmp(&nb);
    }
    a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase())
}

#[derive(Clone, Debug)]
pub struct TableColumn {
    pub title: SharedString,
    pub align: ColumnAlign,
    pub width: Option<Length>,
    pub mono: bool,
    pub sortable: bool,
}

impl TableColumn {
    #[must_use]
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            align: ColumnAlign::Left,
            width: None,
            mono: false,
            sortable: true,
        }
    }

    #[must_use]
    pub const fn align(mut self, align: ColumnAlign) -> Self {
        self.align = align;
        self
    }

    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    #[must_use]
    pub const fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    #[must_use]
    pub const fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }
}

#[derive(Clone, Debug)]
pub struct TableRow {
    pub cells: Vec<SharedString>,
}

impl TableRow {
    #[must_use]
    pub fn new(cells: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        Self {
            cells: cells.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Default)]
struct TableState {
    sort: SortState,
    selected: Option<usize>,
}

#[derive(IntoElement)]
pub struct Table {
    id: ElementId,
    columns: Vec<TableColumn>,
    rows: Vec<TableRow>,
    on_select: Option<SelectHandler>,
    style: StyleRefinement,
}

impl Table {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, columns: Vec<TableColumn>, rows: Vec<TableRow>) -> Self {
        Self {
            id: id.into(),
            columns,
            rows,
            on_select: None,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl Styled for Table {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Table {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| TableState::default());
        let sort = state.read(cx).sort;
        let selected = state.read(cx).selected;
        let order = sort.sorted_indices(&self.rows);
        let theme = cx.theme();
        let surface = Surface::new(Kind::Card.role());
        let ground = theme.ground_of(surface);
        let fg = theme.foreground;
        let muted = theme.foreground.muted();
        let mono_family = theme.mono_font_family.clone();
        let drop = cap_center_drop(px(TextStyle::Caption.design_px()), px(12.0), cx);

        let header = h_flex()
            .w_full()
            .h(px(30.0))
            .px(Space::S12)
            .gap(Space::S12)
            .items_center()
            .children(self.columns.iter().enumerate().map(|(col_idx, col)| {
                let is_sorted = sort.column == Some(col_idx);
                let sort_icon = match (is_sorted, sort.dir) {
                    (true, Some(SortDir::Asc)) => Some(IconName::ChevronUp),
                    (true, Some(SortDir::Desc)) => Some(IconName::ChevronDown),
                    _ => None,
                };
                let st = state.clone();
                let sortable = col.sortable;
                h_flex()
                    .id((self.id.clone(), format!("col-{col_idx}")))
                    .when_some(col.width, |el, w| el.flex_none().w(w))
                    .when(col.width.is_none(), |el| el.flex_1().min_w_0())
                    .items_center()
                    .gap(Space::S4)
                    .when(col.align == ColumnAlign::Right, Styled::justify_end)
                    .type_style_strong(TextStyle::Caption, cx)
                    .text_color(if is_sorted { fg } else { muted })
                    .when(sortable, |el| {
                        el.cursor_pointer()
                            .hover(move |s| s.text_color(fg))
                            .on_click(move |_, _, cx| {
                                st.update(cx, |table, cx| {
                                    table.sort.toggle(col_idx);
                                    cx.notify();
                                });
                            })
                    })
                    .child(div().truncate().child(col.title.clone()))
                    .when_some(sort_icon, |el, icon| {
                        el.child(
                            div()
                                .flex_none()
                                .relative()
                                .top(drop)
                                .child(Icon::new(icon).size(px(12.0))),
                        )
                    })
            }));

        let on_select = self.on_select;
        let body_rows = order.into_iter().map(|row_idx| {
            let row = &self.rows[row_idx];
            let active = selected == Some(row_idx);
            let row_surface = Surface::new(SurfaceRole::MENU_ITEM).on(ground);
            let layer = theme.layer(
                row_surface,
                if active {
                    PaintState::Selected
                } else {
                    PaintState::Rest
                },
            );
            let st = state.clone();
            let cb = on_select.clone();
            h_flex()
                .id((self.id.clone(), format!("row-{row_idx}")))
                .role(Role::Row)
                .aria_selected(active)
                .w_full()
                .h(px(Density::get().control_px()))
                .px(Space::S12)
                .gap(Space::S12)
                .items_center()
                .cursor_pointer()
                .layer(&layer)
                .on_click(move |_, window, cx| {
                    st.update(cx, |table, cx| {
                        table.selected = Some(row_idx);
                        cx.notify();
                    });
                    if let Some(handler) = &cb {
                        handler(row_idx, window, cx);
                    }
                })
                .children(self.columns.iter().enumerate().map(|(col_idx, col)| {
                    let text = row.cells.get(col_idx).cloned().unwrap_or_default();
                    h_flex()
                        .when_some(col.width, |el, w| el.flex_none().w(w))
                        .when(col.width.is_none(), |el| el.flex_1().min_w_0())
                        .when(col.align == ColumnAlign::Right, Styled::justify_end)
                        .child(
                            div()
                                .truncate()
                                .when(col.mono, |el| el.font_family(mono_family.clone()))
                                .type_style(TextStyle::Small, cx)
                                .text_color(if col_idx == 0 { fg } else { muted })
                                .child(text),
                        )
                }))
        });

        v_flex()
            .author_id(self.id.clone())
            .role(Role::Table)
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .surface(surface, cx)
            .refine_style(&self.style)
            .child(header)
            .child(Separator::horizontal().look(RuleLook::Bar))
            .child(v_flex().p(Space::S4).gap(Space::S2).children(body_rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_sort_cycles_and_orders_numeric_and_text_columns() {
        let rows = vec![
            TableRow::new(["work", "12.4 ms", "3"]),
            TableRow::new(["agents", "1.8 ms", "11"]),
            TableRow::new(["logs", "4.2 ms", "2"]),
        ];
        let mut sort = SortState::default();
        assert_eq!(sort.sorted_indices(&rows), vec![0, 1, 2]);

        sort.toggle(1);
        assert_eq!(sort.dir, Some(SortDir::Asc));
        assert_eq!(sort.sorted_indices(&rows), vec![1, 2, 0]);

        sort.toggle(1);
        assert_eq!(sort.dir, Some(SortDir::Desc));
        assert_eq!(sort.sorted_indices(&rows), vec![0, 2, 1]);

        sort.toggle(1);
        assert_eq!(sort.column, None);
        assert_eq!(sort.sorted_indices(&rows), vec![0, 1, 2]);

        sort.toggle(0);
        assert_eq!(sort.sorted_indices(&rows), vec![1, 2, 0]);
    }
}
