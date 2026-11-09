from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text, read_uint
from zzmojo.window_chooser_text import window_chooser_text


def main() raises:
    var args = argv()
    var input: List[UInt8]
    with open(args[1], "r") as source:
        input = source.read_bytes()
    var reader = Decoder(input^)
    reader.offset = 0
    var count = Int(read_uint(reader, 4096))
    for _ in range(count):
        var text = read_text(reader)
        var expected = read_text(reader)
        assert_equal(window_chooser_text(text), expected)
    assert_equal(reader.offset, len(reader.data))
    print(count)
