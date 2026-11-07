from std.ffi import external_call, OwnedDLHandle
from std.sys import stderr
from std.sys.info import CompilationTarget
from .file_protocol import (
    ClientFileRequest,
    ClientFileResponse,
    MAX_CLIENT_FILE_BYTES,
)

comptime MAX_STDIN_BYTES = 1024 * 1024
comptime STDIN_CHUNK_BYTES = 16 * 1024
comptime _SignalPointer = Pointer[UInt64, MutUntrackedOrigin]
comptime _OptionalSignalPointer = Optional[_SignalPointer]
comptime _BytePointer = Pointer[UInt8, MutUntrackedOrigin]
comptime _ErrnoPointer = Pointer[Int32, MutUntrackedOrigin]
comptime _ERRNO: StaticString = "__errno_location" if CompilationTarget.is_linux() else "__error"


def _terminate_stdin(signal: Int32) abi("C"):
    _ = signal
    external_call["_exit", NoneType](Int32(0))


struct StdinSignal(Movable):
    var previous: List[UInt64]
    var installed: Bool

    def __init__(out self) raises:
        self.previous = List[UInt64](length=32, fill=0)
        self.installed = False
        var action = List[UInt64](length=32, fill=0)
        var handler: def(Int32) thin abi("C") -> NoneType = _terminate_stdin
        action[0] = Pointer(to=handler).unsafe_bitcast[UInt64]()[]
        var library = OwnedDLHandle()
        var empty = library.get_function[Int32]("sigemptyset")
        _ = empty(action.unsafe_ptr().unsafe_offset(1))
        var result = external_call["sigaction", Int32](
            Int32(15),
            _OptionalSignalPointer(action.unsafe_ptr().unsafe_origin_cast[MutUntrackedOrigin]()),
            _OptionalSignalPointer(self.previous.unsafe_ptr().unsafe_origin_cast[MutUntrackedOrigin]()),
        )
        _ = action
        if result != 0:
            raise Error("could not install standard input signal handler")
        self.installed = True

    def __deinit__(deinit self):
        if self.installed:
            _ = external_call["sigaction", Int32](
                Int32(15),
                _OptionalSignalPointer(self.previous.unsafe_ptr().unsafe_origin_cast[MutUntrackedOrigin]()),
                _OptionalSignalPointer(None),
            )
            _ = self.previous


def errno_text(number: Int32) raises -> String:
    var library = OwnedDLHandle()
    var strerror = library.get_function[Optional[_BytePointer]]("strerror")
    var pointer = strerror(number)
    if not pointer:
        return "Unknown error"
    return String(unsafe_from_utf8_ptr=pointer.value())


def _last_error() raises -> String:
    var number = external_call[_ERRNO, _ErrnoPointer]()[]
    return errno_text(number)


def _reserve_standard_descriptors() raises:
    var path = String("/dev/null")
    var name = path.as_c_string_span().ptr()
    for number in range(3):
        if external_call["fcntl", Int32, num_fixed_args=2](Int32(number), Int32(1), Int32(0)) >= 0:
            continue
        var fd = external_call["open", Int32, num_fixed_args=2](name, Int32(2))
        if fd < 0:
            raise Error(_last_error())
        if fd != Int32(number):
            var result = external_call["dup2", Int32](fd, Int32(number))
            _ = external_call["close", Int32](fd)
            if result < 0:
                raise Error("could not reserve standard descriptor")
    _ = path


def _read_bytes(fd: Int32, maximum: Int, once: Bool) raises -> List[UInt8]:
    var library = OwnedDLHandle()
    var read = library.get_function[Int]("read")
    var chunk = List[UInt8](length=min(STDIN_CHUNK_BYTES, maximum + 1), fill=0)
    var data = List[UInt8]()
    while len(data) <= maximum:
        var size = min(len(chunk), maximum + 1 - len(data))
        var count = read(fd, chunk.unsafe_ptr(), size)
        if count < 0:
            if external_call[_ERRNO, _ErrnoPointer]()[] == 4:
                continue
            raise Error(errno_text(5))
        if count == 0:
            break
        data.extend(Span(chunk)[:count])
        if once:
            break
    _ = chunk
    return data^


