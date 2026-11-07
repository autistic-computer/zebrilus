from .protocol import (
    PROTOCOL_VERSION,
    Decoder,
    append_uint,
    append_text,
    append_bytes,
    envelope,
    validate_envelope,
    decode_error,
)
from .tree_wire import (
    read_uint,
    read_enum,
    read_bool,
    read_text,
    read_count,
    read_optional_id,
)
from .client_context import (
    MAX_CONTEXT_PATH,
    MAX_CONTEXT_BYTES,
    MAX_CONTEXT_ENTRIES,
    MAX_CONTEXT_ENTRY,
)

comptime EXEC: UInt64 = 48
comptime EXEC_EXIT: UInt64 = 49
comptime EXEC_UTF8: UInt8 = 1
comptime EXEC_STDIN: UInt8 = 2
comptime EXEC_NESTED: UInt8 = 4
comptime EXEC_RESUME: UInt8 = 8
comptime EXEC_PREPARED: UInt8 = 16
comptime EXEC_LAST: UInt8 = 32
comptime MAX_EXEC_TTY_BYTES = 1024


@fieldwise_init
struct CommandSource(Copyable, Movable):
    var source: String
    var line: UInt32
    var column: UInt32


struct Invocation(Copyable, Movable):
    var name: String
    var args: List[List[UInt8]]
    var source: Optional[CommandSource]
    var blocks: List[UInt32]
    var expanded_alias: Bool
    var stdin: Optional[List[UInt8]]
    var stdin_available: Bool

    def __init__(out self):
        self.name = String()
        self.args = List[List[UInt8]]()
        self.source = None
        self.blocks = List[UInt32]()
        self.expanded_alias = False
        self.stdin = None
        self.stdin_available = False


def read_invocation(mut reader: Decoder) raises -> Invocation:
    var command = Invocation()
    command.name = read_text(reader)
    var count = read_count(reader)
    for _ in range(count):
        command.args.append(reader.bytes())
    if read_bool(reader):
        command.source = CommandSource(
            read_text(reader),
            UInt32(read_uint(reader, 4294967295)),
            UInt32(read_uint(reader, 4294967295)),
        )
    var blocks = read_count(reader)
    for _ in range(blocks):
        command.blocks.append(UInt32(read_uint(reader, 4294967295)))
    command.expanded_alias = read_bool(reader)
    if read_bool(reader):
        command.stdin = reader.bytes()
    command.stdin_available = read_bool(reader)
    return command^


def append_invocation(mut output: List[UInt8], command: Invocation):
    append_text(output, command.name)
    append_uint(output, UInt64(len(command.args)))
    for arg in command.args:
        append_bytes(output, Span(arg))
    output.append(UInt8(Bool(command.source)))
    if command.source:
        ref source = command.source.value()
        append_text(output, source.source)
        append_uint(output, UInt64(source.line))
        append_uint(output, UInt64(source.column))
    append_uint(output, UInt64(len(command.blocks)))
    for block in command.blocks:
        append_uint(output, UInt64(block))
    output.append(UInt8(command.expanded_alias))
    output.append(UInt8(Bool(command.stdin)))
    if command.stdin:
        append_bytes(output, Span(command.stdin.value()))
    output.append(UInt8(command.stdin_available))


def split_invocations(words: Span[List[UInt8], _]) raises -> List[Invocation]:
    var commands = List[Invocation]()
    var current = Invocation()
    var has_name = False
    for word in words:
        var bytes = word.copy()
        var end = False
        if len(bytes) > 0 and bytes[len(bytes) - 1] == 59:
            _ = bytes.pop()
            if len(bytes) > 0 and bytes[len(bytes) - 1] == 92:
                _ = bytes.pop()
                bytes.append(59)
            else:
                end = True
        if not end or len(bytes) > 0:
            if not has_name:
                current.name = String(from_utf8=Span(bytes))
                has_name = True
            else:
                current.args.append(bytes^)
        if end and has_name:
            commands.append(current^)
            current = Invocation()
            has_name = False
    if has_name:
        commands.append(current^)
    return commands^


