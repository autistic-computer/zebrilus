from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text
from zzmojo.key_spelling import canonical_protocol_key
from zzmojo.input_protocol import canonical_key


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(String(args[1]), "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = Int(reader.uint())
    for _ in range(count):
        var raw = read_text(reader)
        var expected = read_text(reader)
        assert_equal(canonical_protocol_key(raw), expected)
        if len(args) > 2:
            assert_equal(canonical_key(raw), expected)
    reader.finish()
    print(count)
