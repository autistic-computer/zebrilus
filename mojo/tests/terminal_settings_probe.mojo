from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_count, read_text, read_uint
from zzmojo.appearance_protocol import TerminalAppearance, FontFeature
from zzmojo.terminal_settings import font_request


def families(mut reader: Decoder) raises -> List[String]:
    var result = List[String]()
    var count = read_count(reader, 32)
    for _ in range(count):
        result.append(read_text(reader, 256))
    return result^


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    var appearance = TerminalAppearance()
    appearance.font_families = families(reader)
    appearance.font_families_bold = families(reader)
    appearance.font_families_italic = families(reader)
    appearance.font_families_bold_italic = families(reader)
    appearance.font_weight = UInt16(read_uint(reader, 65535))
    var count = read_count(reader, 64)
    for _ in range(count):
        var tag: UInt32 = 0
        for index in range(4):
            tag |= UInt32(reader.byte()) << UInt32(index * 8)
        appearance.font_features.append(
            FontFeature(tag, UInt32(read_uint(reader, 4294967295)))
        )
    reader.finish()
    var request = font_request(
        appearance, "FallbackMono", args[3] == "1", args[4] == "1"
    )
    var output = List[UInt8]()
    append_text(output, request.family)
    append_uint(output, UInt64(len(request.fallbacks)))
    for fallback in request.fallbacks:
        append_text(output, fallback)
    append_uint(output, UInt64(len(request.features)))
    for feature in request.features:
        append_uint(output, 4)
        for index in range(4):
            output.append(UInt8((feature.tag >> UInt32(index * 8)) & 255))
        append_uint(output, UInt64(feature.value))
    append_uint(output, UInt64(request.weight))
    output.append(UInt8(request.italic))
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
