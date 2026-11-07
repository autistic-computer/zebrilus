from std.ffi import external_call, OwnedDLHandle
from std.os.path import dirname
from std.sys.info import CompilationTarget
from .cli_native import environment_value, canonical_path, _errno
from .client_context import (
    _environment_pointer,
    _copy_c_string,
    _name_key,
    MAX_CONTEXT_PATH,
)
from .client_files import errno_text
from .cli_parse import MAX_CLI_BYTES, MAX_CLI_WORDS

comptime _Byte = Pointer[UInt8, ImmUntrackedOrigin]
comptime _MutableByte = Pointer[UInt8, MutUntrackedOrigin]
comptime _Opaque = MutOpaquePointer[MutUntrackedOrigin]


@fieldwise_init
struct BackendSpawn(TrivialRegisterPassable):
    var server: UInt64
    var process: Int32


@fieldwise_init
struct _PollFd(TrivialRegisterPassable):
    var fd: Int32
    var events: Int16
    var returned: Int16


struct _Pipe(Movable):
    var read: Int32
    var write: Int32

    def __init__(out self) raises:
        self.read = -1
        self.write = -1
        var descriptors = List[Int32](length=2, fill=-1)
        if external_call["pipe", Int32](descriptors.unsafe_ptr()) != 0:
            raise Error("daemon pipe creation failed")
        for index in range(2):
            if descriptors[index] < 3:
                var duplicate = external_call["fcntl", Int32, num_fixed_args=2](
                    descriptors[index], Int32(0), Int32(3)
                )
                if duplicate < 0:
                    for descriptor in descriptors:
                        _ = external_call["close", Int32](descriptor)
                    raise Error("daemon descriptor duplication failed")
                _ = external_call["close", Int32](descriptors[index])
                descriptors[index] = duplicate
        self.read = descriptors[0]
        self.write = descriptors[1]
        for descriptor in descriptors:
            if (
                external_call["fcntl", Int32, num_fixed_args=2](
                    descriptor, Int32(2), Int32(1)
                )
                != 0
            ):
                var number = _errno()
                _ = external_call["close", Int32](self.read)
                _ = external_call["close", Int32](self.write)
                self.read = -1
                self.write = -1
                raise Error("daemon descriptor setup: " + errno_text(number))

    def close_write(mut self):
        if self.write >= 0:
            _ = external_call["close", Int32](self.write)
            self.write = -1

    def __deinit__(deinit self):
        if self.read >= 0:
            _ = external_call["close", Int32](self.read)
        if self.write >= 0:
            _ = external_call["close", Int32](self.write)


def clock_ns(monotonic: Bool = True) raises -> UInt64:
    var value = List[Int64](length=2, fill=0)
    var kind = Int32(
        1 if CompilationTarget.is_linux() else 6
    ) if monotonic else Int32(0)
    if external_call["clock_gettime", Int32](kind, value.unsafe_ptr()) != 0:
        raise Error("daemon clock is unavailable")
    if value[0] < 0:
        return 0
    return UInt64(value[0]) * 1_000_000_000 + UInt64(value[1])


def executable_path() raises -> String:
    var buffer = List[UInt8](length=MAX_CONTEXT_PATH + 1, fill=0)
    var library = OwnedDLHandle()
    comptime if CompilationTarget.is_linux():
        var source = String("/proc/self/exe")
        var count = external_call["readlink", Int](
            source.as_c_string_span().ptr(),
            buffer.unsafe_ptr(),
            MAX_CONTEXT_PATH,
        )
        _ = source
        if count < 0 or count >= MAX_CONTEXT_PATH:
            raise Error("could not resolve the client executable")
        return String(from_utf8=Span(buffer)[:count])
    else:
        var get_path = library.get_function[Int32]("_NSGetExecutablePath")
        var size = UInt32(len(buffer))
        if get_path(buffer.unsafe_ptr(), Pointer(to=size)) != 0:
            raise Error("client executable path exceeds byte limit")
        var copied = _copy_c_string(
            Optional[_MutableByte](
                buffer.unsafe_ptr().unsafe_origin_cast[MutUntrackedOrigin]()
            ),
            MAX_CONTEXT_PATH,
        )
        _ = buffer
        if not copied:
            raise Error("could not resolve the client executable")
        var resolved = canonical_path(Span(copied.value()))
        if resolved:
            return String(from_utf8=Span(resolved.value()))
        return String(from_utf8=Span(copied.value()))


def backend_executable() raises -> List[UInt8]:
    var configured = environment_value("ZZ_DAEMON")
    if configured:
        if len(configured.value()) == 0:
            raise Error("ZZ_DAEMON requires a non-empty executable path")
        var has_separator = False
        for byte in configured.value():
            if byte == 47:
                has_separator = True
        if not has_separator:
            raise Error(
                "ZZ_DAEMON requires an executable path containing `/`; PATH"
                " lookup is disabled"
            )
        return configured.take()
    var path = dirname(dirname(executable_path())) + "/share/backend/zz"
    var bytes = List[UInt8](path.as_bytes())
    var terminated = bytes.copy()
    terminated.append(0)
    if external_call["access", Int32](terminated.unsafe_ptr(), Int32(1)) != 0:
        raise Error(
            "daemon backend is missing or not executable: "
            + path
            + "; build the backend or set ZZ_DAEMON"
        )
    return bytes^


