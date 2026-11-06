from std.ffi import external_call
from std.os import getenv
from std.os.path import dirname, realpath, exists
from std.sys import argv
from std.sys.info import CompilationTarget
from .bend_gate import BendGate
from .client_context import ClientContext
from .protocol import (
    MAX_FRAME_BYTES,
    PROTOCOL_VERSION,
    Decoder,
    Response,
    client_hello,
    command_request,
    decode_response,
    validate_envelope,
)

comptime ERRNO: StaticString = "__errno_location" if CompilationTarget.is_linux() else "__error"


def io_error(operation: String) -> Error:
    var number = external_call[ERRNO, Pointer[Int32, MutUntrackedOrigin]]()[]
    return Error(operation + " failed (errno " + String(number) + ")")


def bend_executable() raises -> String:
    var configured = getenv("ZZ_BEND", "")
    if configured.byte_length() > 0:
        return configured
    var args = argv()
    var executable = String(args[0])
    if "/" not in executable:
        for directory in getenv("PATH", "").split(":"):
            var candidate = String(directory, "/", executable)
            if exists(candidate):
                executable = candidate
                break
    var sibling = String(dirname(realpath(executable)), "/zz-bend")
    if not exists(sibling):
        raise Error(
            "verified core executable missing: "
            + sibling
            + "; build the Bend core or set ZZ_BEND"
        )
    return sibling


@fieldwise_init
struct Connection(Movable):
    var fd: Int32
    var next_request: UInt64
    var server_id: UInt64
    var client_id: UInt64
    var gate: BendGate

    def __deinit__(deinit self):
        if self.fd >= 0:
            _ = external_call["close", Int32](self.fd)

    def write(self, data: Span[UInt8, _]) raises:
        var offset = 0
        while offset < len(data):
            var count = external_call["send", Int](
                Int(self.fd),
                data.unsafe_ptr().unsafe_offset(offset),
                len(data) - offset,
                Int(0x4000 if CompilationTarget.is_linux() else 0),
            )
            if count < 0:
                var number = external_call[
                    ERRNO, Pointer[Int32, MutUntrackedOrigin]
                ]()[]
                if number == 4:
                    continue
                raise io_error("send")
            if count == 0:
                raise Error("socket closed during send")
            offset += count

    def read_exact(self, size: Int) raises -> List[UInt8]:
        var data = List[UInt8](unsafe_uninit_length=size)
        var offset = 0
        while offset < size:
            var count = external_call["read", Int](
                Int(self.fd),
                data.unsafe_ptr().unsafe_offset(offset),
                size - offset,
            )
            if count < 0:
                var number = external_call[
                    ERRNO, Pointer[Int32, MutUntrackedOrigin]
                ]()[]
                if number == 4:
                    continue
                raise io_error("read")
            if count == 0:
                raise Error("daemon disconnected")
            offset += count
        return data^

    def read_frame(self) raises -> List[UInt8]:
        var frame = self.read_exact(4)
        var size: UInt32 = 0
        for index in range(4):
            size |= UInt32(frame[index]) << UInt32(index * 8)
        if size < 4 or size > UInt32(MAX_FRAME_BYTES):
            raise Error("invalid daemon frame length")
        var following = self.read_exact(Int(size))
        frame.extend(Span(following))
        var status = self.gate.validate(Span(frame))
        if status != 0:
            raise Error(
                "Bend rejected daemon envelope (status " + String(status) + ")"
            )
        _ = validate_envelope(Span(frame))
        return frame^

    def command(
        mut self, name: String, args: Span[String, _]
    ) raises -> Response:
        var request = self.next_request
        if request == UInt64.MAX:
            raise Error("request identifiers exhausted")
        self.next_request += 1
        var frame = command_request(request, name, args)
        self.write(Span(frame))
        for _ in range(4096):
            var incoming = self.read_frame()
            if incoming[4] != 0:
                raise Error("unexpected terminal frame for command client")
            var decoder = Decoder(incoming.copy())
            var tag = decoder.uint()
            if tag != 3:
                continue
            var response = decode_response(incoming^)
            if response.request != request:
                raise Error("response request identifier mismatch")
            return response^
        raise Error("too many unsolicited daemon frames")


def open_connection(path: String) raises -> Connection:
    comptime PATH_LIMIT = 108 if CompilationTarget.is_linux() else 104
    if path.byte_length() == 0 or path.byte_length() >= PATH_LIMIT:
        raise Error("Unix socket path is empty or too long")
    for byte in path.as_bytes():
        if byte == 0:
            raise Error("Unix socket path contains NUL")
    var gate = BendGate(bend_executable())
    var address = List[UInt8](length=PATH_LIMIT + 2, fill=0)
    comptime if CompilationTarget.is_linux():
        address[0] = 1
    else:
        address[0] = UInt8(path.byte_length() + 3)
        address[1] = 1
    for index in range(path.byte_length()):
        address[index + 2] = path.as_bytes()[index]
    var fd = external_call["socket", Int32](Int32(1), Int32(1), Int32(0))
    if fd < 0:
        raise io_error("socket")
    if external_call["fcntl", Int32](fd, Int32(2), Int32(1)) != 0:
        _ = external_call["close", Int32](fd)
        raise io_error("socket descriptor inheritance")
    var connection = Connection(fd, 1, 0, 0, gate^)
    comptime if not CompilationTarget.is_linux():
        var enabled: Int32 = 1
        _ = external_call["setsockopt", Int32](
            fd, Int32(0xFFFF), Int32(0x1022), Pointer(to=enabled), UInt32(4)
        )
    var timeout: List[Int] = [10, 0]
    comptime receive_option = 20 if CompilationTarget.is_linux() else 0x1006
    comptime send_option = 21 if CompilationTarget.is_linux() else 0x1005
    comptime socket_level = 1 if CompilationTarget.is_linux() else 0xFFFF
    if (
        external_call["setsockopt", Int32](
            fd,
            Int32(socket_level),
            Int32(receive_option),
            timeout.unsafe_ptr(),
            UInt32(16),
        )
        != 0
    ):
        raise io_error("receive timeout")
    if (
        external_call["setsockopt", Int32](
            fd,
            Int32(socket_level),
            Int32(send_option),
            timeout.unsafe_ptr(),
            UInt32(16),
        )
        != 0
    ):
        raise io_error("send timeout")
    if (
        external_call["connect", Int32](
            fd, address.unsafe_ptr(), UInt32(path.byte_length() + 3)
        )
        != 0
    ):
        raise io_error("connect " + path)
    return connection^


def connect(
    path: String, instance: UInt64 = 0, origin: Optional[UInt64] = None
) raises -> Connection:
    var connection = open_connection(path)
    var process = external_call["getpid", UInt32]()
    var context = ClientContext.capture()
    var hello = client_hello(
        instance if instance != 0 else UInt64(process),
        process,
        origin,
        context^,
    )
    connection.write(Span(hello))
    var response = connection.read_frame()
    var decoder = Decoder(response^)
    if decoder.uint() != 1:
        raise Error("daemon did not send server hello")
    if decoder.uint() != UInt64(PROTOCOL_VERSION):
        raise Error("server hello version mismatch")
    connection.server_id = decoder.uint()
    connection.client_id = decoder.uint()
    var acknowledged_instance = decoder.uint()
    if acknowledged_instance != (
        instance if instance != 0 else UInt64(process)
    ):
        raise Error("server hello client instance mismatch")
    return connection^
