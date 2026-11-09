from .protocol import Decoder, MAX_FRAME_BYTES
from .tree_model import (
    TmuxColour,
    PaneKind,
    PaneSummary,
    PaneStatus,
    PaneSnapshot,
    LayoutNode,
    LayoutTree,
    FloatingPane,
    ModalPane,
    WindowSnapshot,
    SessionViewer,
    SessionSnapshot,
    MuxSnapshot,
    TreeOp,
    TreeDelta,
    ClientView,
)


def read_bool(mut reader: Decoder) raises -> Bool:
    var value = reader.byte()
    if value > 1:
        raise Error("invalid postcard boolean or option")
    return value != 0


def read_uint(
    mut reader: Decoder, maximum: UInt64 = UInt64.MAX, width: Int = 0
) raises -> UInt64:
    var start = reader.offset
    var value = reader.uint()
    var bytes = (
        width if width
        > 0 else 3 if maximum
        <= 65535 else 5 if maximum
        <= 4294967295 else 10
    )
    if reader.offset - start > bytes or value > maximum:
        raise Error("postcard integer exceeds type range")
    return value


def read_enum(mut reader: Decoder, maximum: UInt8) raises -> UInt8:
    return UInt8(read_uint(reader, UInt64(maximum), 5))


def read_count(
    mut reader: Decoder, maximum: Int = MAX_FRAME_BYTES
) raises -> Int:
    var value = reader.uint()
    if value > UInt64(maximum) or value > UInt64(
        len(reader.data) - reader.offset
    ):
        raise Error(
            "postcard collection length exceeds remaining frame or limit"
        )
    return Int(value)


def read_text(
    mut reader: Decoder, maximum: Int = MAX_FRAME_BYTES
) raises -> String:
    var size = read_count(reader, maximum)
    var result = String(
        from_utf8=Span(reader.data)[reader.offset : reader.offset + size]
    )
    reader.offset += size
    return result^


def read_optional_id(mut reader: Decoder) raises -> Optional[UInt64]:
    if read_bool(reader):
        return reader.uint()
    return None


def read_optional_text(
    mut reader: Decoder, maximum: Int = MAX_FRAME_BYTES
) raises -> Optional[String]:
    if read_bool(reader):
        return read_text(reader, maximum)
    return None


def read_optional_u32(mut reader: Decoder) raises -> Optional[UInt32]:
    if read_bool(reader):
        return UInt32(read_uint(reader, 4294967295))
    return None


def read_strings(
    mut reader: Decoder,
    maximum: Int = MAX_FRAME_BYTES,
    text_maximum: Int = MAX_FRAME_BYTES,
) raises -> List[String]:
    var count = read_count(reader, maximum)
    var result = List[String](capacity=count)
    for _ in range(count):
        result.append(read_text(reader, text_maximum))
    return result^


def read_ids(mut reader: Decoder) raises -> List[UInt64]:
    var count = read_count(reader)
    var result = List[UInt64](capacity=count)
    for _ in range(count):
        result.append(reader.uint())
    return result^


def read_colour(mut reader: Decoder) raises -> TmuxColour:
    var result = TmuxColour()
    result.kind = read_enum(reader, 5)
    if result.kind == 0 or result.kind == 1 or result.kind == 5:
        result.value = UInt32(reader.byte())
    elif result.kind == 2:
        result.value = UInt32(read_uint(reader, 16777215))
    return result


def read_optional_colour(mut reader: Decoder) raises -> Optional[TmuxColour]:
    if read_bool(reader):
        return read_colour(reader)
    return None


def _validate_editor_path(value: String) raises:
    if value.byte_length() > 16384 or not value.startswith("/"):
        raise Error("editor path must be absolute and within its byte limit")
    for character in value.codepoints():
        var code = Int(character)
        if code <= 31 or (code >= 127 and code <= 159):
            raise Error("editor path contains a control character")


