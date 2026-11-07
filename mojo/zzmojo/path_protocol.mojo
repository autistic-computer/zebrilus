from .protocol import Decoder, append_uint, append_text, envelope, validate_envelope
from .tree_wire import read_bool, read_enum, read_text, read_count, read_optional_id, read_optional_text


comptime MAX_PATH_TEXT: Int = 4096
comptime MAX_PATH_ENTRIES: Int = 50000
comptime PATH_REQUEST: UInt8 = 0
comptime PATH_BEGIN: UInt8 = 1
comptime PATH_CHUNK: UInt8 = 2
comptime PATH_GIT: UInt8 = 3
comptime PATH_CANCEL: UInt8 = 4
comptime PATH_OPEN: UInt8 = 5
comptime PATH_PICKER_EVENT: UInt8 = 55


@fieldwise_init
struct PathEntry(Copyable, Movable):
    var rel: String
    var kind: UInt8
    var symlink: Bool


@fieldwise_init
struct PathMark(Copyable, Movable):
    var rel: String
    var kind: UInt8


@fieldwise_init
struct PathRoot(Copyable, Movable):
    var root: String
    var display_root: String
    var cwd: Optional[String]
    var insert: UInt8


struct PathMessage(Copyable, Movable):
    var kind: UInt8
    var request: UInt64
    var pane: Optional[UInt64]
    var dir: Optional[String]
    var root: Optional[PathRoot]
    var error: Optional[String]
    var entries: List[PathEntry]
    var marks: List[PathMark]
    var done: Bool
    var truncated: Bool
    var sequence: UInt64

    def __init__(out self, kind: UInt8 = PATH_REQUEST, request: UInt64 = 0):
        self.kind = kind
        self.request = request
        self.pane = None
        self.dir = None
        self.root = None
        self.error = None
        self.entries = List[PathEntry]()
        self.marks = List[PathMark]()
        self.done = False
        self.truncated = False
        self.sequence = 0


def _text(mut output: List[UInt8], value: String) raises:
    if value.byte_length() > MAX_PATH_TEXT:
        raise Error("path listing text exceeds wire limit")
    append_text(output, value)


def _optional_text(mut output: List[UInt8], value: Optional[String]) raises:
    output.append(UInt8(Bool(value)))
    if value:
        _text(output, value.value())


def _insert(mut output: List[UInt8], value: UInt8) raises:
    if value > 5:
        raise Error("unknown path insertion style")
    output.append(0 if value < 4 else value - 3)
    if value < 4:
        output.append(value)


def _read_insert(mut reader: Decoder) raises -> UInt8:
    var kind = read_enum(reader, 2)
    return read_enum(reader, 3) if kind == 0 else kind + 3


def encode_path_message(message: PathMessage) raises -> List[UInt8]:
    var output = List[UInt8]()
    if message.kind == PATH_OPEN:
        output.append(10)
        append_uint(output, message.sequence)
        output.append(PATH_PICKER_EVENT)
        if not message.pane:
            raise Error("path picker notification has no pane")
        append_uint(output, message.pane.value())
        _optional_text(output, message.dir)
        return envelope(Span(output))
    if message.kind > PATH_CANCEL:
        raise Error("unknown path listing message")
    output.append(43 + message.kind)
    append_uint(output, message.request)
    if message.kind == PATH_REQUEST:
        output.append(UInt8(Bool(message.pane)))
        if message.pane:
            append_uint(output, message.pane.value())
        _optional_text(output, message.dir)
    elif message.kind == PATH_BEGIN:
        if message.error:
            output.append(1)
            _text(output, message.error.value())
        elif message.root:
            output.append(0)
            ref root = message.root.value()
            _text(output, root.root)
            _text(output, root.display_root)
            _optional_text(output, root.cwd)
            _insert(output, root.insert)
        else:
            raise Error("path listing begin has no result")
    elif message.kind == PATH_CHUNK:
        if len(message.entries) > MAX_PATH_ENTRIES:
            raise Error("path listing entries exceed wire limit")
        append_uint(output, UInt64(len(message.entries)))
        for entry in message.entries:
            _text(output, entry.rel)
            if entry.kind > 1:
                raise Error("unknown path entry kind")
            output.append(entry.kind)
            output.append(UInt8(entry.symlink))
        output.append(UInt8(message.done))
        output.append(UInt8(message.truncated))
    elif message.kind == PATH_GIT:
        if len(message.marks) > MAX_PATH_ENTRIES:
            raise Error("path listing marks exceed wire limit")
        append_uint(output, UInt64(len(message.marks)))
        for mark in message.marks:
            _text(output, mark.rel)
            if mark.kind > 3:
                raise Error("unknown path git mark")
            output.append(mark.kind)
    return envelope(Span(output))


def decode_path_message(var bytes: List[UInt8]) raises -> Optional[PathMessage]:
    if validate_envelope(Span(bytes)) != 0:
        return None
    var reader = Decoder(bytes^)
    var tag = read_enum(reader, 75)
    var result = PathMessage()
    if tag == 10:
        result.sequence = reader.uint()
        if read_enum(reader, 65) != PATH_PICKER_EVENT:
            return None
        result.kind = PATH_OPEN
        result.pane = reader.uint()
        result.dir = read_optional_text(reader, MAX_PATH_TEXT)
        return result^
    if tag < 43 or tag > 47:
        return None
    result.kind = tag - 43
    result.request = reader.uint()
    if result.kind == PATH_REQUEST:
        result.pane = read_optional_id(reader)
        result.dir = read_optional_text(reader, MAX_PATH_TEXT)
    elif result.kind == PATH_BEGIN:
        if read_enum(reader, 1) == 1:
            result.error = read_text(reader, MAX_PATH_TEXT)
        else:
            var root = read_text(reader, MAX_PATH_TEXT)
            var display = read_text(reader, MAX_PATH_TEXT)
            var cwd = read_optional_text(reader, MAX_PATH_TEXT)
            var insert = _read_insert(reader)
            result.root = PathRoot(root^, display^, cwd^, insert)
    elif result.kind == PATH_CHUNK:
        var count = read_count(reader, MAX_PATH_ENTRIES)
        if count > (len(reader.data) - reader.offset) // 3:
            raise Error("path entry count exceeds remaining bytes")
        result.entries.reserve(count)
        for _ in range(count):
            var rel = read_text(reader, MAX_PATH_TEXT)
            var kind = read_enum(reader, 1)
            var symlink = read_bool(reader)
            result.entries.append(PathEntry(rel^, kind, symlink))
        result.done = read_bool(reader)
        result.truncated = read_bool(reader)
    elif result.kind == PATH_GIT:
        var count = read_count(reader, MAX_PATH_ENTRIES)
        if count > (len(reader.data) - reader.offset) // 2:
            raise Error("path mark count exceeds remaining bytes")
        result.marks.reserve(count)
        for _ in range(count):
            var rel = read_text(reader, MAX_PATH_TEXT)
            var kind = read_enum(reader, 3)
            result.marks.append(PathMark(rel^, kind))
    return result^
