from .protocol import validate_envelope
from .terminal_model import (
    ROWS,
    CURSOR_AT,
    SCROLL,
    SCROLLBAR,
    DICTIONARY,
    OVERLAYS,
    CURSOR,
    PRESENTATION,
    COLORS,
    MODE,
    SEARCH,
    UNSEEN,
    INPUT_MODES,
    STATUS,
    KITTY,
    ALL_FIELDS,
    FULL_FIELDS,
    MAX_STYLE_COUNT,
    MAX_GRAPHEME_COUNT,
    MAX_GRAPHEME_BYTES,
    MAX_GRID_CELLS,
    GRAPHEME,
    TerminalCell,
    TerminalStyle,
    TerminalOverlay,
    TerminalCursor,
    TerminalScrollbar,
    TerminalMode,
    TerminalSearch,
    KittyPlacement,
    TerminalMetadata,
    TerminalSpan,
    TerminalPatch,
    TerminalViewport,
    TerminalHistory,
    TerminalUpdate,
    valid_scalar,
    exposed_rows_are_replaced,
)


struct Reader(Movable):
    var data: List[UInt8]
    var offset: Int

    def __init__(out self, var data: List[UInt8], offset: Int = 8):
        self.data = data^
        self.offset = offset

    def preflight(self, count: Int, minimum: Int) raises:
        if (
            count < 0
            or minimum < 0
            or count > (len(self.data) - self.offset) // max(minimum, 1)
        ):
            raise Error("protocol frame is truncated")

    def byte(mut self) raises -> UInt8:
        if self.offset >= len(self.data):
            raise Error("protocol frame is truncated")
        var value = self.data[self.offset]
        self.offset += 1
        return value

    def boolean(mut self) raises -> Bool:
        var value = self.byte()
        if value > 1:
            raise Error("invalid boolean value")
        return value == 1

    def uint(mut self) raises -> UInt64:
        var value: UInt64 = 0
        for shift in range(0, 64, 7):
            var byte = self.byte()
            var bits = UInt64(byte & 127)
            if shift == 63 and bits > 1:
                raise Error("varint overflows u64")
            value |= bits << UInt64(shift)
            if byte & 128 == 0:
                return value
        raise Error("varint overflows u64")

    def zigzag(mut self) raises -> UInt64:
        var value = self.uint()
        return (value >> 1) ^ (UInt64(0) - (value & 1))

    def u16(mut self) raises -> UInt16:
        var value = self.uint()
        if value > 65535:
            raise Error("terminal value exceeds u16")
        return UInt16(value)

    def u32(mut self) raises -> UInt32:
        var value = self.uint()
        if value > UInt64(4294967295):
            raise Error("terminal value exceeds u32")
        return UInt32(value)

    def count(mut self, maximum: Int) raises -> Int:
        var count = self.uint()
        if count > UInt64(maximum):
            raise Error("terminal count exceeds its protocol limit")
        return Int(count)

    def little32(mut self) raises -> UInt32:
        var value: UInt32 = 0
        for shift in range(0, 32, 8):
            value |= UInt32(self.byte()) << UInt32(shift)
        return value

    def little64(mut self) raises -> UInt64:
        var value: UInt64 = 0
        for shift in range(0, 64, 8):
            value |= UInt64(self.byte()) << UInt64(shift)
        return value

    def rgb(mut self) raises -> UInt32:
        var value = UInt32(self.byte())
        value |= UInt32(self.byte()) << 8
        value |= UInt32(self.byte()) << 16
        return value

    def bytes(mut self, count: Int) raises -> List[UInt8]:
        self.preflight(count, 1)
        var result = List[UInt8](
            Span(self.data)[self.offset : self.offset + count]
        )
        self.offset += count
        return result^

    def string(mut self, maximum: Int) raises -> String:
        var count = self.count(maximum)
        self.preflight(count, 1)
        var result = String(
            from_utf8=Span(self.data)[self.offset : self.offset + count]
        )
        self.offset += count
        return result^

    def optional_string(mut self, maximum: Int) raises -> Optional[String]:
        var count = self.count(maximum + 1)
        if count == 0:
            return None
        count -= 1
        self.preflight(count, 1)
        var result = String(
            from_utf8=Span(self.data)[self.offset : self.offset + count]
        )
        self.offset += count
        return result^

    def scalar(mut self) raises -> UInt32:
        var first = self.byte()
        if first < 128:
            return UInt32(first)
        var length: Int
        if first >= 192 and first <= 223:
            length = 2
        elif first >= 224 and first <= 239:
            length = 3
        elif first >= 240 and first <= 247:
            length = 4
        else:
            raise Error("terminal glyph is not valid UTF-8")
        self.preflight(length - 1, 1)
        var text = String(
            from_utf8=Span(self.data)[
                self.offset - 1 : self.offset + length - 1
            ]
        )
        self.offset += length - 1
        for codepoint in text.codepoints():
            return UInt32(Int(codepoint))
        raise Error("terminal glyph is not valid UTF-8")

    def fields(mut self) raises -> UInt16:
        var fields = self.u16()
        if fields & ~ALL_FIELDS != 0:
            raise Error("terminal frame carries unknown fields")
        return fields

    def finish(self) raises:
        if self.offset != len(self.data):
            raise Error("terminal update has trailing bytes")