@inline(.never)
def read_pane_kind(mut reader: Decoder) raises -> PaneKind:
    var result = PaneKind()
    result.kind = read_enum(reader, 4)
    if result.kind == 2:
        result.tabs = read_strings(reader)
        result.active_tab = reader.uint()
        result.profile = read_text(reader)
    elif result.kind == 3:
        result.path = read_optional_text(reader)
        var cwd = read_text(reader)
        _validate_editor_path(cwd)
        result.cwd = cwd^
        if result.path:
            _validate_editor_path(result.path.value())
    elif result.kind == 4:
        result.provider = read_enum(reader, 1)
        result.cwd = read_optional_text(reader)
        result.session_id = read_optional_text(reader)
        result.model = read_optional_text(reader)
        result.effort = read_optional_text(reader)
    return result^


def read_summary(mut reader: Decoder) raises -> PaneSummary:
    var result = PaneSummary()
    result.changed_at = reader.uint()
    result.unseen = read_bool(reader)
    if read_bool(reader):
        var command = read_enum(reader, 1)
        result.command = command
        if command == 1:
            result.command_exit = reader.byte()
    result.name = read_text(reader, 256)
    result.cwd = read_text(reader, 4096)
    result.repo = read_text(reader, 4096)
    result.branch = read_text(reader, 256)
    result.process = read_strings(reader, 32, 1024)
    result.flip_armed = read_bool(reader)
    result.shell = read_text(reader, 256)
    return result^


def read_status(mut reader: Decoder) raises -> Optional[PaneStatus]:
    if not read_bool(reader):
        return None
    var result = PaneStatus()
    result.state = read_enum(reader, 4)
    if read_bool(reader):
        result.kind = read_enum(reader, 2)
    if read_bool(reader):
        result.progress = reader.byte()
    result.app = read_text(reader, 64)
    result.title = read_text(reader, 256)
    result.message = read_text(reader, 2048)
    return result^


def _prompt_cursor(mut reader: Decoder) raises:
    _ = reader.byte()
    _ = read_optional_colour(reader)


def _viewport_bytes(mut reader: Decoder, optional: Bool) raises:
    if optional and not read_bool(reader):
        return
    from .terminal_wire import decode_viewport_bytes

    var bytes = reader.bytes()
    _ = decode_viewport_bytes(bytes^)


def _chooser_state(mut reader: Decoder) raises:
    var count = read_count(reader)
    for _ in range(count):
        _ = read_text(reader)
        _ = read_text(reader)
        _ = read_enum(reader, 3)
        _ = reader.uint()
        _ = reader.byte()
        _ = reader.byte()
        if read_bool(reader):
            _ = read_enum(reader, 3)
        _ = read_text(reader, 64)
        _ = read_text(reader, 512)
    if read_bool(reader):
        _ = read_text(reader)
        _ = read_bool(reader)
    _ = read_uint(reader, 4294967295)
    _ = read_enum(reader, 2)
    _ = read_bool(reader)
    _ = read_text(reader, 512)
    _ = read_bool(reader)


def _chooser_preview(mut reader: Decoder) raises:
    var kind = read_enum(reader, 4)
    if kind == 0:
        var count = read_count(reader)
        for _ in range(count):
            _ = read_text(reader)
            _ = read_text(reader)
            _ = read_text(reader)
            _viewport_bytes(reader, True)
        _ = read_uint(reader, 4294967295)
    elif kind == 1:
        _viewport_bytes(reader, False)
    elif kind == 2 or kind == 4:
        _ = read_strings(reader)
    elif kind == 3:
        _viewport_bytes(reader, True)
        _ = read_strings(reader)
        _ = read_text(reader)
        _ = read_uint(reader, 4294967295)
        _ = read_text(reader)


