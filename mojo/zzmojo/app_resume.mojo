from .app_startup_reply import StartupReply, send_startup_reply
from .client_events import ClientMessage
from std.ffi import external_call
from std.sys.info import CompilationTarget
from .protocol import MAX_FRAME_BYTES
from .exec_protocol import ExecExit, encode_exec_exit, decode_exec_exit
from .transport import Connection, bend_executable
from .bend_gate import BendGate
from .client_files import ClientFiles
from .cli_native import _errno
from .stream_transport import (
    InteractiveConnection,
    connect_interactive_transport,
)


struct OwnedFd(Movable):
    var fd: Int32

    def __init__(out self, fd: Int32):
        self.fd = fd

    def __deinit__(deinit self):
        if self.fd >= 0:
            _ = external_call["close", Int32](self.fd)

    def take(mut self) -> Int32:
        var fd = self.fd
        self.fd = -1
        return fd

    def protect(self) raises:
        if (
            self.fd < 3
            or external_call["fcntl", Int32, num_fixed_args=2](
                self.fd, Int32(2), Int32(1)
            )
            != 0
        ):
            raise Error("invalid application startup descriptor")


def _read_payload(fd: Int32) raises -> ExecExit:
    var size = external_call["lseek", Int64](fd, Int64(0), Int32(2))
    if size < 8 or size > Int64(MAX_FRAME_BYTES + 4):
        raise Error(
            "application resume payload exceeds byte limit or is not seekable"
        )
    if external_call["lseek", Int64](fd, Int64(0), Int32(0)) != 0:
        raise Error("could not rewind application resume payload")
    var bytes = List[UInt8](unsafe_uninit_length=Int(size))
    var offset = 0
    while offset < Int(size):
        var count = external_call["read", Int](
            Int(fd),
            bytes.unsafe_ptr().unsafe_offset(offset),
            Int(size) - offset,
        )
        if count < 0 and _errno() == 4:
            continue
        if count <= 0:
            raise Error("could not read application resume payload")
        offset += count
    var decoded = decode_exec_exit(bytes^)
    if (
        not decoded
        or decoded.value().outcome != 1
        or len(decoded.value().commands) == 0
    ):
        raise Error("application startup requires a prepared execution resume")
    for command in decoded.value().commands:
        if command.failure:
            raise Error(
                "application startup contains a rejected prepared command"
            )
    return decoded.take()


struct ResumeStartup(Movable):
    var socket: OwnedFd
    var resume: ExecExit
    var reply: OwnedFd

    def __init__(
        out self, socket: Int32, payload: Int32, reply: Int32 = -1
    ) raises:
        self.socket = OwnedFd(-1)
        self.resume = ExecExit()
        self.reply = OwnedFd(-1)
        if reply != -1:
            if reply < 3 or reply == socket or reply == payload:
                raise Error(
                    "application startup reply descriptor must be distinct and"
                    " greater than two"
                )
            self.reply = OwnedFd(reply)
            self.reply.protect()
        if socket < 3 or payload < 3:
            raise Error(
                "application startup descriptors must be greater than two"
            )
        self.socket = OwnedFd(socket)
        if socket == payload:
            self.socket.protect()
            raise Error("application startup descriptors must be distinct")
        var stored = OwnedFd(payload)
        self.socket.protect()
        stored.protect()
        var kind: Int32 = 0
        var size: UInt32 = 4
        comptime LEVEL = 1 if CompilationTarget.is_linux() else 0xFFFF
        comptime TYPE = 3 if CompilationTarget.is_linux() else 0x1008
        if (
            external_call["getsockopt", Int32](
                socket,
                Int32(LEVEL),
                Int32(TYPE),
                Pointer(to=kind),
                Pointer(to=size),
            )
            != 0
            or kind != 1
        ):
            raise Error("application startup requires a stream socket")
        var address = List[UInt8](length=128, fill=0)
        size = 128
        if (
            external_call["getpeername", Int32](
                socket, address.unsafe_ptr(), Pointer(to=size)
            )
            != 0
        ):
            raise Error("application startup socket is not connected")
        comptime FAMILY = 0 if CompilationTarget.is_linux() else 1
        if address[FAMILY] != 1:
            raise Error("application startup requires a local socket")
        var resume = _read_payload(stored.fd)
        _ = stored^
        self.resume = resume^

    def connect(var self, native_paths: Bool = False, path_epoch: UInt64 = 0, native_chooser: Bool = False, native_keys: Bool = False) raises -> InteractiveConnection:
        try:
            var gate = BendGate(bend_executable())
            var transport = Connection(
                self.socket.take(), 1, 0, 0, gate^, ClientFiles()
            )
            var stream = connect_interactive_transport(
                transport^,
                "",
                native_search=True,
                prepared=self.resume.commands.copy(),
                native_paths=native_paths,
                path_epoch=path_epoch,
                native_chooser=native_chooser,
                native_keys=native_keys,
            )
            if stream.server_id != self.resume.server:
                raise Error("application resume server identity mismatch")
            var output = List[UInt8]()
            var message = String()
            var code: UInt8 = 0
            for initial in stream.initial_responses:
                ref response = initial.response
                if initial.rejected and stream.model.view.session:
                    stream.model.message = ClientMessage(
                        None, 3, response.error.copy(), None, None
                    )
                    stream.model.revision += 1
                    continue
                var newline = (
                    len(response.output) > 0
                    and response.output[len(response.output) - 1] != 10
                )
                if len(response.output) + Int(newline) > MAX_FRAME_BYTES - len(
                    output
                ):
                    raise Error("application startup output exceeds byte limit")
                output.extend(Span(response.output))
                if newline:
                    output.append(10)
                if initial.rejected:
                    message = response.error.copy()
                    code = 1
                    break
                if response.exit_code != 0:
                    message = "command exited with status " + String(
                        response.exit_code
                    )
                    code = 1
                    break
            stream.initial_responses.clear()
            stream.initial_pending.clear()
            stream.initial_bytes = 0
            if self.reply.fd >= 0:
                send_startup_reply(
                    self.reply.fd, StartupReply(output^, message.copy(), code)
                )
                _ = OwnedFd(self.reply.take())
            if code != 0:
                raise Error(message)
            return stream^
        except error:
            if self.reply.fd >= 0:
                send_startup_reply(
                    self.reply.fd, StartupReply(List[UInt8](), String(error), 1)
                )
                _ = OwnedFd(self.reply.take())
            raise Error(String(error))


def store_resume(resume: ExecExit) raises -> OwnedFd:
    var frame = encode_exec_exit(resume)
    if len(frame) > MAX_FRAME_BYTES + 4:
        raise Error("application resume payload exceeds byte limit")
    var fd: Int32
    comptime if CompilationTarget.is_linux():
        var name = String("zz-app-resume")
        fd = external_call["memfd_create", Int32](
            name.as_c_string_span().ptr(), UInt32(1)
        )
        _ = name
    else:
        var name = String("/tmp/zz-app-resume-XXXXXX")
        var bytes = List[UInt8](name.as_bytes())
        bytes.append(0)
        fd = external_call["mkstemp", Int32](bytes.unsafe_ptr())
        if fd >= 0:
            _ = external_call["unlink", Int32](bytes.unsafe_ptr())
    var stored = OwnedFd(fd)
    stored.protect()
    var offset = 0
    while offset < len(frame):
        var count = external_call["write", Int](
            Int(fd),
            frame.unsafe_ptr().unsafe_offset(offset),
            len(frame) - offset,
        )
        if count < 0 and _errno() == 4:
            continue
        if count <= 0:
            raise Error("could not store application resume payload")
        offset += count
    return stored^
