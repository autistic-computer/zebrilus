from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_count
from zzmojo.prompt_protocol import decode_prompt_event
from zzmojo.chooser_protocol import ChooseState, ChooseItem, ChooseEvent, ChooseSearch, decode_choose_event
from zzmojo.palette_dialog_state import PaletteDialogs
from zzmojo.state import AppModel
from zzmojo.stream_transport import StreamModel
from zzmojo.stream_transport import InteractiveConnection
from zzmojo.transport import Connection
from zzmojo.bend_gate import BendGate
from zzmojo.client_files import ClientFiles


def summary(mut output: List[UInt8], dialogs: PaletteDialogs):
    append_uint(output, dialogs.prompt_revision)
    append_uint(output, dialogs.chooser_revision)
    output.append(UInt8(Bool(dialogs.prompt)))
    if dialogs.prompt:
        append_text(output, dialogs.prompt.value()[].input)
    output.append(UInt8(Bool(dialogs.chooser)))
    if dialogs.chooser:
        ref state = dialogs.chooser.value()
        append_uint(output, UInt64(len(state.items[].values)))
        append_uint(output, UInt64(state.selected))
        output.append(UInt8(Bool(state.search)))
        if state.search:
            append_text(output, state.search.value().query)
            output.append(UInt8(state.search.value().reverse))


def retained_rows() raises:
    var dialogs = PaletteDialogs()
    var event = ChooseEvent()
    var state = ChooseState()
    for _ in range(65536):
        state.items.append(ChooseItem())
    event.state = state^
    dialogs.apply_chooser(event^)
    var address = Int(dialogs.chooser.value().items[].values.unsafe_ptr())
    for index in range(10000):
        var snapshot = dialogs.copy()
        assert_equal(Int(snapshot.chooser.value().items[].values.unsafe_ptr()), address)
        var update = ChooseEvent()
        update.update = True
        update.selected = UInt32(index)
        update.search = ChooseSearch("query", False)
        dialogs.apply_chooser(update^)
        assert_equal(Int(dialogs.chooser.value().items[].values.unsafe_ptr()), address)
        assert_equal(snapshot.chooser.value().selected, UInt32(max(index - 1, 0)))
    var model = AppModel()
    model.dialogs = dialogs.copy()
    var stream = StreamModel()
    stream.dialogs = dialogs.copy()
    assert_equal(Int(model.dialogs.chooser.value().items[].values.unsafe_ptr()), address)
    assert_equal(Int(stream.dialogs.chooser.value().items[].values.unsafe_ptr()), address)


def main() raises:
    retained_rows()
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = read_count(reader, 100000)
    var dialogs = PaletteDialogs()
    var transport = Connection(-1, 1, 0, 0, BendGate(args[3]), ClientFiles())
    var connection = InteractiveConnection(transport^)
    var model = AppModel()
    var output = List[UInt8]()
    for _ in range(count):
        var frame = reader.bytes()
        connection.consume(frame.copy())
        model.refresh(connection)
        var prompt = decode_prompt_event(frame.copy())
        if prompt:
            dialogs.apply_prompt(prompt.take())
        else:
            var chooser = decode_choose_event(frame^)
            assert_true(Bool(chooser))
            dialogs.apply_chooser(chooser.take())
        summary(output, dialogs)
        summary(output, connection.model.dialogs)
        summary(output, model.dialogs)
        assert_equal(len(connection.model.unhandled), 0)
        if dialogs.chooser:
            assert_equal(Int(model.dialogs.chooser.value().items[].values.unsafe_ptr()), Int(connection.model.dialogs.chooser.value().items[].values.unsafe_ptr()))
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_all(Span(output))
