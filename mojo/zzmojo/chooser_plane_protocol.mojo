from std.sys import size_of
from .protocol import Decoder, validate_envelope
from .tree_model import TmuxColour
from .tree_wire import (
    read_uint,
    read_enum,
    read_bool,
    read_text,
    read_count,
    read_optional_colour,
)
from .terminal_model import (
    TerminalViewport,
    TerminalCell,
    TerminalStyle,
    TerminalOverlay,
    KittyPlacement,
)
from .terminal_wire import decode_viewport_bytes

comptime CHOOSER_PLANE_TEXT = 1024 * 1024
comptime CHOOSER_PLANE_ITEMS = 65536
comptime CHOOSER_PLANE_ENTRIES = 131072
comptime CHOOSER_PLANE_BYTES = 32 * 1024 * 1024
comptime CHOOSER_PLANE_STORAGE = 64 * 1024 * 1024
comptime CHOOSER_PLANE_CELLS = 8 * 1024 * 1024
comptime CHOOSER_PLANE_VIEWPORTS = 256


@fieldwise_init
struct ChooserPlaneRow(Movable):
    var name: String
    var text: String
    var align: Bool


struct ChooserPlaneTile(Movable):
    var label: String
    var label_style: String
    var border_style: String
    var viewport: Optional[TerminalViewport]

    def __init__(out self):
        self.label = String()
        self.label_style = String()
        self.border_style = String()
        self.viewport = None


struct ChooserPlanePreview(Movable):
    var kind: UInt8
    var tiles: List[ChooserPlaneTile]
    var current: UInt32
    var viewport: Optional[TerminalViewport]
    var lines: List[String]
    var status_style: String
    var status_width: UInt32
    var border_style: String

    def __init__(out self, kind: UInt8):
        self.kind = kind
        self.tiles = List[ChooserPlaneTile]()
        self.current = 0
        self.viewport = None
        self.lines = List[String]()
        self.status_style = String()
        self.status_width = 0
        self.border_style = String()


struct ChooserPlane(Movable):
    var selected: UInt32
    var rows: List[ChooserPlaneRow]
    var sort: String
    var view: String
    var filter: Bool
    var selection_style: String
    var border_style: String
    var prompt_style: String
    var preview_size: UInt8
    var preview: Optional[ChooserPlanePreview]
    var cursor_style: UInt8
    var cursor_colour: Optional[TmuxColour]
    var prompt_column: UInt16
    var prompt_line: String
    var retained_bytes: Int

    def __init__(out self):
        self.selected = 0
        self.rows = List[ChooserPlaneRow]()
        self.sort = String()
        self.view = String()
        self.filter = False
        self.selection_style = String()
        self.border_style = String()
        self.prompt_style = String()
        self.preview_size = 0
        self.preview = None
        self.cursor_style = 0
        self.cursor_colour = None
        self.prompt_column = 0
        self.prompt_line = String()
        self.retained_bytes = 0


struct ChooserPlaneEvent(Movable):
    var sequence: UInt64
    var plane: Optional[ChooserPlane]

    def __init__(out self):
        self.sequence = 0
        self.plane = None


struct PlaneBudget(Movable):
    var bytes: Int
    var storage: Int
    var cells: Int
    var viewports: Int
    var entries: Int

    def __init__(out self):
        self.bytes = 0
        self.storage = size_of[ChooserPlane]()
        self.cells = 0
        self.viewports = 0
        self.entries = 0

    def charge(mut self, bytes: Int) raises:
        if bytes < 0 or bytes > CHOOSER_PLANE_STORAGE - self.storage:
            raise Error("chooser presentation retained storage limit")
        self.storage += bytes

    def text(mut self, mut reader: Decoder) raises -> String:
        var text = read_text(
            reader, min(CHOOSER_PLANE_TEXT, CHOOSER_PLANE_BYTES - self.bytes)
        )
        self.bytes += text.byte_length()
        self.charge(2 * text.byte_length() + 32)
        return text^

    def count(mut self, mut reader: Decoder, element: Int) raises -> Int:
        var count = read_count(
            reader,
            min(CHOOSER_PLANE_ITEMS, CHOOSER_PLANE_ENTRIES - self.entries),
        )
        self.entries += count
        self.charge(2 * count * element)
        return count


def _optional_text_storage(text: Optional[String]) -> Int:
    return 2 * text.value().byte_length() + 32 if text else 0


