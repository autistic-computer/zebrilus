comptime ROWS: UInt16 = 1 << 0
comptime CURSOR_AT: UInt16 = 1 << 1
comptime SCROLL: UInt16 = 1 << 2
comptime SCROLLBAR: UInt16 = 1 << 3
comptime DICTIONARY: UInt16 = 1 << 4
comptime OVERLAYS: UInt16 = 1 << 5
comptime CURSOR: UInt16 = 1 << 6
comptime PRESENTATION: UInt16 = 1 << 7
comptime COLORS: UInt16 = 1 << 8
comptime MODE: UInt16 = 1 << 9
comptime SEARCH: UInt16 = 1 << 10
comptime UNSEEN: UInt16 = 1 << 11
comptime INPUT_MODES: UInt16 = 1 << 12
comptime STATUS: UInt16 = 1 << 13
comptime KITTY: UInt16 = 1 << 14
comptime ALL_FIELDS: UInt16 = (1 << 15) - 1
comptime FULL_FIELDS: UInt16 = ALL_FIELDS & ~(
    ROWS | CURSOR_AT | SCROLL | DICTIONARY
)
comptime MAX_STYLE_COUNT: Int = 65536
comptime MAX_GRAPHEME_COUNT: Int = 1024 * 1024
comptime MAX_GRAPHEME_BYTES: Int = 16 * 1024 * 1024
comptime MAX_GRID_CELLS: Int = 8 * 1024 * 1024
comptime GRAPHEME: UInt32 = 1 << 31


@fieldwise_init
struct TerminalCell(TrivialRegisterPassable):
    var glyph: UInt32
    var style: UInt16
    var flags: UInt16


@fieldwise_init
struct TerminalStyle(TrivialRegisterPassable):
    var foreground: UInt32
    var background: UInt32
    var underline_color: UInt32
    var attributes: UInt16
    var underline: UInt8
    var background_index: UInt8


@fieldwise_init
struct TerminalOverlay(TrivialRegisterPassable):
    var row: UInt16
    var start: UInt16
    var end: UInt16
    var kind: UInt16


@fieldwise_init
struct TerminalCursor(TrivialRegisterPassable):
    var color: UInt32
    var column: UInt16
    var row: UInt16
    var style: UInt8
    var visible: UInt8
    var blinking: UInt8
    var wide_tail: UInt8


@fieldwise_init
struct TerminalScrollbar(TrivialRegisterPassable):
    var total: UInt32
    var offset: UInt32
    var length: UInt32


@fieldwise_init
struct TerminalMode(TrivialRegisterPassable):
    var kind: UInt8
    var position: UInt32
    var total: UInt32
    var hide_position: Bool


@fieldwise_init
struct TerminalSearch(TrivialRegisterPassable):
    var current: UInt32
    var total: UInt32
    var pending: Bool
    var invalid_pattern: Bool


@fieldwise_init
struct KittyPlacement(Copyable, Movable):
    var image_id: UInt32
    var image_generation: UInt64
    var layer: UInt8
    var viewport_column: Int32
    var viewport_row: Int32
    var absolute_row: UInt64
    var cell_offset_x: UInt32
    var cell_offset_y: UInt32
    var grid_columns: UInt32
    var grid_rows: UInt32
    var pixel_width: UInt32
    var pixel_height: UInt32
    var source_rect: Optional[SIMD[.uint32, 4]]


