from std.sys import argv
from zzmojo.protocol import Decoder, append_text, append_uint
from zzmojo.tree_wire import read_optional_text, read_optional_id, read_bool
from zzmojo.path_text import normalize_dir, split_rest, parent_dir, plain_path, start_dir, insert_path_text
from zzmojo.path_context import files_place, FilesPlace, start_folder, listing_pane
from zzmojo.stream_protocol import decode_stream_frame
from zzmojo.palette_places_model import PalettePlace


def optional_text(mut output: List[UInt8], value: Optional[String]):
    output.append(UInt8(Bool(value)))
    if value:
        append_text(output, value.value())


def optional_id(mut output: List[UInt8], value: Optional[UInt64]):
    output.append(UInt8(Bool(value)))
    if value:
        append_uint(output, value.value())


def execute(mut reader: Decoder) raises -> List[UInt8]:
    var kind = reader.byte()
    var output = List[UInt8]()
    if kind == 0:
        append_text(output, normalize_dir(reader.text()))
    elif kind == 1:
        var rest = reader.text()
        var start = reader.text()
        var result = split_rest(rest, start)
        append_text(output, result.dir)
        append_text(output, result.typed)
        append_text(output, result.filter)
    elif kind == 2:
        var dir = reader.text()
        optional_text(output, parent_dir(dir, read_optional_text(reader)))
    elif kind == 3:
        var root = reader.text()
        var rel = reader.text()
        append_text(output, plain_path(root, rel, read_optional_text(reader)))
    elif kind == 4:
        var cwd = read_optional_text(reader)
        var home = read_optional_text(reader)
        append_text(output, start_dir(cwd, home))
    elif kind == 5:
        var root = reader.text()
        var rel = reader.text()
        var entry_kind = reader.byte()
        var cwd = read_optional_text(reader)
        var style = reader.byte()
        var absolute = read_bool(reader)
        optional_text(output, insert_path_text(root, rel, entry_kind, cwd, style, absolute))
    elif kind == 6:
        var host = read_optional_id(reader)
        var session = read_optional_id(reader)
        var window = read_optional_id(reader)
        var pane = read_optional_id(reader)
        var written = reader.byte()
        var here = UInt32(reader.uint())
        var place = PalettePlace(UInt32(host.value()) if host else Optional[UInt32](None), session, window, pane)
        var result = files_place(place, written, here)
        append_uint(output, UInt64(result.host))
        optional_id(output, result.session)
        optional_id(output, result.window)
        optional_id(output, result.pane)
    elif kind == 7:
        var frame = decode_stream_frame(reader.bytes())
        if not frame.snapshot:
            raise Error("path context fixture has no snapshot")
        var available = read_bool(reader)
        var host = UInt32(reader.uint())
        var session = read_optional_id(reader)
        var window = read_optional_id(reader)
        var pane = read_optional_id(reader)
        var here = read_bool(reader)
        var insert = read_optional_id(reader)
        var home = read_optional_text(reader)
        var place = FilesPlace(host, session, window, pane)
        append_text(output, start_folder(place, here, frame.snapshot.value(), insert, home, available))
        optional_id(output, listing_pane(frame.snapshot.value(), insert))
    else:
        raise Error("unknown path text source case")
    reader.finish()
    return output^


def main() raises:
    var args = argv()
    var input: List[UInt8]
    with open(args[1], "r") as file:
        input = file.read_bytes()
    var reader = Decoder(input^)
    reader.offset = 0
    var output = List[UInt8]()
    for _ in range(Int(reader.uint())):
        var test = Decoder(reader.bytes())
        test.offset = 0
        var result = execute(test)
        output.extend(Span(result))
    reader.finish()
    with open(args[2], "w") as file:
        file.write_bytes(Span(output))
