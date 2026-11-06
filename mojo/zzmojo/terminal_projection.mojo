from .terminal_model import (
    TerminalCell,
    TerminalStyle,
    TerminalViewport,
    KittyPlacement,
    GRAPHEME,
    valid_scalar,
)


@fieldwise_init
struct RowProjection(TrivialRegisterPassable):
    var source_start: UInt16
    var destination_start: UInt16
    var count: UInt16

    @staticmethod
    def new(
        viewport_rows: UInt16, grid_rows: UInt16, bottom_anchored: Bool
    ) -> Self:
        var count = min(viewport_rows, grid_rows)
        if bottom_anchored:
            return Self(viewport_rows - count, grid_rows - count, count)
        return Self(0, 0, count)

    def source_row(self, offset: UInt16) -> Optional[UInt16]:
        if offset < self.count:
            return UInt16(
                min(UInt32(self.source_start) + UInt32(offset), UInt32(65535))
            )
        return None

    def display_row(self, source_row: UInt16) -> Optional[Int]:
        if source_row < self.source_start:
            return None
        var offset = source_row - self.source_start
        if offset < self.count:
            return Int(
                min(
                    UInt32(self.destination_start) + UInt32(offset),
                    UInt32(65535),
                )
            )
        return None


@fieldwise_init
struct GridPlacement(TrivialRegisterPassable):
    var bottom_anchored: Bool
    var project_from_bottom: Bool


@fieldwise_init
struct LocalRowSource(TrivialRegisterPassable):
    var kind: UInt8
    var index: UInt64


@fieldwise_init
struct LocalColors(Copyable, Movable):
    var host: Array[UInt32, 18]
    var local: Array[UInt32, 18]

    def ground(
        self, color: UInt32, kind: UInt8, index: UInt8, default: Int
    ) -> UInt32:
        var rgb = color & 0xFFFFFF
        if kind == 1:
            return self.local[default] & 0xFFFFFF
        if kind == 2 and index < 16:
            return self.local[2 + Int(index)] & 0xFFFFFF
        if kind == 0:
            for offset in range(18):
                if self.host[offset] & 0xFFFFFF == rgb:
                    return self.local[offset] & 0xFFFFFF
        return rgb

    def recolor(self, style: TerminalStyle) -> TerminalStyle:
        var foreground = self.ground(
            style.foreground,
            UInt8((style.attributes >> 12) & 3),
            UInt8(style.foreground >> 24),
            0,
        )
        var background = self.ground(
            style.background,
            UInt8((style.attributes >> 14) & 3),
            style.background_index,
            1,
        )
        return TerminalStyle(
            (style.foreground & 0xFF000000) | foreground,
            background,
            style.underline_color,
            style.attributes,
            style.underline,
            style.background_index,
        )


def _has_glyph(viewport: TerminalViewport, cell: TerminalCell) -> Bool:
    var glyph = cell.glyph
    if glyph == 0:
        return False
    if glyph & GRAPHEME == 0:
        return valid_scalar(glyph)
    var index = Int(glyph & ~GRAPHEME)
    if index + 1 >= len(viewport.grapheme_offsets):
        return False
    var start = Int(viewport.grapheme_offsets[index])
    var end = Int(viewport.grapheme_offsets[index + 1])
    if start > end or end > len(viewport.grapheme_bytes):
        return False
    try:
        _ = String(from_utf8=Span(viewport.grapheme_bytes)[start:end])
        return True
    except:
        return False


def last_visible_row_has_content(
    viewport: TerminalViewport,
    visible_rows: UInt16,
    local: Optional[LocalColors] = None,
) -> Bool:
    var count = min(visible_rows, viewport.rows)
    var row = count - 1 if count > 0 else UInt16(0)
    if row >= viewport.rows:
        return False
    var start = Int(row) * Int(viewport.columns)
    var end = start + Int(viewport.columns)
    if end > len(viewport.cells):
        return False
    for index in range(start, end):
        var cell = viewport.cells[index]
        if _has_glyph(viewport, cell):
            return True
        if Int(cell.style) < len(viewport.styles):
            var style = viewport.styles[Int(cell.style)]
            var background = style.background & 0xFFFFFF
            if local:
                background = local.value().recolor(style).background & 0xFFFFFF
            if background != viewport.metadata.background & 0xFFFFFF:
                return True
    return False


def _add_u32(left: UInt32, right: UInt32) -> UInt32:
    return UInt32(min(UInt64(left) + UInt64(right), UInt64(4294967295)))


