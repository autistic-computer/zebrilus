from .protocol import (
    Decoder,
    MAX_FRAME_BYTES,
    append_uint,
    append_text,
    append_bytes,
    envelope,
    validate_envelope,
)
from .tree_wire import read_enum, read_text, read_count

comptime MAX_CLIPBOARD_REPLY_BYTES: Int = 32 * 1024 * 1024
comptime MAX_CLIPBOARD_EVENTS: Int = 4096
comptime CLIPBOARD_WRITE: UInt8 = 6
comptime CLIPBOARD_QUERY: UInt8 = 63


@fieldwise_init
struct ClipboardResponse(Movable):
    var server: UInt64
    var client: UInt64
    var frame: List[UInt8]


struct ClipboardEvent(Copyable, Movable):
    var kind: UInt8
    var sequence: UInt64
    var pane: UInt64
    var request_id: UInt64
    var target: UInt8
    var text: String
    var producer: UInt8

    def __init__(out self):
        self.kind = 6
        self.sequence = 0
        self.pane = 0
        self.request_id = 0
        self.target = 0
        self.text = String()
        self.producer = 0


def decode_clipboard_event(
    var bytes: List[UInt8],
) raises -> Optional[ClipboardEvent]:
    if validate_envelope(Span(bytes)) != 0:
        return None
    var reader = Decoder(bytes^)
    if read_enum(reader, 75) != 10:
        return None
    var event = ClipboardEvent()
    event.sequence = reader.uint()
    event.kind = read_enum(reader, 65)
    if event.kind == 6:
        event.pane = reader.uint()
        event.request_id = reader.uint()
        event.target = read_enum(reader, 1)
        event.text = read_text(reader, MAX_FRAME_BYTES)
        event.producer = read_enum(reader, 1)
    elif event.kind != CLIPBOARD_QUERY:
        return None
    return event^


def encode_clipboard_event(event: ClipboardEvent) raises -> List[UInt8]:
    var bytes: List[UInt8] = [10]
    append_uint(bytes, event.sequence)
    append_uint(bytes, UInt64(event.kind))
    if event.kind == 6:
        if event.target > 1 or event.producer > 1:
            raise Error("unknown clipboard target or producer")
        append_uint(bytes, event.pane)
        append_uint(bytes, event.request_id)
        append_uint(bytes, UInt64(event.target))
        append_text(bytes, event.text)
        append_uint(bytes, UInt64(event.producer))
    elif event.kind != CLIPBOARD_QUERY:
        raise Error("unknown clipboard event")
    return envelope(Span(bytes))


def clipboard_reply(data: Span[UInt8, _]) raises -> List[UInt8]:
    var bytes: List[UInt8] = [9, 24]
    append_bytes(bytes, data)
    return envelope(Span(bytes))


def decode_clipboard_reply(var frame: List[UInt8]) raises -> List[UInt8]:
    if validate_envelope(Span(frame)) != 0:
        raise Error("clipboard reply has no control envelope")
    var reader = Decoder(frame^)
    if read_enum(reader, 75) != 9 or read_enum(reader, 24) != 24:
        raise Error("frame is not a clipboard reply")
    var count = read_count(reader, MAX_FRAME_BYTES)
    var bytes = List[UInt8](capacity=count)
    for _ in range(count):
        bytes.append(reader.byte())
    return bytes^


struct ClipboardState(Copyable, Movable):
    var writes: List[ClipboardEvent]
    var query: Bool
    var pending_bytes: Int
    var query_server: UInt64
    var query_client: UInt64

    def __init__(out self):
        self.writes = List[ClipboardEvent]()
        self.query = False
        self.pending_bytes = 0
        self.query_server = 0
        self.query_client = 0

    def apply(
        mut self, event: ClipboardEvent, server: UInt64 = 0, client: UInt64 = 0
    ) raises:
        if event.kind == CLIPBOARD_QUERY:
            self.query = True
            self.query_server = server
            self.query_client = client
            return
        if event.kind != 6:
            raise Error("unsupported clipboard event")
        var size = event.text.byte_length()
        if (
            len(self.writes) >= MAX_CLIPBOARD_EVENTS
            or size > MAX_FRAME_BYTES - self.pending_bytes
        ):
            raise Error("pending clipboard events exceed mailbox limit")
        self.writes.append(event.copy())
        self.pending_bytes += size

    def merge(mut self, var other: ClipboardState) raises:
        if (
            len(other.writes) > MAX_CLIPBOARD_EVENTS - len(self.writes)
            or other.pending_bytes > MAX_FRAME_BYTES - self.pending_bytes
        ):
            raise Error("pending clipboard events exceed mailbox limit")
        var pending = other.pending_bytes
        var query = other.query
        var server = other.query_server
        var client = other.query_client
        var writes = List[ClipboardEvent]()
        swap(writes, other.writes)
        self.writes.extend(writes^)
        self.pending_bytes += pending
        if query:
            self.query = True
            self.query_server = server
            self.query_client = client

    def take(mut self) -> ClipboardState:
        var result = ClipboardState()
        swap(result.writes, self.writes)
        result.query = self.query
        result.pending_bytes = self.pending_bytes
        result.query_server = self.query_server
        result.query_client = self.query_client
        self.query = False
        self.pending_bytes = 0
        self.query_server = 0
        self.query_client = 0
        return result^
