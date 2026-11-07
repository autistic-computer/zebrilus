from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool, read_text, read_optional_id, read_optional_text, read_count
from zzmojo.path_protocol import PathMessage, PathRoot, PathEntry, PathMark, decode_path_message, encode_path_message
from zzmojo.path_listing import PathListing, path_mark_letter
from zzmojo.stream_protocol import interactive_hello


def optional_text(mut output: List[UInt8], value: Optional[String]):
    output.append(UInt8(Bool(value)))
    if value:
        append_text(output, value.value())


def write_root(mut output: List[UInt8], value: Optional[PathRoot]):
    output.append(UInt8(Bool(value)))
    if value:
        ref root = value.value()
        append_text(output, root.root)
        append_text(output, root.display_root)
        optional_text(output, root.cwd)
        output.append(root.insert)


def listing_traces(mut reader: Decoder) raises -> List[UInt8]:
    var output = List[UInt8]()
    var traces = Int(reader.uint())
    for _ in range(traces):
        var host = reader.text()
        var dir = reader.text()
        var request = reader.uint()
        var listing = PathListing(host, dir, request)
        var lookups = List[String]()
        for _ in range(Int(reader.uint())):
            lookups.append(reader.text())
        var dirs = List[String]()
        for _ in range(Int(reader.uint())):
            dirs.append(reader.text())
        var hosts = List[String]()
        for _ in range(Int(reader.uint())):
            hosts.append(reader.text())
        for _ in range(Int(reader.uint())):
            var bytes = reader.bytes()
            var decoded = decode_path_message(bytes^)
            if not decoded:
                raise Error("listing trace has no path update")
            var message = decoded.take()
            output.append(UInt8(listing.apply(message^)))
            write_root(output, listing.root)
            append_uint(output, UInt64(len(listing.chunks)))
            for chunk in listing.chunks:
                append_uint(output, UInt64(len(chunk[].entries)))
                for entry in chunk[].entries:
                    append_text(output, entry.rel)
                    output.append(entry.kind)
                    output.append(UInt8(entry.symlink))
            for lookup in lookups:
                var mark = listing.marks.get(lookup)
                output.append(UInt8(Bool(mark)))
                if mark:
                    output.append(mark.value())
                optional_text(output, path_mark_letter(mark.value()) if mark else Optional[String](None))
            optional_text(output, listing.error)
            output.append(UInt8(listing.done))
            output.append(UInt8(listing.truncated))
            output.append(UInt8(listing.is_empty()))
            for host in hosts:
                for dir in dirs:
                    output.append(UInt8(listing.covers(host, dir)))
    reader.finish()
    return output^


def arguments(mut reader: Decoder) raises -> PathMessage:
    var result = PathMessage(UInt8(reader.uint()), reader.uint())
    result.pane = read_optional_id(reader)
    result.dir = read_optional_text(reader, 4097)
    if read_bool(reader):
        var root = read_text(reader, 4097)
        var display = read_text(reader, 4097)
        var cwd = read_optional_text(reader, 4097)
        var insert = UInt8(reader.uint())
        result.root = PathRoot(root^, display^, cwd^, insert)
    result.error = read_optional_text(reader, 4097)
    var count = read_count(reader, 50001)
    for _ in range(count):
        var rel = read_text(reader, 4097)
        var kind = UInt8(reader.uint())
        var symlink = read_bool(reader)
        result.entries.append(PathEntry(rel^, kind, symlink))
    count = read_count(reader, 50001)
    for _ in range(count):
        var rel = read_text(reader, 4097)
        var kind = UInt8(reader.uint())
        result.marks.append(PathMark(rel^, kind))
    result.done = read_bool(reader)
    result.truncated = read_bool(reader)
    result.sequence = reader.uint()
    reader.finish()
    return result^


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[2], "r") as file:
        bytes = file.read_bytes()
    var output: List[UInt8]
    if args[1] == "roundtrip":
        var message = decode_path_message(bytes^)
        if not message:
            raise Error("not a path listing message")
        output = encode_path_message(message.value())
    elif args[1] == "build":
        var reader = Decoder(bytes^)
        reader.offset = 0
        var message = arguments(reader)
        output = encode_path_message(message)
    elif args[1] == "listing":
        var reader = Decoder(bytes^)
        reader.offset = 0
        output = listing_traces(reader)
    elif args[1] == "hello":
        var reader = Decoder(bytes^)
        reader.offset = 0
        var search = read_bool(reader)
        var paths = read_bool(reader)
        var origin = read_optional_id(reader)
        reader.finish()
        output = interactive_hello(42, 7, "", origin=origin, options=31, viewport=False, native_search=search, native_paths=paths)
    else:
        raise Error("unknown path listing probe mode")
    with open(args[3], "w") as file:
        file.write_bytes(Span(output))
