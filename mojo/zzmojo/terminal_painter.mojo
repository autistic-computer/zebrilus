from std.sys import align_of, size_of
from mjui.ui import Ui
from mjui.term.grid import (
    CellMetrics,
    CellStyle,
    GridCell,
    GridCursor,
    GridOverlay,
    GridTables,
    TermOpts,
)
from mjui.term.rows import row_hash
from mjui.term.paint import grid_selection
from mjui.text.shaper import hash_bytes
from .terminal_model import (
    TerminalCell,
    TerminalCursor,
    TerminalOverlay,
    TerminalStyle,
    TerminalViewport,
)
from .terminal_projection import projection_plan
from mjui.term.overlays import SelectionCoverage


def selection_flags(viewport: TerminalViewport, opts: TermOpts) -> List[UInt16]:
    var flags = List[UInt16]()
    if not opts.native_appearance or not opts.rounded_selection:
        return flags^
    var original = List[GridOverlay]()
    for overlay in viewport.metadata.overlays:
        original.append(GridOverlay(overlay.row, overlay.start, overlay.end, overlay.kind))
    try:
        var coverage = SelectionCoverage(Span(original))
        for overlay in original:
            flags.append(overlay.kind | coverage.corners(overlay))
    except:
        flags.clear()
    return flags^


@inline(.never)
def render_viewport(
    mut ui: Ui,
    key: UInt64,
    viewport: TerminalViewport,
    font: Int32,
    size: Float32,
    focused: Bool,
    columns: Int = 0,
    rows: Int = 0,
    command_output: Bool = False,
    marked: Optional[String] = None,
) -> CellMetrics:
    return render_viewport_opts(
        ui,
        key,
        viewport,
        TermOpts(font, size, focused=focused, native_colors=True),
        columns,
        rows,
        command_output,
        marked,
    )


@inline(.never)
def render_viewport_opts(
    mut ui: Ui,
    key: UInt64,
    viewport: TerminalViewport,
    opts: TermOpts,
    columns: Int = 0,
    rows: Int = 0,
    command_output: Bool = False,
    marked: Optional[String] = None,
) -> CellMetrics:
    comptime assert size_of[TerminalCell]() == size_of[GridCell]() == 8
    comptime assert size_of[TerminalStyle]() == size_of[CellStyle]() == 16
    comptime assert size_of[TerminalOverlay]() == size_of[GridOverlay]() == 8
    comptime assert size_of[TerminalCursor]() == size_of[GridCursor]() == 12
    comptime assert align_of[TerminalCell]() == align_of[GridCell]() == 4
    comptime assert align_of[TerminalStyle]() == align_of[CellStyle]() == 4
    comptime assert align_of[TerminalOverlay]() == align_of[GridOverlay]() == 2
    comptime assert align_of[TerminalCursor]() == align_of[GridCursor]() == 4
    var styles = (
        viewport.styles.unsafe_ptr()
        .unsafe_bitcast[CellStyle]()
        .unsafe_origin_cast[ImmUntrackedOrigin]()
    )
    var cells = (
        viewport.cells.unsafe_ptr()
        .unsafe_bitcast[GridCell]()
        .unsafe_origin_cast[ImmUntrackedOrigin]()
    )
    var offsets = viewport.grapheme_offsets.unsafe_ptr().unsafe_origin_cast[
        ImmUntrackedOrigin
    ]()
    var bytes = viewport.grapheme_bytes.unsafe_ptr().unsafe_origin_cast[
        ImmUntrackedOrigin
    ]()
    var tables = GridTables(
        styles,
        len(viewport.styles),
        offsets,
        bytes,
        max(0, len(viewport.grapheme_offsets) - 1),
        viewport.metadata.foreground,
        viewport.metadata.background,
        0,
    )
    var display_columns = columns if columns > 0 else Int(viewport.columns)
    var display_rows = rows if rows > 0 else Int(viewport.rows)
    var plan = projection_plan(viewport, UInt16(display_rows), command_output)
    var visible_columns = min(display_columns, Int(viewport.columns))
    var metrics = ui.grid_begin(
        key, display_columns, display_rows, tables, opts
    )
    for offset in range(Int(plan.rows.count)):
        var source = Int(plan.rows.source_start) + offset
        var destination = Int(plan.rows.destination_start) + offset
        var line = cells.unsafe_offset(source * Int(viewport.columns))
        var revision = row_hash(
            line, visible_columns, ui.term.views[ui.term.cur]
        )
        revision = hash_bytes(styles.unsafe_bitcast[UInt8](), 16, revision)
        revision = (revision & 0x7FFFFFFFFFFFFFFF) | 1
        ui.grid_row(destination, line, visible_columns, revision)
    var overlays = List[GridOverlay]()
    var flags = selection_flags(viewport, opts)
    for index in range(len(viewport.metadata.overlays)):
        var overlay = viewport.metadata.overlays[index]
        var row = plan.live.display_row(overlay.row)
        var end = min(Int(overlay.end), visible_columns)
        if row and Int(overlay.start) < end:
            overlays.append(
                GridOverlay(
                    UInt16(row.value()),
                    overlay.start,
                    UInt16(end),
                    flags[index] if len(flags) > 0 else overlay.kind,
                )
            )
    ui.grid_overlays(
        overlays.unsafe_ptr()
        .unsafe_mut_cast[False]()
        .unsafe_origin_cast[ImmUntrackedOrigin](),
        len(overlays),
    )
    _ = overlays
    if viewport.metadata.cursor:
        var cursor = viewport.metadata.cursor.value()
        var row = plan.live.display_row(cursor.row)
        if row and Int(cursor.column) < visible_columns:
            cursor.row = UInt16(row.value())
            ui.grid_cursor(
                Pointer(to=cursor).unsafe_bitcast[GridCursor]()[unsafe_offset=0]
            )
        else:
            ui.grid_cursor(GridCursor(0, 0, 0, 0, 0, 0, 0))
    else:
        ui.grid_cursor(GridCursor(0, 0, 0, 0, 0, 0, 0))
    if marked and viewport.metadata.cursor:
        var text = marked.value().as_bytes()
        ui.grid_ime(text.unsafe_ptr(), len(text))
    ui.grid_end()
    if opts.native_appearance:
        for overlay in viewport.metadata.overlays:
            var row = plan.live.display_row(overlay.row)
            var start = min(Int(overlay.start), visible_columns)
            var end = min(Int(overlay.end), visible_columns)
            if overlay.kind & 255 == 0 and row and start < end:
                var selected = cells.unsafe_offset(Int(overlay.row) * Int(viewport.columns) + start)
                _ = grid_selection(ui, key, row.value(), start, tables, selected, end - start)
    return metrics