def _signed32(value: UInt32) -> Int32:
    return Int32(Int(value) - 4294967296) if value >= UInt32(
        2147483648
    ) else Int32(value)


def _signed64(value: UInt64) -> Int:
    return -1 - Int(UInt64.MAX - value) if value > UInt64(
        9223372036854775807
    ) else Int(value)


def _control(codepoint: UInt32) -> Bool:
    return codepoint < 32 or (codepoint >= 127 and codepoint <= 159)


def _space(codepoint: UInt32) -> Bool:
    return (
        (codepoint >= 9 and codepoint <= 13)
        or codepoint == 32
        or codepoint == 133
        or codepoint == 160
        or codepoint == 5760
        or (codepoint >= 8192 and codepoint <= 8202)
        or codepoint == 8232
        or codepoint == 8233
        or codepoint == 8239
        or codepoint == 8287
        or codepoint == 12288
    )


def _cursor(cursor: TerminalCursor, columns: UInt16, rows: UInt16) raises:
    if cursor.row >= rows or cursor.column >= columns:
        raise Error("cursor is outside the viewport")


def _scrollbar(scrollbar: TerminalScrollbar) raises:
    var end = min(
        UInt64(scrollbar.offset) + UInt64(scrollbar.length), UInt64(4294967295)
    )
    if (
        scrollbar.offset > scrollbar.total
        or scrollbar.length > scrollbar.total
        or end > UInt64(scrollbar.total)
    ):
        raise Error("scrollbar range is inconsistent")


def _placements(
    placements: List[KittyPlacement],
    columns: UInt16,
    rows: UInt16,
    scrollbar: TerminalScrollbar,
) raises:
    for placement in placements:
        if (
            placement.image_id == 0
            or placement.image_generation == 0
            or placement.grid_columns == 0
            or placement.grid_rows == 0
            or placement.pixel_width == 0
            or placement.pixel_height == 0
        ):
            raise Error("kitty placement has empty image or geometry metadata")
        var right = Int(placement.viewport_column) + Int(placement.grid_columns)
        var bottom = Int(placement.viewport_row) + Int(placement.grid_rows)
        if (
            right <= 0
            or Int(placement.viewport_column) >= Int(columns)
            or bottom <= 0
            or Int(placement.viewport_row) >= Int(rows)
        ):
            raise Error("kitty placement is outside the viewport")
        var expected = Int(scrollbar.offset) + Int(placement.viewport_row)
        if placement.absolute_row != UInt64(max(expected, 0)):
            raise Error("kitty placement absolute row is inconsistent")
        if placement.source_rect:
            var rect = placement.source_rect.value()
            if (
                rect[2] == 0
                or rect[3] == 0
                or UInt64(rect[0]) + UInt64(rect[2]) > UInt64(4294967295)
                or UInt64(rect[1]) + UInt64(rect[3]) > UInt64(4294967295)
            ):
                raise Error("kitty placement source rectangle is invalid")


