from std.ffi import external_call
from std.sys.info import CompilationTarget
from .transport import Connection, open_connection
from .client_files import ClientFiles, StdinSignal
from .file_protocol import (
    CLIENT_FILE_REQUEST,
    decode_file_request,
    encode_file_response,
)
from .protocol import (
    MAX_FRAME_BYTES,
    Decoder,
    Response,
    control_tag,
    control_uint,
    decode_response,
)
from .exec_protocol import (
    EXEC_EXIT,
    ExecRequest,
    ExecExit,
    encode_exec,
    decode_exec_exit,
    decode_exec_notice,
)
from .cli_output import CliOutput, stderr_line
from .stream_protocol import pong


@fieldwise_init
struct ExecutionResult(Movable):
    var code: UInt8
    var exit: ExecExit


def execute_local(
    path: String, request: ExecRequest, var files: ClientFiles
) raises -> ExecutionResult:
    return execute_connected(open_connection(path), request, files^)


def execute_connected(
    var connection: Connection, request: ExecRequest, var files: ClientFiles
) raises -> ExecutionResult:
    connection.files = files^
    var timeout: List[Int] = [0, 0]
    comptime LEVEL = 1 if CompilationTarget.is_linux() else 0xFFFF
    comptime RECEIVE = 20 if CompilationTarget.is_linux() else 0x1006
    comptime SEND = 21 if CompilationTarget.is_linux() else 0x1005
    var options: List[Int] = [RECEIVE, SEND]
    for option in options:
        if (
            external_call["setsockopt", Int32](
                connection.fd,
                Int32(LEVEL),
                Int32(option),
                timeout.unsafe_ptr(),
                UInt32(16),
            )
            != 0
        ):
            raise Error("could not configure command transport")
    _ = timeout
    var signal = StdinSignal()
    var output = CliOutput()
    var frame = encode_exec(request)
    connection.write(Span(frame))
    var code: UInt8 = 0
    var pending: Optional[Response] = None
    var streamed_error = List[UInt8]()
    var error_overflow = False
    while True:
        var incoming = connection.read_frame()
        var tag = control_tag(Span(incoming))
        if UInt64(tag) == CLIENT_FILE_REQUEST:
            var file_request = decode_file_request(incoming^)
            if not file_request:
                raise Error("invalid command file request")
            var file_response = connection.files.answer(file_request.value())
            var outgoing = encode_file_response(file_response)
            connection.write(Span(outgoing))
        elif tag == 3:
            var rejected = control_uint(Span(incoming), 1) == 1
            var response = decode_response(incoming^, cli_errors=True)
            if rejected:
                pending = response^
                continue
            var status = output.emit(
                Span(response.output), response.stdout_claim == 2
            )
            var skip = 0
            var text = response.error.as_bytes()
            if not error_overflow and len(streamed_error) <= len(text):
                var matches = True
                for index in range(len(streamed_error)):
                    if streamed_error[index] != text[index]:
                        matches = False
                        break
                if matches:
                    skip = len(streamed_error)
            stderr_line(text[skip:])
            streamed_error.clear()
            error_overflow = False
            if response.exit_code != 0:
                code = response.exit_code
            if status != 0:
                code = status
        elif UInt64(tag) == EXEC_EXIT:
            var finished = decode_exec_exit(incoming^)
            if not finished:
                raise Error("invalid execution result")
            if pending:
                var failure = pending.take()
                _ = output.emit(Span(failure.output))
                stderr_line(failure.error.as_bytes())
                code = 1 if len(failure.output) > 0 else failure.exit_code
            elif finished.value().outcome == 2:
                stderr_line(finished.value().error.as_bytes())
                code = finished.value().code
            elif finished.value().outcome == 3:
                code = 1
            _ = signal^
            return ExecutionResult(code, finished.take())
        elif tag == 10:
            var notice = decode_exec_notice(incoming^)
            if not notice:
                continue
            if notice.value().kind == 0:
                _ = output.emit(Span(notice.value().data))
            elif notice.value().kind == 1 and notice.value().message_kind == 3:
                var line = notice.value().text + "\n"
                stderr_line(line.as_bytes())
                if not error_overflow:
                    if line.byte_length() > MAX_FRAME_BYTES - len(
                        streamed_error
                    ):
                        streamed_error.clear()
                        error_overflow = True
                    else:
                        streamed_error.extend(line.as_bytes())
        elif tag == 68:
            var reader = Decoder(incoming^)
            _ = reader.uint()
            var outgoing = pong(reader.uint())
            connection.write(Span(outgoing))
