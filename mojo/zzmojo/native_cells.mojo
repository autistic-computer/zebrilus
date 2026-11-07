from std.ffi import external_call
from std.sys.info import CompilationTarget
from std.sys import size_of
from .terminal_model import TerminalCell

comptime MAX_BEND_CELLS = 8 * 1024 * 1024


struct CellBuffer(Movable):
    var fd: Int32
    var path: String
    var temporary: Bool

    def __init__(out self) raises:
        self.fd = -1
        self.path = String()
        self.temporary = False
        comptime if CompilationTarget.is_linux():
            var name = String("zz-terminal")
            self.fd = external_call["memfd_create", Int32](
                name.as_c_string_span().ptr(), UInt32(1)
            )
            _ = name
            if self.fd < 0:
                raise Error("cannot allocate anonymous terminal storage")
            self.path = String(
                "/proc/", external_call["getpid", Int32](), "/fd/", self.fd
            )
        else:
            var name = String("/tmp/zz-terminal-XXXXXX")
            var bytes = List[UInt8]()
            bytes.extend(name.as_bytes())
            bytes.append(0)
            self.fd = external_call["mkstemp", Int32](bytes.unsafe_ptr())
            if self.fd < 0:
                raise Error("cannot allocate private terminal storage")
            self.path = String(from_utf8=Span(bytes)[: len(bytes) - 1])
            self.temporary = True
            if external_call["fcntl", Int32, num_fixed_args=2](self.fd, Int32(2), Int32(1)) != 0:
                _ = external_call["close", Int32](self.fd)
                _ = external_call["unlink", Int32](bytes.unsafe_ptr())
                _ = bytes
                self.fd = -1
                self.temporary = False
                raise Error("cannot protect terminal storage inheritance")

    def __deinit__(deinit self):
        if self.fd >= 0:
            _ = external_call["close", Int32](self.fd)
        if self.temporary:
            var path = self.path^
            _ = external_call["unlink", Int32](path.as_c_string_span().ptr())
            _ = path

    def rewind(self) raises:
        if external_call["lseek", Int64](self.fd, Int64(0), Int32(0)) != 0:
            raise Error("cannot rewind terminal storage")

    def store(self, cells: List[TerminalCell]) raises:
        comptime assert size_of[TerminalCell]() == 8
        if len(cells) > MAX_BEND_CELLS:
            raise Error("terminal storage exceeds its cell limit")
        var length = len(cells) * 8
        if external_call["ftruncate", Int32](Int(self.fd), length) != 0:
            raise Error("cannot size terminal storage")
        self.rewind()
        var pointer = cells.unsafe_ptr().unsafe_bitcast[UInt8]()
        var offset = 0
        while offset < length:
            var count = external_call["write", Int](
                Int(self.fd),
                pointer.unsafe_offset(offset),
                min(length - offset, 65536),
            )
            if count <= 0:
                raise Error("cannot write terminal storage")
            offset += count

    def load(self, count: Int) raises -> List[TerminalCell]:
        if count < 0 or count > MAX_BEND_CELLS:
            raise Error("terminal storage exceeds its cell limit")
        if (
            external_call["lseek", Int64](self.fd, Int64(0), Int32(2))
            != Int64(count * 8)
        ):
            raise Error("Bend returned an invalid terminal cell count")
        self.rewind()
        var cells = List[TerminalCell](length=count, fill=TerminalCell(0, 0, 0))
        var pointer = cells.unsafe_ptr().unsafe_bitcast[UInt8]()
        var offset = 0
        while offset < count * 8:
            var read = external_call["read", Int](
                Int(self.fd),
                pointer.unsafe_offset(offset),
                min(count * 8 - offset, 65536),
            )
            if read <= 0:
                raise Error("cannot read Bend terminal cells")
            offset += read
        return cells^


struct CellBuffers(Movable):
    var retained: CellBuffer
    var changed: CellBuffer
    var result: CellBuffer

    def __init__(out self) raises:
        self.retained = CellBuffer()
        self.changed = CellBuffer()
        self.result = CellBuffer()
