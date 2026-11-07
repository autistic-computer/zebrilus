from .client_context import ClientContext

comptime PROTOCOL_VERSION: UInt16 = 118
comptime MAX_FRAME_BYTES: Int = 64 * 1024 * 1024


def parse_id(text: StringSpan, prefix: UInt8) raises -> UInt64:
    var bytes = text.as_bytes()
    if len(bytes) < 2 or bytes[0] != prefix:
        raise Error("invalid stable identifier prefix")
    var start = 2 if bytes[1] == 43 else 1
    if start == len(bytes):
        raise Error("invalid stable identifier digit")
    var result: UInt64 = 0
    for index in range(start, len(bytes)):
        var digit = bytes[index]
        if digit < 48 or digit > 57:
            raise Error("invalid stable identifier digit")
        var value = UInt64(digit - 48)
        if result > (UInt64.MAX - value) // 10:
            raise Error("stable identifier overflow")
        result = result * 10 + value
    return result


def append_uint(mut output: List[UInt8], var value: UInt64):
    while value >= 128:
        output.append(UInt8(value & 127) | 128)
        value >>= 7
    output.append(UInt8(value))


def append_bytes(mut output: List[UInt8], bytes: Span[UInt8, _]):
    append_uint(output, UInt64(len(bytes)))
    output.extend(bytes)


def append_text(mut output: List[UInt8], text: String):
    append_bytes(output, text.as_bytes())


def envelope(payload: Span[UInt8, _], lane: UInt8 = 0) raises -> List[UInt8]:
    var following = len(payload) + 4
    if following > MAX_FRAME_BYTES:
        raise Error("protocol frame too large")
    if lane > 1:
        raise Error("unsupported protocol lane")
    var output = List[UInt8](capacity=following + 4)
    for shift in range(0, 32, 8):
        output.append(UInt8((following >> shift) & 255))
    output.append(lane)
    output.append(0)
    output.append(UInt8(PROTOCOL_VERSION & 255))
    output.append(UInt8(PROTOCOL_VERSION >> 8))
    output.extend(payload)
    return output^


def validate_envelope(frame: Span[UInt8, _]) raises -> UInt8:
    if len(frame) < 4:
        raise Error("protocol frame truncated")
    var following: UInt32 = 0
    for index in range(4):
        following |= UInt32(frame[index]) << UInt32(index * 8)
    if following > UInt32(MAX_FRAME_BYTES):
        raise Error("protocol frame too large")
    if len(frame) - 4 != Int(following):
        raise Error("protocol frame length mismatch")
    if following < 4:
        raise Error("protocol frame truncated")
    if frame[4] > 1:
        raise Error("unsupported protocol lane")
    var version = UInt16(frame[6]) | (UInt16(frame[7]) << 8)
    if version != PROTOCOL_VERSION:
        raise Error("protocol version mismatch: received " + String(version))
    if frame[5] != 0:
        raise Error("unsupported protocol flags")
    return frame[4]


struct Decoder(Movable):
    var data: List[UInt8]
    var offset: Int

    def __init__(out self, var data: List[UInt8]):
        self.data = data^
        self.offset = 8

    def byte(mut self) raises -> UInt8:
        if self.offset == len(self.data):
            raise Error("truncated postcard payload")
        var value = self.data[self.offset]
        self.offset += 1
        return value

    def uint(mut self) raises -> UInt64:
        var value: UInt64 = 0
        for index in range(10):
            var part = self.byte()
            if index == 9 and part > 1:
                raise Error("postcard integer overflow")
            value |= UInt64(part & 127) << UInt64(index * 7)
            if part < 128:
                return value
        raise Error("postcard integer overflow")

    def bytes(mut self) raises -> List[UInt8]:
        var count = self.uint()
        if count > UInt64(len(self.data) - self.offset):
            raise Error("truncated postcard byte string")
        var output = List[UInt8](
            Span(self.data)[self.offset : self.offset + Int(count)]
        )
        self.offset += Int(count)
        return output^

    def text(mut self) raises -> String:
        var bytes = self.bytes()
        return String(from_utf8=Span(bytes))

    def finish(self) raises:
        if self.offset != len(self.data):
            raise Error("trailing postcard bytes")


def append_client_context(
    mut payload: List[UInt8],
    context: Optional[ClientContext],
    legacy: Bool = True,
) raises:
    if not context:
        payload.append(0)
        append_uint(payload, 0)
        return
    ref details = context.value()
    details.validate()
    payload.append(UInt8(Bool(details.cwd)))
    if details.cwd:
        append_bytes(payload, Span(details.cwd.value()))
    append_uint(payload, UInt64(len(details.environment)) if legacy else 0)
    if legacy:
        for entry in details.environment:
            append_bytes(payload, Span(entry))


