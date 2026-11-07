from std.sys import argv
from std.testing import assert_equal, assert_true, assert_false
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text, read_optional_text, read_bool
from zzmojo.chooser_runtime import ChooserInput
from zzmojo.chooser_presentation import CHOOSER_DISPLAY_BYTES


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(String(args[1]), "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var traces = Int(reader.uint())
    var steps = 0
    for _ in range(traces):
        var input = ChooserInput()
        var count = Int(reader.uint())
        for _ in range(count):
            var kind = reader.byte()
            var changed = False
            var selected = False
            var commit: Optional[String] = None
            var unmark = False
            if kind == 0:
                var revision = reader.uint()
                var index = UInt32(reader.uint())
                var search = read_bool(reader)
                var update = input.synchronize(revision, index, search)
                changed = update.changed
                selected = update.selection_changed
            elif kind == 1:
                input.mark_text(read_text(reader))
            elif kind == 2:
                commit = input.replace_text(read_text(reader))
            elif kind == 3:
                unmark = input.unmark()
            else:
                raise Error("unknown chooser reference operation")
            assert_equal(input.revision, reader.uint())
            var expected_selected: Optional[UInt32] = None
            if read_bool(reader):
                expected_selected = UInt32(reader.uint())
            assert_equal(input.selected, expected_selected)
            assert_equal(input.search_active, read_bool(reader))
            assert_equal(input.marked, read_optional_text(reader))
            var units: Optional[Int] = None
            if read_bool(reader):
                units = Int(reader.uint())
            assert_equal(input.marked_utf16_length(), units)
            var keys = UInt32(0)
            for character in range(2):
                for modifiers in range(16):
                    if input.key_is_text(character != 0, UInt8(modifiers)):
                        keys |= UInt32(1) << UInt32(character * 16 + modifiers)
            assert_equal(keys, UInt32(reader.uint()))
            if kind == 0:
                assert_equal(changed, read_bool(reader))
                assert_equal(selected, read_bool(reader))
            elif kind == 2:
                assert_equal(commit, read_optional_text(reader))
            elif kind == 3:
                assert_equal(unmark, read_bool(reader))
            steps += 1
    reader.finish()
    var input = ChooserInput()
    _ = input.synchronize(1, 0, True)
    input.mark_text("old😀")
    var previous = input.copy()
    var refused = False
    try:
        input.mark_text(String("x") * (CHOOSER_DISPLAY_BYTES + 1))
    except:
        refused = True
    assert_true(refused)
    assert_equal(input.marked, previous.marked)
    refused = False
    try:
        _ = input.replace_text("x", CHOOSER_DISPLAY_BYTES)
    except:
        refused = True
    assert_true(refused)
    assert_equal(input.marked, previous.marked)
    input.mark_text("😀", CHOOSER_DISPLAY_BYTES - 4)
    assert_equal(input.marked, Optional[String]("😀"))
    assert_equal(input.marked_utf16_length(), Optional[Int](2))
    _ = input.synchronize(2, 0, False)
    assert_false(Bool(input.marked))
    _ = input.replace_text(String("x") * (CHOOSER_DISPLAY_BYTES + 1))
    assert_false(Bool(input.marked))
    print(traces, steps)
