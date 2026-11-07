from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.protocol import Decoder, append_uint, envelope
from zzmojo.tree_wire import read_optional_text, read_bool
from zzmojo.desktop_key_tables import DesktopKeyTables, RetainedKeyTables, encode_key_tables, get_key_tables
from zzmojo.key_table_hash import key_table_hash


def same(actual: List[UInt8], expected: List[UInt8]) raises:
    assert_equal(len(actual), len(expected))
    for index in range(len(actual)):
        assert_equal(actual[index], expected[index])


def main() raises:
    var arguments = argv()
    var data: List[UInt8]
    with open(String(arguments[1]), "r") as source:
        data = source.read_bytes()
    if len(data) > 64 * 1024 * 1024:
        raise Error("key table source fixture exceeds byte limit")
    var reader = Decoder(data^)
    reader.offset = 0
    same(get_key_tables(), reader.bytes())
    var welcome = reader.bytes()
    var hashes = Int(reader.uint())
    for _ in range(hashes):
        var bytes = reader.bytes()
        assert_equal(key_table_hash(Span(bytes)), reader.uint())
    var tables = DesktopKeyTables()
    var count = Int(reader.uint())
    var first = List[UInt8]()
    for index in range(count):
        var frame = reader.bytes()
        if index == 0:
            first = frame.copy()
        assert_true(tables.apply(frame^))
        var full: List[UInt8] = [10, 0, 25]
        full.extend(encode_key_tables(tables.tables))
        same(envelope(Span(full)), reader.bytes())
        var hash: List[UInt8] = [10, 0, 59]
        append_uint(hash, tables.hash)
        for words in [tables.root_mouse.copy(), tables.copy_mouse.copy()]:
            append_uint(hash, UInt64(len(words)))
            for word in words:
                append_uint(hash, UInt64(word[0]))
                append_uint(hash, word[1])
        same(envelope(Span(hash)), reader.bytes())
        var expected = read_optional_text(reader)
        assert_equal(tables.prefix.active_table, expected)
        assert_equal(tables.prefix.repeat, read_bool(reader) if expected else False)
    assert_equal(reader.offset, len(reader.data))
    var retained = RetainedKeyTables()
    retained.subscribed = True
    assert_true(retained.needs_full())
    assert_true(retained.consume(first.copy()))
    assert_true(retained.eligible)
    var shared = retained.copy()
    var hash = retained.snapshot.value()[].hash
    var invalid: List[UInt8] = [10, 0, 25, 129, 2]
    assert_true(retained.consume(envelope(Span(invalid))))
    assert_equal(retained.eligible, False)
    assert_true(retained.needs_full())
    assert_true(shared.eligible)
    assert_equal(shared.snapshot.value()[].hash, hash)
    assert_equal(retained.snapshot.value()[].hash, hash)
    retained.requested()
    assert_equal(retained.needs_full(), False)
    var patch: List[UInt8] = [10, 0, 56, 0, 0]
    assert_true(retained.consume(envelope(Span(patch))))
    assert_equal(retained.eligible, False)
    assert_equal(retained.needs_full(), False)
    assert_true(retained.consume(first.copy()))
    assert_true(retained.eligible)
    assert_equal(retained.pending, False)
    var mismatch: List[UInt8] = [10, 0, 59]
    append_uint(mismatch, hash ^ 1)
    mismatch.extend([0, 0])
    assert_true(retained.consume(envelope(Span(mismatch))))
    assert_equal(retained.eligible, False)
    assert_equal(retained.snapshot.value()[].hash, hash ^ 1)
    assert_equal(shared.snapshot.value()[].hash, hash)
    assert_true(retained.consume(welcome^))
    assert_equal(retained.eligible, False)
    assert_equal(retained.snapshot.value()[].hash, UInt64(0))
    assert_equal(len(retained.snapshot.value()[].tables), 0)
    assert_equal(Bool(retained.snapshot.value()[].prefix.active_table), False)
    assert_true(retained.needs_full())
    print(hashes, count)
