from std.ffi import external_call
from std.sys.info import CompilationTarget
from .terminal_model import TerminalViewport, TerminalPatch, TerminalCell
from .native_cells import CellBuffers


@fieldwise_init
struct _PollFd(TrivialRegisterPassable):
    var fd: Int32
    var events: Int16
    var returned: Int16


def _ready(fd: Int32, events: Int16, timeout: Int32 = 2000) raises:
    var descriptor = _PollFd(fd, events, 0)
    var result = external_call["poll", Int32](
        Pointer(to=descriptor), UInt64(1), timeout
    )
    if result <= 0 or descriptor.returned & events == 0:
        raise Error("Bend worker unavailable or timed out")


def _write_pipe(fd: Int32, bytes: Span[UInt8, _], offset: Int) raises -> Int:
    var blocked = SIMD[.uint64, 16](0)
    var previous = SIMD[.uint64, 16](0)
    var pending = SIMD[.uint64, 16](0)
    if external_call["sigemptyset", Int32](Pointer(to=blocked)) != 0:
        raise Error("Bend signal set initialization failed")
    if external_call["sigaddset", Int32](Pointer(to=blocked), Int32(13)) != 0:
        raise Error("Bend pipe signal setup failed")
    comptime block = 0 if CompilationTarget.is_linux() else 1
    comptime restore = 2 if CompilationTarget.is_linux() else 3
    if (
        external_call["pthread_sigmask", Int32](
            Int32(block), Pointer(to=blocked), Pointer(to=previous)
        )
        != 0
    ):
        raise Error("Bend pipe signal blocking failed")
    var had_pending = False
    if external_call["sigpending", Int32](Pointer(to=pending)) == 0:
        had_pending = (
            external_call["sigismember", Int32](Pointer(to=pending), Int32(13))
            == 1
        )
    var count = external_call["write", Int](
        Int(fd), bytes.unsafe_ptr().unsafe_offset(offset), len(bytes) - offset
    )
    comptime errno_name: StaticString = "__errno_location" if CompilationTarget.is_linux() else "__error"
    var number = external_call[
        errno_name, Pointer[Int32, MutUntrackedOrigin]
    ]()[]
    var was_blocked = (
        external_call["sigismember", Int32](Pointer(to=previous), Int32(13))
        == 1
    )
    if count < 0 and number == 32 and not had_pending and not was_blocked:
        if external_call["sigpending", Int32](Pointer(to=pending)) == 0:
            if (
                external_call["sigismember", Int32](
                    Pointer(to=pending), Int32(13)
                )
                == 1
            ):
                var received: Int32 = 0
                _ = external_call["sigwait", Int32](
                    Pointer(to=blocked), Pointer(to=received)
                )
    var restored = external_call["pthread_sigmask", Int32](
        Int32(restore), Pointer(to=previous), Pointer(to=pending)
    )
    if restored != 0:
        raise Error("Bend pipe signal restoration failed")
    return count