struct ClientFiles(Movable):
    var stdin_fd: Int32
    var stdin_error: Int32
    var enabled: Bool
    var spent: Bool
    var was_closed: Bool

    def __init__(out self):
        self.stdin_fd = -1
        self.stdin_error = 9
        self.enabled = False
        self.spent = False
        self.was_closed = False

    @staticmethod
    def capture_stdin() raises -> Self:
        var files = Self()
        files.enabled = True
        files.stdin_fd = external_call["fcntl", Int32, num_fixed_args=2](Int32(0), Int32(0), Int32(3))
        if files.stdin_fd < 0:
            files.stdin_error = external_call[_ERRNO, _ErrnoPointer]()[]
            files.was_closed = files.stdin_error == 9
        elif external_call["fcntl", Int32, num_fixed_args=2](files.stdin_fd, Int32(2), Int32(1)) != 0:
            raise Error("could not protect standard input descriptor")
        _reserve_standard_descriptors()
        return files^

    def __deinit__(deinit self):
        if self.stdin_fd >= 0:
            _ = external_call["close", Int32](self.stdin_fd)

    def _stdin(mut self, binary: Bool, once: Bool) raises -> List[UInt8]:
        if self.was_closed:
            var errors = stderr
            errors.write("[err] evsig_cb: recv: " + errno_text(9) + "\n")
            external_call["exit", NoneType](Int32(1))
        if self.stdin_fd < 0:
            raise Error(errno_text(self.stdin_error))
        var signal = StdinSignal()
        var data = _read_bytes(
            self.stdin_fd,
            STDIN_CHUNK_BYTES - 1 if once else MAX_STDIN_BYTES,
            once,
        )
        _ = signal^
        if len(data) > MAX_STDIN_BYTES:
            raise Error("standard input exceeds " + String(MAX_STDIN_BYTES) + " bytes")
        if not binary and not once:
            try:
                _ = String(from_utf8=Span(data))
            except:
                raise Error("could not read standard input: stream did not contain valid UTF-8")
        return data^

    def _file(self, request: ClientFileRequest) raises -> List[UInt8]:
        for byte in request.path.as_bytes():
            if byte == 0:
                raise Error("file name contained an unexpected NUL byte")
        var library = OwnedDLHandle()
        var open_file = library.get_function[Optional[_BytePointer]]("fopen")
        var file_descriptor = library.get_function[Int32]("fileno")
        var close_file = library.get_function[Int32]("fclose")
        var mode = String("rb")
        if request.operation == 1:
            mode = "ab" if request.append else "wb"
        var path = request.path.copy()
        var file = open_file(path.as_c_string_span().ptr(), mode.as_c_string_span().ptr())
        _ = mode
        _ = path
        if not file:
            raise Error(_last_error())
        var fd = file_descriptor(file.value())
        if external_call["fcntl", Int32, num_fixed_args=2](fd, Int32(2), Int32(1)) != 0:
            var failure = _last_error()
            _ = close_file(file.value())
            raise Error(failure)
        var result = List[UInt8]()
        try:
            if request.operation == 0:
                result = _read_bytes(fd, MAX_CLIENT_FILE_BYTES, False)
                if len(result) > MAX_CLIENT_FILE_BYTES:
                    raise Error("client file exceeds " + String(MAX_CLIENT_FILE_BYTES) + " bytes")
            else:
                var write = library.get_function[Int]("write")
                var offset = 0
                while offset < len(request.data):
                    var count = write(fd, request.data.unsafe_ptr().unsafe_offset(offset), len(request.data) - offset)
                    if count < 0:
                        if external_call[_ERRNO, _ErrnoPointer]()[] == 4:
                            continue
                        raise Error(_last_error())
                    if count == 0:
                        raise Error("failed to write whole buffer")
                    offset += count
        except error:
            _ = close_file(file.value())
            raise error^
        _ = close_file(file.value())
        return result^

    def answer(mut self, request: ClientFileRequest) -> ClientFileResponse:
        var data: List[UInt8]
        var error: Optional[String] = None
        try:
            if request.operation >= 2:
                if not self.enabled or self.spent:
                    raise Error(errno_text(9))
                var once = request.operation == 3
                self.spent = not once
                try:
                    data = self._stdin(request.binary, once)
                    if once:
                        self.spent = len(data) == 0
                except failure:
                    self.spent = True
                    raise failure^
            else:
                data = self._file(request)
        except failure:
            data = List[UInt8]()
            error = String(failure)
        return ClientFileResponse(request.request, data^, error^)
