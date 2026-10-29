use ala::{
    AnyElement, App, Div, ElementId, Hsla, ParentElement as _, Pixels, SharedString, Stateful,
    Styled as _, div, prelude::*, px, relative,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Layer, PaintState, StyledExt as _, roles,
    touch::press_highlight,
};

use super::overview::{HostState, host_state_dot};

pub const BENTO_COLUMNS: usize = 4;
pub const BENTO_GAP: f32 = 10.0;
pub const BENTO_INSET: f32 = 16.0;
const TILE_PADDING: f32 = 14.0;
const WINDOW_PADDING: f32 = 10.0;
const WINDOW_HEADER: f32 = 18.0;
const MINI_GAP: f32 = 4.0;
const MINI_RADIUS: f32 = 10.0;

/// How many grid cells a bento tile covers across and down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BentoSpan {
    pub columns: usize,
    pub rows: usize,
}

impl BentoSpan {
    pub const fn new(columns: usize, rows: usize) -> Self {
        Self { columns, rows }
    }

    /// The tile a window gets for its pane count: a square for one or two
    /// panes, the full width for more, taller still past three.
    pub fn for_window(panes: usize) -> Self {
        match panes {
            0..=2 => Self::new(2, 2),
            3 => Self::new(4, 2),
            _ => Self::new(4, 3),
        }
    }
}

/// Where each of `spans` lands on a grid [`BENTO_COLUMNS`] wide, as
/// `(column, row)`: row by row, each in the first free spot it fits, so a
/// small tile fills a hole a big one left behind.
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

/// A grid of `tiles` `width` wide in square cells, packed by
/// [`bento_pack`], each tile sized to the cells it spans.
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

/// A tile on a phone bento page, drawn as [`roles::PHONE_CARD`] and filling
/// the cells it is given. The caller adds what it holds and its click.
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

/// A host on the Hosts page: its state dot and name, `detail` under them, and
/// `footer` at the bottom: what its agents are doing, or the button that
/// connects it.
pub fn host_tile(
    id: impl Into<ElementId>,
    press: impl Into<ElementId>,
    state: HostState,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    footer: Vec<AnyElement>,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    bento_tile(id, press, cx).child(
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .p(px(TILE_PADDING))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(host_state_dot(state, cx))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(17.0))
                            .font_semibold()
                            .child(title.into()),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.foreground.muted())
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(detail.into()),
            )
            .child(div().flex_1())
            .children(footer),
    )
}

/// A line at the foot of a host tile: a dot in `color` and `text`, both in
/// `color` when it is a warning.
pub fn host_tile_line(color: Hsla, text: impl Into<SharedString>, warning: bool, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(13.0))
        .text_color(if warning {
            color
        } else {
            cx.theme().foreground.muted()
        })
        .child(div().flex_none().size(px(6.0)).rounded_full().bg(color))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text.into()),
        )
}

/// A tile that adds something, drawn as [`roles::PHONE_ADD`]: Add host on
/// the Hosts page.
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

/// A window on a host's bento page: its number, title and folder over
/// `body`, its panes drawn where they sit by [`window_layout`], with a dot
/// when one of them waits on you.
pub fn window_tile(
    id: impl Into<ElementId>,
    press: impl Into<ElementId>,
    index: u32,
    title: impl Into<SharedString>,
    folder: Option<SharedString>,
    attention: bool,
    body: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let muted = theme.foreground.muted();
    bento_tile(id, press, cx).child(
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(WINDOW_PADDING))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.0))
                    .h(px(WINDOW_HEADER))
                    .px(px(4.0))
                    .text_size(px(15.0))
                    .child(div().flex_none().text_color(muted).child(index.to_string()))
                    .child(
                        div()
                            .flex_none()
                            .max_w(relative(0.6))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_semibold()
                            .child(title.into()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .text_color(muted)
                            .children(folder),
                    )
                    .when(attention, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .size(px(8.0))
                                .rounded_full()
                                .bg(theme.warning),
                        )
                    }),
            )
            .child(div().relative().flex_1().min_h_0().child(body)),
    )
}

/// A window's panes laid out the way the window splits them: each of
/// `panes` at its `(x, y, width, height)` share of the area, with a gap
/// between neighbours.
pub fn window_layout(panes: Vec<([f32; 4], AnyElement)>) -> Div {
    let half = px(MINI_GAP / 2.0);
    div()
        .absolute()
        .inset(-half)
        .children(panes.into_iter().map(|([x, y, width, height], pane)| {
            div()
                .absolute()
                .left(relative(x))
                .top(relative(y))
                .w(relative(width))
                .h(relative(height))
                .p(half)
                .child(pane)
        }))
}

/// One pane in a window tile: its icon and title over `preview`, on
/// `background`, ringed in the accent when it is the pane the window opens
/// to and dotted when it waits on you. A window with one pane leaves the
/// title to the tile.
pub fn mini_pane(
    icon: IconName,
    title: Option<SharedString>,
    preview: Option<AnyElement>,
    background: Hsla,
    current: bool,
    attention: bool,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let frame = theme.layer(
        roles::PHONE_MINI,
        if current {
            PaintState::Selected
        } else {
            PaintState::Rest
        },
    );
    div()
        .relative()
        .size_full()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(6.0))
        .overflow_hidden()
        .layer(&Layer {
            fill: background,
            glass: None,
            radius: px(MINI_RADIUS),
            ..frame
        })
        .when_some(title, |pane, title| {
            pane.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(11.0))
                    .text_color(theme.foreground.muted())
                    .child(Icon::new(icon).size(px(12.0)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title),
                    )
                    .when(attention, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .size(px(6.0))
                                .rounded_full()
                                .bg(theme.warning),
                        )
                    }),
            )
        })
        .child(div().flex_1().min_h_0().overflow_hidden().children(preview))
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

    #[test]
    fn a_window_grows_with_its_panes() {
        assert_eq!(BentoSpan::for_window(1), BentoSpan::new(2, 2));
        assert_eq!(BentoSpan::for_window(2), BentoSpan::new(2, 2));
        assert_eq!(BentoSpan::for_window(3), BentoSpan::new(4, 2));
        assert_eq!(BentoSpan::for_window(6), BentoSpan::new(4, 3));
    }
}