def physical_directory() raises -> Optional[List[UInt8]]:
    var buffer = List[UInt8](length=MAX_CONTEXT_PATH + 1, fill=0)
    var library = OwnedDLHandle()
    var getcwd = library.get_function[Optional[_MutableByte]]("getcwd")
    var pointer = getcwd(buffer.unsafe_ptr(), len(buffer))
    var path = _copy_c_string(pointer, MAX_CONTEXT_PATH)
    _ = buffer
    if not path or len(path.value()) == 0 or path.value()[0] != 47:
        return None
    try:
        _ = String(from_utf8=Span(path.value()))
    except:
        return None
    return path^


def backend_environment() raises -> List[List[UInt8]]:
    var pointer = _environment_pointer()
    var values = Dict[String, List[UInt8]]()
    var index = 0
    var total = 0
    while pointer[unsafe_offset=index]:
        if index == MAX_CLI_WORDS:
            raise Error("daemon environment exceeds entry limit")
        var entry = _copy_c_string(pointer[unsafe_offset=index], MAX_CLI_BYTES)
        index += 1
        if not entry or len(entry.value()) > MAX_CLI_BYTES - total:
            raise Error("daemon environment exceeds byte limit")
        var bytes = entry.take()
        total += len(bytes)
        var separator = -1
        for position in range(len(bytes)):
            if bytes[position] == 61:
                separator = position
                break
        if separator < 0:
            continue
        var name = String(from_utf8_lossy=Span(bytes)[:separator])
        if name == "ZZ_TMUX_EXECUTABLE" or name == "ZZ_APP_STARTUP_DIRECTORY":
            continue
        values[_name_key(Span(bytes)[:separator])] = bytes^
    var keys = List[String]()
    for key in values:
        keys.append(key)
    sort(Span(keys))
    var entries = List[List[UInt8]]()
    for key in keys:
        var entry = values[key].copy()
        entry.append(0)
        entries.append(entry^)
    return entries^


def daemon_arguments(
    executable: Span[UInt8, _],
    socket: Span[UInt8, _],
    configs: List[List[UInt8]],
    server: UInt64,
    ready: Int32,
    cwd: Optional[List[UInt8]],
) raises -> List[List[UInt8]]:
    var args = List[List[UInt8]]()
    args.append(List[UInt8](executable))
    args.append(List[UInt8](String("--socket").as_bytes()))
    args.append(List[UInt8](socket))
    for config in configs:
        args.append(List[UInt8](String("-f").as_bytes()))
        args.append(config.copy())
    args.append(List[UInt8](String("daemon").as_bytes()))
    args.append(List[UInt8](String("--bootstrap-server-id").as_bytes()))
    args.append(List[UInt8](String(server).as_bytes()))
    args.append(List[UInt8](String("--bootstrap-ready-fd").as_bytes()))
    args.append(List[UInt8](String(ready).as_bytes()))
    if cwd:
        args.append(List[UInt8](String("--bootstrap-client-cwd").as_bytes()))
        args.append(cwd.value().copy())
    var total = 0
    for index in range(len(args)):
        if len(args[index]) > MAX_CLI_BYTES - total:
            raise Error("daemon arguments exceed byte limit")
        total += len(args[index])
        for byte in args[index]:
            if byte == 0:
                raise Error("daemon arguments contain NUL")
        args[index].append(0)
    return args^


def _reap(context: _Opaque) abi("C") -> Optional[_Opaque]:
    var process = context.unsafe_bitcast[Int32]()[unsafe_offset=0]
    external_call["free", NoneType](context)
    var status: Int32 = 0
    while (
        external_call["waitpid", Int32](process, Pointer(to=status), Int32(0))
        < 0
    ):
        if _errno() != 4:
            break
    return None


def _detach_reaper(process: Int32) raises:
    var library = OwnedDLHandle()
    var allocate = library.get_function[Optional[_Opaque]]("malloc")
    var context = allocate(UInt(4))
    if not context:
        return
    context.value().unsafe_bitcast[Int32]()[unsafe_offset=0] = process
    var thread: UInt64 = 0
    var callback: def(_Opaque) thin abi("C") -> Optional[_Opaque] = _reap
    var create = library.get_function[Int32]("pthread_create")
    var detach = library.get_function[Int32]("pthread_detach")
    if (
        create(
            Pointer(to=thread),
            Optional[_Opaque](None),
            callback,
            context.value(),
        )
        != 0
    ):
        external_call["free", NoneType](context.value())
    else:
        _ = detach(thread)


