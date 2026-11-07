from .protocol import Decoder, append_uint, append_text, envelope, validate_envelope
from .tree_wire import read_uint, read_enum, read_bool, read_text, read_count
from .input_protocol import KeyInput, _append_key, _read_key

comptime CHOOSER_MAX_ITEMS = 65536
comptime CHOOSER_MAX_TEXT = 1024 * 1024
comptime CHOOSER_MAX_BYTES = 32 * 1024 * 1024


@fieldwise_init
struct ChooseSearch(Copyable, Movable):
    var query: String
    var reverse: Bool


struct ChooseItem(Copyable, Movable):
    var label: String
    var detail: String
    var target_kind: UInt8
    var target: UInt64
    var depth: UInt8
    var flags: UInt8
    var pane_kind: Optional[UInt8]
    var key: String
    var text: String

    def __init__(out self):
        self.label = String()
        self.detail = String()
        self.target_kind = 0
        self.target = 0
        self.depth = 0
        self.flags = 0
        self.pane_kind = None
        self.key = String()
        self.text = String()


struct ChooseState(Copyable, Movable):
    var items: List[ChooseItem]
    var search: Optional[ChooseSearch]
    var selected: UInt32
    var kind: UInt8
    var filter_no_matches: Bool
    var prompt: String
    var help: Bool

    def __init__(out self):
        self.items = List[ChooseItem]()
        self.search = None
        self.selected = 0
        self.kind = 0
        self.filter_no_matches = False
        self.prompt = String()
        self.help = False


struct ChooseEvent(Copyable, Movable):
    var sequence: UInt64
    var update: Bool
    var state: Optional[ChooseState]
    var search: Optional[ChooseSearch]
    var selected: UInt32

    def __init__(out self):
        self.sequence = 0
        self.update = False
        self.state = None
        self.search = None
        self.selected = 0


struct ChooseAction(Copyable, Movable):
    var kind: UInt8
    var index: UInt32
    var reverse: Bool
    var text: String
    var key: KeyInput

    def __init__(out self, kind: UInt8):
        self.kind = kind
        self.index = 0
        self.reverse = False
        self.text = String()
        self.key = KeyInput()


def _read_search(mut reader: Decoder) raises -> Optional[ChooseSearch]:
    if read_bool(reader):
        return ChooseSearch(read_text(reader, CHOOSER_MAX_TEXT), read_bool(reader))
    return None


def _append_search(mut output: List[UInt8], search: Optional[ChooseSearch]) raises:
    output.append(UInt8(Bool(search)))
    if search:
        if search.value().query.byte_length() > CHOOSER_MAX_TEXT:
            raise Error("choose-tree search text limit")
        append_text(output, search.value().query)
        output.append(UInt8(search.value().reverse))


def read_choose_state(mut reader: Decoder) raises -> ChooseState:
    var state = ChooseState()
    var count = read_count(reader, CHOOSER_MAX_ITEMS)
    var total = 0
    for _ in range(count):
        var item = ChooseItem()
        item.label = read_text(reader, min(CHOOSER_MAX_TEXT, CHOOSER_MAX_BYTES - total))
        total += item.label.byte_length()
        item.detail = read_text(reader, min(CHOOSER_MAX_TEXT, CHOOSER_MAX_BYTES - total))
        total += item.detail.byte_length()
        item.target_kind = read_enum(reader, 3)
        item.target = reader.uint()
        item.depth = reader.byte()
        item.flags = reader.byte()
        if read_bool(reader):
            item.pane_kind = read_enum(reader, 3)
        item.key = read_text(reader, min(64, CHOOSER_MAX_BYTES - total))
        total += item.key.byte_length()
        item.text = read_text(reader, min(512, CHOOSER_MAX_BYTES - total))
        total += item.text.byte_length()
        state.items.append(item^)
    state.search = _read_search(reader)
    state.selected = UInt32(read_uint(reader, 4294967295))
    state.kind = read_enum(reader, 2)
    state.filter_no_matches = read_bool(reader)
    state.prompt = read_text(reader, 512)
    state.help = read_bool(reader)
    return state^


