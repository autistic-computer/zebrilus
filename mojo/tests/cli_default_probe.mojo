from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.exec_protocol import append_invocation
from zzmojo.cli_default import parse_default


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 64 * 1024 * 1024:
        raise Error("default fixtures exceed their byte limit")
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 100_000:
        raise Error("default fixtures exceed their item limit")
    var output = List[UInt8]()
    for _ in range(Int(count)):
        var text = reader.text()
        var home: Optional[String] = None
        if reader.uint() != 0:
            home = reader.text()
        var homes = Dict[String, Optional[String]]()
        var home_count = reader.uint()
        if home_count > 1024:
            raise Error("default home fixtures exceed their item limit")
        for _ in range(Int(home_count)):
            var name = reader.text()
            var value: Optional[String] = None
            if reader.uint() != 0:
                value = reader.text()
            homes[name] = value.copy()
        var parsed = parse_default(text, home, homes)
        var item = List[UInt8]()
        append_uint(item, UInt64(parsed.kind))
        if parsed.kind == 1:
            append_uint(item, UInt64(parsed.line))
            append_uint(item, UInt64(parsed.column))
            append_text(item, parsed.error)
        append_uint(item, UInt64(len(parsed.commands)))
        for command in parsed.commands:
            append_invocation(item, command)
        append_uint(item, UInt64(Bool(parsed.needed_home)))
        if parsed.needed_home:
            append_text(item, parsed.needed_home.value())
        append_uint(output, UInt64(len(item)))
        output.extend(Span(item))
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
