from std.ffi import external_call
from std.sys.info import CompilationTarget
from .verified_input_router import MAX_ROUTER_BYTES


struct RouterWordBuffer(Movable):
    var fd: Int32
    var path: String
    var temporary: Bool
    var chunk: List[UInt8]

    def __init__(out self) raises:
        self.fd = -1
        self.path = String()
        self.temporary = False
        self.chunk = List[UInt8](length=65536, fill=0)
        comptime if CompilationTarget.is_linux():
            var name = String("zz-input")
            self.fd = external_call["memfd_create", Int32](name.as_c_string_span().ptr(), UInt32(1))
            _ = name
            if self.fd < 0:
                raise Error("cannot allocate anonymous input router storage")
            self.path = String("/proc/", external_call["getpid", Int32](), "/fd/", self.fd)
        else:
            var name = String("/tmp/zz-input-XXXXXX")
            var bytes = List[UInt8]()
            bytes.extend(name.as_bytes())
            bytes.append(0)
            self.fd = external_call["mkstemp", Int32](bytes.unsafe_ptr())
            if self.fd < 0:
                raise Error("cannot allocate private input router storage")
            self.path = String(from_utf8=Span(bytes)[: len(bytes) - 1])
            self.temporary = True
            if external_call["fcntl", Int32, num_fixed_args=2](self.fd, Int32(2), Int32(1)) != 0:
                _ = external_call["close", Int32](self.fd)
                _ = external_call["unlink", Int32](bytes.unsafe_ptr())
                _ = bytes
                self.fd = -1
                self.temporary = False
                raise Error("cannot protect input router storage inheritance")

    def __deinit__(deinit self):
        if self.fd >= 0:
            _ = external_call["close", Int32](self.fd)
        if self.temporary:
            var path = self.path^
            _ = external_call["unlink", Int32](path.as_c_string_span().ptr())
            _ = path

    def rewind(self) raises:
        if external_call["lseek", Int64](self.fd, Int64(0), Int32(0)) != 0:
            raise Error("cannot rewind input router storage")

    def clear(self) raises:
        if external_call["ftruncate", Int32](Int(self.fd), Int(0)) != 0:
            raise Error("cannot clear input router storage")
        self.rewind()

    def store(mut self, words: List[UInt32]) raises:
        if len(words) > MAX_ROUTER_BYTES // 4:
            raise Error("normalized input router storage exceeds its byte limit")
        if external_call["ftruncate", Int32](Int(self.fd), len(words) * 4) != 0:
            raise Error("cannot size input router storage")
        self.rewind()
        var base = 0
        while base < len(words):
            var size = min(len(words) - base, 16384)
            for index in range(size):
                var word = words[base + index]
                for byte in range(4):
                    self.chunk[index * 4 + byte] = UInt8((word >> UInt32(byte * 8)) & 255)
            var offset = 0
            while offset < size * 4:
                var count = external_call["write", Int](Int(self.fd), self.chunk.unsafe_ptr().unsafe_offset(offset), size * 4 - offset)
                if count <= 0:
                    raise Error("cannot write normalized input router storage")
                offset += count
            base += size

    def load(mut self, maximum: Int = MAX_ROUTER_BYTES) raises -> List[UInt32]:
        var length = Int(external_call["lseek", Int64](self.fd, Int64(0), Int32(2)))
        if length < 0 or length > min(maximum, MAX_ROUTER_BYTES) or length % 4 != 0:
            raise Error("Bend returned an invalid normalized input router byte count")
        self.rewind()
        var words = List[UInt32](length=length // 4, fill=0)
        var base = 0
        while base < length:
            var size = min(length - base, 65536)
            var offset = 0
            while offset < size:
                var count = external_call["read", Int](Int(self.fd), self.chunk.unsafe_ptr().unsafe_offset(offset), size - offset)
                if count <= 0:
                    raise Error("cannot read normalized input router storage")
                offset += count
            for index in range(size // 4):
                var word: UInt32 = 0
                for byte in range(4):
                    word |= UInt32(self.chunk[index * 4 + byte]) << UInt32(byte * 8)
                words[base // 4 + index] = word
            base += size
        return words^
