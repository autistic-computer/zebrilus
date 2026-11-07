from std.memory import ArcPointer
from std.math import floor
from mjui.term.paint import grid_selection
from mjui.ui import Ui
from mjui.term.grid import (
    GridCell,
    CellStyle,
    GridTables,
    GridCursor,
    GridOverlay,
    TermOpts,
    CellMetrics,
    cell_metrics_options,
)
from .terminal_model import TerminalViewport
from .terminal_history import HistorySnapshot, HistoryRow
from .terminal_projection import local_row_source, projection_plan, _add_u32
from .terminal_painter import render_viewport_opts, selection_flags
from .terminal_composition import composition_cursor


def _row_tables(row: HistoryRow, viewport: TerminalViewport) -> GridTables:
    ref dictionary = row.dictionary[]
    return GridTables(
        dictionary.styles[]
        .values.unsafe_ptr()
        .unsafe_bitcast[CellStyle]()
        .unsafe_mut_cast[False]()
        .unsafe_origin_cast[ImmUntrackedOrigin](),
        len(dictionary.styles[].values),
        dictionary.graphemes[]
        .offsets.unsafe_ptr()
        .unsafe_mut_cast[False]()
        .unsafe_origin_cast[ImmUntrackedOrigin](),
        dictionary.graphemes[]
        .bytes.unsafe_ptr()
        .unsafe_mut_cast[False]()
        .unsafe_origin_cast[ImmUntrackedOrigin](),
        max(0, len(dictionary.graphemes[].offsets) - 1),
        viewport.metadata.foreground,
        viewport.metadata.background,
        0,
    )


def render_retained_viewport(
    mut ui: Ui,
    key: UInt64,
    viewport: TerminalViewport,
    snapshot: ArcPointer[HistorySnapshot],
    target: Optional[UInt32],
    sub_row: Float32,
    font: Int32,
    size: Float32,
    focused: Bool,
    columns: Int,
    rows: Int,
    marked: Optional[String] = None,
) -> CellMetrics:
    return render_retained_viewport_opts(ui, key, viewport, snapshot, target, sub_row, TermOpts(font, size, focused=focused, native_colors=True), columns, rows, marked)