def _append_optional_id(mut output: List[UInt8], id: Optional[UInt64]):
    output.append(UInt8(Bool(id)))
    if id:
        append_uint(output, id.value())


def _read_optional_bytes(
    mut reader: Decoder, maximum: Int
) raises -> Optional[List[UInt8]]:
    if not read_bool(reader):
        return None
    var size = read_count(reader, maximum)
    var bytes = List[UInt8](
        Span(reader.data)[reader.offset : reader.offset + size]
    )
    reader.offset += size
    return bytes^


def valid_exec_environment(bytes: Span[UInt8, _]) -> Bool:
    if len(bytes) > MAX_CONTEXT_BYTES + MAX_CONTEXT_ENTRIES:
        return False
    var count = 0
    var total = 0
    var start = 0
    for end in range(len(bytes) + 1):
        if end < len(bytes) and bytes[end] != 0:
            continue
        var length = end - start
        if length > 0:
            count += 1
            total += length
            if (
                count > MAX_CONTEXT_ENTRIES
                or total > MAX_CONTEXT_BYTES
                or length > MAX_CONTEXT_ENTRY
            ):
                return False
            var separator = False
            for index in range(start + 1, end):
                if bytes[index] == 61:
                    separator = True
                    break
            if bytes[start] == 61 or not separator:
                return False
        start = end + 1
    return True


struct ExecRequest(Copyable, Movable):
    var version: UInt16
    var flags: UInt8
    var instance: UInt64
    var origin: Optional[UInt64]
    var cwd: Optional[List[UInt8]]
    var tty: Optional[String]
    var size: Optional[Tuple[UInt16, UInt16]]
    var features: UInt32
    var startup_reentry: Optional[UInt64]
    var spawned_server: Optional[UInt64]
    var expected_server: Optional[UInt64]
    var process: UInt32
    var environment: List[UInt8]
    var commands: List[Invocation]
    var raw_control_line: Optional[String]

    def __init__(out self):
        self.version = PROTOCOL_VERSION
        self.flags = 0
        self.instance = 0
        self.origin = None
        self.cwd = None
        self.tty = None
        self.size = None
        self.features = 0
        self.startup_reentry = None
        self.spawned_server = None
        self.expected_server = None
        self.process = 0
        self.environment = List[UInt8]()
        self.commands = List[Invocation]()
        self.raw_control_line = None

    def validate(self) raises:
        if Bool(self.cwd) and len(self.cwd.value()) > MAX_CONTEXT_PATH:
            raise Error("exec working directory exceeds byte limit")
        if (
            Bool(self.tty)
            and self.tty.value().byte_length() > MAX_EXEC_TTY_BYTES
        ):
            raise Error("exec terminal name exceeds byte limit")
        if not valid_exec_environment(Span(self.environment)):
            raise Error("invalid exec environment")
        if (
            Bool(self.raw_control_line)
            and self.raw_control_line.value().byte_length() > 65536
        ):
            raise Error("exec control line exceeds byte limit")


def decode_exec(var frame: List[UInt8]) raises -> Optional[ExecRequest]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_uint(reader, 4294967295) != EXEC:
        return None
    var request = ExecRequest()
    request.version = UInt16(read_uint(reader, 65535))
    request.flags = reader.byte()
    request.instance = reader.uint()
    request.origin = read_optional_id(reader)
    request.cwd = _read_optional_bytes(reader, MAX_CONTEXT_PATH)
    if read_bool(reader):
        request.tty = read_text(reader, MAX_EXEC_TTY_BYTES)
    if read_bool(reader):
        request.size = (
            UInt16(read_uint(reader, 65535)),
            UInt16(read_uint(reader, 65535)),
        )
    request.features = UInt32(read_uint(reader, 4294967295))
    request.startup_reentry = read_optional_id(reader)
    request.spawned_server = read_optional_id(reader)
    request.expected_server = read_optional_id(reader)
    request.process = UInt32(read_uint(reader, 4294967295))
    var count = read_count(reader, MAX_CONTEXT_BYTES + MAX_CONTEXT_ENTRIES)
    request.environment = List[UInt8](
        Span(reader.data)[reader.offset : reader.offset + count]
    )
    reader.offset += count
    count = read_count(reader)
    for _ in range(count):
        request.commands.append(read_invocation(reader))
    if read_bool(reader):
        request.raw_control_line = read_text(reader, 65536)
    request.validate()
    return request^


