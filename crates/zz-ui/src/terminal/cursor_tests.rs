use super::*;

#[test]
fn wide_tail_cursor_resolves_to_the_leading_cell() {
    let mut viewport = TerminalViewport::blank(2, 1, zz_terminal::SessionStatus::Running);
    let expected = {
        let cells = Arc::make_mut(&mut viewport.cells);
        cells[0] = PackedCell::new(u32::from('界'), 0, CellWidth::Wide);
        cells[1] = PackedCell::new(0, 0, CellWidth::SpacerTail);
        cells[0]
    };
    let cursor = Cursor::new(
        1,
        0,
        true,
        false,
        true,
        CursorStyle::Block,
        Color::rgb(1, 2, 3),
    );

    let paint = cursor_cell(&viewport, cursor, 2, RowProjection::new(1, 1, false))
        .expect("cursor paint cell");
    assert_eq!(paint.column, 0);
    assert_eq!(paint.row, 0);
    assert_eq!(paint.width, 2);
    assert_eq!(paint.cell, expected);
}

#[test]
fn cursor_shapes_use_device_pixel_strokes_and_hollow_when_unfocused() {
    let bounds = Bounds::new(point(px(2.0), px(3.0)), size(px(8.0), px(20.0)));
    let scale = 1.3;
    let stroke = device_pixel(scale);
    let cursor_color = Color::rgb(0xaa, 0xbb, 0xcc);

    let bar = cursor_quad(bounds, CursorStyle::Bar, cursor_color, true, scale);
    assert_eq!(bar.bounds.size.width, stroke);
    assert_eq!(bar.bounds.size.height, bounds.size.height);

    let underline = cursor_quad(bounds, CursorStyle::Underline, cursor_color, true, scale);
    assert_eq!(underline.bounds.size.height, stroke * 2.0);
    assert_eq!(underline.bounds.bottom(), bounds.bottom());

    let hollow = cursor_quad(bounds, CursorStyle::Block, cursor_color, false, scale);
    assert_eq!(hollow.bounds, bounds);
    assert_eq!(hollow.border_widths.top, stroke);
    assert_eq!(hollow.border_widths.right, stroke);
    assert_eq!(hollow.border_widths.bottom, stroke);
    assert_eq!(hollow.border_widths.left, stroke);
}

#[test]
fn unfocused_cursor_stays_visible_and_composition_suppresses_it() {
    let cursor = Cursor::new(
        0,
        0,
        true,
        true,
        false,
        CursorStyle::Block,
        Color::rgb(1, 2, 3),
    );
    assert!(!cursor_visible_for_paint(Some(cursor), false, true, false));
    assert!(cursor_visible_for_paint(Some(cursor), false, false, false));
    assert!(!cursor_visible_for_paint(Some(cursor), true, false, true));
}