def _viewport_storage(viewport: TerminalViewport) -> Int:
    ref metadata = viewport.metadata
    return (
        size_of[TerminalViewport]()
        + viewport.cells.capacity() * size_of[TerminalCell]()
        + viewport.styles.capacity() * size_of[TerminalStyle]()
        + viewport.grapheme_offsets.capacity() * size_of[UInt32]()
        + viewport.grapheme_bytes.capacity()
        + metadata.overlays.capacity() * size_of[TerminalOverlay]()
        + metadata.kitty.capacity() * size_of[KittyPlacement]()
        + 2 * (metadata.title.byte_length() + metadata.failure.byte_length())
        + 64
        + _optional_text_storage(metadata.working_directory)
        + _optional_text_storage(metadata.hovered_uri)
        + _optional_text_storage(metadata.exit_signal)
    )


@inline(.never)
def _viewport(
    mut reader: Decoder, mut budget: PlaneBudget
) raises -> TerminalViewport:
    if budget.viewports == CHOOSER_PLANE_VIEWPORTS:
        raise Error("chooser presentation viewport count limit")
    var count = read_count(reader, CHOOSER_PLANE_BYTES - budget.bytes)
    var start = reader.offset
    var end = start + count
    _ = reader.uint()
    _ = reader.uint()
    _ = read_uint(reader, 4294967295)
    var columns = read_uint(reader, 65535)
    var rows = read_uint(reader, 65535)
    if reader.offset > end:
        raise Error("chooser viewport header exceeds body")
    var cells = Int(columns) * Int(rows)
    if cells > CHOOSER_PLANE_CELLS - budget.cells:
        raise Error("chooser presentation aggregate cell limit")
    budget.cells += cells
    budget.viewports += 1
    budget.bytes += count
    reader.offset = end
    var viewport = decode_viewport_bytes(
        List[UInt8](Span(reader.data)[start:end])
    )
    budget.charge(_viewport_storage(viewport))
    return viewport^


def _optional_viewport(
    mut reader: Decoder, mut budget: PlaneBudget
) raises -> Optional[TerminalViewport]:
    if read_bool(reader):
        return _viewport(reader, budget)
    return None


def _lines(mut reader: Decoder, mut budget: PlaneBudget) raises -> List[String]:
    var count = budget.count(reader, size_of[String]())
    var lines = List[String](capacity=count)
    for _ in range(count):
        lines.append(budget.text(reader))
    return lines^


@inline(.never)
def _preview(
    mut reader: Decoder, mut budget: PlaneBudget
) raises -> ChooserPlanePreview:
    var preview = ChooserPlanePreview(read_enum(reader, 4))
    if preview.kind == 0:
        var count = budget.count(reader, size_of[ChooserPlaneTile]())
        preview.tiles.reserve(count)
        for _ in range(count):
            var tile = ChooserPlaneTile()
            tile.label = budget.text(reader)
            tile.label_style = budget.text(reader)
            tile.border_style = budget.text(reader)
            tile.viewport = _optional_viewport(reader, budget)
            preview.tiles.append(tile^)
        preview.current = UInt32(read_uint(reader, 4294967295))
    elif preview.kind == 1:
        preview.viewport = _viewport(reader, budget)
    elif preview.kind == 2 or preview.kind == 4:
        preview.lines = _lines(reader, budget)
    else:
        preview.viewport = _optional_viewport(reader, budget)
        preview.lines = _lines(reader, budget)
        preview.status_style = budget.text(reader)
        preview.status_width = UInt32(read_uint(reader, 4294967295))
        preview.border_style = budget.text(reader)
    return preview^


@inline(.never)
def decode_chooser_plane(
    var frame: List[UInt8],
) raises -> Optional[ChooserPlaneEvent]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_enum(reader, 75) != 10:
        return None
    var event = ChooserPlaneEvent()
    event.sequence = reader.uint()
    if read_enum(reader, 65) != 51:
        return None
    if not read_bool(reader):
        return event^
    var budget = PlaneBudget()
    var plane = ChooserPlane()
    plane.selected = UInt32(read_uint(reader, 4294967295))
    var count = budget.count(reader, size_of[ChooserPlaneRow]())
    plane.rows.reserve(count)
    for _ in range(count):
        plane.rows.append(
            ChooserPlaneRow(
                budget.text(reader), budget.text(reader), read_bool(reader)
            )
        )
    plane.sort = budget.text(reader)
    plane.view = budget.text(reader)
    plane.filter = read_bool(reader)
    plane.selection_style = budget.text(reader)
    plane.border_style = budget.text(reader)
    plane.prompt_style = budget.text(reader)
    plane.preview_size = read_enum(reader, 2)
    if read_bool(reader):
        plane.preview = _preview(reader, budget)
    plane.cursor_style = reader.byte()
    plane.cursor_colour = read_optional_colour(reader)
    plane.prompt_column = UInt16(read_uint(reader, 65535))
    plane.prompt_line = budget.text(reader)
    plane.retained_bytes = budget.storage
    event.plane = plane^
    return event^
