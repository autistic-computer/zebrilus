from std.sys import argv
from zzmojo.cli_command_help import command_help
from zzmojo.protocol import Decoder, append_text, append_uint


def main() raises:
    var args = argv()
    if len(args) != 3:
        raise Error("usage: cli-command-help-probe INPUT OUTPUT")
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 16 * 1024 * 1024:
        raise Error("command help fixture exceeds its byte limit")
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 10000:
        raise Error("command help fixture exceeds its case limit")
    var output = List[UInt8]()
    for _ in range(Int(count)):
        var name = reader.text()
        var result = command_help(name)
        append_uint(output, UInt64(result.code))
        append_text(output, result.text)
        append_uint(output, UInt64(Bool(result.error)))
        if result.error:
            append_text(output, result.error.value())
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
