use ala::{
    AnyElement, App, Div, ElementId, ParentElement as _, Pixels, SharedString, Stateful,
    Styled as _, div, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Layer, PaintState, StyledExt as _, roles,
    touch::press_highlight,
};

pub const BENTO_COLUMNS: usize = 4;
pub const BENTO_GAP: f32 = 10.0;
pub const BENTO_INSET: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BentoSpan {
    pub columns: usize,
    pub rows: usize,
}

impl BentoSpan {
    pub const fn new(columns: usize, rows: usize) -> Self {
        Self { columns, rows }
    }
}

pub fn bento_pack(spans: &[BentoSpan]) -> Vec<(usize, usize)> {
    let mut taken: Vec<[bool; BENTO_COLUMNS]> = Vec::new();
    spans
        .iter()
        .map(|span| {
            let columns = span.columns.clamp(1, BENTO_COLUMNS);
            let rows = span.rows.max(1);
            let fits = |taken: &Vec<[bool; BENTO_COLUMNS]>, column: usize, row: usize| {
                (row..row + rows).all(|row| {
                    (column..column + columns)
                        .all(|column| !taken.get(row).is_some_and(|cells| cells[column]))
                })
            };
            let spot = (0..=taken.len())
                .flat_map(|row| (0..=BENTO_COLUMNS - columns).map(move |column| (column, row)))
                .find(|&(column, row)| fits(&taken, column, row))
                .unwrap_or_default();
            let (column, row) = spot;
            if taken.len() < row + rows {
                taken.resize(row + rows, [false; BENTO_COLUMNS]);
            }
            for cells in &mut taken[row..row + rows] {
                for cell in &mut cells[column..column + columns] {
                    *cell = true;
                }
            }
            spot
        })
        .collect()
}

pub fn bento_grid(width: Pixels, tiles: Vec<(BentoSpan, AnyElement)>) -> Div {
    let gap = px(BENTO_GAP);
    let cell = ((width - gap * (BENTO_COLUMNS - 1) as f32) / BENTO_COLUMNS as f32).max(px(0.0));
    let spans: Vec<BentoSpan> = tiles.iter().map(|(span, _)| *span).collect();
    let spots = bento_pack(&spans);
    let extent = |cells: usize| cell * cells as f32 + gap * cells.saturating_sub(1) as f32;
    let rows = spans
        .iter()
        .zip(&spots)
        .map(|(span, (_, row))| row + span.rows.max(1))
        .max()
        .unwrap_or(0);
    div()
        .debug_selector(|| "bento-grid".to_owned())
        .relative()
        .flex_none()
        .w(width)
        .h(extent(rows))
        .children(
            tiles
                .into_iter()
                .zip(spots)
                .map(|((span, tile), (column, row))| {
                    div()
                        .absolute()
                        .left((cell + gap) * column as f32)
                        .top((cell + gap) * row as f32)
                        .w(extent(span.columns.clamp(1, BENTO_COLUMNS)))
                        .h(extent(span.rows.max(1)))
                        .child(tile)
                }),
        )
}

pub fn bento_tile(
    id: impl Into<ElementId>,
    press: impl Into<ElementId>,
    cx: &App,
) -> Stateful<Div> {
    let frame = cx.theme().layer(roles::PHONE_CARD, PaintState::Rest);
    let radius = frame.radius;
    div()
        .id(id)
        .relative()
        .size_full()
        .overflow_hidden()
        .cursor_pointer()
        .layer(&Layer {
            glass: None,
            ..frame
        })
        .child(press_highlight(press, None, radius))
}

pub fn bento_add_tile(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let frame = theme.layer(roles::PHONE_ADD, PaintState::Rest);
    let radius = frame.radius;
    div()
        .id(id)
        .relative()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .cursor_pointer()
        .layer(&Layer {
            glass: None,
            ..frame
        })
        .text_size(px(15.0))
        .text_color(theme.foreground.muted())
        .child(press_highlight("bento-add-press", None, radius))
        .child(Icon::new(IconName::Plus).size(px(18.0)))
        .child(title.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_pack_row_by_row_and_fill_holes() {
        let wide = BentoSpan::new(4, 3);
        let square = BentoSpan::new(2, 2);
        let short = BentoSpan::new(2, 1);
        assert_eq!(
            bento_pack(&[wide, square, square, short, short]),
            vec![(0, 0), (0, 3), (2, 3), (0, 5), (2, 5)]
        );
        assert_eq!(
            bento_pack(&[short, square, short]),
            vec![(0, 0), (2, 0), (0, 1)]
        );
        assert_eq!(bento_pack(&[BentoSpan::new(9, 0)]), vec![(0, 0)]);
    }
}
