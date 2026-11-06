from std.testing import TestSuite, assert_equal, assert_true
from std.ffi import external_call
from zzmojo.client_context import (
    ClientContext,
    normalize_environment,
    MAX_CONTEXT_ENTRY,
)


def test_raw_environment_order_duplicates_and_limits() raises:
    var entries: List[List[UInt8]] = [
        [90, 61, 49],
        [65, 61, 49],
        [65, 61, 50],
        [66, 61],
        [61, 120],
        [67, 61, 255],
        [68, 61, 61],
        [69],
        [70, 61, 49],
        [70, 61, 0],
    ]
    var oversized = List[UInt8](length=MAX_CONTEXT_ENTRY + 1, fill=120)
    oversized[0] = 90
    oversized[1] = 61
    entries.append(oversized^)
    var result = normalize_environment(entries)
    var expected: List[List[UInt8]] = [
        [65, 61, 50],
        [66, 61],
        [67, 61, 255],
        [68, 61, 61],
    ]
    assert_equal(result, expected)
    var context = ClientContext()
    context.environment = result^
    var blob: List[UInt8] = [
        65,
        61,
        50,
        0,
        66,
        61,
        0,
        67,
        61,
        255,
        0,
        68,
        61,
        61,
        0,
    ]
    assert_equal(context.blob(), blob)


def test_capture_preserves_native_non_utf8_environment() raises:
    var name = String("ZZ_PORT_RAW_TEST")
    var raw: List[UInt8] = [255, 0]
    var set = external_call["setenv", Int32](
        name.unsafe_ptr(), raw.unsafe_ptr(), Int32(1)
    )
    _ = raw
    assert_equal(set, Int32(0))
    var context = ClientContext.capture()
    var removed = external_call["unsetenv", Int32](name.unsafe_ptr())
    _ = name
    assert_equal(removed, Int32(0))
    var expected = List[UInt8](String("ZZ_PORT_RAW_TEST=").as_bytes())
    expected.append(255)
    var found = False
    for entry in context.environment:
        if entry == expected:
            found = True
    assert_true(found)


def test_environment_count_and_byte_budgets_skip_later_large_entries() raises:
    var entries = List[List[UInt8]]()
    for index in range(4097):
        var entry = String("n", 10000 + index, "=x")
        entries.append(List[UInt8](entry.as_bytes()))
    var limited = normalize_environment(entries)
    assert_equal(len(limited), 4096)
    assert_equal(limited[0], List[UInt8](String("n10000=x").as_bytes()))
    assert_equal(limited[4095], List[UInt8](String("n14095=x").as_bytes()))
    entries.clear()
    for index in range(257):
        var prefix = String("a", 10000 + index, "=")
        var entry = List[UInt8](prefix.as_bytes())
        entry.resize(MAX_CONTEXT_ENTRY, 120)
        entries.append(entry^)
    var tail = String("z=fit")
    entries.append(List[UInt8](tail.as_bytes()))
    limited = normalize_environment(entries)
    assert_equal(len(limited), 257)
    assert_equal(limited[256], List[UInt8](tail.as_bytes()))


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
