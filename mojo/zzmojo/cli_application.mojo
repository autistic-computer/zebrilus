from .app_startup_reply import receive_startup_reply
from std.ffi import external_call, OwnedDLHandle
from std.os.path import dirname
from std.sys.info import CompilationTarget
from .cli_backend import (
    executable_path,
    physical_directory,
    capture_process_environment,
    _Pipe,
    _wait_readable,
    clock_ns,
    _detach_reaper,
)
from .cli_native import _errno
from .client_files import errno_text
from .cli_parse import MAX_CLI_BYTES
from .app_resume import store_resume
from .exec_protocol import ExecExit
from .transport import Connection

comptime _Byte = Pointer[UInt8, ImmUntrackedOrigin]


def application_executable() raises -> String:
    var path = dirname(dirname(executable_path())) + "/share/frontend/zz-app"
    if (
        external_call["access", Int32](path.as_c_string_span().ptr(), Int32(1))
        != 0
    ):
        raise Error(
            "desktop application is missing or not executable: "
            + path
            + "; build the native application"
        )
    return path^


def _application_environment() raises -> List[List[UInt8]]:
    var excluded: List[String] = [
        "TMUX",
        "TMUX_PANE",
        "ZZ_APP_STARTUP_DIRECTORY",
    ]
    var environment = capture_process_environment(Span(excluded))
    var cwd = physical_directory()
    if cwd:
        var entry = List[UInt8](String("ZZ_APP_STARTUP_DIRECTORY=").as_bytes())
        entry.extend(Span(cwd.value()))
        entry.append(0)
        environment.append(entry^)
    var total = 0
    for entry in environment:
        if len(entry) > MAX_CLI_BYTES - total:
            raise Error("application environment exceeds byte limit")
        total += len(entry)
    return environment^


def _spawn_application(
    executable: String, words: List[String], inherited: Span[Int32, _]
) raises -> Int32:
    var args = List[List[UInt8]]()
    var first = List[UInt8](executable.as_bytes())
    first.append(0)
    args.append(first^)
    var total = executable.byte_length()
    for word in words:
        if word.byte_length() > MAX_CLI_BYTES - total:
            raise Error("application arguments exceed byte limit")
        total += word.byte_length()
        var bytes = List[UInt8](word.as_bytes())
        for byte in bytes:
            if byte == 0:
                raise Error("application arguments contain NUL")
        bytes.append(0)
        args.append(bytes^)
    var environment = _application_environment()
    var argv = List[Optional[_Byte]]()
    var envp = List[Optional[_Byte]]()
    for argument in args:
        argv.append(
            argument.unsafe_ptr().unsafe_origin_cast[ImmUntrackedOrigin]()
        )
    argv.append(None)
    for entry in environment:
        envp.append(entry.unsafe_ptr().unsafe_origin_cast[ImmUntrackedOrigin]())
    envp.append(None)
    var executable_ptr = argv[0].value()
    var argv_ptr = argv.unsafe_ptr()
    var env_ptr = envp.unsafe_ptr()
    var keep = List[Int32](inherited)
    var keep_ptr = keep.unsafe_ptr()
    var keep_count = len(keep)
    var failed = _Pipe()
    var failed_read = failed.read
    var failed_write = failed.write
    var null_path = String("/dev/null")
    var null_fd = external_call["open", Int32, num_fixed_args=2](
        null_path.as_c_string_span().ptr(), Int32(2)
    )
    _ = null_path
    if null_fd < 0:
        raise Error("could not open application standard streams")
    if null_fd < 3:
        var duplicate = external_call["fcntl", Int32, num_fixed_args=2](
            null_fd, Int32(0), Int32(3)
        )
        _ = external_call["close", Int32](null_fd)
        if duplicate < 0:
            raise Error("application standard stream duplication failed")
        null_fd = duplicate
    var library = OwnedDLHandle()
    var execve = library.get_function[Int32]("execve")
    var signal = library.get_function[UInt]("signal")
    var empty = SIMD[.uint64, 16](0)
    var previous = SIMD[.uint64, 16](0)
    _ = external_call["sigemptyset", Int32](Pointer(to=empty))
    var empty_ptr = Pointer(to=empty)
    var previous_ptr = Pointer(to=previous)
    var process = external_call["fork", Int32]()
    if process == 0:
        comptime SETMASK = 2 if CompilationTarget.is_linux() else 3
        _ = external_call["sigprocmask", Int32](
            Int32(SETMASK), empty_ptr, previous_ptr
        )
        _ = signal(Int32(13), UInt(0))
        var marked = False
        comptime if CompilationTarget.is_linux():
            marked = (
                external_call["syscall", Int, num_fixed_args=1](
                    Int(436), UInt32(3), UInt32.MAX, UInt32(4)
                )
                == 0
            )
        if not marked:
            for fd in range(3, 65536):
                _ = external_call["fcntl", Int32, num_fixed_args=2](
                    Int32(fd), Int32(2), Int32(1)
                )
        for index in range(keep_count):
            _ = external_call["fcntl", Int32, num_fixed_args=2](
                keep_ptr[unsafe_offset=index], Int32(2), Int32(0)
            )
        _ = external_call["close", Int32](failed_read)
        for descriptor in range(3):
            _ = external_call["dup2", Int32](null_fd, Int32(descriptor))
        _ = external_call["close", Int32](null_fd)
        _ = execve(executable_ptr, argv_ptr, env_ptr)
        var error = _errno()
        _ = external_call["write", Int](
            Int(failed_write), Pointer(to=error), Int(4)
        )
        external_call["_exit", NoneType](Int32(127))
    _ = args
    _ = environment
    _ = argv
    _ = envp
    _ = keep
    _ = empty
    _ = previous
    _ = external_call["close", Int32](null_fd)
    failed.close_write()
    if process < 0:
        raise Error("could not fork application process")
    _detach_reaper(process)
    if not _wait_readable(failed.read, clock_ns() + 6_000_000_000):
        raise Error("application executable did not start before the deadline")
    var number: Int32 = 0
    while True:
        var count = external_call["read", Int](
            Int(failed.read), Pointer(to=number), Int(4)
        )
        if count < 0 and _errno() == 4:
            continue
        if count != 0:
            if count < 0:
                raise Error(
                    "application executable status: " + errno_text(_errno())
                )
            if count != 4:
                raise Error("application executable returned a truncated error")
            raise Error(
                "could not launch desktop application: " + errno_text(number)
            )
        return process


def launch_application(path: String) raises -> Int32:
    var words: List[String] = ["--socket", path.copy()]
    var inherited = List[Int32]()
    return _spawn_application(application_executable(), words, Span(inherited))


def launch_resume(
    path: String,
    connection: Connection,
    resume: ExecExit,
    empty_attach: Bool = False,
) raises -> UInt8:
    var executable = application_executable()
    var payload = store_resume(resume)
    var reply = _Pipe()
    var words: List[String] = [
        "--socket",
        path.copy(),
        "--resume-fds",
        String(connection.fd),
        String(payload.fd),
        String(reply.write),
    ]
    var inherited: List[Int32] = [connection.fd, payload.fd, reply.write]
    _ = _spawn_application(executable, words, Span(inherited))
    _ = payload^
    reply.close_write()
    var code = receive_startup_reply(reply.read, empty_attach)
    _ = reply^
    return code