def client_hello(
    instance: UInt64,
    process: UInt32,
    origin: Optional[UInt64] = None,
    context: Optional[ClientContext] = None,
) raises -> List[UInt8]:
    var payload: List[UInt8] = [0]
    append_uint(payload, UInt64(PROTOCOL_VERSION))
    append_uint(payload, instance)
    append_uint(payload, 1)
    payload.append(1)
    append_text(payload, "zz Mojo")
    append_uint(payload, 1)
    append_text(payload, "client-utf8-v1")
    payload.append(0)
    if origin:
        payload.append(1)
        append_uint(payload, origin.value())
    else:
        payload.append(0)
    append_client_context(payload, context)
    append_uint(payload, UInt64(process))
    return envelope(Span(payload))


def command_request(
    request: UInt64, name: String, args: Span[String, _],
    stdin_available: Bool = False,
) raises -> List[UInt8]:
    if name.byte_length() == 0:
        raise Error("empty command")
    var payload: List[UInt8] = [2]
    append_uint(payload, request)
    append_text(payload, name)
    append_uint(payload, UInt64(len(args)))
    for argument in args:
        append_text(payload, argument)
    payload.extend([0, 0, 0, 0, UInt8(stdin_available), 0])
    return envelope(Span(payload))


def control_uint(frame: Span[UInt8, _], field: Int = 0) raises -> UInt32:
    if len(frame) < 9 or frame[4] != 0:
        raise Error("expected control frame")
    if field < 0 or field > 1:
        raise Error("unsupported control prefix field")
    var offset = 8
    var tag: UInt32 = 0
    for _ in range(field + 1):
        tag = 0
        var done = False
        for index in range(5):
            if offset >= len(frame):
                raise Error("truncated control integer")
            var byte = frame[offset]
            offset += 1
            if index == 4 and byte > 15:
                raise Error("control integer exceeds u32 range")
            tag |= UInt32(byte & 127) << UInt32(index * 7)
            if byte < 128:
                done = True
                break
        if not done:
            raise Error("control integer exceeds u32 range")
    return tag


def control_tag(frame: Span[UInt8, _]) raises -> UInt32:
    return control_uint(frame)


@fieldwise_init
struct Response(Copyable, Movable):
    var request: UInt64
    var output: List[UInt8]
    var error: String
    var exit_code: UInt8
    var stdout_claim: UInt8


def decode_error(
    mut decoder: Decoder, cli_errors: Bool = False
) raises -> Tuple[String, UInt8]:
    var prefix: String
    for _ in range(32):
        var tag = decoder.uint()
        if tag == 13:
            continue
        if tag == 0:
            var client = decoder.uint()
            var server = decoder.uint()
            return ((
                "server protocol mismatch: client "
                + String(client)
                + ", server "
                + String(server)
            ), UInt8(1))
        if tag == 6 or tag == 7:
            var pane = decoder.uint()
            return ((
                "pane is not attached: %" if tag == 6 else "pane has exited: %"
            ) + String(pane), UInt8(1))
        if tag == 1:
            prefix = "target not found: "
        elif tag == 2:
            prefix = "ambiguous target: "
        elif tag == 3:
            prefix = "invalid target: "
        elif tag == 4 or tag == 15:
            prefix = "unsupported command: "
        elif tag == 5 or tag == 12 or tag == 14 or tag == 16:
            prefix = "" if cli_errors else "invalid command: "
        elif tag == 8:
            prefix = "internal server error: "
        elif tag == 9:
            prefix = "can't find session: "
        elif tag == 10:
            prefix = "can't find window: "
        elif tag == 11:
            prefix = "can't find pane: "
        else:
            raise Error("unknown server error tag")
        return prefix + decoder.text(), UInt8(2 if tag == 14 or tag == 15 or tag == 16 else 1)
    raise Error("server error nesting exceeds limit")


def decode_response(
    var frame: List[UInt8], cli_errors: Bool = False
) raises -> Response:
    if validate_envelope(Span(frame)) != 0:
        raise Error("command response on terminal lane")
    var decoder = Decoder(frame^)
    if decoder.uint() != 3:
        raise Error("expected command response")
    var kind = decoder.uint()
    var request = decoder.uint()
    if kind == 0:
        var output = decoder.bytes()
        var exit_code = decoder.byte()
        var error = decoder.text()
        var claim = decoder.uint()
        if claim > 2:
            raise Error("unknown stdout claim")
        return Response(request, output^, error^, exit_code, UInt8(claim))
    if kind == 1:
        var error, code = decode_error(decoder, cli_errors)
        var output = decoder.bytes()
        return Response(request, output^, error^, code, 0)
    raise Error("unknown command response variant")
