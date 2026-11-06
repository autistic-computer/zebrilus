from zzmojo.terminal_model import (
    TerminalCell,
    TerminalStyle,
    TerminalMetadata,
    TerminalViewport,
    TerminalHistory,
)


def _n(mut output: String, value: Int):
    output += String(value)
    output += ","


def _u(mut output: String, value: UInt64):
    output += String(value)
    output += ","


def _text(mut output: String, value: String):
    _n(output, value.byte_length())
    for byte in value.as_bytes():
        _n(output, Int(byte))


def _optional_text(mut output: String, value: Optional[String]):
    if not value:
        _n(output, -1)
    else:
        _text(output, value.value())


def _tables(
    mut output: String,
    cells: List[TerminalCell],
    styles: List[TerminalStyle],
    offsets: List[UInt32],
    bytes: List[UInt8],
):
    _n(output, len(cells))
    for cell in cells:
        _n(output, Int(cell.glyph))
        _n(output, Int(cell.style))
        _n(output, Int(cell.flags))
    _n(output, len(styles))
    for style in styles:
        _n(output, Int(style.foreground))
        _n(output, Int(style.background))
        _n(output, Int(style.underline_color))
        _n(output, Int(style.attributes))
        _n(output, Int(style.underline))
        _n(output, Int(style.background_index))
    _n(output, len(offsets))
    for offset in offsets:
        _n(output, Int(offset))
    _n(output, len(bytes))
    for byte in bytes:
        _n(output, Int(byte))


def _metadata(mut output: String, value: TerminalMetadata):
    _n(output, Int(value.foreground))
    _n(output, Int(value.background))
    _text(output, value.title)
    _optional_text(output, value.working_directory)
    _optional_text(output, value.hovered_uri)
    if value.cursor:
        _n(output, 1)
        var cursor = value.cursor.value()
        _n(output, Int(cursor.color))
        _n(output, Int(cursor.column))
        _n(output, Int(cursor.row))
        _n(output, Int(cursor.style))
        _n(output, Int(cursor.visible))
        _n(output, Int(cursor.blinking))
        _n(output, Int(cursor.wide_tail))
    else:
        _n(output, -1)
    _n(output, Int(value.scrollbar.total))
    _n(output, Int(value.scrollbar.offset))
    _n(output, Int(value.scrollbar.length))
    _n(output, len(value.overlays))
    for overlay in value.overlays:
        _n(output, Int(overlay.row))
        _n(output, Int(overlay.start))
        _n(output, Int(overlay.end))
        _n(output, Int(overlay.kind))
    _n(output, Int(value.mode.kind))
    _n(output, Int(value.mode.position))
    _n(output, Int(value.mode.total))
    _n(output, Int(value.mode.hide_position))
    if value.search:
        _n(output, 1)
        var search = value.search.value()
        _n(output, Int(search.current))
        _n(output, Int(search.total))
        _n(output, Int(search.pending))
        _n(output, Int(search.invalid_pattern))
    else:
        _n(output, -1)
    _n(output, Int(value.unseen_output))
    _n(output, Int(value.kitty_keyboard))
    _n(output, Int(value.mouse_tracking))
    _n(output, Int(value.status))
    _n(output, Int(value.exit_code))
    _optional_text(output, value.exit_signal)
    _text(output, value.failure)
    _n(output, len(value.kitty))
    for placement in value.kitty:
        _n(output, Int(placement.image_id))
        _u(output, placement.image_generation)
        _n(output, Int(placement.layer))
        _n(output, Int(placement.viewport_column))
        _n(output, Int(placement.viewport_row))
        _u(output, placement.absolute_row)
        _n(output, Int(placement.cell_offset_x))
        _n(output, Int(placement.cell_offset_y))
        _n(output, Int(placement.grid_columns))
        _n(output, Int(placement.grid_rows))
        _n(output, Int(placement.pixel_width))
        _n(output, Int(placement.pixel_height))
        if placement.source_rect:
            _n(output, 1)
            var rect = placement.source_rect.value()
            for index in range(4):
                _n(output, Int(rect[index]))
        else:
            _n(output, -1)


def viewport_fingerprint(viewport: TerminalViewport) -> String:
    var output = String()
    _u(output, viewport.generation)
    _u(output, viewport.view_generation)
    _n(output, Int(viewport.dictionary_generation))
    _n(output, Int(viewport.columns))
    _n(output, Int(viewport.rows))
    _tables(
        output,
        viewport.cells,
        viewport.styles,
        viewport.grapheme_offsets,
        viewport.grapheme_bytes,
    )
    _metadata(output, viewport.metadata)
    return output^


def history_fingerprint(history: TerminalHistory) -> String:
    var output = String()
    _n(output, Int(history.start))
    _n(output, Int(history.total))
    _n(output, Int(history.offset))
    _n(output, Int(history.columns))
    _n(output, Int(history.rows))
    _tables(
        output,
        history.cells,
        history.styles,
        history.grapheme_offsets,
        history.grapheme_bytes,
    )
    return output^