@inline(.never)
def _metadata(
    mut reader: Reader, fields: UInt16, columns: UInt16, rows: UInt16
) raises -> TerminalMetadata:
    var metadata = TerminalMetadata()
    if fields & CURSOR_AT != 0:
        var column = reader.uint()
        if column >> 1 > 65535:
            raise Error("cursor is outside the viewport")
        var cursor = TerminalCursor(
            0, UInt16(column >> 1), reader.u16(), 0, 0, 0, UInt8(column & 1)
        )
        _cursor(cursor, columns, rows)
        metadata.cursor = cursor
    if fields & SCROLLBAR != 0:
        metadata.scrollbar = TerminalScrollbar(
            reader.u32(), reader.u32(), reader.u32()
        )
        _scrollbar(metadata.scrollbar)
    if fields & OVERLAYS != 0:
        var count = reader.count(1024 * 1024)
        reader.preflight(count, 4)
        for _ in range(count):
            var overlay = TerminalOverlay(
                reader.u16(), reader.u16(), reader.u16(), reader.u16()
            )
            if (
                overlay.row >= rows
                or overlay.start > overlay.end
                or overlay.end > columns
            ):
                raise Error("overlay span is outside the viewport")
            if overlay.kind & 255 > 4:
                raise Error("overlay span has an unknown kind")
            metadata.overlays.append(overlay)
    if fields & CURSOR != 0:
        var flags = reader.byte()
        if flags & 1 == 0:
            if flags != 0:
                raise Error("cursor flags contain reserved bits")
        else:
            if flags >> 4 > 3:
                raise Error("unknown cursor style")
            var column = reader.u16()
            var row = reader.u16()
            var cursor = TerminalCursor(
                reader.rgb(),
                column,
                row,
                flags >> 4,
                UInt8((flags & 2) != 0),
                UInt8((flags & 4) != 0),
                UInt8((flags & 8) != 0),
            )
            _cursor(cursor, columns, rows)
            metadata.cursor = cursor
    if fields & PRESENTATION != 0:
        metadata.title = reader.string(64 * 1024)
        metadata.working_directory = reader.optional_string(16 * 1024)
        metadata.hovered_uri = reader.optional_string(16 * 1024)
        if metadata.working_directory:
            for codepoint in metadata.working_directory.value().codepoints():
                if _control(UInt32(Int(codepoint))):
                    raise Error("working directory is invalid")
        if metadata.hovered_uri:
            for codepoint in metadata.hovered_uri.value().codepoints():
                var code = UInt32(Int(codepoint))
                if _control(code) or _space(code):
                    raise Error("hovered URI is invalid")
    if fields & COLORS != 0:
        metadata.foreground = reader.rgb()
        metadata.background = reader.rgb()
    if fields & MODE != 0:
        var kind = reader.byte()
        if kind > 2:
            raise Error("unknown terminal interaction mode")
        var position: UInt32 = 0
        var total: UInt32 = 0
        var hide = False
        if kind > 0:
            position = reader.u32()
            total = reader.u32()
            if position == 0 or total == 0 or position > total:
                raise Error("copy-mode position is inconsistent")
            if kind == 1:
                hide = reader.boolean()
        metadata.mode = TerminalMode(kind, position, total, hide)
    if fields & SEARCH != 0:
        var flags = reader.byte()
        if flags & 248 != 0 or (flags & 1 == 0 and flags != 0):
            raise Error("unknown search status flags")
        if flags & 1 != 0:
            var current = min(reader.u32(), UInt32((1 << 29) - 1))
            var total = reader.u32()
            if current > total:
                raise Error("search status is inconsistent")
            metadata.search = TerminalSearch(
                current, total, flags & 2 != 0, flags & 4 != 0
            )
    if fields & UNSEEN != 0:
        metadata.unseen_output = reader.u32()
    if fields & INPUT_MODES != 0:
        var flags = reader.byte()
        if flags & 252 != 0:
            raise Error("unknown terminal input mode flags")
        metadata.kitty_keyboard = flags & 1 != 0
        metadata.mouse_tracking = flags & 2 != 0
    if fields & STATUS != 0:
        metadata.status = reader.byte()
        if metadata.status == 2:
            metadata.exit_code = reader.u32()
            metadata.exit_signal = reader.optional_string(1024 * 1024)
        elif metadata.status == 3:
            metadata.failure = reader.string(1024 * 1024)
        elif metadata.status > 3:
            raise Error("unknown terminal status")
    if fields & KITTY != 0:
        var count = reader.count(65536)
        reader.preflight(count, 72)
        for _ in range(count):
            var image = reader.little32()
            var generation = reader.little64()
            var layer = reader.byte()
            if layer > 2:
                raise Error("kitty placement has an unknown paint layer")
            var has_source = reader.boolean()
            if reader.byte() != 0 or reader.byte() != 0:
                raise Error("kitty placement reserved field is nonzero")
            var column = _signed32(reader.little32())
            var row = _signed32(reader.little32())
            var absolute = reader.little64()
            var x = reader.little32()
            var y = reader.little32()
            var grid_columns = reader.little32()
            var grid_rows = reader.little32()
            var width = reader.little32()
            var height = reader.little32()
            var source = SIMD[.uint32, 4](
                reader.little32(),
                reader.little32(),
                reader.little32(),
                reader.little32(),
            )
            var rect: Optional[SIMD[.uint32, 4]] = None
            if has_source:
                rect = source
            elif source.reduce_or() != 0:
                raise Error("kitty placement empty source rectangle is nonzero")
            metadata.kitty.append(
                KittyPlacement(
                    image,
                    generation,
                    layer,
                    column,
                    row,
                    absolute,
                    x,
                    y,
                    grid_columns,
                    grid_rows,
                    width,
                    height,
                    rect,
                )
            )
        _placements(metadata.kitty, columns, rows, metadata.scrollbar)
    return metadata^