def _wait_readable(fd: Int32, deadline: UInt64) raises -> Bool:
    var library = OwnedDLHandle()
    var poll = library.get_function[Int32]("poll")
    while True:
        var now = clock_ns()
        if now >= deadline:
            return False
        var timeout = Int32(max(UInt64(1), (deadline - now) // 1_000_000))
        var descriptor = _PollFd(fd, 1, 0)
        var result = poll(Pointer(to=descriptor), UInt(1), timeout)
        if result > 0:
            return True
        if result == 0:
            return False
        if _errno() != 4:
            raise Error("daemon readiness poll: " + errno_text(_errno()))


def spawn_backend(
    socket: Span[UInt8, _], configs: List[List[UInt8]], sequence: UInt64 = 1
) raises -> BackendSpawn:
    var executable = backend_executable()
    var ready = _Pipe()
    var failed = _Pipe()
    var server = (
        clock_ns(False)
        ^ (UInt64(external_call["getpid", UInt32]()) << 32)
        ^ sequence
    )
    var args = daemon_arguments(
        Span(executable),
        socket,
        configs,
        server,
        ready.write,
        physical_directory(),
    )
    var environment = backend_environment()
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
    var args_ptr = argv.unsafe_ptr()
    var env_ptr = envp.unsafe_ptr()
    var null_path = String("/dev/null")
    var null_fd = external_call["open", Int32, num_fixed_args=2](
        null_path.as_c_string_span().ptr(), Int32(2)
    )
    _ = null_path
    if null_fd < 0:
        raise Error("could not open daemon standard streams")
    if null_fd < 3:
        var duplicate = external_call["fcntl", Int32, num_fixed_args=2](
            null_fd, Int32(0), Int32(3)
        )
        _ = external_call["close", Int32](null_fd)
        if duplicate < 0:
            raise Error("daemon standard stream duplication failed")
        null_fd = duplicate
    var library = OwnedDLHandle()
    var execve = library.get_function[Int32]("execve")
    var signal = library.get_function[UInt]("signal")
    var empty = SIMD[.uint64, 16](0)
    _ = external_call["sigemptyset", Int32](Pointer(to=empty))
    var empty_ptr = Pointer(to=empty)
    var child_previous = SIMD[.uint64, 16](0)
    var child_previous_ptr = Pointer(to=child_previous)
    var ready_read = ready.read
    var ready_write = ready.write
    var failed_read = failed.read
    var failed_write = failed.write
    var process = external_call["fork", Int32]()
    if process == 0:
        comptime SETMASK = 2 if CompilationTarget.is_linux() else 3
        _ = external_call["sigprocmask", Int32](
            Int32(SETMASK), empty_ptr, child_previous_ptr
        )
        _ = signal(Int32(13), UInt(0))
        _ = external_call["setsid", Int32]()
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
        _ = external_call["fcntl", Int32, num_fixed_args=2](
            ready_write, Int32(2), Int32(0)
        )
        _ = external_call["close", Int32](ready_read)
        _ = external_call["close", Int32](failed_read)
        for descriptor in range(3):
            _ = external_call["dup2", Int32](null_fd, Int32(descriptor))
        _ = external_call["close", Int32](null_fd)
        _ = execve(executable_ptr, args_ptr, env_ptr)
        var error = _errno()
        _ = external_call["write", Int](
            Int(failed_write), Pointer(to=error), Int(4)
        )
        external_call["_exit", NoneType](Int32(127))
    _ = empty
    _ = child_previous
    _ = args
    _ = environment
    _ = argv
    _ = envp
    _ = external_call["close", Int32](null_fd)
    ready.close_write()
    failed.close_write()
    if process < 0:
        raise Error("could not fork daemon process")
    var deadline = clock_ns() + 6_000_000_000
    if not _wait_readable(failed.read, deadline):
        _detach_reaper(process)
        raise Error("daemon executable did not start before the deadline")
    var number: Int32 = 0
    var received = 0
    while received < 4:
        var count = external_call["read", Int](
            Int(failed.read),
            Pointer(to=number).unsafe_bitcast[UInt8]().unsafe_offset(received),
            4 - received,
        )
        if count < 0:
            var read_error = _errno()
            if read_error == 4:
                continue
            _detach_reaper(process)
            raise Error("daemon executable status: " + errno_text(read_error))
        if count == 0:
            if received != 0:
                _detach_reaper(process)
                raise Error("daemon executable returned a truncated error")
            break
        received += count
    if received > 0:
        var status: Int32 = 0
        _ = external_call["waitpid", Int32](
            process, Pointer(to=status), Int32(0)
        )
        raise Error("could not launch daemon backend: " + errno_text(number))
    _detach_reaper(process)
    if _wait_readable(ready.read, clock_ns() + 6_000_000_000):
        var byte: UInt8 = 0
        _ = external_call["read", Int](
            Int(ready.read), Pointer(to=byte), Int(1)
        )
    return BackendSpawn(server, process)
