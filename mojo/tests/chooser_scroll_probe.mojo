from std.sys import argv
from std.memory import Pointer
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.chooser_runtime import chooser_center_scroll, chooser_wheel_scroll


def scalar(mut reader: Decoder) raises -> Float32:
    var bits = UInt32(reader.uint())
    return Pointer(to=bits).unsafe_bitcast[Float32]()[unsafe_offset=0]


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(String(args[1]), "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = Int(reader.uint())
    for index in range(count):
        var kind = reader.byte()
        var scroll = scalar(reader)
        var actual: Float32
        if kind == 0:
            var row = scalar(reader)
            var viewport = scalar(reader)
            var rows = UInt32(reader.uint())
            var selected = UInt32(reader.uint())
            actual = chooser_center_scroll(
                scroll, row, viewport, rows, selected
            )
        else:
            var maximum = scalar(reader)
            var line = scalar(reader)
            var x = scalar(reader)
            var y = scalar(reader)
            var precise = reader.byte() != 0
            actual = chooser_wheel_scroll(scroll, maximum, line, x, y, precise)
        var expected = UInt32(reader.uint())
        var bits = Pointer(to=actual).unsafe_bitcast[UInt32]()[unsafe_offset=0]
        if bits != expected:
            print(
                "chooser scroll case",
                index,
                "kind",
                kind,
                "actual",
                bits,
                "expected",
                expected,
            )
        assert_equal(bits, expected)
    reader.finish()
    print(count)