@inline(.never)
def _styles(mut reader: Reader) raises -> List[TerminalStyle]:
    var count = reader.count(MAX_STYLE_COUNT)
    reader.preflight(count, 7)
    var styles = List[TerminalStyle](capacity=count)
    for _ in range(count):
        var flags = reader.byte()
        if flags & 192 != 0:
            raise Error("packed style flags contain reserved bits")
        var underline = (flags >> 1) & 7
        if underline > 5:
            raise Error("style contains an invalid packed value")
        var foreground = reader.rgb()
        var background = reader.rgb()
        var underline_color = reader.rgb() if flags & 1 != 0 else UInt32(
            4294967295
        )
        var attributes = reader.u16() if flags & 16 != 0 else UInt16(0)
        var classes = reader.u32() if flags & 32 != 0 else UInt32(0)
        if attributes & 61440 != 0:
            raise Error("style attributes contain reserved bits")
        var foreground_index = (classes >> 8) & 255
        var background_index = (classes >> 16) & 255
        if (
            classes & 0xFF0000F0 != 0
            or (classes & 3 != 2 and foreground_index != 0)
            or ((classes >> 2) & 3 != 2 and background_index != 0)
        ):
            raise Error("packed style colour classes are invalid")
        foreground |= foreground_index << 24
        attributes |= UInt16(classes & 15) << 12
        styles.append(
            TerminalStyle(
                foreground,
                background,
                underline_color,
                attributes,
                underline,
                UInt8(background_index),
            )
        )
    return styles^


@inline(.never)
def _graphemes(
    mut reader: Reader,
    mut lengths: List[UInt32],
    mut bytes: List[UInt8],
    full: Bool,
) raises:
    var count = reader.count(MAX_GRAPHEME_COUNT)
    reader.preflight(count, 1)
    var total = 0
    var sizes = List[Int](capacity=count)
    lengths.clear()
    if full:
        lengths.append(0)
    for _ in range(count):
        var length = reader.count(MAX_GRAPHEME_BYTES)
        if length > MAX_GRAPHEME_BYTES - total:
            raise Error("grapheme arena exceeds limit")
        total += length
        sizes.append(length)
        lengths.append(UInt32(total if full else length))
    bytes = reader.bytes(total)
    var cursor = 0
    for length in sizes:
        _ = String(from_utf8=Span(bytes)[cursor : cursor + length])
        cursor += length


def _raw_cell(
    mut reader: Reader, style: UInt16, graphemes: Int
) raises -> TerminalCell:
    var code = reader.uint()
    var glyph: UInt32
    if code & 1 != 0:
        if code >> 1 >= UInt64(graphemes):
            raise Error("cell references a missing grapheme")
        glyph = GRAPHEME | UInt32(code >> 1)
    else:
        if code >> 1 > UInt64(4294967295) or not valid_scalar(
            UInt32(code >> 1)
        ):
            raise Error("cell contains an invalid Unicode scalar")
        glyph = UInt32(code >> 1)
    return TerminalCell(glyph, style, reader.u16())


