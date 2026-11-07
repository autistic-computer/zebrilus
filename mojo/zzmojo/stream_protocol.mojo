from .protocol import (
    PROTOCOL_VERSION,
    Decoder,
    append_uint,
    append_text,
    append_bytes,
    envelope,
    validate_envelope,
    append_client_context,
)
from .client_context import ClientContext
from .tree_model import MuxSnapshot, TreeDelta, ClientView
from .tree_wire import (
    read_uint,
    read_count,
    read_snapshot,
    read_delta,
    read_view,
)

comptime HELLO: UInt64 = 50
comptime WELCOME: UInt64 = 51
comptime BATCH: UInt64 = 52
comptime EVENT: UInt64 = 10
comptime SNAPSHOT: UInt64 = 0
comptime TREE_DELTA: UInt64 = 57
comptime CLIENT_VIEW: UInt64 = 58
comptime PING: UInt64 = 68
comptime PONG: UInt64 = 69
comptime REQUIRED_CAPABILITIES: UInt64 = (UInt64(1) << 29) | (UInt64(1) << 30)


struct StreamFrame(Movable):
    var kind: UInt64
    var sequence: UInt64
    var event_kind: Optional[UInt64]
    var protocol_version: UInt16
    var server_id: UInt64
    var client_id: UInt64
    var instance: UInt64
    var capabilities: UInt64
    var clock: UInt64
    var ping_token: UInt64
    var rtt_us: UInt32
    var batch: List[List[UInt8]]
    var snapshot: Optional[MuxSnapshot]
    var delta: Optional[TreeDelta]
    var view: Optional[ClientView]
    var raw: List[UInt8]

    def __init__(out self):
        self.kind = 0
        self.sequence = 0
        self.event_kind = None
        self.protocol_version = 0
        self.server_id = 0
        self.client_id = 0
        self.instance = 0
        self.capabilities = 0
        self.clock = 0
        self.ping_token = 0
        self.rtt_us = 0
        self.batch = List[List[UInt8]]()
        self.snapshot = None
        self.delta = None
        self.view = None
        self.raw = List[UInt8]()


def interactive_hello(
    instance: UInt64,
    process: UInt32,
    session: String,
    columns: UInt16 = 80,
    rows: UInt16 = 24,
    cell_width_px: UInt32 = 8,
    cell_height_px: UInt32 = 16,
    origin: Optional[UInt64] = None,
    options: UInt64 = 0,
    viewport: Bool = True,
    context: Optional[ClientContext] = None,
    native_search: Bool = False,
) raises -> List[UInt8]:
    var payload = List[UInt8]()
    append_uint(payload, HELLO)
    append_uint(payload, UInt64(PROTOCOL_VERSION))
    append_uint(payload, instance)
    append_uint(payload, 0)
    payload.append(1)
    append_text(payload, "zz Mojo")
    append_uint(payload, UInt64(4 if native_search else 3))
    append_text(payload, "client-terminal-v1")
    append_text(payload, "client-utf8-v1")
    append_text(payload, "control-plane-v2")
    if native_search:
        append_text(payload, "client-native-terminal-search-v1")
    payload.append(0)
    payload.append(UInt8(1 if origin else 0))
    if origin:
        append_uint(payload, origin.value())
    append_client_context(payload, context, legacy=False)
    append_uint(payload, UInt64(process))
    payload.append(UInt8(viewport))
    if viewport:
        append_uint(payload, UInt64(columns))
        append_uint(payload, UInt64(rows))
        append_uint(payload, UInt64(cell_width_px))
        append_uint(payload, UInt64(cell_height_px))
    append_uint(payload, 2)
    payload.append(1)
    append_uint(payload, options)
    append_uint(payload, 0)
    payload.append(1)
    if session.byte_length() > 0:
        payload.append(1)
        append_uint(payload, 0)
        append_text(payload, session)
    else:
        payload.append(0)
    if context:
        var blob = context.value().blob()
        append_bytes(payload, Span(blob))
    else:
        append_uint(payload, 0)
    return envelope(Span(payload))


