from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.exec_protocol import append_invocation
from zzmojo.cli_attach import parse_attach


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 64 * 1024 * 1024:
        raise Error("attach fixtures exceed their byte limit")
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 100_000:
        raise Error("attach fixtures exceed their item limit")
    var output = List[UInt8]()
    for _ in range(Int(count)):
        var words = List[List[UInt8]]()
        var word_count = reader.uint()
        if word_count > 8192:
            raise Error("attach fixture words exceed their item limit")
        for _ in range(Int(word_count)):
            words.append(reader.bytes())
        var parsed = parse_attach(Span(words))
        var item = List[UInt8]()
        append_uint(item, UInt64(parsed.code))
        append_uint(item, UInt64(Bool(parsed.error)))
        if parsed.error:
            append_text(item, parsed.error.value())
        else:
            append_uint(item, UInt64(parsed.restart))
            append_uint(item, UInt64(parsed.default_attach))
            append_invocation(item, parsed.command)
        append_uint(output, UInt64(len(item)))
        output.extend(Span(item))
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
