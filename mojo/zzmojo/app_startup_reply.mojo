from std.ffi import external_call
from .protocol import (
    MAX_FRAME_BYTES,
    Decoder,
    append_bytes,
    append_text,
    append_uint,
)
from .tree_wire import read_text, read_uint
from .cli_native import _errno
from .cli_output import write_bytes, stderr_line
from .client_files import errno_text


@fieldwise_init
struct StartupReply(Movable):
    var output: List[UInt8]
    var error: String
    var code: UInt8


def send_startup_reply(fd: Int32, reply: StartupReply) raises:
    var body = List[UInt8]()
    append_uint(body, UInt64(reply.code))
    append_bytes(body, Span(reply.output))
    append_text(body, reply.error)
    if len(body) > MAX_FRAME_BYTES:
        raise Error("application startup reply exceeds byte limit")
    var packet: List[UInt8] = [90, 90, 65, 49]
    for shift in range(0, 32, 8):
        packet.append(UInt8((len(body) >> shift) & 255))
    packet.extend(Span(body))
    if write_bytes(fd, Span(packet)) != 0:
        raise Error("could not send application startup reply")


def _read_reply(fd: Int32, length: Int) raises -> List[UInt8]:
    var bytes = List[UInt8](unsafe_uninit_length=length)
    var offset = 0
    while offset < length:
        var count = external_call["read", Int](
            Int(fd), bytes.unsafe_ptr().unsafe_offset(offset), length - offset
        )
        if count < 0 and _errno() == 4:
            continue
        if count <= 0:
            raise Error("desktop application closed before completing startup")
        offset += count
    return bytes^


def receive_startup_reply(
    fd: Int32, empty_attach: Bool = False
) raises -> UInt8:
    var header = _read_reply(fd, 8)
    if header[0] != 90 or header[1] != 90 or header[2] != 65 or header[3] != 49:
        raise Error("invalid application startup reply")
    var size: UInt32 = 0
    for index in range(4):
        size |= UInt32(header[index + 4]) << UInt32(index * 8)
    if size < 3 or size > UInt32(MAX_FRAME_BYTES):
        raise Error("application startup reply exceeds byte limit")
    var body = _read_reply(fd, Int(size))
    var reader = Decoder(body^)
    reader.offset = 0
    var code = UInt8(read_uint(reader, 255))
    var output = reader.bytes()
    var message = read_text(reader)
    reader.finish()
    var error = write_bytes(1, Span(output))
    if error != 0:
        stderr_line(errno_text(error).as_bytes())
        return 1
    if empty_attach and message == "can't find session: current session":
        message = "no sessions"
    stderr_line(message.as_bytes())
    return code
