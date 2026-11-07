from .protocol import (
    Decoder,
    append_uint,
    append_text,
    envelope,
    validate_envelope,
)
from .tree_wire import (
    read_enum,
    read_uint,
    read_count,
    read_text,
    read_bool,
    read_optional_text,
)
from .input_protocol import KeyInput, _append_key

comptime MAX_BROWSER_COMMAND_TOKENS = 4096
comptime MAX_BROWSER_COMMAND_BYTES = 4 * 1024 * 1024


@fieldwise_init
struct BrowserKeyToken(Copyable, Movable):
    var kind: UInt8
    var text: String
    var raw: UInt8


struct BrowserCommand(Copyable, Movable):
    var kind: UInt8
    var text: String
    var tokens: List[BrowserKeyToken]
    var key: Optional[KeyInput]
    var request_id: UInt64
    var count: UInt32

    def __init__(out self, kind: UInt8 = 0):
        self.kind = kind
        self.text = String()
        self.tokens = List[BrowserKeyToken]()
        self.key = None
        self.request_id = 0
        self.count = 0


@fieldwise_init
struct BrowserCommandEvent(Copyable, Movable):
    var sequence: UInt64
    var pane: UInt64
    var command: BrowserCommand


def append_browser_command(
    mut bytes: List[UInt8], command: BrowserCommand
) raises:
    if command.kind > 7 or len(command.tokens) > MAX_BROWSER_COMMAND_TOKENS:
        raise Error("browser command kind or token count exceeds admission")
    var retained = command.text.byte_length()
    for token in command.tokens:
        retained += token.text.byte_length() + 12
    if command.key and command.key.value().text:
        retained += command.key.value().text.value().byte_length()
    if retained > MAX_BROWSER_COMMAND_BYTES - len(bytes) - 128:
        raise Error("browser command exceeds encoding admission")
    append_uint(bytes, UInt64(command.kind))
    if command.kind == 0:
        append_text(bytes, command.text)
    elif command.kind == 4 or command.kind == 7:
        append_uint(bytes, UInt64(len(command.tokens)))
        for token in command.tokens:
            append_uint(bytes, UInt64(token.kind))
            if token.kind <= 1:
                append_text(bytes, token.text)
            elif token.kind == 2:
                bytes.append(token.raw)
            else:
                raise Error("unknown browser key token kind")
        if command.kind == 7:
            append_uint(bytes, UInt64(command.count))
    elif command.kind == 5:
        if not command.key:
            raise Error("browser key command has no key")
        _append_key(bytes, command.key.value())
    elif command.kind == 6:
        if command.text.byte_length() > 65536:
            raise Error("browser screenshot path exceeds source admission")
        append_uint(bytes, command.request_id)
        append_text(bytes, command.text)
    if len(bytes) > MAX_BROWSER_COMMAND_BYTES:
        raise Error("browser command exceeds retained byte admission")


def _browser_character(mut reader: Decoder) raises -> UInt32:
    var text = read_text(reader, 4)
    var count = 0
    var scalar: UInt32 = 0
    for character in text.codepoints():
        count += 1
        scalar = UInt32(Int(character))
    if count != 1:
        raise Error("browser key character must contain exactly one scalar")
    return scalar


def _browser_key(mut reader: Decoder) raises -> KeyInput:
    var input = KeyInput()
    input.action = read_enum(reader, 2)
    input.key = read_enum(reader, 17)
    if input.key == 0:
        input.codepoint = _browser_character(reader)
    elif input.key == 15:
        input.value = UInt16(reader.byte())
    elif input.key == 17:
        input.value = UInt16(read_uint(reader, 65535))
    input.modifiers = reader.byte()
    if input.modifiers > 15:
        raise Error("browser key modifiers contain reserved bits")
    input.text = read_optional_text(reader)
    if read_bool(reader):
        input.unshifted = _browser_character(reader)
    return input^


def read_browser_command(mut reader: Decoder) raises -> BrowserCommand:
    var command = BrowserCommand(read_enum(reader, 7))
    if command.kind == 0:
        command.text = read_text(reader, MAX_BROWSER_COMMAND_BYTES)
    elif command.kind == 4 or command.kind == 7:
        var count = read_count(reader, MAX_BROWSER_COMMAND_TOKENS)
        for _ in range(count):
            var token = BrowserKeyToken(read_enum(reader, 2), String(), 0)
            if token.kind <= 1:
                token.text = read_text(reader, MAX_BROWSER_COMMAND_BYTES)
            else:
                token.raw = reader.byte()
            command.tokens.append(token^)
        if command.kind == 7:
            command.count = UInt32(read_uint(reader, 4294967295))
    elif command.kind == 5:
        command.key = _browser_key(reader)
    elif command.kind == 6:
        command.request_id = reader.uint()
        command.text = read_text(reader, 65536)
    return command^


def browser_command_event(frame: List[UInt8]) raises -> BrowserCommandEvent:
    if len(frame) > MAX_BROWSER_COMMAND_BYTES:
        raise Error("browser event exceeds retained byte admission")
    if validate_envelope(Span(frame)) != 0:
        raise Error("browser command is not a control frame")
    var reader = Decoder(frame.copy())
    if reader.uint() != 10:
        raise Error("browser command is not an event")
    var sequence = reader.uint()
    if reader.uint() != 7:
        raise Error("event is not a browser command")
    var pane = reader.uint()
    var command = read_browser_command(reader)
    reader.finish()
    return BrowserCommandEvent(sequence, pane, command^)


def encode_browser_command_event(
    event: BrowserCommandEvent,
) raises -> List[UInt8]:
    var bytes: List[UInt8] = [10]
    append_uint(bytes, event.sequence)
    append_uint(bytes, 7)
    append_uint(bytes, event.pane)
    append_browser_command(bytes, event.command)
    return envelope(Span(bytes))
