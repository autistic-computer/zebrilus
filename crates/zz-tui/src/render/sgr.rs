use super::*;

pub(super) fn sgr_is_reset(bytes: &[u8]) -> bool {
    let Some(mut rest) = bytes.strip_prefix(b"\x1b[0m") else {
        return false;
    };
    while !rest.is_empty() {
        let Some(after) = rest
            .strip_prefix(b"\x1b[39m")
            .or_else(|| rest.strip_prefix(b"\x1b[49m"))
            .or_else(|| rest.strip_prefix(b"\x1b[0m"))
        else {
            return false;
        };
        rest = after;
    }
    true
}

pub(super) fn cursor_only(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    while !rest.is_empty() {
        if let Some(after) = rest
            .strip_prefix(b"\x1b[?25h")
            .or_else(|| rest.strip_prefix(b"\x1b[?25l"))
        {
            rest = after;
            continue;
        }
        let Some(after) = rest.strip_prefix(b"\x1b[") else {
            return false;
        };
        let digits = after
            .iter()
            .take_while(|byte| byte.is_ascii_digit() || **byte == b';')
            .count();
        if after.get(digits) != Some(&b'H') {
            return false;
        }
        rest = &after[digits + 1..];
    }
    true
}

pub(super) fn fill_unpainted_cells(output: &mut Vec<u8>, model: &Model) {
    let columns = usize::from(model.size.columns);
    let rows = usize::from(model.size.rows);
    if columns == 0 || rows == 0 {
        return;
    }
    let mut painted = vec![false; columns * rows];
    let mut mark = |rect: Rect| {
        let right = usize::from(rect.x.saturating_add(rect.width)).min(columns);
        let bottom = usize::from(rect.y.saturating_add(rect.height)).min(rows);
        for row in usize::from(rect.y)..bottom {
            for column in usize::from(rect.x)..right {
                painted[row * columns + column] = true;
            }
        }
    };
    for divider in &model.layout.dividers {
        mark(divider.rect);
    }
    for entry in &model.layout.panes {
        let Some(pane) = model.pane_snapshot(entry.pane) else {
            continue;
        };
        if entry.border_status.is_on() {
            mark(entry.status_row());
        }
        if !matches!(pane.kind, PaneKindSnapshot::Terminal) || pane.mode.is_none() {
            mark(entry.content());
        }
    }
    let (status_x, status_width) = model.status_area();
    mark(Rect {
        x: status_x,
        y: model.status_origin_y(),
        width: status_width,
        height: model.status_block_rows(),
    });
    let mut themed = false;
    for row in 0..rows {
        let cells = &painted[row * columns..(row + 1) * columns];
        let mut column = 0;
        while column < columns {
            if cells[column] {
                column += 1;
                continue;
            }
            let start = column;
            while column < columns && !cells[column] {
                column += 1;
            }
            if !themed {
                let background = model.appearance.background;
                write!(
                    output,
                    "\x1b[48;2;{};{};{}m",
                    background.r, background.g, background.b
                )
                .expect("writing to Vec cannot fail");
                themed = true;
            }
            write_cursor_position(
                output,
                u16::try_from(start).unwrap_or(u16::MAX),
                u16::try_from(row).unwrap_or(u16::MAX),
            );
            write!(output, "\x1b[{}X", column - start).expect("writing to Vec cannot fail");
        }
    }
    if themed {
        output.extend_from_slice(b"\x1b[0m");
    }
}

pub(super) fn clear_screen(output: &mut Vec<u8>, background: Color) {
    write!(
        output,
        "\x1b[0;48;2;{};{};{}m\x1b[2J",
        background.r, background.g, background.b
    )
    .expect("writing to Vec cannot fail");
}

pub(super) fn write_cursor_position(output: &mut Vec<u8>, column: u16, row: u16) {
    output.extend_from_slice(b"\x1b[");
    write_decimal(output, row.saturating_add(1));
    output.push(b';');
    write_decimal(output, column.saturating_add(1));
    output.push(b'H');
}

