from std.testing import assert_equal, assert_false, assert_true, TestSuite
from zzmojo.terminal_view import (
    PointerGrid,
    accumulate_scroll,
    cursor_should_blink,
    dead_label,
    openable_uri,
    presented_uri,
    scroll_fraction,
    scrollbar_strip_hit,
    search_result_text,
    selection_autoscroll_lines,
)


def test_pointer_and_scroll_contract() raises:
    var grid = PointerGrid(10, 20, 80, 32, 8, 16, 0, 2, 10, 2)
    var cell = grid.project(27, 45, 8, True)
    assert_equal((cell.column, cell.row, cell.click_count), (2, 1, 3))
    assert_equal(
        (cell.x, cell.y, cell.screen_width, cell.screen_height),
        (34, 50, 160, 64),
    )
    assert_equal(
        (cell.cell_width, cell.cell_height, cell.rectangle), (16, 32, True)
    )
    var edge = grid.project(900, -200, 1, False)
    assert_equal((edge.column, edge.row, edge.x, edge.y), (9, 0, 160, 0))
    assert_true(grid.contains(90, 52))
    assert_false(grid.contains(90.01, 52))
    assert_equal(selection_autoscroll_lines(20, 52, 16, 3), -2)
    assert_equal(selection_autoscroll_lines(20, 52, 16, 1000), 8)
    assert_equal(selection_autoscroll_lines(20, 52, 16, 52), 0)
    assert_equal(scroll_fraction(20, 100, 70), UInt32(2147483648))
    assert_equal(scroll_fraction(20, 100, 120), UInt32(4294967295))
    assert_true(scrollbar_strip_hit(90, 20, 52, 10, 100, 20, 80, 52))
    assert_false(scrollbar_strip_hit(90, 20, 52, 10, 20, 20, 80, 52))
    var remainder = Float32(0)
    assert_equal(accumulate_scroll(remainder, 0.75), 0)
    assert_equal(accumulate_scroll(remainder, 0.75), 1)
    assert_equal(remainder, Float32(0.5))
    assert_equal(accumulate_scroll(remainder, -2.25), -1)
    assert_equal(remainder, Float32(-0.75))


def test_cursor_and_unicode_presentations() raises:
    var visible = UInt64(1) << 58
    var blinking = visible | (UInt64(1) << 59)
    assert_false(cursor_should_blink(None, 1, True))
    assert_false(cursor_should_blink(blinking, 0, True))
    assert_true(cursor_should_blink(visible, 1, True))
    assert_false(cursor_should_blink(visible, 2, True))
    assert_true(cursor_should_blink(blinking, 2, True))
    assert_false(cursor_should_blink(blinking, 2, False))
    var label = String()
    for _ in range(240):
        label += "é"
    assert_equal(presented_uri(label), label)
    assert_equal(presented_uri(label + "x"), label + "…")
    assert_true(openable_uri("https://example.com"))
    assert_false(openable_uri("file:///etc/passwd"))
    assert_equal(search_result_text(2, 9, False, False), "2 / 9")
    assert_equal(search_result_text(2, 9, True, True), "Invalid pattern")
    assert_equal(dead_label(True, UInt32(137)).or_else(""), "Dead · 137")
    assert_false(Bool(dead_label(False, UInt32(137))))


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