@inline(.never)
def _runs(
    mut reader: Reader,
    room: Int,
    styles: Int,
    graphemes: Int,
    mut cells: List[TerminalCell],
) raises:
    var first = len(cells)
    var style: UInt16 = 0
    while True:
        var header = reader.uint()
        if header == 0:
            return
        var count64 = header >> 3
        if count64 == 0:
            raise Error("terminal run is empty")
        if header & 4 != 0:
            style = reader.u16()
            if Int(style) >= styles:
                raise Error("cell references a missing style")
        var kind = header & 3
        var width64 = count64 * (UInt64(2) if kind == 1 else UInt64(1))
        if count64 > UInt64(room) or width64 > UInt64(
            room - (len(cells) - first)
        ):
            raise Error("terminal run overflows its row")
        var count = Int(count64)
        if kind == 0 or kind == 1:
            for _ in range(count):
                var glyph = reader.scalar()
                if kind == 1:
                    if glyph == 0:
                        raise Error("wide terminal cell has no glyph")
                    cells.append(TerminalCell(glyph, style, 1))
                    cells.append(TerminalCell(0, style, 2))
                else:
                    cells.append(TerminalCell(glyph, style, 0))
        elif kind == 2:
            var cell = _raw_cell(reader, style, graphemes)
            for _ in range(count):
                cells.append(cell)
        else:
            for _ in range(count):
                cells.append(_raw_cell(reader, style, graphemes))


@inline(.never)
def _full_rows(
    mut reader: Reader,
    columns: UInt16,
    rows: UInt16,
    styles: Int,
    graphemes: Int,
) raises -> List[TerminalCell]:
    var count = Int(columns) * Int(rows)
    if count > MAX_GRID_CELLS:
        raise Error("terminal grid exceeds its cell limit")
    var cells = List[TerminalCell](length=count, fill=TerminalCell(0, 0, 0))
    var next = 0
    while True:
        var step = reader.uint()
        if step == 0:
            return cells^
        if step - 1 >= UInt64(Int(rows) - next):
            raise Error("terminal row is outside the viewport")
        var row = next + Int(step - 1)
        var head = reader.uint()
        if head & 1 != 0:
            raise Error("full terminal row clears its tail")
        if head >> 1 >= UInt64(columns):
            raise Error("terminal row starts outside the viewport")
        var start = Int(head >> 1)
        var run = List[TerminalCell]()
        _runs(reader, Int(columns) - start, styles, graphemes, run)
        if len(run) == 0:
            raise Error("full terminal row carries no cells")
        for index in range(len(run)):
            cells[row * Int(columns) + start + index] = run[index]
        next = row + 1


@inline(.never)
def decode_viewport_body(mut reader: Reader) raises -> TerminalViewport:
    var viewport = TerminalViewport()
    viewport.view_generation = reader.uint()
    viewport.generation = viewport.view_generation - reader.zigzag()
    viewport.dictionary_generation = reader.u32()
    viewport.columns = reader.u16()
    viewport.rows = reader.u16()
    var fields = reader.fields()
    if fields & ~FULL_FIELDS != 0:
        raise Error("full viewport carries patch-only fields")
    viewport.metadata = _metadata(
        reader, fields, viewport.columns, viewport.rows
    )
    viewport.styles = _styles(reader)
    if len(viewport.styles) == 0:
        raise Error("style dictionary is empty")
    _graphemes(reader, viewport.grapheme_offsets, viewport.grapheme_bytes, True)
    viewport.cells = _full_rows(
        reader,
        viewport.columns,
        viewport.rows,
        len(viewport.styles),
        len(viewport.grapheme_offsets) - 1,
    )
    return viewport^