def render_retained_viewport_opts(
    mut ui: Ui,
    key: UInt64,
    viewport: TerminalViewport,
    snapshot: ArcPointer[HistorySnapshot],
    target: Optional[UInt32],
    sub_row: Float32,
    opts: TermOpts,
    columns: Int,
    rows: Int,
    marked: Optional[String] = None,
) -> CellMetrics:
    ref history = snapshot[]
    if (
        history.generation != viewport.generation
        or history.view_generation != viewport.view_generation
        or history.columns != viewport.columns
        or history.rows != viewport.rows
        or (not target and sub_row <= 0)
    ):
        return render_viewport_opts(ui, key, viewport, opts, columns, rows, marked=marked)
    var metrics = cell_metrics_options(ui.shaper.fonts[Int(opts.font)], opts, ui.scale)
    var peek = Int(sub_row > 0)
    var pixel_offset = Float32(
        metrics.h
    ) - sub_row * ui.scale if peek != 0 else Float32(0)
    if opts.native_appearance and peek != 0:
        var device = min(max(sub_row * ui.scale, Float32(0)), Float32(metrics.h))
        var lower = floor(device)
        var shift = Int(lower) + (1 if device - lower >= Float32(0.5) else 0)
        pixel_offset = Float32(metrics.h - shift)
    var options = opts
    options.pixel_offset = pixel_offset
    var styles = (
        viewport.styles.unsafe_ptr()
        .unsafe_bitcast[CellStyle]()
        .unsafe_mut_cast[False]()
        .unsafe_origin_cast[ImmUntrackedOrigin]()
    )
    var live = GridTables(
        styles,
        len(viewport.styles),
        viewport.grapheme_offsets.unsafe_ptr()
        .unsafe_mut_cast[False]()
        .unsafe_origin_cast[ImmUntrackedOrigin](),
        viewport.grapheme_bytes.unsafe_ptr()
        .unsafe_mut_cast[False]()
        .unsafe_origin_cast[ImmUntrackedOrigin](),
        max(0, len(viewport.grapheme_offsets) - 1),
        viewport.metadata.foreground,
        viewport.metadata.background,
        0,
    )
    var plan = projection_plan(viewport, UInt16(rows), False, True, target)
    var top = target.or_else(viewport.metadata.scrollbar.offset)
    var visible_columns = min(columns, Int(viewport.columns))
    metrics = ui.grid_begin(key, columns, rows, live, options)
    if opts.native_appearance:
        var cursor = composition_cursor(viewport, plan.live, visible_columns, peek)
        ui.grid_cursor(cursor.or_else(GridCursor(0, 0, 0, 0, 0, 0, 0)))
        if cursor and marked:
            var text = marked.value().as_bytes()
            ui.grid_ime(text.unsafe_ptr(), len(text))
    for offset in range(Int(plan.rows.count) + peek):
        if offset == 0 and peek != 0 and top == 0:
            continue
        var absolute = (
            _add_u32(top, UInt32(offset - peek)) if offset >= peek else top - 1
        )
        var source = local_row_source(
            absolute,
            viewport.metadata.scrollbar.offset,
            viewport.rows,
            UInt64(len(history.history)),
            UInt64(len(history.below)),
        )
        if source.kind == 1:
            ui.term.views[ui.term.cur].tables = live
            var cells = (
                viewport.cells.unsafe_ptr()
                .unsafe_offset(Int(source.index) * Int(viewport.columns))
                .unsafe_bitcast[GridCell]()
                .unsafe_mut_cast[False]()
                .unsafe_origin_cast[ImmUntrackedOrigin]()
            )
            ui.grid_row(
                offset,
                cells,
                visible_columns,
                history.row_revisions[Int(source.index)],
            )
        elif source.kind == 0 or source.kind == 2:
            var row = (
                history.history[Int(source.index)].copy() if source.kind
                == 0 else history.below[Int(source.index)].copy()
            )
            ui.term.views[ui.term.cur].tables = _row_tables(row, viewport)
            var cells = (
                row.cells[]
                .values.unsafe_ptr()
                .unsafe_bitcast[GridCell]()
                .unsafe_mut_cast[False]()
                .unsafe_origin_cast[ImmUntrackedOrigin]()
            )
            ui.grid_row(
                offset,
                cells,
                min(visible_columns, len(row.cells[].values)),
                row.revision,
            )
    ui.term.views[ui.term.cur].tables = live
    var overlays = List[GridOverlay]()
    var flags = selection_flags(viewport, opts)
    for index in range(len(viewport.metadata.overlays)):
        var overlay = viewport.metadata.overlays[index]
        var row = plan.live.display_row(overlay.row)
        var end = min(Int(overlay.end), visible_columns)
        if row and Int(overlay.start) < end:
            overlays.append(
                GridOverlay(
                    UInt16(row.value() + peek),
                    overlay.start,
                    UInt16(end),
                    flags[index] if len(flags) > 0 else overlay.kind,
                )
            )
    ui.grid_overlays(
        overlays.unsafe_ptr()
        .unsafe_mut_cast[False]()
        .unsafe_mut_cast[False]()
        .unsafe_origin_cast[ImmUntrackedOrigin](),
        len(overlays),
    )
    _ = overlays
    if not opts.native_appearance:
        var cursor = GridCursor(0, 0, 0, 0, 0, 0, 0)
        if viewport.metadata.cursor:
            var source = viewport.metadata.cursor.value()
            var row = plan.live.display_row(source.row)
            if row and Int(source.column) < visible_columns:
                source.row = UInt16(row.value() + peek)
                cursor = Pointer(to=source).unsafe_bitcast[GridCursor]()[
                    unsafe_offset=0
                ]
        ui.grid_cursor(cursor)
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
                var cells = viewport.cells.unsafe_ptr().unsafe_offset(Int(overlay.row) * Int(viewport.columns) + start).unsafe_bitcast[GridCell]().unsafe_mut_cast[False]().unsafe_origin_cast[ImmUntrackedOrigin]()
                _ = grid_selection(ui, key, row.value() + peek, start, live, cells, end - start)
    return metrics