def encode_exec(request: ExecRequest) raises -> List[UInt8]:
    request.validate()
    var output = List[UInt8]()
    append_uint(output, EXEC)
    append_uint(output, UInt64(request.version))
    output.append(request.flags)
    append_uint(output, request.instance)
    _append_optional_id(output, request.origin)
    output.append(UInt8(Bool(request.cwd)))
    if request.cwd:
        append_bytes(output, Span(request.cwd.value()))
    output.append(UInt8(Bool(request.tty)))
    if request.tty:
        append_text(output, request.tty.value())
    output.append(UInt8(Bool(request.size)))
    if request.size:
        append_uint(output, UInt64(request.size.value()[0]))
        append_uint(output, UInt64(request.size.value()[1]))
    append_uint(output, UInt64(request.features))
    _append_optional_id(output, request.startup_reentry)
    _append_optional_id(output, request.spawned_server)
    _append_optional_id(output, request.expected_server)
    append_uint(output, UInt64(request.process))
    append_bytes(output, Span(request.environment))
    append_uint(output, UInt64(len(request.commands)))
    for command in request.commands:
        append_invocation(output, command)
    output.append(UInt8(Bool(request.raw_control_line)))
    if request.raw_control_line:
        append_text(output, request.raw_control_line.value())
    return envelope(Span(output))


@fieldwise_init
struct PreparedInvocation(Copyable, Movable):
    var invocation: Invocation
    var canonical: Optional[String]
    var alias_matched: Bool
    var failure: Optional[List[UInt8]]
    var error: String
    var code: UInt8


struct ExecExit(Copyable, Movable):
    var server: UInt64
    var outcome: UInt8
    var resume_kind: UInt8
    var commands: List[PreparedInvocation]
    var failure: List[UInt8]
    var error: String
    var code: UInt8

    def __init__(out self):
        self.server = 0
        self.outcome = 0
        self.resume_kind = 0
        self.commands = List[PreparedInvocation]()
        self.failure = List[UInt8]()
        self.error = String()
        self.code = 0


@fieldwise_init
struct _Failure(Movable):
    var bytes: Optional[List[UInt8]]
    var message: String
    var code: UInt8


def _read_failure(mut reader: Decoder) raises -> _Failure:
    var start = reader.offset
    var message, code = decode_error(reader, cli_errors=True)
    var end = reader.offset
    reader.offset = start
    var bytes = List[UInt8]()
    for _ in range(32):
        var tag = read_uint(reader, 16, 5)
        append_uint(bytes, tag)
        if tag == 13:
            continue
        if tag == 0:
            append_uint(bytes, read_uint(reader, 65535))
            append_uint(bytes, read_uint(reader, 65535))
        elif tag == 6 or tag == 7:
            append_uint(bytes, reader.uint())
        else:
            append_text(bytes, read_text(reader))
        break
    if reader.offset != end:
        raise Error("invalid server failure length")
    return _Failure(Optional[List[UInt8]](bytes^), message^, code)


