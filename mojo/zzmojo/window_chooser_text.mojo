from .chooser_presentation import CHOOSER_DISPLAY_BYTES
from .palette_text import scalar_width


def window_chooser_text(text: String) raises -> String:
    if text.byte_length() > CHOOSER_DISPLAY_BYTES:
        raise Error("window chooser insertion exceeds 64 KiB")
    var output = List[UInt8](capacity=text.byte_length())
    var offset = 0
    for character in text.codepoints():
        var scalar = UInt32(Int(character))
        var width = scalar_width(scalar)
        if scalar >= 32 and (scalar < 127 or scalar > 159):
            output.extend(text.as_bytes()[offset : offset + width])
        offset += width
    return String(from_utf8=Span(output))


def utf16_units(scalar: UInt32) -> Int:
    return 2 if scalar > 0xFFFF else 1


def window_chooser_marked_offset(
    original: String, offset: Int, sanitized: String
) -> Int:
    var units = 0
    var position = 0
    for character in original.codepoints():
        var scalar = UInt32(Int(character))
        var width = scalar_width(scalar)
        if position + width > offset:
            break
        units += utf16_units(scalar)
        position += width
    var walked = 0
    var bytes = 0
    for character in sanitized.codepoints():
        var scalar = UInt32(Int(character))
        var next = walked + utf16_units(scalar)
        if next > units:
            break
        walked = next
        bytes += scalar_width(scalar)
    return bytes
