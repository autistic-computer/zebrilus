from std.sys import argv
from zzmojo.cli_preflight import preflight
from zzmojo.exec_protocol import Invocation, append_invocation
from zzmojo.protocol import Decoder, append_text, append_uint


def main() raises:
    var args = argv()
    if len(args) != 3:
        raise Error("usage: cli-preflight-probe INPUT OUTPUT")
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 16 * 1024 * 1024:
        raise Error("preflight fixture exceeds limit")
    var reader = Decoder(data^)
    reader.offset = 0
    var cases = reader.uint()
    if cases > 100000:
        raise Error("preflight fixture case count exceeds limit")
    var output = List[UInt8]()
    for _ in range(Int(cases)):
        var commands = List[Invocation]()
        var count = reader.uint()
        if count > 4096:
            raise Error("preflight fixture command count exceeds limit")
        var before = List[UInt8]()
        for _ in range(Int(count)):
            var command = Invocation()
            command.name = String(from_utf8=Span(reader.bytes()))
            var arguments = reader.uint()
            if arguments > 4096:
                raise Error("preflight fixture argument count exceeds limit")
            for _ in range(Int(arguments)):
                command.args.append(reader.bytes())
            append_invocation(before, command)
            commands.append(command^)
        var result = preflight(Span(commands))
        var after = List[UInt8]()
        for command in commands:
            append_invocation(after, command)
        if before != after:
            raise Error("preflight mutated invocation bytes")
        append_text(output, result.canonical)
        output.append(UInt8(result.starts_server))
        output.append(result.code)
        output.append(UInt8(Bool(result.error)))
        if result.error:
            append_text(output, result.error.value())
    if reader.offset != len(reader.data):
        raise Error("trailing preflight fixture bytes")
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
    var typed = Invocation()
    typed.name = "if-shell"
    typed.args.append(List[UInt8](String("true").as_bytes()))
    typed.args.append(List[UInt8](String("{ unknown-command }").as_bytes()))
    typed.blocks.append(1)
    var typed_commands = List[Invocation]()
    typed_commands.append(typed^)
    var refused = preflight(Span(typed_commands))
    if not refused.error or refused.error.value() != "typed configuration blocks require a running server":
        raise Error("cold typed config block was admitted")
    var limit = Invocation()
    limit.name = "inspect"
    for _ in range(4095):
        limit.args.append(List[UInt8]())
    var limit_commands = List[Invocation]()
    limit_commands.append(limit^)
    var exact_limit = preflight(Span(limit_commands))
    if exact_limit.error or exact_limit.canonical != "inspect":
        raise Error("exact CLI word limit was rejected")
    limit_commands[0].args.append(List[UInt8]())
    var too_many = preflight(Span(limit_commands))
    if not too_many.error or too_many.error.value() != "command line exceeds preflight limit":
        raise Error("oversized CLI word count was admitted")
    limit_commands[0].args.clear()
    var payload = List[UInt8](length=4 * 1024 * 1024 - 7, fill=UInt8(97))
    limit_commands[0].args.append(payload^)
    var exact_bytes = preflight(Span(limit_commands))
    if exact_bytes.error:
        raise Error("exact CLI byte limit was rejected")
    limit_commands[0].args[0].append(97)
    var oversized = preflight(Span(limit_commands))
    if not oversized.error or oversized.error.value() != "command line exceeds preflight limit":
        raise Error("oversized CLI byte count was admitted")