@inline(.never)
def _patch(mut reader: Reader) raises -> TerminalPatch:
    var patch = TerminalPatch()
    patch.base_view_generation = reader.uint()
    patch.base_generation = patch.base_view_generation - reader.zigzag()
    patch.view_generation = patch.base_view_generation + reader.zigzag()
    patch.generation = patch.base_generation + reader.zigzag()
    patch.dictionary_generation = reader.u32()
    patch.columns = reader.u16()
    patch.rows = reader.u16()
    if Int(patch.columns) * Int(patch.rows) > MAX_GRID_CELLS:
        raise Error("terminal grid exceeds its cell limit")
    patch.fields = reader.fields()
    if patch.fields & CURSOR != 0 and patch.fields & CURSOR_AT != 0:
        raise Error("terminal patch moves and replaces the cursor")
    if patch.fields & KITTY != 0 and patch.fields & SCROLLBAR == 0:
        raise Error("terminal patch places images without a scrollbar")
    if patch.fields & SCROLL != 0:
        patch.scroll = _signed64(reader.zigzag())
        if (
            patch.scroll < -32768
            or patch.scroll > 32767
            or patch.scroll == 0
            or abs(patch.scroll) >= Int(patch.rows)
        ):
            raise Error("terminal row shift is outside the viewport")
    patch.metadata = _metadata(reader, patch.fields, patch.columns, patch.rows)
    var styles = MAX_STYLE_COUNT
    var graphemes = MAX_GRAPHEME_COUNT
    if patch.fields & DICTIONARY != 0:
        patch.style_base = reader.u32()
        patch.styles = _styles(reader)
        patch.grapheme_base = reader.u32()
        _graphemes(reader, patch.grapheme_lengths, patch.grapheme_bytes, False)
        if (
            len(patch.styles) == 0
            and len(patch.grapheme_lengths) == 0
            and len(patch.grapheme_bytes) == 0
        ):
            raise Error("terminal patch dictionary append is empty")
        styles = Int(patch.style_base) + len(patch.styles)
        graphemes = Int(patch.grapheme_base) + len(patch.grapheme_lengths)
        if styles > MAX_STYLE_COUNT or graphemes > MAX_GRAPHEME_COUNT:
            raise Error("terminal patch dictionary exceeds its protocol limit")
    if patch.fields & ROWS != 0:
        var next = 0
        while True:
            var step = reader.uint()
            if step == 0:
                break
            if step - 1 >= UInt64(Int(patch.rows) - next):
                raise Error("terminal patch row is outside the viewport")
            var row = next + Int(step - 1)
            var head = reader.uint()
            if head >> 1 > UInt64(patch.columns):
                raise Error("terminal patch span starts outside the viewport")
            var start = Int(head >> 1)
            var before = len(patch.cells)
            _runs(
                reader,
                Int(patch.columns) - start,
                styles,
                graphemes,
                patch.cells,
            )
            patch.spans.append(
                TerminalSpan(
                    UInt16(row),
                    UInt16(start),
                    UInt16(len(patch.cells) - before),
                    head & 1 != 0,
                )
            )
            next = row + 1
        if len(patch.spans) == 0:
            raise Error("terminal patch rows section is empty")
    if not exposed_rows_are_replaced(
        patch.spans, patch.scroll, patch.rows, patch.columns
    ):
        raise Error("terminal patch does not replace newly exposed rows")
    return patch^


@inline(.never)
def _history(mut reader: Reader) raises -> TerminalHistory:
    var history = TerminalHistory()
    history.start = reader.u32()
    history.total = reader.u32()
    history.offset = reader.u32()
    history.columns = reader.u16()
    history.rows = UInt16(reader.count(512))
    history.styles = _styles(reader)
    _graphemes(reader, history.grapheme_offsets, history.grapheme_bytes, True)
    history.cells = _full_rows(
        reader,
        history.columns,
        history.rows,
        len(history.styles),
        len(history.grapheme_offsets) - 1,
    )
    return history^


@inline(.never)
def decode_terminal_frame(var frame: List[UInt8]) raises -> TerminalUpdate:
    if validate_envelope(Span(frame)) != 1:
        raise Error("terminal update is not on the terminal lane")
    var reader = Reader(frame^)
    var update = TerminalUpdate()
    update.kind = reader.byte()
    if update.kind > 3:
        raise Error("unknown terminal update type")
    update.pane = reader.uint()
    update.sequence = reader.uint()
    if update.kind == 0 or update.kind == 2:
        if update.kind == 2:
            var output = reader.uint()
            if output == 0:
                raise Error("command output viewport has a zero output ID")
            update.output_id = output
        update.viewport = decode_viewport_body(reader)
    elif update.kind == 1:
        update.patch = _patch(reader)
    else:
        update.history = _history(reader)
    reader.finish()
    return update^


@inline(.never)
def decode_viewport_bytes(var bytes: List[UInt8]) raises -> TerminalViewport:
    var reader = Reader(bytes^, 0)
    var viewport = decode_viewport_body(reader)
    reader.finish()
    return viewport^