def _chooser_presentation(mut reader: Decoder) raises:
    _ = read_uint(reader, 4294967295)
    var count = read_count(reader)
    for _ in range(count):
        _ = read_text(reader)
        _ = read_text(reader)
        _ = read_bool(reader)
    _ = read_text(reader)
    _ = read_text(reader)
    _ = read_bool(reader)
    _ = read_text(reader)
    _ = read_text(reader)
    _ = read_text(reader)
    _ = read_enum(reader, 2)
    if read_bool(reader):
        _chooser_preview(reader)
    _prompt_cursor(reader)
    _ = read_uint(reader, 65535)
    _ = read_text(reader)


@fieldwise_init
struct ModeRecord(Movable):
    var kind: Optional[UInt8]
    var bytes: List[UInt8]


@inline(.never)
def read_mode(mut reader: Decoder) raises -> ModeRecord:
    var start = reader.offset
    var kind: Optional[UInt8] = None
    if read_bool(reader):
        var tag = read_enum(reader, 3)
        kind = tag
        if tag == 0:
            _ = read_text(reader)
            _ = read_text(reader)
        elif tag == 1:
            _ = read_strings(reader)
            _ = read_uint(reader, 4294967295)
            _ = read_uint(reader, 4294967295)
            for _ in range(3):
                _ = read_text(reader)
            _ = read_uint(reader, 65535)
            var rows = read_count(reader)
            for _ in range(rows):
                var columns = read_count(reader)
                for _ in range(columns):
                    _ = read_uint(reader, 65535)
            _ = read_text(reader)
            _prompt_cursor(reader)
        elif tag == 2:
            _chooser_state(reader)
            _chooser_presentation(reader)
            _ = read_uint(reader, 4294967295)
            _ = read_text(reader)
            _ = read_uint(reader, 65535)
            _ = read_bool(reader)
        elif tag == 3:
            var areas = read_count(reader)
            for _ in range(areas):
                _ = reader.uint()
                _ = read_uint(reader, 4294967295)
                for _ in range(4):
                    _ = read_uint(reader, 65535)
                _ = read_text(reader)
                _ = read_text(reader)
                _viewport_bytes(reader, True)
            var borders = read_count(reader)
            for _ in range(borders):
                _ = read_uint(reader, 65535)
                _ = read_uint(reader, 65535)
                _ = reader.byte()
            _ = read_text(reader)
            _ = read_bool(reader)
            _ = read_bool(reader)
            var clears = read_count(reader)
            for _ in range(clears):
                _ = read_uint(reader, 4294967295)
                for _ in range(4):
                    _ = read_uint(reader, 65535)
    var raw = List[UInt8](Span(reader.data)[start : reader.offset])
    return ModeRecord(kind, raw^)


@inline(.never)
def read_pane(mut reader: Decoder) raises -> PaneSnapshot:
    var result = PaneSnapshot()
    result.id = reader.uint()
    result.title = read_text(reader)
    result.kind = read_pane_kind(reader)
    result.synchronized_input = read_bool(reader)
    result.bell = read_bool(reader)
    result.dead = read_bool(reader)
    result.dead_status = read_optional_u32(reader)
    result.border_colour = read_optional_colour(reader)
    result.active_border_colour = read_optional_colour(reader)
    result.border_status_text = read_text(reader)
    var mode = read_mode(reader)
    result.mode_kind = mode.kind
    result.mode_bytes = mode.bytes^
    mode.bytes = List[UInt8]()
    result.status = read_status(reader)
    result.summary = read_summary(reader)
    return result^


@inline(.never)
def _layout_node(
    mut reader: Decoder, mut layout: LayoutTree, depth: Int
) raises -> Int:
    if depth >= 256 or len(layout.nodes) >= 65535:
        raise Error("layout depth or node count exceeds protocol limit")
    var node = LayoutNode()
    node.kind = read_enum(reader, 2)
    if node.kind == 0:
        node.id = reader.uint()
    elif node.kind == 1:
        node.id = reader.uint()
        node.axis = read_enum(reader, 1)
        var bits: UInt32 = 0
        for index in range(4):
            bits |= UInt32(reader.byte()) << UInt32(8 * index)
        node.ratio = Pointer(to=bits).unsafe_bitcast[Float32]()[]
    var index = len(layout.nodes)
    layout.nodes.append(node)
    if node.kind == 1:
        var first = _layout_node(reader, layout, depth + 1)
        var second = _layout_node(reader, layout, depth + 1)
        layout.nodes[index].first = first
        layout.nodes[index].second = second
    return index


