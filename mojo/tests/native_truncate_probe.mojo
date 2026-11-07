from std.sys import argv
from mjui.ui import Ui
from mjui.native_color import color_float, color_bits
from mjui.text.native_truncate import native_end_truncate, _char_width
from zzmojo.protocol import Decoder, append_text


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var ui = Ui(1)
    var regular = Int32(ui.add_font("bench/assets/fonts/Inter-Regular.ttf"))
    var mono = Int32(ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf"))
    var output = List[UInt8]()
    var count = Int(reader.uint())
    if count > 4096:
        raise Error("native scalar width corpus exceeds count limit")
    for _ in range(count):
        var role = reader.uint()
        var size = color_float(UInt32(reader.uint()))
        var point = Int(reader.uint())
        if role > 1:
            raise Error("unknown truncation font")
        var value = String(chr(point))
        append_text(output, String(color_bits(_char_width(ui, value, regular if role == 0 else mono, size, [regular, mono]))))
    count = Int(reader.uint())
    if count > 10000:
        raise Error("native truncation corpus exceeds count limit")
    for _ in range(count):
        var role = reader.uint()
        var size = color_float(UInt32(reader.uint()))
        var width = color_float(UInt32(reader.uint()))
        var text = reader.text()
        if role > 1:
            raise Error("unknown truncation font")
        append_text(output, native_end_truncate(ui, text, regular if role == 0 else mono, size, width, [regular, mono]))
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
