use super::*;

pub(super) fn cursor_cell(
    viewport: &TerminalViewport,
    cursor: Cursor,
    visible_columns: u16,
    row_projection: RowProjection,
) -> Option<CursorCellPaint> {
    let row = row_projection.display_row(cursor.row())?;
    let mut column = usize::from(cursor.column());
    if column >= usize::from(visible_columns.min(viewport.columns)) {
        return None;
    }
    let cells = viewport.row(cursor.row())?;
    let mut cell = *cells.get(column)?;
    if (cursor.at_wide_tail() || matches!(cell.width(), CellWidth::SpacerTail)) && column > 0 {
        column -= 1;
        cell = *cells.get(column)?;
    }
    let width = if matches!(cell.width(), CellWidth::Wide) {
        2
    } else {
        1
    };
    Some(CursorCellPaint {
        column,
        row,
        width: width.min(usize::from(visible_columns).saturating_sub(column)),
        cell,
    })
}

pub(super) fn cursor_glyph_line(
    viewport: &TerminalViewport,
    cursor: CursorCellPaint,
    origin: Point<Pixels>,
    font_size: Pixels,
    base_font: &Font,
    appearance: &TerminalAppearance,
    window: &mut Window,
) -> Option<PositionedLine> {
    let style = style_for(viewport, appearance.local_colors.as_ref(), cursor.cell);
    if style.invisible() {
        return None;
    }
    let text = match glyph_for(viewport, cursor.cell) {
        Glyph::Empty => return None,
        Glyph::Scalar(character) if character.is_whitespace() => return None,
        Glyph::Scalar(character) => character.to_string(),
        Glyph::Grapheme(text) if text.chars().all(char::is_whitespace) => return None,
        Glyph::Grapheme(text) => text.to_owned(),
    };
    let font = styled_font(style, base_font, appearance);
    let glyph_options = glyph_render_options(
        TerminalFontStyle::from_packed(style),
        &font,
        appearance,
        window,
    );
    let run = TextRun {
        len: text.len(),
        font,
        color: color(viewport.background, 1.0),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    Some(PositionedLine {
        line: window
            .text_system()
            .shape_line(text.into(), font_size, &[run], None),
        origin,
        glyph_options,
    })
}

pub(super) fn cursor_quad(
    bounds: Bounds<Pixels>,
    style: CursorStyle,
    cursor_color: Color,
    focused: bool,
    scale: f32,
) -> PaintQuad {
    let cursor_hsla = color(cursor_color, 1.0);
    let stroke = device_pixel(scale);
    if !focused || matches!(style, CursorStyle::BlockHollow) {
        return quad(
            bounds,
            0.0,
            color(cursor_color, 0.0),
            stroke,
            cursor_hsla,
            BorderStyle::Solid,
        );
    }
    match style {
        CursorStyle::Bar => fill(
            Bounds::new(bounds.origin, size(stroke, bounds.size.height)),
            cursor_hsla,
        ),
        CursorStyle::Underline => fill(
            Bounds::new(
                point(bounds.origin.x, bounds.bottom() - stroke * 2.0),
                size(bounds.size.width, stroke * 2.0),
            ),
            cursor_hsla,
        ),
        CursorStyle::Block => fill(bounds, cursor_hsla),
        CursorStyle::BlockHollow => quad(
            bounds,
            0.0,
            color(cursor_color, 0.0),
            stroke,
            cursor_hsla,
            BorderStyle::Solid,
        ),
    }
}

pub(super) fn cursor_visible_for_paint(
    cursor: Option<Cursor>,
    composing: bool,
    focused: bool,
    blink_visible: bool,
) -> bool {
    !composing && cursor.is_some_and(|cursor| cursor.visible() && (!focused || blink_visible))
}