struct TerminalMetadata(Copyable, Movable):
    var foreground: UInt32
    var background: UInt32
    var title: String
    var working_directory: Optional[String]
    var hovered_uri: Optional[String]
    var overlays: List[TerminalOverlay]
    var kitty: List[KittyPlacement]
    var cursor: Optional[TerminalCursor]
    var scrollbar: TerminalScrollbar
    var mode: TerminalMode
    var search: Optional[TerminalSearch]
    var unseen_output: UInt32
    var kitty_keyboard: Bool
    var mouse_tracking: Bool
    var status: UInt8
    var exit_code: UInt32
    var exit_signal: Optional[String]
    var failure: String

    def __init__(out self):
        self.foreground = 0
        self.background = 0
        self.title = String()
        self.working_directory = None
        self.hovered_uri = None
        self.overlays = List[TerminalOverlay]()
        self.kitty = List[KittyPlacement]()
        self.cursor = None
        self.scrollbar = TerminalScrollbar(0, 0, 0)
        self.mode = TerminalMode(0, 0, 0, False)
        self.search = None
        self.unseen_output = 0
        self.kitty_keyboard = False
        self.mouse_tracking = False
        self.status = 0
        self.exit_code = 0
        self.exit_signal = None
        self.failure = String()

    def merge(mut self, next: Self, fields: UInt16) raises:
        if fields & CURSOR_AT != 0:
            if not self.cursor or not next.cursor:
                raise Error("cursor movement has no existing cursor")
            var cursor = self.cursor.value()
            cursor.column = next.cursor.value().column
            cursor.row = next.cursor.value().row
            cursor.wide_tail = next.cursor.value().wide_tail
            self.cursor = cursor
        if fields & CURSOR != 0:
            self.cursor = next.cursor
        if fields & SCROLLBAR != 0:
            self.scrollbar = next.scrollbar
        if fields & OVERLAYS != 0:
            self.overlays = next.overlays.copy()
        if fields & KITTY != 0:
            self.kitty = next.kitty.copy()
        if fields & PRESENTATION != 0:
            self.title = next.title.copy()
            self.working_directory = next.working_directory.copy()
            self.hovered_uri = next.hovered_uri.copy()
        if fields & COLORS != 0:
            self.foreground = next.foreground
            self.background = next.background
        if fields & MODE != 0:
            self.mode = next.mode
        if fields & SEARCH != 0:
            self.search = next.search
        if fields & UNSEEN != 0:
            self.unseen_output = next.unseen_output
        if fields & INPUT_MODES != 0:
            self.kitty_keyboard = next.kitty_keyboard
            self.mouse_tracking = next.mouse_tracking
        if fields & STATUS != 0:
            self.status = next.status
            self.exit_code = next.exit_code
            self.exit_signal = next.exit_signal.copy()
            self.failure = next.failure.copy()


@fieldwise_init
struct TerminalSpan(TrivialRegisterPassable):
    var row: UInt16
    var start: UInt16
    var length: UInt16
    var clear: Bool


struct TerminalPatch(Copyable, Movable):
    var base_generation: UInt64
    var base_view_generation: UInt64
    var generation: UInt64
    var view_generation: UInt64
    var dictionary_generation: UInt32
    var columns: UInt16
    var rows: UInt16
    var fields: UInt16
    var scroll: Int
    var metadata: TerminalMetadata
    var style_base: UInt32
    var styles: List[TerminalStyle]
    var grapheme_base: UInt32
    var grapheme_lengths: List[UInt32]
    var grapheme_bytes: List[UInt8]
    var spans: List[TerminalSpan]
    var cells: List[TerminalCell]

    def __init__(out self):
        self.base_generation = 0
        self.base_view_generation = 0
        self.generation = 0
        self.view_generation = 0
        self.dictionary_generation = 0
        self.columns = 0
        self.rows = 0
        self.fields = 0
        self.scroll = 0
        self.metadata = TerminalMetadata()
        self.style_base = 0
        self.styles = List[TerminalStyle]()
        self.grapheme_base = 0
        self.grapheme_lengths = List[UInt32]()
        self.grapheme_bytes = List[UInt8]()
        self.spans = List[TerminalSpan]()
        self.cells = List[TerminalCell]()


def valid_scalar(value: UInt32) -> Bool:
    return value <= 0x10FFFF and not (value >= 0xD800 and value <= 0xDFFF)


