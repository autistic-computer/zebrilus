from std.sys import argv
from std.testing import assert_equal, assert_raises
from zzmojo.protocol import Decoder, validate_envelope
from zzmojo.tree_wire import read_text, read_bool
from zzmojo.prefix_bindings import read_prefix_tables, PrefixTable, PrefixBinding, MAX_PREFIX_BINDINGS
from zzmojo.exec_protocol import Invocation
from zzmojo.which_key import WhichKeyModel, WhichKeyRow, which_key_group_title


def check(mut source: Decoder, rows: List[WhichKeyRow], mut totals: List[Int]) raises:
    assert_equal(len(rows), Int(source.uint()))
    totals[0] += len(rows)
    for row in rows:
        assert_equal(len(row.keys), Int(source.uint()))
        totals[1] += len(row.keys)
        for set in row.keys:
            assert_equal(len(set.keys), Int(source.uint()))
            totals[2] += len(set.keys)
            for key in set.keys:
                assert_equal(key, read_text(source))
            assert_equal(set.yours, read_bool(source))
        assert_equal(row.label, read_text(source))
        assert_equal(UInt64(row.group), source.uint())
        assert_equal(row.repeat, read_bool(source))
        assert_equal(row.core, read_bool(source))
        assert_equal(row.first_key(), read_text(source))
        assert_equal(row.yours(), read_bool(source))


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(String(args[1]), "r") as file:
        bytes = file.read_bytes()
    if len(bytes) > 64 * 1024 * 1024:
        raise Error("which-key reference exceeds fixture limit")
    var source = Decoder(bytes^)
    source.offset = 0
    var model = WhichKeyModel()
    var count = Int(source.uint())
    var totals: List[Int] = [0, 0, 0, 0]
    for index in range(count):
        var frame = source.bytes()
        _ = validate_envelope(Span(frame))
        var reader = Decoder(frame^)
        assert_equal(reader.uint(), UInt64(10))
        assert_equal(reader.uint(), UInt64(0))
        assert_equal(reader.uint(), UInt64(25))
        var tables = read_prefix_tables(reader)
        reader.finish()
        var table = read_text(source)
        var prefix = read_text(source)
        try:
            check(source, model.rows(tables, table, prefix), totals)
            check(source, model.rows(tables, table, prefix, False), totals)
        except error:
            print("which-key case", index, "table", table, "prefix", prefix)
            raise error
        var bindings = -1
        for item in range(len(tables)):
            if tables[item].name == table:
                bindings = item
                break
        var amount = Int(source.uint())
        assert_equal(amount, len(tables[bindings].bindings) if bindings >= 0 else 0)
        for item in range(amount):
            assert_equal(model.opens_all_keys(tables[bindings].bindings[item].commands), read_bool(source))
            totals[3] += 1
    source.finish()
    var names: List[String] = ["Panes", "Windows", "Sessions", "Copy and paste", "Other", "Yours"]
    for index in range(6):
        assert_equal(which_key_group_title(UInt8(index + 1)), names[index])
    with assert_raises():
        _ = which_key_group_title(0)
    var bindings = List[PrefixBinding]()
    for _ in range(MAX_PREFIX_BINDINGS + 1):
        bindings.append(PrefixBinding("x", List[Invocation](), False, None))
    var tables: List[PrefixTable] = [PrefixTable("prefix", bindings^)]
    with assert_raises():
        _ = model.rows(tables, "prefix", "C-b")
    print(count, totals[0], totals[1], totals[2], totals[3])