pub(super) fn write_decimal(output: &mut Vec<u8>, value: u16) {
    let mut digits = [0_u8; 5];
    let mut start = digits.len();
    let mut rest = value;
    loop {
        start -= 1;
        digits[start] = b"0123456789"[usize::from(rest % 10)];
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    output.extend_from_slice(&digits[start..]);
}

pub(super) fn write_colored_text(
    output: &mut Vec<u8>,
    column: u16,
    row: u16,
    text: &str,
    foreground: Color,
    background: Color,
) {
    write_cursor_position(output, column, row);
    write!(
        output,
        "\x1b[0;38;2;{};{};{};48;2;{};{};{}m",
        foreground.r, foreground.g, foreground.b, background.r, background.g, background.b
    )
    .expect("writing to Vec cannot fail");
    output.extend_from_slice(text.as_bytes());
    output.extend_from_slice(b"\x1b[0m");
}

pub(super) fn write_styled_text(
    output: &mut Vec<u8>,
    column: u16,
    row: u16,
    line: &StyledLine,
    foreground: Color,
    background: Color,
    appearance: &TerminalAppearance,
) {
    write_cursor_position(output, column, row);
    for segment in &line.segments {
        write_tmux_sgr(output, &segment.style, foreground, background, appearance);
        output.extend_from_slice(segment.text.as_bytes());
    }
    output.extend_from_slice(b"\x1b[0m");
}

fn grounded(style: &TmuxStyle, base: Option<&TmuxStyle>) -> TmuxStyle {
    let base = base.cloned().unwrap_or_default();
    let mut grounded = base.clone();
    apply_style(&mut grounded, style, &base);
    grounded.fg = grounded.fg.or(Some(TmuxColour::Default));
    grounded.bg = grounded.bg.or(Some(TmuxColour::Default));
    grounded
}

pub(super) fn write_border_sgr(
    output: &mut Vec<u8>,
    style: &TmuxStyle,
    appearance: &TerminalAppearance,
) {
    write_tmux_sgr(
        output,
        &grounded(style, None),
        appearance.foreground,
        appearance.background,
        appearance,
    );
}

pub(super) fn write_colored_sgr(output: &mut Vec<u8>, foreground: Color, background: Color) {
    write!(
        output,
        "\x1b[0;38;2;{};{};{};48;2;{};{};{}m",
        foreground.r, foreground.g, foreground.b, background.r, background.g, background.b
    )
    .expect("writing to Vec cannot fail");
}

pub(super) fn write_border_runs(
    output: &mut Vec<u8>,
    cells: &BTreeMap<(u16, u16), (Rc<Vec<u8>>, String)>,
    columns: u16,
) {
    if cells.is_empty() {
        return;
    }
    let mut last = None;
    let mut rendition: Option<&Rc<Vec<u8>>> = None;
    for (&(row, column), (sgr, glyph)) in cells {
        match last {
            Some((last_row, last_column)) if last_row == row && last_column + 1 == column => {}
            Some((last_row, last_column)) if last_row + 1 == row && last_column == column => {
                output.extend_from_slice(b"\x08\n");
            }
            _ => write_cursor_position(output, column, row),
        }
        if rendition.is_none_or(|current| !Rc::ptr_eq(current, sgr) && current != sgr) {
            output.extend_from_slice(sgr);
            rendition = Some(sgr);
        }
        output.extend_from_slice(glyph.as_bytes());
        last = (text_display_width(glyph) == 1 && column.saturating_add(1) < columns)
            .then_some((row, column));
    }
    output.extend_from_slice(b"\x1b[0m");
}

pub(super) fn write_styled_text_over(
    output: &mut Vec<u8>,
    column: u16,
    row: u16,
    line: &StyledLine,
    base: &TmuxStyle,
    appearance: &TerminalAppearance,
) {
    write_cursor_position(output, column, row);
    for segment in &line.segments {
        write_tmux_sgr(
            output,
            &grounded(&segment.style, Some(base)),
            appearance.foreground,
            appearance.background,
            appearance,
        );
        output.extend_from_slice(segment.text.as_bytes());
    }
    output.extend_from_slice(b"\x1b[0m");
}

pub(super) fn write_tmux_sgr(
    output: &mut Vec<u8>,
    style: &TmuxStyle,
    default_foreground: Color,
    default_background: Color,
    appearance: &TerminalAppearance,
) {
    output.extend_from_slice(b"\x1b[0m");
    let attributes = &style.attributes;
    if attributes.noattr != TmuxAttributeState::On {
        if attributes.bold == TmuxAttributeState::On {
            output.extend_from_slice(b"\x1b[1m");
        }
        if attributes.dim == TmuxAttributeState::On {
            output.extend_from_slice(b"\x1b[2m");
        }
        if attributes.italics == TmuxAttributeState::On {
            output.extend_from_slice(b"\x1b[3m");
        }
        for (state, sequence) in [
            (attributes.underscore, b"\x1b[4m".as_slice()),
            (attributes.double_underscore, b"\x1b[4:2m".as_slice()),
            (attributes.curly_underscore, b"\x1b[4:3m".as_slice()),
            (attributes.dotted_underscore, b"\x1b[4:4m".as_slice()),
            (attributes.dashed_underscore, b"\x1b[4:5m".as_slice()),
        ] {
            if state == TmuxAttributeState::On {
                output.extend_from_slice(sequence);
            }
        }
        if attributes.blink == TmuxAttributeState::On {
            output.extend_from_slice(b"\x1b[5m");
        }
        if attributes.reverse == TmuxAttributeState::On {
            output.extend_from_slice(b"\x1b[7m");
        }
        if attributes.hidden == TmuxAttributeState::On {
            output.extend_from_slice(b"\x1b[8m");
        }
        if attributes.strikethrough == TmuxAttributeState::On {
            output.extend_from_slice(b"\x1b[9m");
        }
        if attributes.overline == TmuxAttributeState::On {
            output.extend_from_slice(b"\x1b[53m");
        }
    }
    if let Some(colour) = style.us {
        let colour = resolve_tmux_colour(colour, default_foreground, appearance);
        write!(output, "\x1b[58;2;{};{};{}m", colour.r, colour.g, colour.b)
            .expect("writing to Vec cannot fail");
    }
    write_ground(output, style.fg, default_foreground, Ground::Foreground);
    write_ground(output, style.bg, default_background, Ground::Background);
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Ground {
    Foreground,
    Background,
}

fn write_ground(output: &mut Vec<u8>, colour: Option<TmuxColour>, fallback: Color, ground: Ground) {
    let Some(colour) = colour else {
        write_default_ground(output, ground);
        return;
    };
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => {
            write_palette_ground(output, index, ground);
        }
        TmuxColour::Rgb(value) => write_rgb_ground(output, Color::from_packed(value), ground),
        TmuxColour::Theme(index) => {
            write_rgb_ground(output, theme_colour(index).unwrap_or(fallback), ground);
        }
        TmuxColour::Default | TmuxColour::Terminal => write_default_ground(output, ground),
    }
}

pub(super) fn write_palette_ground(output: &mut Vec<u8>, index: u8, ground: Ground) {
    let index = match crate::tty::terminal_colours() {
        Some(colours) => downgrade_palette(index, colours),
        None => index,
    };
    let (low, bright, extended) = match ground {
        Ground::Foreground => (30, 90, 38),
        Ground::Background => (40, 100, 48),
    };
    if index < 8 {
        write!(output, "\x1b[{}m", low + u16::from(index))
    } else if index < 16 {
        write!(output, "\x1b[{}m", bright + u16::from(index) - 8)
    } else {
        write!(output, "\x1b[{extended};5;{index}m")
    }
    .expect("writing to Vec cannot fail");
}

fn write_default_ground(output: &mut Vec<u8>, ground: Ground) {
    output.extend_from_slice(match ground {
        Ground::Foreground => b"\x1b[39m",
        Ground::Background => b"\x1b[49m",
    });
}

pub(super) fn write_glyph(output: &mut Vec<u8>, glyph: Glyph<'_>, width: u16) {
    match glyph {
        Glyph::Empty => output.push(b' '),
        Glyph::Scalar(character) if u32::from(character) < 0x7f => {
            output.push(character as u8);
        }
        _ if !crate::tty::terminal_takes_utf8() => {
            output.extend(std::iter::repeat_n(b'_', usize::from(width)));
        }
        Glyph::Scalar(character) => {
            let mut bytes = [0; 4];
            output.extend_from_slice(character.encode_utf8(&mut bytes).as_bytes());
        }
        Glyph::Grapheme(grapheme) => output.extend_from_slice(grapheme.as_bytes()),
    }
}

pub(super) fn write_cell_ground(
    output: &mut Vec<u8>,
    class: ColourClass,
    colour: Color,
    default: Color,
    ground: Ground,
) {
    match class {
        ColourClass::Default => write_default_ground(output, ground),
        ColourClass::Palette(index) => write_palette_ground(output, index, ground),
        ColourClass::Resolved if colour == default => write_default_ground(output, ground),
        ColourClass::Rgb | ColourClass::Resolved => write_rgb_ground(output, colour, ground),
    }
}

pub(super) fn write_rgb_ground(output: &mut Vec<u8>, colour: Color, ground: Ground) {
    if crate::tty::terminal_colours().is_some_and(|colours| colours < RGB_COLOURS) {
        write_palette_ground(output, colour_find_rgb(colour), ground);
        return;
    }
    let base = match ground {
        Ground::Foreground => 38,
        Ground::Background => 48,
    };
    write!(
        output,
        "\x1b[{base};2;{};{};{}m",
        colour.r, colour.g, colour.b
    )
    .expect("writing to Vec cannot fail");
}

pub(super) fn trailing_clear(
    viewport: &TerminalViewport,
    row: u16,
    width: u16,
    default_style: PackedStyle,
    overlay_mask: &[bool],
    selection_mask: &[bool],
) -> Option<u16> {
    let mut last_background = viewport.background;
    let mut run = None;
    for column in 0..width {
        let cell = viewport.cell(row, column).unwrap_or(PackedCell::EMPTY);
        let style = viewport.style(cell).unwrap_or(default_style);
        let marked = overlay_mask[usize::from(column)] || selection_mask[usize::from(column)];
        let blank = !marked
            && cell.width() == CellWidth::Narrow
            && style.background() == last_background
            && !(style.bold()
                || style.faint()
                || style.italic()
                || style.blink()
                || style.invisible()
                || style.strikethrough()
                || style.overline()
                || style.hyperlink()
                || style.underline() != UnderlineStyle::None)
            && matches!(viewport.glyph(cell), Glyph::Empty | Glyph::Scalar(' '));
        if blank {
            run.get_or_insert(column);
        } else {
            run = None;
            last_background = style.background();
        }
    }
    let start = run?;
    (width - start >= 10 && last_background == viewport.background).then_some(start)
}

pub(super) fn write_line_sgr(output: &mut Vec<u8>, line: &TmuxStyle) {
    let attributes = &line.attributes;
    for (state, sequence) in [
        (attributes.bold, b"\x1b[1m".as_slice()),
        (attributes.dim, b"\x1b[2m".as_slice()),
        (attributes.italics, b"\x1b[3m".as_slice()),
        (attributes.underscore, b"\x1b[4m".as_slice()),
        (attributes.double_underscore, b"\x1b[4:2m".as_slice()),
        (attributes.curly_underscore, b"\x1b[4:3m".as_slice()),
        (attributes.dotted_underscore, b"\x1b[4:4m".as_slice()),
        (attributes.dashed_underscore, b"\x1b[4:5m".as_slice()),
        (attributes.blink, b"\x1b[5m".as_slice()),
        (attributes.reverse, b"\x1b[7m".as_slice()),
        (attributes.hidden, b"\x1b[8m".as_slice()),
        (attributes.strikethrough, b"\x1b[9m".as_slice()),
        (attributes.overline, b"\x1b[53m".as_slice()),
    ] {
        if state == TmuxAttributeState::On {
            output.extend_from_slice(sequence);
        }
    }
    for (colour, ground) in [(line.fg, Ground::Foreground), (line.bg, Ground::Background)] {
        if let Some(colour) =
            colour.filter(|colour| !matches!(colour, TmuxColour::Default | TmuxColour::Terminal))
        {
            write_ground(output, Some(colour), Color::default(), ground);
        }
    }
}

pub(super) fn write_selection_sgr(output: &mut Vec<u8>, selection: &TmuxStyle) {
    let attributes = &selection.attributes;
    for (state, sequence) in [
        (attributes.bold, b"\x1b[1m".as_slice()),
        (attributes.dim, b"\x1b[2m".as_slice()),
        (attributes.italics, b"\x1b[3m".as_slice()),
        (attributes.underscore, b"\x1b[4m".as_slice()),
        (attributes.blink, b"\x1b[5m".as_slice()),
        (attributes.reverse, b"\x1b[7m".as_slice()),
        (attributes.hidden, b"\x1b[8m".as_slice()),
        (attributes.strikethrough, b"\x1b[9m".as_slice()),
        (attributes.overline, b"\x1b[53m".as_slice()),
    ] {
        if state == TmuxAttributeState::On {
            output.extend_from_slice(sequence);
        }
    }
    for (colour, ground) in [
        (selection.fg, Ground::Foreground),
        (selection.bg, Ground::Background),
    ] {
        if let Some(colour) =
            colour.filter(|colour| !matches!(colour, TmuxColour::Default | TmuxColour::Terminal))
        {
            write_ground(output, Some(colour), Color::default(), ground);
        }
    }
}

pub(super) fn write_sgr(
    output: &mut Vec<u8>,
    style: PackedStyle,
    reverse: bool,
    default_foreground: Color,
    default_background: Color,
    defaults: &TmuxStyle,
) {
    output.extend_from_slice(b"\x1b[0m");
    if style.bold() {
        output.extend_from_slice(b"\x1b[1m");
    }
    if style.faint() {
        output.extend_from_slice(b"\x1b[2m");
    }
    if style.italic() {
        output.extend_from_slice(b"\x1b[3m");
    }
    match style.underline() {
        UnderlineStyle::None if style.hyperlink() => output.extend_from_slice(b"\x1b[4m"),
        UnderlineStyle::None => {}
        UnderlineStyle::Single => output.extend_from_slice(b"\x1b[4m"),
        UnderlineStyle::Double => output.extend_from_slice(b"\x1b[4:2m"),
        UnderlineStyle::Curly => output.extend_from_slice(b"\x1b[4:3m"),
        UnderlineStyle::Dotted => output.extend_from_slice(b"\x1b[4:4m"),
        UnderlineStyle::Dashed => output.extend_from_slice(b"\x1b[4:5m"),
    }
    if let Some(color) = style.underline_color() {
        write!(output, "\x1b[58;2;{};{};{}m", color.r, color.g, color.b)
            .expect("writing to Vec cannot fail");
    }
    if style.blink() {
        output.extend_from_slice(b"\x1b[5m");
    }
    if style.invisible() {
        output.extend_from_slice(b"\x1b[8m");
    }
    if style.strikethrough() {
        output.extend_from_slice(b"\x1b[9m");
    }
    if style.overline() {
        output.extend_from_slice(b"\x1b[53m");
    }
    if style.foreground() == default_foreground && defaults.fg.is_some() {
        write_ground(output, defaults.fg, default_foreground, Ground::Foreground);
    } else {
        write_cell_ground(
            output,
            style.foreground_class(),
            style.foreground(),
            default_foreground,
            Ground::Foreground,
        );
    }
    if style.background() == default_background && defaults.bg.is_some() {
        write_ground(output, defaults.bg, default_background, Ground::Background);
    } else {
        write_cell_ground(
            output,
            style.background_class(),
            style.background(),
            default_background,
            Ground::Background,
        );
    }
    if reverse {
        output.extend_from_slice(b"\x1b[7m");
    }
}
