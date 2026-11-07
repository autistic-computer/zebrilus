from std.ffi import external_call, OwnedDLHandle
from std.sys.info import CompilationTarget
from .client_files import errno_text

comptime _ERRNO: StaticString = "__errno_location" if CompilationTarget.is_linux() else "__error"


def write_bytes(fd: Int32, bytes: Span[UInt8, _]) -> Int32:
    var offset = 0
    while offset < len(bytes):
        var count = external_call["write", Int](Int(fd), bytes.unsafe_ptr().unsafe_offset(offset), len(bytes) - offset)
        if count < 0:
            var number = external_call[_ERRNO, Pointer[Int32, MutUntrackedOrigin]]()[]
            if number == 4:
                continue
            return number
        if count == 0:
            return -1
        offset += count
    return 0


def stderr_line(bytes: Span[UInt8, _]):
    if len(bytes) == 0:
        return
    _ = write_bytes(2, bytes)
    if bytes[len(bytes) - 1] != 10:
        var newline: List[UInt8] = [10]
        _ = write_bytes(2, Span(newline))


struct CliOutput(Movable):
    var owner: Optional[Bool]

    def __init__(out self) raises:
        self.owner = None
        var library = OwnedDLHandle()
        var signal = library.get_function[UInt]("signal")
        _ = signal(Int32(13), UInt(1))

    def emit(mut self, bytes: Span[UInt8, _], raw: Bool = False) raises -> UInt8:
        if len(bytes) == 0:
            return 0
        if raw and Bool(self.owner):
            var message = errno_text(9) + ": -"
            stderr_line(message.as_bytes())
            return 1
        if Bool(self.owner) and self.owner.value():
            return 0
        self.owner = raw
        var error = write_bytes(1, bytes)
        if error == 0 and not raw and bytes[len(bytes) - 1] != 10:
            var newline: List[UInt8] = [10]
            error = write_bytes(1, Span(newline))
        if raw and error == 9:
            var message = errno_text(error) + ": -"
            stderr_line(message.as_bytes())
            return 1
        return 0
