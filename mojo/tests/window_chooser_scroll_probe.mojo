from std.sys import argv
from std.memory import Pointer
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.chooser_runtime import chooser_wheel_scroll
from zzmojo.window_chooser_scroll import window_chooser_scroll


def scalar(mut reader: Decoder) raises -> Float32:
    var bits = UInt32(reader.uint())
    return Pointer(to=bits).unsafe_bitcast[Float32]()[]


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var count = Int(reader.uint())
    for index in range(count):
        var kind = reader.byte()
        var before = scalar(reader)
        var actual: Float32
        if kind == 1:
            var maximum = scalar(reader)
            var line = scalar(reader)
            var x = scalar(reader)
            var y = scalar(reader)
            var precise = reader.byte() != 0
            actual = chooser_wheel_scroll(before, maximum, line, x, y, precise)
        else:
            var row = scalar(reader)
            var viewport = scalar(reader)
            var rows = UInt32(reader.uint())
            var selected = UInt32(reader.uint())
            actual = window_chooser_scroll(
                before, row, viewport, rows, selected, kind == 3
            )
        var expected = UInt32(reader.uint())
        var bits = Pointer(to=actual).unsafe_bitcast[UInt32]()[]
        if bits != expected:
            print(index, kind, before, bits, expected)
        assert_equal(bits, expected)
    reader.finish()
    print(count)
