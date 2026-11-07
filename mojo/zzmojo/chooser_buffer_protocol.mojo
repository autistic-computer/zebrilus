from .protocol import Decoder, validate_envelope
from .chooser_protocol import (
    ChooseSearch,
    CHOOSER_MAX_ITEMS,
    CHOOSER_MAX_TEXT,
    CHOOSER_MAX_BYTES,
)
from .tree_wire import read_uint, read_enum, read_bool, read_text, read_count


struct ChooseBufferItem(Copyable, Movable):
    var name: String
    var preview: String
    var size_bytes: UInt64
    var created_unix_seconds: UInt64
    var key: String
    var text: String
    var tagged: Bool

    def __init__(out self):
        self.name = String()
        self.preview = String()
        self.size_bytes = 0
        self.created_unix_seconds = 0
        self.key = String()
        self.text = String()
        self.tagged = False


struct ChooseBufferState(Movable):
    var items: List[ChooseBufferItem]
    var search: Optional[ChooseSearch]
    var selected: UInt32
    var filter_no_matches: Bool
    var help: Bool
    var prompt: String

    def __init__(out self):
        self.items = List[ChooseBufferItem]()
        self.search = None
        self.selected = 0
        self.filter_no_matches = False
        self.help = False
        self.prompt = String()


struct ChooseBufferEvent(Movable):
    var sequence: UInt64
    var update: Bool
    var state: Optional[ChooseBufferState]
    var search: Optional[ChooseSearch]
    var selected: UInt32

    def __init__(out self):
        self.sequence = 0
        self.update = False
        self.state = None
        self.search = None
        self.selected = 0


def _search(
    mut reader: Decoder, mut bytes: Int
) raises -> Optional[ChooseSearch]:
    if not read_bool(reader):
        return None
    var text = read_text(
        reader, min(CHOOSER_MAX_TEXT, CHOOSER_MAX_BYTES - bytes)
    )
    bytes += text.byte_length()
    return ChooseSearch(text^, read_bool(reader))


@inline(.never)
def decode_choose_buffer(
    var frame: List[UInt8],
) raises -> Optional[ChooseBufferEvent]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_enum(reader, 75) != 10:
        return None
    var event = ChooseBufferEvent()
    event.sequence = reader.uint()
    var kind = read_enum(reader, 65)
    var bytes = 0
    if kind == 14:
        event.update = True
        event.search = _search(reader, bytes)
        event.selected = UInt32(read_uint(reader, 4294967295))
        return event^
    if kind != 13:
        return None
    if not read_bool(reader):
        return event^
    var state = ChooseBufferState()
    var count = read_count(reader, CHOOSER_MAX_ITEMS)
    state.items.reserve(count)
    for _ in range(count):
        var item = ChooseBufferItem()
        item.name = read_text(
            reader, min(CHOOSER_MAX_TEXT, CHOOSER_MAX_BYTES - bytes)
        )
        bytes += item.name.byte_length()
        item.preview = read_text(
            reader, min(CHOOSER_MAX_TEXT, CHOOSER_MAX_BYTES - bytes)
        )
        bytes += item.preview.byte_length()
        item.size_bytes = reader.uint()
        item.created_unix_seconds = reader.uint()
        item.key = read_text(reader, min(64, CHOOSER_MAX_BYTES - bytes))
        bytes += item.key.byte_length()
        item.text = read_text(reader, min(512, CHOOSER_MAX_BYTES - bytes))
        bytes += item.text.byte_length()
        item.tagged = read_bool(reader)
        state.items.append(item^)
    state.search = _search(reader, bytes)
    state.selected = UInt32(read_uint(reader, 4294967295))
    state.filter_no_matches = read_bool(reader)
    state.help = read_bool(reader)
    state.prompt = read_text(
        reader, min(CHOOSER_MAX_TEXT, CHOOSER_MAX_BYTES - bytes)
    )
    event.state = state^
    return event^