def decode_exec_exit(var frame: List[UInt8]) raises -> Optional[ExecExit]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_uint(reader, 4294967295) != EXEC_EXIT:
        return None
    var exit = ExecExit()
    exit.server = reader.uint()
    exit.outcome = read_enum(reader, 3)
    if exit.outcome == 1:
        exit.resume_kind = read_enum(reader, 1)
        var count = read_count(reader)
        for _ in range(count):
            var command = read_invocation(reader)
            var canonical: Optional[String] = None
            if read_bool(reader):
                canonical = read_text(reader)
            var alias = read_bool(reader)
            var failed = read_enum(reader, 1)
            var failure: Optional[List[UInt8]] = None
            var message = String()
            var code: UInt8 = 0
            if failed == 1:
                var decoded = _read_failure(reader)
                code = decoded.code
                message = decoded.message.copy()
                failure = decoded.bytes.take()
            exit.commands.append(
                PreparedInvocation(
                    command^, canonical^, alias, failure^, message^, code
                )
            )
    elif exit.outcome == 2:
        var decoded = _read_failure(reader)
        exit.code = decoded.code
        exit.error = decoded.message.copy()
        exit.failure = decoded.bytes.take()
    return exit^


def _append_failure(mut output: List[UInt8], bytes: Span[UInt8, _]) raises:
    var frame = List[UInt8](length=8, fill=0)
    frame.extend(bytes)
    var reader = Decoder(frame^)
    var decoded = _read_failure(reader)
    reader.finish()
    output.extend(Span(decoded.bytes.value()))


def append_prepared(
    mut output: List[UInt8], command: PreparedInvocation
) raises:
    append_invocation(output, command.invocation)
    output.append(UInt8(Bool(command.canonical)))
    if command.canonical:
        append_text(output, command.canonical.value())
    output.append(UInt8(command.alias_matched))
    append_uint(output, UInt64(Bool(command.failure)))
    if command.failure:
        _append_failure(output, Span(command.failure.value()))


def encode_exec_exit(exit: ExecExit) raises -> List[UInt8]:
    if exit.outcome > 3 or (exit.outcome == 1 and exit.resume_kind > 1):
        raise Error("unknown exec outcome")
    var output = List[UInt8]()
    append_uint(output, EXEC_EXIT)
    append_uint(output, exit.server)
    append_uint(output, UInt64(exit.outcome))
    if exit.outcome == 1:
        append_uint(output, UInt64(exit.resume_kind))
        append_uint(output, UInt64(len(exit.commands)))
        for command in exit.commands:
            append_prepared(output, command)
    elif exit.outcome == 2:
        _append_failure(output, Span(exit.failure))
    return envelope(Span(output))


struct ExecNotice(Copyable, Movable):
    var kind: UInt8
    var sequence: UInt64
    var pane: Optional[UInt64]
    var message_kind: UInt8
    var text: String
    var data: List[UInt8]

    def __init__(out self):
        self.kind = 0
        self.sequence = 0
        self.pane = None
        self.message_kind = 0
        self.text = String()
        self.data = List[UInt8]()


def decode_exec_notice(var frame: List[UInt8]) raises -> Optional[ExecNotice]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_uint(reader, 4294967295) != 10:
        return None
    var notice = ExecNotice()
    notice.sequence = reader.uint()
    var tag = read_enum(reader, 65)
    if tag == 52:
        notice.kind = 0
        notice.data = reader.bytes()
    elif tag == 16:
        notice.kind = 1
        notice.pane = read_optional_id(reader)
        notice.message_kind = read_enum(reader, 3)
        notice.text = read_text(reader)
    elif tag == 53:
        notice.kind = 2
    else:
        return None
    return notice^


def encode_exec_notice(notice: ExecNotice) raises -> List[UInt8]:
    var output: List[UInt8] = [10]
    append_uint(output, notice.sequence)
    if notice.kind == 0:
        append_uint(output, 52)
        append_bytes(output, Span(notice.data))
    elif notice.kind == 1:
        if notice.message_kind > 3:
            raise Error("unknown client message kind")
        append_uint(output, 16)
        _append_optional_id(output, notice.pane)
        append_uint(output, UInt64(notice.message_kind))
        append_text(output, notice.text)
    elif notice.kind == 2:
        append_uint(output, 53)
    else:
        raise Error("unknown command execution notice")
    return envelope(Span(output))
