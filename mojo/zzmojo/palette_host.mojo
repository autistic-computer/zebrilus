from std.ffi import OwnedDLHandle
from .palette_snapshot import palette_hostname


def palette_local_hostname() -> String:
    try:
        var library = OwnedDLHandle()
        var gethostname = library.get_function[Int32]("gethostname")
        var buffer = List[UInt8](length=256, fill=0)
        if gethostname(buffer.unsafe_ptr(), UInt(len(buffer))) != 0:
            return String("localhost")
        for end in range(len(buffer)):
            if buffer[end] == 0:
                return palette_hostname(String(from_utf8=Span(buffer)[:end]))
    except:
        pass
    return String("localhost")
