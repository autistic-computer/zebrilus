from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_bool, read_optional_id
from zzmojo.stream_protocol import interactive_hello


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = Int(reader.uint())
    for _ in range(count):
        var search = read_bool(reader)
        var paths = read_bool(reader)
        var chooser = read_bool(reader)
        var origin = read_optional_id(reader)
        var expected = reader.bytes()
        var actual = interactive_hello(
            42,
            7,
            "",
            origin=origin,
            options=31,
            viewport=False,
            native_search=search,
            native_paths=paths,
            native_chooser=chooser,
        )
        assert_equal(len(actual), len(expected))
        for index in range(len(actual)):
            assert_equal(actual[index], expected[index])
        if not chooser:
            var defaults = interactive_hello(
                42,
                7,
                "",
                origin=origin,
                options=31,
                viewport=False,
                native_search=search,
                native_paths=paths,
            )
            assert_equal(len(defaults), len(expected))
            for index in range(len(defaults)):
                assert_equal(defaults[index], expected[index])
    reader.finish()
    print(count)
