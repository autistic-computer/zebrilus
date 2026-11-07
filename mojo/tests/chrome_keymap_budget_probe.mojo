from std.testing import assert_equal, assert_true, assert_false, assert_raises
from zzmojo.chrome_keymap import (
    ChromeKeymap, MAX_CHROME_BINDINGS, MAX_CHROME_TABLES,
    MAX_CHROME_RETAINED_BYTES,
)
from zzmojo.chrome_actions import ChromeActions, ChromeAction, MAX_CHROME_STRING_BYTES
from zzmojo.input_protocol import KeyInput


def empty(scope: UInt32) raises -> ChromeKeymap:
    var map = ChromeKeymap(0, scope)
    for value in map.bindings():
        assert_true(map.unbind(value.table, value.key))
    return map^


def main() raises:
    var map = empty(10)
    for index in range(MAX_CHROME_BINDINGS):
        assert_false(Bool(map.bind("custom", String("key-", index), "detach")))
    var before = map.version()
    var records = map.bindings()
    with assert_raises():
        _ = map.bind("custom", "extra", "detach")
    assert_equal(map.version(), before)
    assert_equal(len(map.bindings()), len(records))
    for value in records:
        var found = map.action_for(value.table, value.key)
        assert_true(Bool(found))
        assert_true(found.value() == value.action)
    assert_false(Bool(map.bind("custom", "key-0", "detach")))
    assert_equal(map.version(), before)
    assert_false(Bool(map.bind("custom", "key-0", "new-session")))
    assert_equal(map.version(), before + 1)
    assert_equal(len(map.bindings()), MAX_CHROME_BINDINGS)
    var tables = empty(11)
    for index in range(MAX_CHROME_TABLES):
        assert_false(Bool(tables.bind(String("table-", index), "x", "detach")))
    before = tables.version()
    with assert_raises():
        _ = tables.bind("extra", "x", "detach")
    assert_equal(tables.version(), before)
    assert_false(tables.binds("extra"))
    assert_true(tables.unbind("table-0", "x"))
    assert_false(tables.binds("table-0"))
    assert_false(Bool(tables.bind("extra", "x", "detach")))
    var bytes = empty(12)
    var key = List[UInt8](length=65535, fill=120)
    for index in range(16):
        key[0] = UInt8(97 + index)
        assert_false(Bool(bytes.bind("t", String(from_utf8=Span(key)), "detach")))
    var actual_bytes = 0
    for value in bytes.bindings():
        actual_bytes += value.table.byte_length() + value.key.byte_length()
    assert_equal(actual_bytes, MAX_CHROME_RETAINED_BYTES)
    before = bytes.version()
    with assert_raises():
        _ = bytes.bind("t", "overflow", "detach")
    assert_equal(bytes.version(), before)
    assert_equal(len(bytes.bindings()), 16)
    var raw = List[UInt8](length=MAX_CHROME_STRING_BYTES + 1, fill=97)
    var oversized = String(from_utf8=Span(raw))
    with assert_raises():
        _ = bytes.bind("t", oversized, "detach")
    assert_equal(bytes.version(), before)
    var bad = KeyInput()
    bad.text = oversized.copy()
    with assert_raises():
        _ = bytes.lookups(bad)
    assert_equal(bytes.version(), before)
    var catalog = ChromeActions()
    var first = ChromeAction(2, 9)
    var second = ChromeAction(2, 255)
    assert_equal(catalog.name(first), catalog.name(second))
    var arena = bytes.action_arena()
    var one = bytes.action_reference(first)
    var two = bytes.action_reference(second)
    assert_true(one != two)
    var first_body = arena.body(one)
    var second_body = arena.body(two)
    assert_equal(first_body[0], second_body[0])
    assert_true(first_body[1] != second_body[1])
    assert_equal(bytes.action_from_reference(one).index, UInt8(9))
    assert_equal(bytes.action_from_reference(two).index, UInt8(255))
    print("chrome mutation refusal, exact limits, stable versions and distinct action identities passed")