struct BendGate(Movable):
    var read_fd: Int32
    var write_fd: Int32
    var process: Int32
    var buffers: Optional[CellBuffers]

    def __init__(out self, path: String) raises:
        self.read_fd = -1
        self.write_fd = -1
        self.process = -1
        self.buffers = None
        if path.byte_length() == 0:
            raise Error("Bend executable path is empty")
        for byte in path.as_bytes():
            if byte == 0:
                raise Error("Bend executable path contains NUL")
        var input_pipe = List[Int32](length=2, fill=-1)
        if external_call["pipe", Int32](input_pipe.unsafe_ptr()) != 0:
            raise Error("Bend input pipe failed")
        var output_pipe = List[Int32](length=2, fill=-1)
        if external_call["pipe", Int32](output_pipe.unsafe_ptr()) != 0:
            _ = external_call["close", Int32](input_pipe[0])
            _ = external_call["close", Int32](input_pipe[1])
            raise Error("Bend output pipe failed")
        var descriptors: List[Int32] = [
            input_pipe[0],
            input_pipe[1],
            output_pipe[0],
            output_pipe[1],
        ]
        for index in range(4):
            if descriptors[index] < 3:
                var duplicate = external_call["fcntl", Int32, num_fixed_args=2](
                    descriptors[index], Int32(0), Int32(3)
                )
                if duplicate < 0:
                    for descriptor in descriptors:
                        _ = external_call["close", Int32](descriptor)
                    raise Error("Bend descriptor duplication failed")
                _ = external_call["close", Int32](descriptors[index])
                descriptors[index] = duplicate
            if (
                external_call["fcntl", Int32, num_fixed_args=2](
                    descriptors[index], Int32(2), Int32(1)
                )
                != 0
            ):
                for descriptor in descriptors:
                    _ = external_call["close", Int32](descriptor)
                raise Error("Bend descriptor inheritance setup failed")
        var input_read = descriptors[0]
        var input_write = descriptors[1]
        var output_read = descriptors[2]
        var output_write = descriptors[3]
        var executable = path.copy()
        var executable_ptr = executable.as_c_string_span().ptr()
        var mode = String("serve")
        var mode_ptr = mode.as_c_string_span().ptr()
        var process = external_call["fork", Int32]()
        if process == 0:
            _ = external_call["close", Int32](input_write)
            _ = external_call["close", Int32](output_read)
            if external_call["dup2", Int32](input_read, Int32(0)) < 0:
                external_call["_exit", NoneType](Int32(126))
            if external_call["dup2", Int32](output_write, Int32(1)) < 0:
                external_call["_exit", NoneType](Int32(126))
            _ = external_call["close", Int32](input_read)
            _ = external_call["close", Int32](output_write)
            _ = external_call["execl", Int32, num_fixed_args=1](
                executable_ptr,
                executable_ptr,
                mode_ptr,
                Optional[Pointer[UInt8, ImmUntrackedOrigin]](None),
            )
            external_call["_exit", NoneType](Int32(127))
            _ = mode
            _ = executable
        _ = mode
        _ = executable
        _ = external_call["close", Int32](input_read)
        _ = external_call["close", Int32](output_write)
        if process < 0:
            _ = external_call["close", Int32](input_write)
            _ = external_call["close", Int32](output_read)
            raise Error("Bend worker fork failed")
        self.read_fd = output_read
        self.write_fd = input_write
        self.process = process

    def __deinit__(deinit self):
        if self.read_fd >= 0:
            _ = external_call["close", Int32](self.read_fd)
        if self.write_fd >= 0:
            _ = external_call["close", Int32](self.write_fd)
        if self.process > 0:
            _ = external_call["kill", Int32](self.process, Int32(15))
            var status: Int32 = 0
            _ = external_call["waitpid", Int32](
                self.process, Pointer(to=status), Int32(0)
            )

    def validate(self, frame: Span[UInt8, _]) raises -> UInt8:
        var declared: UInt32 = 0
        if len(frame) >= 4:
            for index in range(4):
                declared |= UInt32(frame[index]) << UInt32(index * 8)
        var lane: UInt8 = 0
        var flags: UInt8 = 0
        var version: UInt16 = 0
        if len(frame) >= 8:
            lane = frame[4]
            flags = frame[5]
            version = UInt16(frame[6]) | (UInt16(frame[7]) << 8)
        var line = (
            "validate "
            + String(len(frame))
            + " "
            + String(declared)
            + " "
            + String(lane)
            + " "
            + String(flags)
            + " "
            + String(version)
            + "\n"
        )
        return self._verdict(line, 6)

    def terminal(
        self, viewport: TerminalViewport, patch: TerminalPatch
    ) raises -> UInt8:
        return self._verdict(
            self._terminal_line("terminal", viewport, patch), 4
        )

    def tree(
        self,
        mode: String,
        current: String,
        update: String,
        result: String,
        timeout: Int32 = 8000,
    ) raises -> UInt8:
        if mode != "delta" and mode != "overlay" and mode != "view":
            raise Error("invalid Bend tree operation")
        if timeout <= 0:
            raise Error("invalid Bend tree deadline")
        for path in [current, update, result]:
            if path.byte_length() == 0:
                raise Error("empty Bend tree buffer path")
            for byte in path.as_bytes():
                if byte <= 32 or byte == 127:
                    raise Error("invalid Bend tree buffer path")
        var line = String(
            "tree ", mode, " ", current, " ", update, " ", result, "\n"
        )
        return self._verdict(line, 4, timeout)

    def cells(
        mut self, viewport: TerminalViewport, patch: TerminalPatch
    ) raises -> List[TerminalCell]:
        if not self.buffers:
            self.buffers = CellBuffers()
        ref buffers = self.buffers.value()
        buffers.retained.store(viewport.cells)
        buffers.changed.store(patch.cells)
        var prefix = String(
            "cells ",
            buffers.retained.path,
            " ",
            buffers.changed.path,
            " ",
            buffers.result.path,
        )
        var timeout = Int32(2000 + len(viewport.cells) * 2 // 1000)
        if (
            self._verdict(
                self._terminal_line(prefix, viewport, patch), 4, timeout
            )
            != 0
        ):
            raise Error("Bend rejected terminal cell mutation")
        return buffers.result.load(len(viewport.cells))

    def _terminal_line(
        self, prefix: String, viewport: TerminalViewport, patch: TerminalPatch
    ) -> String:
        var values: List[UInt64] = [
            viewport.generation >> 32,
            viewport.generation & 0xFFFFFFFF,
            viewport.view_generation >> 32,
            viewport.view_generation & 0xFFFFFFFF,
            UInt64(viewport.dictionary_generation),
            UInt64(viewport.columns),
            UInt64(viewport.rows),
            UInt64(len(viewport.styles)),
            UInt64(len(viewport.grapheme_offsets) - 1),
            UInt64(len(viewport.grapheme_bytes)),
            patch.base_generation >> 32,
            patch.base_generation & 0xFFFFFFFF,
            patch.base_view_generation >> 32,
            patch.base_view_generation & 0xFFFFFFFF,
            patch.generation >> 32,
            patch.generation & 0xFFFFFFFF,
            patch.view_generation >> 32,
            patch.view_generation & 0xFFFFFFFF,
            UInt64(patch.dictionary_generation),
            UInt64(patch.columns),
            UInt64(patch.rows),
            UInt64(patch.style_base),
            UInt64(len(patch.styles)),
            UInt64(patch.grapheme_base),
            UInt64(len(patch.grapheme_lengths)),
            UInt64(len(patch.grapheme_bytes)),
            UInt64(patch.scroll > 0),
            UInt64(abs(patch.scroll)),
            UInt64(len(patch.cells)),
        ]
        var line = prefix
        for value in values:
            line += " " + String(value)
        for span in patch.spans:
            line += String(
                " ",
                span.row,
                " ",
                span.start,
                " ",
                span.length,
                " ",
                Int(span.clear),
            )
        line += "\n"
        return line

    def _verdict(
        self, line: String, maximum: UInt8, timeout: Int32 = 2000
    ) raises -> UInt8:
        var bytes = line.as_bytes()
        var offset = 0
        while offset < len(bytes):
            _ready(self.write_fd, 4, timeout)
            var count = _write_pipe(self.write_fd, bytes, offset)
            if count <= 0:
                raise Error("Bend worker write failed")
            offset += count
        var response = List[UInt8](length=2, fill=0)
        offset = 0
        while offset < 2:
            _ready(self.read_fd, 1, timeout)
            var count = external_call["read", Int](
                Int(self.read_fd),
                response.unsafe_ptr().unsafe_offset(offset),
                2 - offset,
            )
            if count <= 0:
                raise Error("Bend worker read failed")
            offset += count
        if response[0] < 48 or response[0] > 48 + maximum or response[1] != 10:
            raise Error("invalid Bend worker verdict")
        return response[0] - 48
