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
