from std.sys import argv
from std.testing import assert_equal
from mjui.term.appearance import adjust_line_height, cursor_should_blink, native_color
from zzmojo.terminal_appearance import terminal_metrics, _round_positive
from zzmojo.geometry import grid_dimension


def next_u32(data: List[UInt8], mut offset: Int) raises -> UInt32:
    if len(data) - offset < 4:
        raise Error("truncated terminal appearance oracle")
    var value: UInt32 = 0
    for index in range(4):
        value |= UInt32(data[offset + index]) << UInt32(index * 8)
    offset += 4
    return value


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var offset = 0
    assert_equal(next_u32(data, offset), UInt32(0x41505A5A))
    var colors = Int(next_u32(data, offset))
    var heights = Int(next_u32(data, offset))
    var cursors = Int(next_u32(data, offset))
    var metrics_count = Int(next_u32(data, offset))
    for index in range(colors):
        var packed = next_u32(data, offset)
        var color = native_color(packed)
        var bits = Pointer(to=color).unsafe_bitcast[UInt32]()
        for channel in range(8):
            var expected = next_u32(data, offset)
            if bits[unsafe_offset=channel] != expected:
                print("native terminal color mismatch", index, packed, channel, bits[unsafe_offset=channel], expected)
            assert_equal(bits[unsafe_offset=channel], expected)
        assert_equal(color.model, UInt32(2))
    for _ in range(heights):
        var kind = UInt8(next_u32(data, offset))
        var raw_base = next_u32(data, offset)
        var raw_value = next_u32(data, offset)
        var base = Pointer(to=raw_base).unsafe_bitcast[Float32]()[]
        var value = Pointer(to=raw_value).unsafe_bitcast[Float32]()[]
        var actual = adjust_line_height(base, kind, value)
        assert_equal(actual.to_bits[DType.uint32](), next_u32(data, offset))
    for _ in range(cursors):
        var flags = next_u32(data, offset)
        var policy = UInt8(next_u32(data, offset))
        var expected = next_u32(data, offset) != 0
        assert_equal(cursor_should_blink(flags & 1 != 0, flags & 2 != 0, flags & 4 != 0, policy, flags & 8 != 0), expected)
    for _ in range(metrics_count):
        var values = List[Float32]()
        for _ in range(6):
            var bits = next_u32(data, offset)
            values.append(Pointer(to=bits).unsafe_bitcast[Float32]()[])
        var kind = UInt8(next_u32(data, offset))
        var metrics = terminal_metrics(values[0], values[1], values[2], values[3], values[4], kind, values[5])
        assert_equal(metrics.width.to_bits[DType.uint32](), next_u32(data, offset))
        assert_equal(metrics.height.to_bits[DType.uint32](), next_u32(data, offset))
        assert_equal(metrics.baseline.to_bits[DType.uint32](), next_u32(data, offset))
        assert_equal(UInt32(grid_dimension(317.25, metrics.width, values[4])), next_u32(data, offset))
        assert_equal(UInt32(grid_dimension(211.875, metrics.height, values[4])), next_u32(data, offset))
        assert_equal(UInt32(_round_positive(metrics.width * values[4]).clamp(1, 65535)), next_u32(data, offset))
        assert_equal(UInt32(_round_positive(metrics.height * values[4]).clamp(1, 65535)), next_u32(data, offset))
    assert_equal(offset, len(data))
    print("native terminal appearance math matches", colors, heights, cursors, metrics_count)
