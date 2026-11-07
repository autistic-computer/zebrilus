from .protocol import append_uint, envelope


def history_request(
    pane: UInt64, start: UInt32, count: UInt32
) raises -> List[UInt8]:
    var payload: List[UInt8] = [14]
    append_uint(payload, pane)
    append_uint(payload, UInt64(start))
    append_uint(payload, UInt64(count))
    return envelope(Span(payload))


def parse_history_budget(value: String) -> UInt64:
    var bytes = value.as_bytes()
    var start = 1 if len(bytes) > 0 and bytes[0] == 43 else 0
    if start == len(bytes):
        return 0
    var result: UInt64 = 0
    for index in range(start, len(bytes)):
        if bytes[index] < 48 or bytes[index] > 57:
            return 0
        var digit = UInt64(bytes[index] - 48)
        if result > (UInt64.MAX - digit) // 10:
            return 0
        result = result * 10 + digit
    return min(result, UInt64(10_000))


def decode_history_budget(var frame: List[UInt8]) raises -> Optional[UInt64]:
    from .protocol import Decoder, validate_envelope
    from .tree_wire import read_count, read_enum, read_text

    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if reader.uint() != 10:
        return None
    _ = reader.uint()
    if reader.uint() != 2:
        return None
    var count = read_count(reader)
    var result: Optional[UInt64] = None
    for _ in range(count):
        var key = read_enum(reader, 20)
        var value = read_text(reader, 65536)
        _ = read_enum(reader, 3)
        if key == 9:
            result = parse_history_budget(value)
    reader.finish()
    return result