def read_layout(mut reader: Decoder) raises -> LayoutTree:
    var result = LayoutTree()
    result.root = _layout_node(reader, result, 0)
    return result^


def read_floating(mut reader: Decoder) raises -> List[FloatingPane]:
    var count = read_count(reader)
    var output = List[FloatingPane](capacity=count)
    for _ in range(count):
        var pane = FloatingPane()
        pane.pane = reader.uint()
        var x = read_uint(reader, 4294967295)
        var y = read_uint(reader, 4294967295)
        pane.xoff = Int32(Int64(x >> 1) ^ -Int64(x & 1))
        pane.yoff = Int32(Int64(y >> 1) ^ -Int64(y & 1))
        pane.sx = UInt16(read_uint(reader, 65535))
        pane.sy = UInt16(read_uint(reader, 65535))
        pane.visible = read_bool(reader)
        pane.border_lines = read_enum(reader, 6)
        pane.border_status = read_enum(reader, 2)
        output.append(pane)
    return output^


def read_modal(mut reader: Decoder) raises -> Optional[ModalPane]:
    if not read_bool(reader):
        return None
    var pane = ModalPane()
    pane.pane = reader.uint()
    pane.capture_keys = read_bool(reader)
    pane.close_on_click = read_bool(reader)
    pane.close_on_cancel = read_bool(reader)
    return pane


@inline(.never)
def read_window(mut reader: Decoder) raises -> WindowSnapshot:
    var result = WindowSnapshot()
    result.id = reader.uint()
    result.index = UInt32(read_uint(reader, 4294967295))
    result.name = read_text(reader)
    result.automatic_rename = read_bool(reader)
    result.active_pane = reader.uint()
    result.zoomed_pane = read_optional_id(reader)
    result.layout = read_layout(reader)
    var count = read_count(reader)
    for _ in range(count):
        var id = reader.uint()
        var pane = read_pane(reader)
        result.panes[id] = pane^
    result.layout_dump = read_text(reader)
    result.visible_layout_dump = read_text(reader)
    result.status_label = read_text(reader, 1024)
    result.activity = read_bool(reader)
    result.silence = read_bool(reader)
    result.pane_border_status = read_enum(reader, 2)
    result.pane_border_lines = read_enum(reader, 6)
    result.pane_border_indicators = read_enum(reader, 3)
    result.pane_order = read_ids(reader)
    result.pane_z_order = read_ids(reader)
    result.floating = read_floating(reader)
    result.modal = read_modal(reader)
    result.sx = UInt16(read_uint(reader, 65535))
    result.sy = UInt16(read_uint(reader, 65535))
    result.pane_base_index = UInt32(read_uint(reader, 4294967295))
    return result^


def read_viewers(mut reader: Decoder) raises -> List[SessionViewer]:
    var count = read_count(reader)
    var output = List[SessionViewer](capacity=count)
    for _ in range(count):
        var viewer = SessionViewer()
        viewer.name = read_text(reader)
        viewer.window = reader.uint()
        viewer.is_self = read_bool(reader)
        output.append(viewer^)
    return output^


@inline(.never)
def read_session(mut reader: Decoder) raises -> SessionSnapshot:
    var result = SessionSnapshot()
    result.id = reader.uint()
    result.name = read_text(reader)
    result.active_window = reader.uint()
    var count = read_count(reader)
    for _ in range(count):
        result.windows.append(read_window(reader))
    result.viewers = read_viewers(reader)
    return result^


