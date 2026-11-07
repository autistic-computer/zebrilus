from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.palette_commands import PaletteCommands


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 16 * 1024 * 1024:
        raise Error("palette command fixtures exceed their byte limit")
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 100_000:
        raise Error("palette command fixtures exceed their item limit")
    var model = PaletteCommands()
    var output = List[UInt8]()
    for _ in range(Int(count)):
        var query = reader.text()
        var browser = reader.uint() != 0
        var editor = reader.uint() != 0
        var rows = model.rows(query, browser, editor)
        append_uint(output, UInt64(len(rows)))
        for row in rows:
            append_uint(output, UInt64(row.command))
            append_text(output, model.commands[row.command].name)
            append_text(output, model.commands[row.command].description)
            append_uint(output, UInt64(len(row.hits)))
            for hit in row.hits:
                append_uint(output, UInt64(hit.start))
                append_uint(output, UInt64(hit.end))
        if len(output) > 16 * 1024 * 1024:
            raise Error("palette command results exceed their byte limit")
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
