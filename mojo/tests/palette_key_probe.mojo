from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.input_protocol import decode_input_frame
from zzmojo.palette_keymap import palette_open_shortcut


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = Int(reader.uint())
    for _ in range(count):
        var apple = reader.uint() != 0
        var overlay = reader.uint() != 0
        var capture = reader.uint() != 0
        var expected = reader.uint() != 0
        var frame = decode_input_frame(reader.bytes())
        assert_equal(
            palette_open_shortcut(frame.key.value(), apple, overlay, capture),
            expected,
        )
    reader.finish()
    print(
        "palette defaults: source UI-table ordering over prefix, modal/capture"
        " policy and native key normalization passed"
    )
