from std.sys import argv
from zzmojo.protocol import Decoder, append_uint
from zzmojo.palette_navigation import PaletteNavRow, PaletteLayers, initial_selection, navigate, parent_index, child_index


def optional_index(mut output: List[UInt8], value: Optional[Int]):
    append_uint(output, UInt64(Bool(value)))
    if value:
        append_uint(output, UInt64(value.value()))


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 16 * 1024 * 1024:
        raise Error("palette navigation fixtures exceed their byte limit")
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 100_000:
        raise Error("palette navigation fixtures exceed their item limit")
    var output = List[UInt8]()
    for _ in range(Int(count)):
        var rows = List[PaletteNavRow]()
        var row_count = reader.uint()
        if row_count > 65536:
            raise Error("palette navigation fixture rows exceed their limit")
        for _ in range(Int(row_count)):
            var kind = UInt8(reader.uint())
            var current = reader.uint() != 0
            var bits = UInt32(reader.uint())
            var indent = Pointer(to=bits).unsafe_bitcast[Float32]()[]
            var expansion = reader.uint()
            var expanded: Optional[Bool] = None
            if expansion > 2:
                raise Error("invalid palette expansion fixture")
            if expansion != 0:
                expanded = expansion == 2
            var choice = UInt32(reader.uint())
            rows.append(PaletteNavRow(kind, current, indent, expanded, choice))
        var window: Optional[UInt32] = None
        if reader.uint() != 0:
            window = UInt32(reader.uint())
        var selected: Optional[Int] = None
        if reader.uint() != 0:
            selected = Int(reader.uint())
        var negative = reader.uint() != 0
        var magnitude = reader.uint()
        var direction = -Int(magnitude) if negative else Int(magnitude)
        optional_index(output, initial_selection(Span(rows), window))
        optional_index(output, navigate(Span(rows), selected, direction))
        for index in range(len(rows) + 2):
            optional_index(output, parent_index(Span(rows), index))
        for index in range(len(rows) + 2):
            optional_index(output, child_index(Span(rows), index))
    var layer_count = reader.uint()
    if layer_count > 1024:
        raise Error("palette layer fixtures exceed their item limit")
    for _ in range(Int(layer_count)):
        var bits = UInt8(reader.uint())
        var count = reader.uint()
        if count > 64:
            raise Error("palette layer fixture steps exceed their limit")
        var layers = PaletteLayers()
        if bits & 1 != 0:
            layers.command = 0
        if bits & 2 != 0:
            layers.mode = UInt8(0)
        if bits & 4 != 0:
            layers.host = UInt32(0)
        for _ in range(Int(count)):
            append_uint(output, UInt64(layers.pop()))
        var remaining = UInt8(Bool(layers.command)) | (UInt8(Bool(layers.mode)) << 1) | (UInt8(Bool(layers.host)) << 2)
        append_uint(output, UInt64(remaining))
    var large = List[PaletteNavRow](length=65537, fill=PaletteNavRow(0, False, 0, None, 0))
    var refused = False
    try:
        _ = navigate(Span(large), None, 0)
    except error:
        refused = String(error) == "palette navigation exceeds its row limit"
    append_uint(output, UInt64(refused))
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
