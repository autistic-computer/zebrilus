from .protocol import Decoder, append_uint, envelope, validate_envelope
from .exec_protocol import Invocation, append_invocation, read_invocation
from .tree_wire import read_bool, read_enum

comptime PALETTE_COMMAND_BYTES = 4 * 1024 * 1024


@fieldwise_init
struct PaletteCommandRequest(Copyable, Movable):
    var request: UInt64
    var command: Invocation


def validate_palette_command(command: Invocation) raises:
    var bytes = command.name.byte_length()
    if (
        bytes == 0
        or bytes > PALETTE_COMMAND_BYTES
        or len(command.args) > 4096
        or len(command.blocks) > 4096
    ):
        raise Error("palette command exceeds its name or argument limit")
    for argument in command.args:
        if len(argument) > PALETTE_COMMAND_BYTES - bytes:
            raise Error("palette command exceeds its byte limit")
        bytes += len(argument)
    if command.source:
        if (
            command.source.value().source.byte_length()
            > PALETTE_COMMAND_BYTES - bytes
        ):
            raise Error("palette command source exceeds its byte limit")
        bytes += command.source.value().source.byte_length()
    if (
        command.stdin
        and len(command.stdin.value()) > PALETTE_COMMAND_BYTES - bytes
    ):
        raise Error("palette command stdin exceeds its byte limit")


def palette_command_request(
    request: UInt64, command: Invocation
) raises -> List[UInt8]:
    validate_palette_command(command)
    var output: List[UInt8] = [2]
    append_uint(output, request)
    append_invocation(output, command)
    output.append(0)
    if len(output) + 8 > PALETTE_COMMAND_BYTES:
        raise Error("palette command frame exceeds its byte limit")
    return envelope(Span(output))


def read_palette_command(
    var frame: List[UInt8],
) raises -> PaletteCommandRequest:
    if (
        len(frame) > PALETTE_COMMAND_BYTES
        or validate_envelope(Span(frame)) != 0
    ):
        raise Error("palette command frame exceeds its lane or byte limit")
    var reader = Decoder(frame^)
    if read_enum(reader, 75) != 2:
        raise Error("palette frame is not a command request")
    var request = reader.uint()
    var command = read_invocation(reader)
    if read_bool(reader):
        raise Error("palette command cannot claim prepared execution")
    reader.finish()
    validate_palette_command(command)
    return PaletteCommandRequest(request, command^)


def palette_cancel_prefix(request: UInt64) raises -> List[UInt8]:
    var output: List[UInt8] = [9, 13]
    append_uint(output, request)
    return envelope(Span(output))