@inline(.never)
def decode_stream_frame(var frame: List[UInt8]) raises -> StreamFrame:
    if validate_envelope(Span(frame)) != 0:
        raise Error("expected control lane frame")
    var reader = Decoder(frame^)
    var result = StreamFrame()
    result.kind = read_uint(reader, 4294967295)
    if result.kind > 75:
        raise Error("unknown protocol message tag")
    if result.kind == WELCOME:
        result.protocol_version = UInt16(read_uint(reader, 65535))
        if result.protocol_version != PROTOCOL_VERSION:
            raise Error("server welcome version mismatch")
        result.server_id = reader.uint()
        result.client_id = reader.uint()
        result.instance = reader.uint()
        result.capabilities = reader.uint()
        if result.capabilities & REQUIRED_CAPABILITIES != REQUIRED_CAPABILITIES:
            raise Error(
                "daemon lacks the control-plane or pane-frame capability"
            )
        result.clock = reader.uint()
    elif result.kind == BATCH:
        result.sequence = reader.uint()
        var count = read_count(reader, 4096)
        for _ in range(count):
            var nested = reader.bytes()
            var lane = validate_envelope(Span(nested))
            if lane == 0:
                var check = Decoder(nested.copy())
                if check.uint() == BATCH:
                    raise Error("nested protocol batches are forbidden")
            result.batch.append(nested^)
    elif result.kind == PING or result.kind == PONG:
        result.ping_token = reader.uint()
        if result.kind == PING:
            result.rtt_us = UInt32(read_uint(reader, 4294967295))
    elif result.kind == EVENT:
        result.sequence = reader.uint()
        var kind = read_uint(reader, 4294967295)
        if kind > 65:
            raise Error("unknown protocol event tag")
        result.event_kind = kind
        if kind == SNAPSHOT:
            result.snapshot = read_snapshot(reader)
        elif kind == TREE_DELTA:
            result.delta = read_delta(reader)
        elif kind == CLIENT_VIEW:
            result.view = read_view(reader)
    result.raw = reader.data^
    reader.data = List[UInt8]()
    return result^


def request_full(pane: UInt64) raises -> List[UInt8]:
    var payload: List[UInt8] = [13]
    append_uint(payload, pane)
    return envelope(Span(payload))


def attach_session(session: String) raises -> List[UInt8]:
    var payload: List[UInt8] = [4]
    append_text(payload, session)
    return envelope(Span(payload))


def tree_sync() raises -> List[UInt8]:
    var payload: List[UInt8] = [53]
    return envelope(Span(payload))


def input_text(pane: UInt64, text: String) raises -> List[UInt8]:
    var payload: List[UInt8] = [9, 0]
    append_uint(payload, pane)
    append_text(payload, text)
    return envelope(Span(payload))


def resize_terminal(
    pane: UInt64,
    columns: UInt16,
    rows: UInt16,
    cell_width_px: UInt32,
    cell_height_px: UInt32,
    layout_generation: Optional[UInt64] = None,
) raises -> List[UInt8]:
    var payload: List[UInt8] = [9]
    append_uint(payload, UInt64(22 if layout_generation else 4))
    append_uint(payload, pane)
    append_uint(payload, UInt64(columns))
    append_uint(payload, UInt64(rows))
    append_uint(payload, UInt64(cell_width_px))
    append_uint(payload, UInt64(cell_height_px))
    if layout_generation:
        append_uint(payload, layout_generation.value())
    return envelope(Span(payload))


def client_terminal_size(
    columns: UInt16, rows: UInt16, layout_generation: Optional[UInt64] = None
) raises -> List[UInt8]:
    var payload: List[UInt8] = [9]
    append_uint(payload, UInt64(23 if layout_generation else 17))
    append_uint(payload, UInt64(columns))
    append_uint(payload, UInt64(rows))
    if layout_generation:
        append_uint(payload, layout_generation.value())
    return envelope(Span(payload))


def client_focus(focused: Bool) raises -> List[UInt8]:
    var payload: List[UInt8] = [9, 18]
    payload.append(UInt8(1 if focused else 0))
    return envelope(Span(payload))


def pong(token: UInt64) raises -> List[UInt8]:
    var payload: List[UInt8] = [69]
    append_uint(payload, token)
    return envelope(Span(payload))


@inline(.never)
def encode_stream_frame(frame: StreamFrame) raises -> List[UInt8]:
    from .tree_encode import write_snapshot, write_delta, write_view

    var payload = List[UInt8]()
    append_uint(payload, frame.kind)
    if frame.kind == WELCOME:
        append_uint(payload, UInt64(frame.protocol_version))
        append_uint(payload, frame.server_id)
        append_uint(payload, frame.client_id)
        append_uint(payload, frame.instance)
        append_uint(payload, frame.capabilities)
        append_uint(payload, frame.clock)
    elif frame.kind == BATCH:
        append_uint(payload, frame.sequence)
        append_uint(payload, UInt64(len(frame.batch)))
        for nested in frame.batch:
            append_bytes(payload, Span(nested))
    elif frame.kind == PING or frame.kind == PONG:
        append_uint(payload, frame.ping_token)
        if frame.kind == PING:
            append_uint(payload, UInt64(frame.rtt_us))
    elif frame.kind == EVENT and frame.event_kind:
        append_uint(payload, frame.sequence)
        append_uint(payload, frame.event_kind.value())
        if frame.event_kind.value() == SNAPSHOT and frame.snapshot:
            write_snapshot(payload, frame.snapshot.value())
        elif frame.event_kind.value() == TREE_DELTA and frame.delta:
            write_delta(payload, frame.delta.value())
        elif frame.event_kind.value() == CLIENT_VIEW and frame.view:
            write_view(payload, frame.view.value())
        else:
            return frame.raw.copy()
    else:
        return frame.raw.copy()
    return envelope(Span(payload))
