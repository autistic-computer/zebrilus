from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool
from zzmojo.path_protocol import (
    PathMessage,
    PathEntry,
    PathMark,
    PATH_CHUNK,
    PATH_GIT,
)
from zzmojo.path_listing import PathListing
from zzmojo.path_rank import PathRanker
from zzmojo.path_rows import path_rows


def main() raises:
    var args = argv()
    var input: List[UInt8]
    with open(args[1], "r") as file:
        input = file.read_bytes()
    var reader = Decoder(input^)
    reader.offset = 0
    var output = List[UInt8]()
    var ranker = PathRanker()
    for _ in range(Int(reader.uint())):
        var base = reader.text()
        var filter = reader.text()
        var listing = PathListing("local", "/")
        for _ in range(Int(reader.uint())):
            var message = PathMessage(PATH_CHUNK)
            for _ in range(Int(reader.uint())):
                message.entries.append(
                    PathEntry(
                        reader.text(), UInt8(reader.uint()), read_bool(reader)
                    )
                )
            _ = listing.apply(message^)
        var marks = PathMessage(PATH_GIT)
        for _ in range(Int(reader.uint())):
            marks.marks.append(PathMark(reader.text(), UInt8(reader.uint())))
        _ = listing.apply(marks^)
        var rows = path_rows(ranker, listing, base, filter)
        append_uint(output, UInt64(len(rows)))
        for row in rows:
            append_text(output, row.entry.rel)
            append_uint(output, UInt64(row.entry.kind))
            output.append(UInt8(row.entry.symlink))
            append_text(output, row.name)
            append_uint(output, UInt64(row.mark.value()) + 1 if row.mark else 0)
            append_uint(output, UInt64(len(row.matches)))
            for hit in row.matches:
                append_uint(output, UInt64(hit.start))
                append_uint(output, UInt64(hit.end))
    reader.finish()
    with open(args[2], "w") as file:
        file.write_bytes(Span(output))
