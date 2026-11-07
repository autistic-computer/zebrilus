from std.sys import argv
from std.testing import assert_equal
from mjui.term.appearance import cursor_visible
from mjui.term.native_paint import native_cursor
from mjui.term.grid import GridOverlay, TermOpts
from mjui.term.overlays import SelectionCoverage, overlay_color, copy_cursor_line_color, OV_CORNER_TL, OV_CORNER_TR, OV_CORNER_BR, OV_CORNER_BL


def next_u32(data: List[UInt8], mut offset: Int) raises -> UInt32:
    if len(data) - offset < 4:
        raise Error("truncated terminal overlay oracle")
    var value: UInt32 = 0
    for index in range(4):
        value |= UInt32(data[offset + index]) << UInt32(index * 8)
    offset += 4
    return value


def next_float(data: List[UInt8], mut offset: Int) raises -> Float32:
    var bits = next_u32(data, offset)
    return Pointer(to=bits).unsafe_bitcast[Float32]()[]


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var offset = 0
    assert_equal(next_u32(data, offset), UInt32(0x4F565A5A))
    var cases = Int(next_u32(data, offset))
    var colors = Int(next_u32(data, offset))
    var cursors = Int(next_u32(data, offset))
    var cursor_paints = Int(next_u32(data, offset))
    var masks: List[UInt16] = [OV_CORNER_TL, OV_CORNER_TR, OV_CORNER_BR, OV_CORNER_BL]
    for _ in range(cases):
        var count = Int(next_u32(data, offset))
        var spans = List[GridOverlay]()
        for _ in range(count):
            var row = UInt16(next_u32(data, offset))
            var start = UInt16(next_u32(data, offset))
            var end = UInt16(next_u32(data, offset))
            var kind = UInt16(next_u32(data, offset))
            spans.append(GridOverlay(row, start, end, kind))
        var coverage = SelectionCoverage(Span(spans))
        for span in spans:
            var corners = coverage.corners(span)
            for mask in masks:
                var radius: Float32 = 3.125 if corners & mask != 0 else 0
                assert_equal(radius.to_bits[DType.uint32](), next_u32(data, offset))
    for _ in range(colors):
        var packed = next_u32(data, offset)
        var kind = UInt16(next_u32(data, offset))
        var opts = TermOpts(0, 13, selection=packed, match_bg=packed, match_current=packed, link=(packed & 0xFFFFFF00) | 255, copy_cursor=packed)
        var color = copy_cursor_line_color(opts) if kind == 5 else overlay_color(kind, opts)
        var values = SIMD[.float32, 4](color.h, color.s, color.l, color.ha)
        for channel in range(4):
            assert_equal(values[channel].to_bits[DType.uint32](), next_u32(data, offset))
    for _ in range(cursors):
        var flags = next_u32(data, offset)
        assert_equal(cursor_visible(flags & 1 != 0, flags & 2 != 0, flags & 16 != 0, flags & 8 != 0, flags & 32 != 0), next_u32(data, offset) != 0)
    for _ in range(cursor_paints):
        var x = next_float(data, offset)
        var y = next_float(data, offset)
        var w = next_float(data, offset)
        var h = next_float(data, offset)
        var style = UInt8(next_u32(data, offset))
        var packed = next_u32(data, offset)
        var focused = next_u32(data, offset) != 0
        var q = native_cursor(x, y, w, h, style, packed, focused)
        var bounds = SIMD[.float32, 8](q.x, q.y, q.w, q.h, q.stroke, 0, 0, 0)
        for index in range(5):
            assert_equal(bounds[index].to_bits[DType.uint32](), next_u32(data, offset))
        for index in range(2):
            var color = q.fill if index == 0 else q.edge
            var hsla = SIMD[.float32, 4](color.h, color.s, color.l, color.ha)
            for channel in range(4):
                assert_equal(hsla[channel].to_bits[DType.uint32](), next_u32(data, offset))
    assert_equal(offset, len(data))
    print("native terminal overlay helpers match", cases, colors, cursors, cursor_paints)