def grid_placement(
    viewport: TerminalViewport,
    command_output: Bool = False,
    scrolled_locally: Bool = False,
    local: Optional[LocalColors] = None,
) -> GridPlacement:
    var anchorable = viewport.metadata.mode.kind != 2 and not command_output
    var scrollbar = viewport.metadata.scrollbar
    var at_live_bottom = (
        _add_u32(scrollbar.offset, scrollbar.length) >= scrollbar.total
    )
    var scrolled_back = scrolled_locally or not at_live_bottom
    var last_row_filled = last_visible_row_has_content(
        viewport, viewport.rows, local
    )
    return GridPlacement(
        anchorable and (scrolled_back or last_row_filled),
        anchorable and not scrolled_back and last_row_filled,
    )


def local_row_source(
    absolute_row: UInt32,
    server_offset: UInt32,
    live_rows: UInt16,
    history_rows: UInt64,
    below_rows: UInt64,
) -> LocalRowSource:
    if absolute_row >= server_offset:
        var row = absolute_row - server_offset
        if row < UInt32(live_rows):
            return LocalRowSource(1, UInt64(row))
        var below = UInt64(row - UInt32(live_rows))
        if below < below_rows:
            return LocalRowSource(2, below)
        return LocalRowSource(3, 0)
    var position = UInt64(server_offset - absolute_row)
    if position <= history_rows:
        return LocalRowSource(0, history_rows - position)
    return LocalRowSource(3, 0)


def local_live_projection(
    target_offset: UInt32,
    server_offset: UInt32,
    viewport_rows: UInt16,
    grid_rows: UInt16,
) -> RowProjection:
    var display_count = UInt32(min(viewport_rows, grid_rows))
    var display_end = _add_u32(target_offset, display_count)
    var live_end = _add_u32(server_offset, UInt32(viewport_rows))
    var intersection_start = max(target_offset, server_offset)
    var intersection_end = min(display_end, live_end)
    if intersection_start >= intersection_end:
        return RowProjection(0, 0, 0)
    return RowProjection(
        UInt16(intersection_start - server_offset),
        UInt16(intersection_start - target_offset),
        UInt16(intersection_end - intersection_start),
    )


def peek_row_source(
    top_offset: UInt32,
    distance: UInt32,
    server_offset: UInt32,
    live_rows: UInt16,
    history_rows: UInt64,
) -> Optional[LocalRowSource]:
    if distance > top_offset:
        return None
    var source = local_row_source(
        top_offset - distance, server_offset, live_rows, history_rows, 0
    )
    if source.kind == 3:
        return None
    return source


def kitty_display_row(
    placement: KittyPlacement,
    local_scroll_target: Optional[UInt32],
    projection: RowProjection,
) -> Optional[Int64]:
    var anchor: Int64
    if local_scroll_target:
        var target = UInt64(local_scroll_target.value())
        if placement.absolute_row >= target:
            var difference = placement.absolute_row - target
            if difference > UInt64(Int64.MAX):
                return None
            anchor = Int64(difference)
        else:
            anchor = -Int64(target - placement.absolute_row)
    else:
        anchor = Int64(placement.viewport_row)
    if anchor > Int64.MAX - Int64(placement.grid_rows):
        return None
    var placement_end = anchor + Int64(placement.grid_rows)
    var projected_start = Int64(projection.source_start)
    var projected_end = projected_start + Int64(projection.count)
    var first_visible = max(anchor, projected_start)
    if (
        first_visible >= placement_end
        or first_visible >= projected_end
        or first_visible > 65535
    ):
        return None
    var display = projection.display_row(UInt16(first_visible))
    if not display:
        return None
    return Int64(display.value()) - (first_visible - anchor)


@fieldwise_init
struct ProjectionPlan(TrivialRegisterPassable):
    var placement: GridPlacement
    var rows: RowProjection
    var live: RowProjection


def projection_plan(
    viewport: TerminalViewport,
    grid_rows: UInt16,
    command_output: Bool = False,
    scrolled_locally: Bool = False,
    local_scroll_target: Optional[UInt32] = None,
    local: Optional[LocalColors] = None,
) -> ProjectionPlan:
    var placement = grid_placement(
        viewport,
        command_output,
        scrolled_locally or Bool(local_scroll_target),
        local,
    )
    var rows = RowProjection.new(
        viewport.rows, grid_rows, placement.project_from_bottom
    )
    var live = local_live_projection(
        local_scroll_target.value(),
        viewport.metadata.scrollbar.offset,
        viewport.rows,
        grid_rows,
    ) if local_scroll_target else rows
    return ProjectionPlan(placement, rows, live)
