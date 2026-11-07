from std.ffi import external_call
from std.sys.info import CompilationTarget


def resolve_home(user: Optional[String] = None) raises -> Optional[String]:
    comptime if not CompilationTarget.is_linux():
        raise Error("native home lookup is not implemented for this platform")
    else:
        var name = user.value().copy() if user else String()
        for byte in name.as_bytes():
            if byte == 0:
                return None
        if name.byte_length() > 4096:
            raise Error("native home lookup exceeds its name limit")
        var size = 1024
        while size <= 1024 * 1024:
            var buffer = List[UInt8](length=size, fill=0)
            var record = List[UInt64](length=6, fill=0)
            var result: UInt64 = 0
            var status: Int32
            if user:
                status = external_call["getpwnam_r", Int32](name.as_c_string_span().ptr(), record.unsafe_ptr().unsafe_bitcast[NoneType](), buffer.unsafe_ptr(), UInt64(size), Pointer(to=result).unsafe_bitcast[NoneType]())
            else:
                var uid = external_call["getuid", UInt32]()
                status = external_call["getpwuid_r", Int32](uid, record.unsafe_ptr().unsafe_bitcast[NoneType](), buffer.unsafe_ptr(), UInt64(size), Pointer(to=result).unsafe_bitcast[NoneType]())
            _ = name
            if status == 34:
                size *= 2
                continue
            if status != 0 or result == 0:
                return None
            var base = UInt64(Int(buffer.unsafe_ptr()))
            var address = record[4]
            if address < base or address >= base + UInt64(size):
                raise Error("native home lookup returned an invalid buffer")
            var offset = Int(address - base)
            var end = offset
            while end < size and buffer[end] != 0:
                end += 1
            if end == size:
                raise Error("native home lookup returned an unterminated path")
            try:
                return String(from_utf8=Span(buffer)[offset:end])
            except:
                return None
        raise Error("native home lookup exceeds its storage limit")
