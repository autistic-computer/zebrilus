from std.sys import argv
from std.testing import assert_equal
from mjui.term.colors import (
    faint_foreground,
    resolved_foreground,
    ensure_minimum_contrast,
    is_decorative_character,
)


def next_u32(data: List[UInt8], mut offset: Int) raises -> UInt32:
    if len(data) - offset < 4:
        raise Error("truncated terminal color oracle")
    var value = UInt32(0)
    for shift in range(4):
        value |= UInt32(data[offset + shift]) << UInt32(shift * 8)
    offset += 4
    return value


def main() raises:
    var args = argv()
    if len(args) != 2:
        raise Error("usage: terminal-color-probe ORACLE")
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var offset = 0
    assert_equal(next_u32(data, offset), UInt32(0x524C435A))
    var faint_count = next_u32(data, offset)
    var resolved_count = next_u32(data, offset)
    var decorative_count = next_u32(data, offset)
    for index in range(Int(faint_count)):
        var foreground = next_u32(data, offset)
        var background = next_u32(data, offset)
        var attributes = UInt16(next_u32(data, offset))
        var expected = next_u32(data, offset)
        var actual = faint_foreground(foreground, background, attributes)
        if actual != expected:
            print(
                "faint mismatch",
                index,
                foreground,
                background,
                actual,
                expected,
            )
        assert_equal(actual, expected)
    for index in range(Int(resolved_count)):
        var foreground = next_u32(data, offset)
        var background = next_u32(data, offset)
        var attributes = UInt16(next_u32(data, offset))
        var minimum_bits = next_u32(data, offset)
        var minimum = Pointer(to=minimum_bits).unsafe_bitcast[Float32]()[
            unsafe_offset=0
        ]
        var decorative = next_u32(data, offset) != 0
        var expected = next_u32(data, offset)
        var expected_contrast = next_u32(data, offset)
        var expected_faint = next_u32(data, offset)
        var actual = resolved_foreground(
            foreground, background, attributes, minimum, decorative
        )
        var actual_contrast = ensure_minimum_contrast(
            foreground, background, minimum
        )
        if actual != expected or actual_contrast != expected_contrast:
            print(
                "resolved mismatch",
                index,
                foreground,
                background,
                attributes,
                minimum,
                decorative,
                actual,
                expected,
                actual_contrast,
                expected_contrast,
            )
        assert_equal(actual, expected)
        assert_equal(actual_contrast, expected_contrast)
        assert_equal(
            faint_foreground(foreground, background, attributes), expected_faint
        )
    for _ in range(Int(decorative_count)):
        var character = next_u32(data, offset)
        var expected = next_u32(data, offset) != 0
        assert_equal(is_decorative_character(character), expected)
    assert_equal(offset, len(data))
    print(
        "Terminal colors matched Rust exactly:",
        faint_count,
        "faint pairs,",
        resolved_count,
        "resolutions,",
        decorative_count,
        "Unicode scalars",
    )
