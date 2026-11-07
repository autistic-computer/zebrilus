from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text, read_bool, read_optional_text
from zzmojo.which_key_caps import which_key_stroke, which_key_format


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    if len(bytes) > 16 * 1024 * 1024:
        raise Error("which-key cap fixture exceeds limit")
    var source = Decoder(bytes^)
    source.offset = 0
    var count = Int(source.uint())
    var accepted = 0
    for index in range(count):
        var text = read_text(source)
        var parsed = which_key_stroke(text)
        var present = read_bool(source)
        try:
            assert_equal(Bool(parsed), present)
            if present:
                ref key = parsed.value()
                assert_equal(key.key, read_text(source))
                assert_equal(UInt64(key.modifiers), source.uint())
                assert_equal(key.key_char, read_optional_text(source))
                assert_equal(which_key_format(key), read_text(source))
                accepted += 1
        except error:
            print("which-key cap", index, "input", text)
            raise error
    source.finish()
    print(count, accepted)
