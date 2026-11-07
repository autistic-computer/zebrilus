from std.ffi import external_call, OwnedDLHandle
from std.sys.info import CompilationTarget
from .client_context import (
    _environment_pointer,
    _copy_c_string,
    MAX_CONTEXT_ENTRY,
    MAX_CONTEXT_ENTRIES,
    MAX_CONTEXT_BYTES,
    working_directory,
)
from .exec_protocol import ExecRequest, EXEC_UTF8, EXEC_NESTED, EXEC_STDIN, EXEC_LAST
from .protocol import parse_id

comptime _BytePointer = Pointer[UInt8, MutUntrackedOrigin]


def capture_exec_environment() raises -> List[UInt8]:
    var pointer = _environment_pointer()
    var result = List[UInt8]()
    var count = 0
    var total = 0
    var index = 0
    while pointer[unsafe_offset=index] and count < MAX_CONTEXT_ENTRIES:
        var entry = _copy_c_string(pointer[unsafe_offset=index], MAX_CONTEXT_ENTRY)
        index += 1
        if not entry:
            continue
        ref bytes = entry.value()
        var separator = -1
        for position in range(len(bytes)):
            if bytes[position] == 61:
                separator = position
                break
        if separator <= 0 or len(bytes) > MAX_CONTEXT_BYTES - total:
            continue
        total += len(bytes)
        count += 1
        result.extend(Span(bytes))
        result.append(0)
    return result^


def _lookup(var name: String) raises -> Optional[List[UInt8]]:
    var library = OwnedDLHandle()
    var lookup = library.get_function[Optional[_BytePointer]]("getenv")
    var pointer = lookup(name.as_c_string_span().ptr())
    _ = name
    return _copy_c_string(pointer, MAX_CONTEXT_BYTES)


def _utf8_locale(bytes: Span[UInt8, _]) -> Bool:
    var uppercase = List[UInt8](bytes)
    for index in range(len(uppercase)):
        if uppercase[index] >= 97 and uppercase[index] <= 122:
            uppercase[index] -= 32
    var text = Span(uppercase)
    for start in range(len(text)):
        if len(text) - start >= 4 and text[start] == 85 and text[start + 1] == 84 and text[start + 2] == 70:
            if text[start + 3] == 56:
                return True
            if len(text) - start >= 5 and text[start + 3] == 45 and text[start + 4] == 56:
                return True
    return False


def caller_tty() raises -> Optional[String]:
    var library = OwnedDLHandle()
    var ttyname = library.get_function[Int32]("ttyname_r")
    var bytes = List[UInt8](length=1025, fill=0)
    for fd in range(3):
        if ttyname(Int32(fd), bytes.unsafe_ptr(), len(bytes)) != 0:
            continue
        var result = _copy_c_string(Optional[_BytePointer](bytes.unsafe_ptr().unsafe_origin_cast[MutUntrackedOrigin]()), 1024)
        _ = bytes
        if result:
            try:
                return String(from_utf8=Span(result.value()))
            except:
                return None
        return None
    return None


def caller_size() -> Optional[Tuple[UInt16, UInt16]]:
    comptime REQUEST: UInt = 0x5413 if CompilationTarget.is_linux() else 0x40087468
    var dimensions = List[UInt16](length=4, fill=0)
    var status = external_call["ioctl", Int32, num_fixed_args=2](
        Int32(1), REQUEST, dimensions.unsafe_ptr()
    )
    if status == 0 and dimensions[0] > 0 and dimensions[1] > 0:
        return (dimensions[1], dimensions[0])
    return None


def capture_exec_context(origin: Optional[UInt64] = None) raises -> ExecRequest:
    var request = ExecRequest()
    request.process = external_call["getpid", UInt32]()
    request.instance = UInt64(request.process)
    request.origin = origin
    request.cwd = working_directory()
    request.environment = capture_exec_environment()
    request.tty = caller_tty()
    request.size = caller_size()
    request.flags = EXEC_STDIN | EXEC_LAST
    var nested = _lookup("TMUX")
    if nested:
        request.flags |= EXEC_UTF8
        if len(nested.value()) > 0:
            request.flags |= EXEC_NESTED
    else:
        var names: List[String] = ["LC_ALL", "LC_CTYPE", "LANG"]
        for name in names:
            var value = _lookup(name.copy())
            if Bool(value) and len(value.value()) > 0:
                if _utf8_locale(Span(value.value())):
                    request.flags |= EXEC_UTF8
                break
    var reentry = _lookup("ZZ_STARTUP_REENTRY")
    if reentry:
        try:
            var token = String(from_utf8=Span(reentry.value()))
            request.startup_reentry = parse_id("%" + token, 37)
        except:
            pass
    return request^
