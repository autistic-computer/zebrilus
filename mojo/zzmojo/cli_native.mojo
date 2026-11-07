from std.ffi import external_call, OwnedDLHandle
from std.sys.info import CompilationTarget
from .client_context import _copy_c_string, MAX_CONTEXT_BYTES
from .client_files import errno_text
from .cli_parse import default_socket, path_join

comptime _BytePointer = Pointer[UInt8, MutUntrackedOrigin]
comptime _ERRNO: StaticString = "__errno_location" if CompilationTarget.is_linux() else "__error"


def environment_value(var name: String) raises -> Optional[List[UInt8]]:
    var library = OwnedDLHandle()
    var getenv = library.get_function[Optional[_BytePointer]]("getenv")
    var pointer = getenv(name.as_c_string_span().ptr())
    _ = name
    if not pointer:
        return None
    var value = _copy_c_string(pointer, MAX_CONTEXT_BYTES)
    if not value:
        raise Error("environment value exceeds byte limit")
    return value^


def local_default_socket() raises -> List[UInt8]:
    var user: Optional[String] = None
    var raw_user = environment_value("USER")
    if raw_user:
        try:
            user = String(from_utf8=Span(raw_user.value()))
        except:
            pass
    var temporary = environment_value("TMPDIR")
    var root = temporary.value().copy() if temporary else List[UInt8](
        String("/tmp").as_bytes()
    )
    return default_socket(
        environment_value("ZZ_SOCKET"),
        environment_value("XDG_RUNTIME_DIR"),
        Span(root),
        user,
    )


def canonical_path(path: Span[UInt8, _]) raises -> Optional[List[UInt8]]:
    var library = OwnedDLHandle()
    var realpath = library.get_function[Optional[_BytePointer]]("realpath")
    var bytes = List[UInt8](path)
    bytes.append(0)
    var pointer = realpath(bytes.unsafe_ptr(), Optional[_BytePointer](None))
    _ = bytes
    if not pointer:
        return None
    var value = _copy_c_string(pointer, MAX_CONTEXT_BYTES)
    _ = external_call["free", NoneType](pointer.value())
    if not value:
        raise Error("canonical socket root exceeds byte limit")
    return value^


def _errno() -> Int32:
    return external_call[_ERRNO, Pointer[Int32, MutUntrackedOrigin]]()[]


def _fs_error(
    prefix: String, path: Span[UInt8, _], number: Int32
) raises -> String:
    return (
        prefix
        + String(from_utf8_lossy=path)
        + " ("
        + errno_text(number)
        + " (os error "
        + String(number)
        + "))"
    )


def label_socket(
    label: Span[UInt8, _], temporary: Optional[List[UInt8]]
) raises -> List[UInt8]:
    var root: Optional[List[UInt8]] = None
    if temporary and len(temporary.value()) > 0:
        root = canonical_path(Span(temporary.value()))
    if not root:
        root = canonical_path(String("/tmp").as_bytes())
    if not root:
        raise Error("no suitable socket path")
    var uid = external_call["getuid", UInt32]()
    var base_name = String("tmux-", uid)
    var base = path_join(Span(root.value()), base_name.as_bytes())
    var c_path = base.copy()
    c_path.append(0)
    var library = OwnedDLHandle()
    var mkdir = library.get_function[Int32]("mkdir")
    if mkdir(c_path.unsafe_ptr(), UInt32(0o700)) != 0:
        var number = _errno()
        if number != 17:
            raise Error(
                _fs_error("couldn't create directory ", Span(base), number)
            )
    var storage = List[UInt64](length=32, fill=0)
    var data = storage.unsafe_ptr().unsafe_bitcast[UInt8]()
    var status: Int32
    comptime mode_offset = 28 if CompilationTarget.is_linux() else 4
    comptime uid_offset = 20 if CompilationTarget.is_linux() else 16
    comptime if CompilationTarget.is_linux():
        var statx = library.get_function[Int32]("statx")
        status = statx(
            Int32(-100), c_path.unsafe_ptr(), Int32(0x100), UInt32(0x7FF), data
        )
    else:
        var lstat = library.get_function[Int32]("lstat")
        status = lstat(c_path.unsafe_ptr(), data)
    _ = c_path
    if status != 0:
        raise Error(_fs_error("couldn't read directory ", Span(base), _errno()))
    var mode: UInt32 = 0
    for index in range(2):
        mode |= UInt32(data[unsafe_offset=mode_offset + index]) << UInt32(
            index * 8
        )
    var owner: UInt32 = 0
    for index in range(4):
        owner |= UInt32(data[unsafe_offset=uid_offset + index]) << UInt32(
            index * 8
        )
    _ = storage
    if mode & 0xF000 != 0x4000:
        raise Error(String(from_utf8_lossy=Span(base)) + " is not a directory")
    if owner != uid or mode & 0o007 != 0:
        raise Error(
            "directory "
            + String(from_utf8_lossy=Span(base))
            + " has unsafe permissions"
        )
    return path_join(Span(base), label)


def connect_error(path: String, number: Int32) raises -> String:
    if number == Int32(111 if CompilationTarget.is_linux() else 61):
        return "no server running on " + path
    var message = (
        "error connecting to " + path + " (" + errno_text(number) + ")"
    )
    if number == 1:
        message += (
            ": a sandbox keeps this command away from the zz daemon; rerun it"
            " with escalated permissions"
        )
    return message
