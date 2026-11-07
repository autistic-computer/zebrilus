from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.palette_ui_keys import palette_ui_key


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = Int(reader.uint())
    var output = List[UInt8]()
    for _ in range(count):
        var value = palette_ui_key(reader.text())
        output.append(UInt8(Bool(value)))
        if value:
            append_text(output, value.value().key)
            append_uint(output, UInt64(value.value().modifiers))
            output.append(UInt8(Bool(value.value().key_char)))
            if value.value().key_char:
                append_text(output, value.value().key_char.value())
    with open(args[2], "w") as file:
        file.write_bytes(Span(output))
