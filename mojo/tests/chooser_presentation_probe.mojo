from std.sys import argv
from std.testing import assert_equal, assert_true, assert_false
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_count, read_text
from zzmojo.chooser_protocol import ChooseItem, ChooseState, ChooseSearch, decode_choose_event
from zzmojo.palette_dialog_state import ChooseSnapshot
from zzmojo.chooser_presentation import ChooserPresentation, chooser_row, chooser_modal_height, chooser_key_is_text, CHOOSER_DISPLAY_BYTES


def bounds() raises:
    var item = ChooseItem()
    item.label = String(from_utf8=List[UInt8](length=CHOOSER_DISPLAY_BYTES, fill=97))
    item.detail = "detail"
    var exact = chooser_row(item)
    assert_equal(exact.label.byte_length(), CHOOSER_DISPLAY_BYTES)
    item.label += "x"
    var refused = False
    try:
        _ = chooser_row(item)
    except:
        refused = True
    assert_true(refused)
    assert_equal(item.label.byte_length(), CHOOSER_DISPLAY_BYTES + 1)
    for count in [-1, 65537]:
        refused = False
        try:
            _ = chooser_modal_height(count, False, False)
        except:
            refused = True
        assert_true(refused)
    var state = ChooseState()
    state.search = ChooseSearch("", True)
    var model = ChooserPresentation(ChooseSnapshot(state^))
    var marked = String(from_utf8=List[UInt8](length=CHOOSER_DISPLAY_BYTES, fill=97))
    var accepted = model.search(marked)
    assert_true(accepted)
    assert_equal(accepted.value()[0], "?")
    assert_equal(accepted.value()[1].byte_length(), CHOOSER_DISPLAY_BYTES)
    marked += "x"
    refused = False
    try:
        _ = model.search(marked)
    except:
        refused = True
    assert_true(refused)
    assert_equal(model.snapshot.search.value().query, "")
    refused = False
    try:
        _ = model.row(0)
    except:
        refused = True
    assert_true(refused)
    assert_false(chooser_key_is_text(True, True, 2))


def main() raises:
    bounds()
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var input = Decoder(data^)
    input.offset = 0
    var output = List[UInt8]()
    var count = read_count(input, 4096)
    for _ in range(count):
        var event = decode_choose_event(input.bytes())
        if not event or not event.value().state:
            raise Error("chooser presentation fixture has no state")
        var state = event.take()
        var snapshot = ChooseSnapshot(state.state.take())
        var address = Int(snapshot.items[].values.unsafe_ptr())
        var model = ChooserPresentation(snapshot.copy())
        assert_equal(Int(model.snapshot.items[].values.unsafe_ptr()), address)
        append_text(output, model.title)
        append_text(output, model.subtitle)
        var height = model.height
        append_uint(output, UInt64(Pointer(to=height).unsafe_bitcast[UInt32]()[]))
        output.append(UInt8(model.show_key_gutter))
        append_uint(output, UInt64(model.snapshot.selected))
        append_uint(output, UInt64(len(model.snapshot.items[].values)))
        for index in range(len(model.snapshot.items[].values)):
            var row = model.row(index)
            append_text(output, row.key)
            append_text(output, row.label)
            append_text(output, row.detail)
            append_text(output, row.target)
            output.append(row.depth)
            output.append(row.disclosure)
            output.append(UInt8(row.active))
            output.append(UInt8(row.tagged))
            output.append(UInt8(Bool(row.pane_kind)))
            if row.pane_kind:
                output.append(row.pane_kind.value())
        var marked = read_text(input, CHOOSER_DISPLAY_BYTES)
        var search = model.search(marked)
        output.append(UInt8(Bool(search)))
        if search:
            append_text(output, search.value()[0])
            append_text(output, search.value()[1])
    var policies = read_count(input, 64)
    for _ in range(policies):
        var flags = input.byte()
        output.append(UInt8(chooser_key_is_text(flags & 1 != 0, flags & 2 != 0, flags >> 2)))
    input.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
    print("chooser rows, retained snapshots, modal sizes, composition and key routing exact")
