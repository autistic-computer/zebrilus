from std.ffi import external_call, OwnedDLHandle
from std.sys.info import CompilationTarget

comptime MAX_CONTEXT_PATH = 16384
comptime MAX_CONTEXT_ENTRY = 16367
comptime MAX_CONTEXT_ENTRIES = 4096
comptime MAX_CONTEXT_BYTES = 4 * 1024 * 1024
comptime _BytePointer = Pointer[UInt8, MutUntrackedOrigin]
comptime _OptionalBytePointer = Optional[_BytePointer]
comptime _EnvironmentPointer = Pointer[_OptionalBytePointer, MutUntrackedOrigin]


def _name_key(name: Span[UInt8, _]) raises -> String:
    var digits = String("0123456789abcdef")
    var bytes = List[UInt8](capacity=len(name) * 2)
    for byte in name:
        bytes.append(digits.as_bytes()[Int(byte >> 4)])
        bytes.append(digits.as_bytes()[Int(byte & 15)])
    return String(from_utf8=Span(bytes))


def normalize_environment(
    entries: List[List[UInt8]],
) raises -> List[List[UInt8]]:
    var values = Dict[String, List[UInt8]]()
    for entry in entries:
        var separator = -1
        var valid = len(entry) <= MAX_CONTEXT_ENTRY
        for index in range(len(entry)):
            if entry[index] == 0:
                valid = False
            if entry[index] == 61 and separator < 0:
                separator = index
        if separator <= 0 or separator >= MAX_CONTEXT_ENTRY:
            continue
        var key = _name_key(Span(entry)[:separator])
        values[key] = entry.copy() if valid else List[UInt8]()
    var keys = List[String](capacity=len(values))
    for key in values:
        keys.append(key)
    sort(Span(keys))
    var result = List[List[UInt8]]()
    var total = 0
    for key in keys:
        ref value = values[key]
        if len(result) == MAX_CONTEXT_ENTRIES:
            break
        if len(value) == 0 or len(value) > MAX_CONTEXT_BYTES - total:
            continue
        total += len(value)
        result.append(value.copy())
    return result^


def _copy_c_string(
    pointer: _OptionalBytePointer, maximum: Int
) raises -> Optional[List[UInt8]]:
    if not pointer:
        return None
    var length = external_call["strnlen", Int](pointer.value(), maximum + 1)
    if length > maximum:
        return None
    return List[UInt8](Span(unsafe_ptr=pointer.value(), length=length))


def _environment_pointer() raises -> _EnvironmentPointer:
    var library = OwnedDLHandle()
    comptime if CompilationTarget.is_linux():
        var symbol = library.get_symbol[_EnvironmentPointer]("environ")
        if not symbol:
            raise Error("process environment is unavailable")
        return symbol.value()[]
    else:
        var environment = library.get_function[
            Pointer[_EnvironmentPointer, MutUntrackedOrigin]
        ]("_NSGetEnviron")
        return environment()[]


def process_environment() raises -> List[List[UInt8]]:
    var entries = List[List[UInt8]]()
    var pointer = _environment_pointer()
    var index = 0
    while pointer[unsafe_offset=index]:
        var bytes = pointer[unsafe_offset=index].value()
        var entry = _copy_c_string(
            _OptionalBytePointer(bytes), MAX_CONTEXT_ENTRY
        )
        if entry:
            entries.append(entry.value().copy())
        else:
            var prefix = List[UInt8]()
            for position in range(MAX_CONTEXT_ENTRY):
                prefix.append(bytes[unsafe_offset=position])
                if bytes[unsafe_offset=position] == 61:
                    prefix.append(0)
                    entries.append(prefix^)
                    break
        index += 1
    return normalize_environment(entries)


def working_directory() raises -> Optional[List[UInt8]]:
    var library = OwnedDLHandle()
    var getcwd = library.get_function[_OptionalBytePointer]("getcwd")
    var buffer = List[UInt8](length=MAX_CONTEXT_PATH + 1, fill=0)
    var pointer = getcwd(buffer.unsafe_ptr(), len(buffer))
    if not pointer:
        return None
    var physical = _copy_c_string(pointer, MAX_CONTEXT_PATH)
    _ = buffer
    var name = String("PWD")
    var getenv = library.get_function[_OptionalBytePointer]("getenv")
    var declared_pointer = getenv(name.as_c_string_span().ptr())
    _ = name
    var declared = _copy_c_string(declared_pointer, MAX_CONTEXT_PATH)
    if Bool(declared) and len(declared.value()) > 0:
        var declared_path = declared.value().copy()
        declared_path.append(0)
        var realpath = library.get_function[_OptionalBytePointer]("realpath")
        var resolved = realpath(
            declared_path.unsafe_ptr(), _OptionalBytePointer(None)
        )
        _ = declared_path
        if resolved:
            var canonical = _copy_c_string(resolved, MAX_CONTEXT_PATH)
            _ = external_call["free", NoneType](resolved.value())
            if (
                Bool(canonical)
                and Bool(physical)
                and canonical.value() == physical.value()
            ):
                return declared^
    return physical^


struct ClientContext(Copyable, Movable):
    var cwd: Optional[List[UInt8]]
    var environment: List[List[UInt8]]

    def __init__(out self):
        self.cwd = None
        self.environment = List[List[UInt8]]()

    @staticmethod
    def capture() raises -> Self:
        var context = Self()
        context.cwd = working_directory()
        context.environment = process_environment()
        return context^

    def blob(self) -> List[UInt8]:
        var bytes = List[UInt8]()
        for entry in self.environment:
            bytes.extend(Span(entry))
            bytes.append(0)
        return bytes^

    def validate(self) raises:
        if Bool(self.cwd) and len(self.cwd.value()) > MAX_CONTEXT_PATH:
            raise Error("client working directory exceeds its byte limit")
        if len(self.environment) > MAX_CONTEXT_ENTRIES:
            raise Error("client environment exceeds its entry limit")
        var total = 0
        for entry in self.environment:
            if len(entry) > MAX_CONTEXT_ENTRY:
                raise Error("client environment entry exceeds its byte limit")
            var separator = -1
            for index in range(len(entry)):
                if entry[index] == 0:
                    raise Error("client environment entry contains a null byte")
                if entry[index] == 61 and separator < 0:
                    separator = index
            if separator <= 0:
                raise Error("client environment entry has no valid name")
            total += len(entry)
            if total > MAX_CONTEXT_BYTES:
                raise Error("client environment exceeds its byte limit")
