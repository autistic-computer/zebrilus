from std.ffi import external_call, OwnedDLHandle
from std.os.path import dirname
from std.sys.info import CompilationTarget
from .transport import Connection, open_connection
from .exec_protocol import Invocation
from .cli_preflight import preflight
from .cli_backend import spawn_backend, clock_ns
from .cli_native import parse_connect_errno, connect_error, _errno
from .client_files import errno_text


@fieldwise_init
struct CliStartup(Movable):
    var connection: Optional[Connection]
    var spawned: Optional[UInt64]
    var error: Optional[String]
    var code: UInt8


def _failed(message: String, code: UInt8 = 1) -> CliStartup:
    return CliStartup(None, None, message.copy(), code)


def _spawnable(number: Int32) -> Bool:
    comptime REFUSED = 111 if CompilationTarget.is_linux() else 61
    comptime RESET = 104 if CompilationTarget.is_linux() else 54
    return number == 2 or number == Int32(REFUSED) or number == Int32(RESET)


def _label_error(path: String) raises -> Optional[Tuple[Int32, String]]:
    var parent = dirname(path)
    var library = OwnedDLHandle()
    var storage = List[UInt64](length=32, fill=0)
    var data = storage.unsafe_ptr().unsafe_bitcast[UInt8]()
    var c_path = parent.copy()
    var pointer = c_path.as_c_string_span().ptr()
    var status: Int32
    comptime if CompilationTarget.is_linux():
        var statx = library.get_function[Int32]("statx")
        status = statx(Int32(-100), pointer, Int32(0), UInt32(0x7FF), data)
    else:
        var stat = library.get_function[Int32]("stat")
        status = stat(pointer, data)
    var number = _errno()
    _ = c_path
    if status != 0:
        return (
            number,
            "error creating " + path + " (" + errno_text(number) + ")",
        )
    comptime OFFSET = 28 if CompilationTarget.is_linux() else 4
    var mode = (
        UInt32(data[unsafe_offset=OFFSET])
        | UInt32(data[unsafe_offset=OFFSET + 1]) << 8
    )
    _ = storage
    if mode & 0xF000 != 0x4000:
        return (Int32(20), "error connecting to " + path + " (Not a directory)")
    return None


def prepare_startup(
    path: String,
    commands: Span[Invocation, _],
    configs: List[List[UInt8]],
    no_start: Bool,
    socket_source: UInt8,
) raises -> CliStartup:
    var failure: Optional[Int32]
    try:
        return CliStartup(open_connection(path), None, None, 0)
    except error:
        failure = parse_connect_errno(String(error))
        if not failure:
            raise Error(String(error))
    var checked = preflight(commands)
    if checked.error:
        return _failed(checked.error.value(), checked.code)
    if checked.canonical == "attach-session":
        return _failed(
            "zz: interactive attach startup is not implemented by this client"
        )
    if no_start or not checked.starts_server or not _spawnable(failure.value()):
        return _failed(connect_error(path, failure.value()))
    if socket_source == 2:
        var invalid = _label_error(path)
        if invalid:
            var code = UInt8(
                0 if checked.canonical == "new-session"
                and invalid.value()[0] == 2 else 1
            )
            return _failed(invalid.value()[1], code)
    var started = spawn_backend(path.as_bytes(), configs)
    var deadline = clock_ns() + 6_000_000_000
    var backoff = 0
    while True:
        try:
            return CliStartup(open_connection(path), started.server, None, 0)
        except error:
            var number = parse_connect_errno(String(error))
            if not number:
                raise Error(String(error))
            if not _spawnable(number.value()) or clock_ns() >= deadline:
                return _failed(connect_error(path, number.value()))
        var delay = UInt32(
            500 if backoff
            == 0 else 1000 if backoff
            == 1 else 2000 if backoff
            == 2 else 5000
        )
        backoff += 1
        _ = external_call["usleep", Int32](delay)