def exposed_rows_are_replaced(
    spans: List[TerminalSpan], scroll: Int, rows: UInt16, columns: UInt16
) -> Bool:
    var exposed = abs(scroll)
    if exposed == 0:
        return True
    if exposed > Int(rows) or len(spans) < exposed:
        return False
    var first_span = 0 if scroll > 0 else len(spans) - exposed
    var first_row = 0 if scroll > 0 else Int(rows) - exposed
    for offset in range(exposed):
        var span = spans[first_span + offset]
        if (
            Int(span.row) != first_row + offset
            or span.start != 0
            or (span.length != columns and not span.clear)
        ):
            return False
    return True


struct TerminalViewport(Copyable, Movable):
    var generation: UInt64
    var view_generation: UInt64
    var dictionary_generation: UInt32
    var columns: UInt16
    var rows: UInt16
    var metadata: TerminalMetadata
    var cells: List[TerminalCell]
    var styles: List[TerminalStyle]
    var grapheme_offsets: List[UInt32]
    var grapheme_bytes: List[UInt8]

    def __init__(out self):
        self.generation = 0
        self.view_generation = 0
        self.dictionary_generation = 0
        self.columns = 0
        self.rows = 0
        self.metadata = TerminalMetadata()
        self.cells = List[TerminalCell]()
        self.styles = List[TerminalStyle]()
        self.grapheme_offsets = [UInt32(0)]
        self.grapheme_bytes = List[UInt8]()

    @inline(.never)
    def validate_patch(self, patch: TerminalPatch) raises -> TerminalMetadata:
        if (
            self.generation != patch.base_generation
            or self.view_generation != patch.base_view_generation
        ):
            raise Error("terminal patch generation does not match")
        if self.dictionary_generation != patch.dictionary_generation:
            raise Error("terminal patch dictionary generation does not match")
        if self.columns != patch.columns or self.rows != patch.rows:
            raise Error("terminal patch dimensions do not match")
        if Int(self.columns) * Int(self.rows) != len(self.cells):
            raise Error("retained terminal plane dimensions are invalid")
        var appends = (
            len(patch.styles) != 0
            or len(patch.grapheme_lengths) != 0
            or len(patch.grapheme_bytes) != 0
        )
        if appends and (
            Int(patch.style_base) != len(self.styles)
            or Int(patch.grapheme_base) != len(self.grapheme_offsets) - 1
        ):
            raise Error("terminal patch dictionary base does not match")
        var style_count = len(self.styles) + len(patch.styles)
        var grapheme_count = (
            len(self.grapheme_offsets) - 1 + len(patch.grapheme_lengths)
        )
        if style_count > MAX_STYLE_COUNT or grapheme_count > MAX_GRAPHEME_COUNT:
            raise Error("terminal patch dictionary exceeds its limit")
        var bytes_count = len(self.grapheme_bytes) + len(patch.grapheme_bytes)
        if bytes_count > 4294967295:
            raise Error("terminal patch grapheme arena exceeds limit")
        var appended_bytes = 0
        for length in patch.grapheme_lengths:
            if Int(length) > len(patch.grapheme_bytes) - appended_bytes:
                raise Error("terminal patch grapheme lengths do not match")
            var end = appended_bytes + Int(length)
            _ = String(from_utf8=Span(patch.grapheme_bytes)[appended_bytes:end])
            appended_bytes = end
        if appended_bytes != len(patch.grapheme_bytes):
            raise Error("terminal patch grapheme lengths do not match")
        if patch.scroll != 0 and abs(patch.scroll) >= Int(self.rows):
            raise Error("terminal row shift is outside the viewport")
        var previous_row = -1
        var span_cells = 0
        for span in patch.spans:
            if (
                Int(span.row) >= Int(self.rows)
                or Int(span.row) <= previous_row
                or Int(span.start) + Int(span.length) > Int(self.columns)
            ):
                raise Error("terminal patch span is outside the viewport")
            previous_row = Int(span.row)
            span_cells += Int(span.length)
        if span_cells != len(patch.cells):
            raise Error("terminal patch cell count does not match spans")
        for cell in patch.cells:
            if Int(cell.style) >= style_count:
                raise Error("terminal cell references a missing style")
            if cell.glyph & GRAPHEME != 0:
                if Int(cell.glyph & ~GRAPHEME) >= grapheme_count:
                    raise Error("terminal cell references a missing grapheme")
            elif not valid_scalar(cell.glyph):
                raise Error("terminal cell contains an invalid Unicode scalar")
        if not exposed_rows_are_replaced(
            patch.spans, patch.scroll, self.rows, self.columns
        ):
            raise Error("terminal patch does not replace newly exposed rows")
        var next_metadata = self.metadata.copy()
        next_metadata.merge(patch.metadata, patch.fields)
        if next_metadata.cursor:
            var cursor = next_metadata.cursor.value()
            if cursor.row >= self.rows or cursor.column >= self.columns:
                raise Error("cursor is outside the viewport")
        return next_metadata^

    @inline(.never)
    def _commit_patch(
        mut self, patch: TerminalPatch, var next_metadata: TerminalMetadata
    ):
        self.styles.extend(Span(patch.styles))
        var end_offset = self.grapheme_offsets[len(self.grapheme_offsets) - 1]
        for length in patch.grapheme_lengths:
            end_offset += length
            self.grapheme_offsets.append(end_offset)
        self.grapheme_bytes.extend(Span(patch.grapheme_bytes))
        self.generation = patch.generation
        self.view_generation = patch.view_generation
        self.metadata = next_metadata^

    @inline(.never)
    def _apply_cells(mut self, patch: TerminalPatch):
        var columns = Int(self.columns)
        var shift = patch.scroll * columns
        if shift > 0:
            for index in range(len(self.cells) - 1, shift - 1, -1):
                self.cells[index] = self.cells[index - shift]
        elif shift < 0:
            for index in range(len(self.cells) + shift):
                self.cells[index] = self.cells[index - shift]
        var source = 0
        for span in patch.spans:
            var start = Int(span.row) * columns + Int(span.start)
            for index in range(Int(span.length)):
                self.cells[start + index] = patch.cells[source + index]
            source += Int(span.length)
            if span.clear:
                for index in range(
                    start + Int(span.length), (Int(span.row) + 1) * columns
                ):
                    self.cells[index] = TerminalCell(0, 0, 0)

    @inline(.never)
    def apply(mut self, patch: TerminalPatch) raises:
        var next_metadata = self.validate_patch(patch)
        self._apply_cells(patch)
        self._commit_patch(patch, next_metadata^)

    @inline(.never)
    def apply_verified_cells(
        mut self, patch: TerminalPatch, var cells: List[TerminalCell]
    ) raises:
        var next_metadata = self.validate_patch(patch)
        if len(cells) != Int(self.columns) * Int(self.rows):
            raise Error("verified terminal plane dimensions are invalid")
        self.cells = cells^
        self._commit_patch(patch, next_metadata^)


struct TerminalHistory(Copyable, Movable):
    var start: UInt32
    var total: UInt32
    var offset: UInt32
    var columns: UInt16
    var rows: UInt16
    var cells: List[TerminalCell]
    var styles: List[TerminalStyle]
    var grapheme_offsets: List[UInt32]
    var grapheme_bytes: List[UInt8]

    def __init__(out self):
        self.start = 0
        self.total = 0
        self.offset = 0
        self.columns = 0
        self.rows = 0
        self.cells = List[TerminalCell]()
        self.styles = List[TerminalStyle]()
        self.grapheme_offsets = [UInt32(0)]
        self.grapheme_bytes = List[UInt8]()


struct TerminalUpdate(Copyable, Movable):
    var kind: UInt8
    var pane: UInt64
    var sequence: UInt64
    var output_id: Optional[UInt64]
    var viewport: Optional[TerminalViewport]
    var patch: Optional[TerminalPatch]
    var history: Optional[TerminalHistory]

    def __init__(out self):
        self.kind = 0
        self.pane = 0
        self.sequence = 0
        self.output_id = None
        self.viewport = None
        self.patch = None
        self.history = None
