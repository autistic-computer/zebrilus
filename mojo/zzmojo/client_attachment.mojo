from .protocol import Decoder, append_uint, append_text, envelope, validate_envelope
from .tree_wire import read_enum, read_optional_text, read_text

comptime ATTACHMENT_MAX_TEXT = 1024 * 1024


struct DetachAction(Copyable, Movable):
    var kind: UInt8
    var command: String
    var shell: String

    def __init__(out self, kind: UInt8 = 0, var command: String = String(), var shell: String = String()):
        self.kind = kind
        self.command = command^
        self.shell = shell^


@fieldwise_init
struct ClientDetached(Copyable, Movable):
    var sequence: UInt64
    var session: UInt64
    var by: Optional[String]
    var reason: UInt8
    var action: DetachAction


def decode_detached(var frame: List[UInt8]) raises -> Optional[ClientDetached]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_enum(reader, 75) != 10:
        return None
    var sequence = reader.uint()
    if read_enum(reader, 65) != 22:
        return None
    var session = reader.uint()
    var by = read_optional_text(reader, ATTACHMENT_MAX_TEXT)
    var reason = read_enum(reader, 3)
    var action = DetachAction(read_enum(reader, 2))
    if action.kind == 2:
        action.command = read_text(reader, ATTACHMENT_MAX_TEXT)
        action.shell = read_text(reader, ATTACHMENT_MAX_TEXT)
    return ClientDetached(sequence, session, by^, reason, action^)


def encode_detached(value: ClientDetached) raises -> List[UInt8]:
    if value.reason > 3 or value.action.kind > 2:
        raise Error("unknown detach reason or action")
    if Bool(value.by) and value.by.value().byte_length() > ATTACHMENT_MAX_TEXT:
        raise Error("detach device name exceeds text limit")
    if value.action.kind == 2 and (value.action.command.byte_length() > ATTACHMENT_MAX_TEXT or value.action.shell.byte_length() > ATTACHMENT_MAX_TEXT):
        raise Error("detach action exceeds text limit")
    var output: List[UInt8] = [10]
    append_uint(output, value.sequence)
    append_uint(output, 22)
    append_uint(output, value.session)
    output.append(UInt8(Bool(value.by)))
    if value.by:
        append_text(output, value.by.value())
    append_uint(output, UInt64(value.reason))
    append_uint(output, UInt64(value.action.kind))
    if value.action.kind == 2:
        append_text(output, value.action.command)
        append_text(output, value.action.shell)
    return envelope(Span(output))


def decode_server_stopping(var frame: List[UInt8]) raises -> Optional[UInt64]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_enum(reader, 75) != 10:
        return None
    var sequence = reader.uint()
    if read_enum(reader, 65) != 18:
        return None
    return sequence


def server_stopping_event(sequence: UInt64) raises -> List[UInt8]:
    var output: List[UInt8] = [10]
    append_uint(output, sequence)
    append_uint(output, 18)
    return envelope(Span(output))