def append_choose_state(mut output: List[UInt8], state: ChooseState) raises:
    if len(state.items) > CHOOSER_MAX_ITEMS or state.kind > 2 or state.prompt.byte_length() > 512:
        raise Error("choose-tree state exceeds enum or collection limit")
    var total = 0
    for item in state.items:
        if item.target_kind > 3 or (Bool(item.pane_kind) and item.pane_kind.value() > 3):
            raise Error("unknown choose-tree item enum")
        if item.key.byte_length() > 64 or item.text.byte_length() > 512 or item.label.byte_length() > CHOOSER_MAX_TEXT or item.detail.byte_length() > CHOOSER_MAX_TEXT:
            raise Error("choose-tree item text limit")
        var size = item.label.byte_length() + item.detail.byte_length() + item.key.byte_length() + item.text.byte_length()
        if total > CHOOSER_MAX_BYTES - size:
            raise Error("choose-tree item aggregate limit")
        total += size
    append_uint(output, UInt64(len(state.items)))
    for item in state.items:
        append_text(output, item.label)
        append_text(output, item.detail)
        append_uint(output, UInt64(item.target_kind))
        append_uint(output, item.target)
        output.append(item.depth)
        output.append(item.flags)
        output.append(UInt8(Bool(item.pane_kind)))
        if item.pane_kind:
            append_uint(output, UInt64(item.pane_kind.value()))
        append_text(output, item.key)
        append_text(output, item.text)
    _append_search(output, state.search)
    append_uint(output, UInt64(state.selected))
    append_uint(output, UInt64(state.kind))
    output.append(UInt8(state.filter_no_matches))
    append_text(output, state.prompt)
    output.append(UInt8(state.help))


def decode_choose_event(var bytes: List[UInt8]) raises -> Optional[ChooseEvent]:
    if validate_envelope(Span(bytes)) != 0:
        return None
    var reader = Decoder(bytes^)
    if read_enum(reader, 75) != 10:
        return None
    var event = ChooseEvent()
    event.sequence = reader.uint()
    var kind = read_enum(reader, 65)
    if kind == 11:
        if read_bool(reader):
            event.state = read_choose_state(reader)
    elif kind == 12:
        event.update = True
        event.search = _read_search(reader)
        event.selected = UInt32(read_uint(reader, 4294967295))
    else:
        return None
    reader.finish()
    return event^


def encode_choose_event(event: ChooseEvent) raises -> List[UInt8]:
    var output: List[UInt8] = [10]
    append_uint(output, event.sequence)
    append_uint(output, UInt64(12 if event.update else 11))
    if event.update:
        _append_search(output, event.search)
        append_uint(output, UInt64(event.selected))
    else:
        output.append(UInt8(Bool(event.state)))
        if event.state:
            append_choose_state(output, event.state.value())
    return envelope(Span(output))


def choose_request(action: ChooseAction) raises -> List[UInt8]:
    if action.kind > 39 or action.text.byte_length() > CHOOSER_MAX_TEXT:
        raise Error("choose-tree action exceeds enum or text limit")
    var output: List[UInt8] = [9, 8]
    append_uint(output, UInt64(action.kind))
    if action.kind == 9 or action.kind == 10:
        append_uint(output, UInt64(action.index))
    elif action.kind == 11 or action.kind == 16:
        output.append(UInt8(action.reverse))
    elif action.kind == 12:
        append_text(output, action.text)
    elif action.kind == 18:
        _append_key(output, action.key)
    return envelope(Span(output))


def decode_choose_request(var bytes: List[UInt8]) raises -> ChooseAction:
    if validate_envelope(Span(bytes)) != 0:
        raise Error("choose-tree action requires control lane")
    var reader = Decoder(bytes^)
    if read_enum(reader, 75) != 9 or read_enum(reader, 22) != 8:
        raise Error("expected choose-tree action")
    var action = ChooseAction(read_enum(reader, 39))
    if action.kind == 9 or action.kind == 10:
        action.index = UInt32(read_uint(reader, 4294967295))
    elif action.kind == 11 or action.kind == 16:
        action.reverse = read_bool(reader)
    elif action.kind == 12:
        action.text = read_text(reader, CHOOSER_MAX_TEXT)
    elif action.kind == 18:
        action.key = _read_key(reader)
    reader.finish()
    return action^
