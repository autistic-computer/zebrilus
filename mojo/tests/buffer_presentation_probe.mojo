from std.sys import argv
from std.testing import assert_equal, assert_raises
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text, read_bool
from zzmojo.chooser_protocol import ChooseSearch
from zzmojo.chooser_buffer_protocol import ChooseBufferItem, ChooseBufferState, decode_choose_buffer
from zzmojo.palette_dialog_state import ChooseBufferSnapshot
from zzmojo.buffer_presentation import BufferPresentation, buffer_row, buffer_size, buffer_age


def bounds() raises:
    var item = ChooseBufferItem()
    item.name = String(from_utf8=List[UInt8](length=65536, fill=97))
    assert_equal(buffer_row(item, 0).name.byte_length(), 65536)
    item.name += "x"
    with assert_raises():
        _ = buffer_row(item, 0)
    item.preview = item.name.copy()
    item.text = "Custom"
    var row = buffer_row(item, UInt64.MAX)
    assert_equal(row.name, "Custom")
    assert_equal(row.preview, "")
    assert_equal(row.size, "")
    assert_equal(row.age, "")
    var state = ChooseBufferState()
    state.search = ChooseSearch("q", False)
    var model = BufferPresentation(ChooseBufferSnapshot(state^))
    var marked = String(from_utf8=List[UInt8](length=65535, fill=97))
    assert_equal(model.search(marked).value()[1].byte_length(), 65536)
    marked += "x"
    with assert_raises():
        _ = model.search(marked)
    assert_equal(model.snapshot.search.value().query, "q")
    with assert_raises():
        _ = model.row(0, 0)


def main() raises:
    bounds()
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    if len(bytes) > 32 * 1024 * 1024:
        raise Error("buffer presentation fixture exceeds limit")
    var source = Decoder(bytes^)
    source.offset = 0
    var numeric = Int(source.uint())
    for _ in range(numeric):
        var size = source.uint()
        var created = source.uint()
        var now = source.uint()
        assert_equal(buffer_size(size), read_text(source))
        assert_equal(buffer_age(created, now), read_text(source))
    var count = Int(source.uint())
    var rows = 0
    for _ in range(count):
        var event = decode_choose_buffer(source.bytes())
        if not event or not event.value().state:
            raise Error("buffer presentation fixture has no state")
        var snapshot = ChooseBufferSnapshot(event.value().state.take())
        var address = Int(snapshot.items[].values.unsafe_ptr())
        var model = BufferPresentation(snapshot.copy())
        assert_equal(Int(model.snapshot.items[].values.unsafe_ptr()), address)
        var now = source.uint()
        var marked = read_text(source)
        assert_equal(model.title, read_text(source))
        assert_equal(model.subtitle, read_text(source))
        var height = model.height
        assert_equal(UInt64(Pointer(to=height).unsafe_bitcast[UInt32]()[]), source.uint())
        assert_equal(model.show_key_gutter, read_bool(source))
        assert_equal(UInt64(model.snapshot.selected), source.uint())
        var amount = Int(source.uint())
        assert_equal(len(model.snapshot.items[].values), amount)
        for index in range(amount):
            var row = model.row(index, now)
            assert_equal(row.key, read_text(source))
            assert_equal(row.name, read_text(source))
            assert_equal(row.preview, read_text(source))
            assert_equal(row.size, read_text(source))
            assert_equal(row.age, read_text(source))
            assert_equal(row.tagged, read_bool(source))
            rows += 1
        var search = model.search(marked)
        assert_equal(Bool(search), read_bool(source))
        if search:
            assert_equal(search.value()[0], read_text(source))
            assert_equal(search.value()[1], read_text(source))
    source.finish()
    print(numeric, count, rows, len(source.data))
