from .protocol import (
    Decoder,
    append_uint,
    append_text,
    envelope,
    validate_envelope,
)
from .input_protocol import _append_cell, _append_lines
from .terminal_view import PointerCell
from .tree_wire import read_enum

comptime TERMINAL_UI_COMMAND: UInt64 = 8
comptime COPY_MODE: UInt8 = 16
comptime SEARCH_BEGIN: UInt8 = 17
comptime SEARCH_UPDATE: UInt8 = 18
comptime SEARCH_NEXT: UInt8 = 19
comptime SEARCH_PREVIOUS: UInt8 = 20
comptime SEARCH_CLOSE: UInt8 = 21
comptime COPY_MODE_SCROLL_EXIT: UInt8 = 26
comptime COPY_MODE_WITH: UInt8 = 27
comptime COPY_MODE_COUNTED: UInt8 = 28


struct SearchQuery(Copyable, Movable):
    var text: String
    var mode: UInt8
    var case_policy: UInt8
    var direction: UInt8

    def __init__(
        out self,
        text: String = "",
        mode: UInt8 = 0,
        case_policy: UInt8 = 0,
        direction: UInt8 = 0,
    ):
        self.text = text
        self.mode = mode
        self.case_policy = case_policy
        self.direction = direction


struct CopyRequest(Copyable, Movable):
    var request_id: UInt64
    var clipboard: Bool
    var buffer: UInt8
    var prefix: Optional[String]
    var pipe: Optional[String]
    var clear_selection: Bool
    var cancel: Bool

    def __init__(out self):
        self.request_id = 0
        self.clipboard = False
        self.buffer = 0
        self.prefix = None
        self.pipe = None
        self.clear_selection = False
        self.cancel = False


struct CopyAction(Copyable, Movable):
    var kind: UInt8
    var flag: Bool
    var text: String
    var direction: UInt8
    var copy_request: CopyRequest
    var line: Optional[Int32]
    var selection: UInt8
    var regex: Bool
    var incremental: Bool
    var pointer: PointerCell

    def __init__(out self, kind: UInt8):
        self.kind = kind
        self.flag = False
        self.text = String()
        self.direction = 0
        self.copy_request = CopyRequest()
        self.line = None
        self.selection = 0
        self.regex = False
        self.incremental = False
        self.pointer = PointerCell(0, 0, 0, False, 0, 0, 0, 0, 0, 0)


@fieldwise_init
struct SearchUiCommand(TrivialRegisterPassable):
    var pane: UInt64
    var direction: UInt8


def _optional_text(mut output: List[UInt8], value: Optional[String]):
    output.append(UInt8(Bool(value)))
    if value:
        append_text(output, value.value())


def append_search_query(mut output: List[UInt8], query: SearchQuery) raises:
    if query.mode > 1 or query.case_policy > 2 or query.direction > 1:
        raise Error("search query contains an unknown enum value")
    append_text(output, query.text)
    append_uint(output, UInt64(query.mode))
    append_uint(output, UInt64(query.case_policy))
    append_uint(output, UInt64(query.direction))


def _append_copy_request(mut output: List[UInt8], request: CopyRequest) raises:
    if request.buffer > 2:
        raise Error("copy request contains an unknown buffer action")
    append_uint(output, request.request_id)
    output.append(UInt8(request.clipboard))
    output.append(UInt8(request.buffer != 0))
    if request.buffer == 1:
        append_uint(output, 0)
        _optional_text(output, request.prefix)
    elif request.buffer == 2:
        append_uint(output, 1)
    _optional_text(output, request.pipe)
    output.append(UInt8(request.clear_selection))
    output.append(UInt8(request.cancel))


def append_copy_action(mut output: List[UInt8], action: CopyAction) raises:
    if action.kind > 75:
        raise Error("unknown copy mode action")
    append_uint(output, UInt64(action.kind))
    if (
        action.kind == 21
        or action.kind == 22
        or action.kind == 23
        or action.kind == 36
        or action.kind == 69
        or action.kind == 71
    ):
        output.append(UInt8(action.flag))
    elif action.kind == 35:
        if action.direction > 1:
            raise Error("unknown copy jump direction")
        append_text(output, action.text)
        append_uint(output, UInt64(action.direction))
        output.append(UInt8(action.flag))
    elif action.kind == 37 or action.kind == 47 or action.kind == 73:
        _append_copy_request(output, action.copy_request)
    elif action.kind == 46:
        if action.direction > 1:
            raise Error("unknown copy search direction")
        append_uint(output, UInt64(action.direction))
    elif action.kind == 48:
        output.append(UInt8(Bool(action.line)))
        if action.line:
            _append_lines(output, action.line.value())
        output.append(UInt8(action.flag))
    elif action.kind == 50:
        if action.selection > 2:
            raise Error("unknown copy selection mode")
        append_uint(output, UInt64(action.selection))
    elif action.kind == 74:
        if action.direction > 1:
            raise Error("unknown copy search direction")
        append_text(output, action.text)
        append_uint(output, UInt64(action.direction))
        output.append(UInt8(action.regex))
        output.append(UInt8(action.incremental))
    elif action.kind == 75:
        _append_cell(output, action.pointer)


def _view_prefix(
    pane: UInt64, action: UInt8, command_output: Bool
) -> List[UInt8]:
    var output: List[UInt8] = [9, UInt8(7 if command_output else 5)]
    if not command_output:
        append_uint(output, pane)
    append_uint(output, UInt64(action))
    return output^


def search_request(
    pane: UInt64,
    action: UInt8,
    query: Optional[SearchQuery] = None,
    command_output: Bool = False,
) raises -> List[UInt8]:
    if action < SEARCH_BEGIN or action > SEARCH_CLOSE:
        raise Error("unknown terminal search action")
    var output = _view_prefix(pane, action, command_output)
    if action == SEARCH_BEGIN or action == SEARCH_UPDATE:
        if not query:
            raise Error("terminal search action has no query")
        append_search_query(output, query.value())
    return envelope(Span(output))


def copy_request(
    pane: UInt64,
    action: CopyAction,
    count: Optional[UInt32] = None,
    command_output: Bool = False,
) raises -> List[UInt8]:
    var output = _view_prefix(
        pane, COPY_MODE_COUNTED if count else COPY_MODE, command_output
    )
    append_copy_action(output, action)
    if count:
        append_uint(output, UInt64(count.value()))
    return envelope(Span(output))


def copy_mode_request(
    pane: UInt64,
    scroll_exit: Optional[Bool] = None,
    hide_position: Optional[Bool] = None,
    command_output: Bool = False,
) raises -> List[UInt8]:
    var action: UInt8 = 15
    if hide_position:
        action = COPY_MODE_WITH
    elif scroll_exit:
        action = COPY_MODE_SCROLL_EXIT if scroll_exit.value() else UInt8(15)
    var output = _view_prefix(pane, action, command_output)
    if hide_position:
        output.append(UInt8(scroll_exit.or_else(False)))
        output.append(UInt8(hide_position.value()))
    return envelope(Span(output))


def decode_search_ui(
    var frame: List[UInt8],
) raises -> Optional[SearchUiCommand]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if reader.uint() != 10:
        return None
    _ = reader.uint()
    if reader.uint() != TERMINAL_UI_COMMAND:
        return None
    var pane = reader.uint()
    _ = read_enum(reader, 0)
    var direction = UInt8(read_enum(reader, 1))
    reader.finish()
    return SearchUiCommand(pane, direction)