@inline(.never)
def read_snapshot(mut reader: Decoder) raises -> MuxSnapshot:
    var result = MuxSnapshot()
    result.generation = reader.uint()
    var count = read_count(reader)
    for _ in range(count):
        result.sessions.append(read_session(reader))
    result.focused_window = read_optional_id(reader)
    return result^


@inline(.never)
def read_tree_op(mut reader: Decoder) raises -> TreeOp:
    var op = TreeOp()
    op.kind = read_enum(reader, 18)
    if op.kind == 0:
        op.session_value = read_session(reader)
        return op^
    op.session = reader.uint()
    if op.kind == 1:
        return op^
    if op.kind == 2:
        op.name = read_text(reader)
        return op^
    if op.kind == 3:
        op.window = reader.uint()
        return op^
    if op.kind == 4:
        op.viewers = read_viewers(reader)
        return op^
    if op.kind == 5:
        op.window_value = read_window(reader)
        return op^
    op.window = reader.uint()
    if op.kind == 6:
        return op^
    if op.kind == 7:
        op.name = read_text(reader)
        return op^
    if op.kind == 8:
        op.pane = reader.uint()
        return op^
    if op.kind == 9:
        op.layout = read_layout(reader)
        op.zoomed_pane = read_optional_id(reader)
        op.layout_dump = read_text(reader)
        op.visible_layout_dump = read_text(reader)
        op.pane_order = read_ids(reader)
        op.pane_z_order = read_ids(reader)
        op.floating = read_floating(reader)
        op.modal = read_modal(reader)
        op.sx = UInt16(read_uint(reader, 65535))
        op.sy = UInt16(read_uint(reader, 65535))
        return op^
    if op.kind == 10:
        op.index = UInt32(read_uint(reader, 4294967295))
        op.automatic_rename = read_bool(reader)
        op.activity = read_bool(reader)
        op.silence = read_bool(reader)
        op.pane_base_index = UInt32(read_uint(reader, 4294967295))
        return op^
    if op.kind == 11:
        op.name = read_text(reader, 1024)
        op.pane_border_status = read_enum(reader, 2)
        op.pane_border_lines = read_enum(reader, 6)
        op.pane_border_indicators = read_enum(reader, 3)
        return op^
    if op.kind == 12:
        op.pane_value = read_pane(reader)
        return op^
    op.pane = reader.uint()
    if op.kind == 13:
        return op^
    if op.kind == 14:
        op.name = read_text(reader)
        return op^
    if op.kind == 15:
        op.pane_kind = read_pane_kind(reader)
        op.synchronized_input = read_bool(reader)
        op.bell = read_bool(reader)
        op.dead = read_bool(reader)
        op.dead_status = read_optional_u32(reader)
        return op^
    if op.kind == 16:
        op.border_colour = read_optional_colour(reader)
        op.active_border_colour = read_optional_colour(reader)
        op.name = read_text(reader)
        var mode = read_mode(reader)
        op.mode_kind = mode.kind
        op.mode_bytes = mode.bytes^
        mode.bytes = List[UInt8]()
        return op^
    if op.kind == 17:
        op.status = read_status(reader)
        return op^
    op.summary = read_summary(reader)
    return op^


def read_ops(mut reader: Decoder) raises -> List[TreeOp]:
    var count = read_count(reader)
    var output = List[TreeOp](capacity=count)
    for _ in range(count):
        output.append(read_tree_op(reader))
    return output^


@inline(.never)
def read_delta(mut reader: Decoder) raises -> TreeDelta:
    var result = TreeDelta()
    result.base = reader.uint()
    result.version = reader.uint()
    result.ops = read_ops(reader)
    return result^


@inline(.never)
def read_view(mut reader: Decoder) raises -> ClientView:
    var result = ClientView()
    result.session = read_optional_id(reader)
    result.focused_window = read_optional_id(reader)
    result.read_only = read_bool(reader)
    result.client_flags = read_text(reader)
    result.layout_generation = reader.uint()
    result.attachment_generation = reader.uint()
    result.overlay = read_ops(reader)
    return result^
