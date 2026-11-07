from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.chooser_protocol import (
    decode_choose_event,
    ChooseEvent,
    ChooseState,
    ChooseItem,
    ChooseSearch,
)
from zzmojo.chooser_buffer_protocol import (
    decode_choose_buffer,
    ChooseBufferEvent,
    ChooseBufferState,
    ChooseBufferItem,
)
from zzmojo.chooser_plane_protocol import (
    decode_chooser_plane,
    ChooserPlaneEvent,
    ChooserPlane,
    ChooserPlaneRow,
)
from zzmojo.palette_dialog_state import PaletteDialogs
from zzmojo.stream_protocol import decode_stream_frame, BATCH
from zzmojo.stream_transport import InteractiveConnection, StreamModel
from zzmojo.state import AppModel
from zzmojo.transport import Connection
from zzmojo.bend_gate import BendGate
from zzmojo.client_files import ClientFiles


def buffer_summary(
    mut output: List[UInt8],
    items: List[ChooseBufferItem],
    search: Optional[ChooseSearch],
    selected: UInt32,
    filter_no_matches: Bool,
    help: Bool,
    prompt: String,
):
    append_uint(output, UInt64(selected))
    output.append(UInt8(filter_no_matches))
    output.append(UInt8(help))
    append_text(output, prompt)
    append_uint(output, UInt64(len(items)))
    for item in items:
        append_text(output, item.name)
        append_text(output, item.preview)
        append_uint(output, item.size_bytes)
        append_uint(output, item.created_unix_seconds)
        append_text(output, item.key)
        append_text(output, item.text)
        output.append(UInt8(item.tagged))
    output.append(UInt8(Bool(search)))
    if search:
        append_text(output, search.value().query)
        output.append(UInt8(search.value().reverse))


def buffer_event_summary(event: ChooseBufferEvent) -> List[UInt8]:
    var output = List[UInt8]()
    append_uint(output, event.sequence)
    output.append(UInt8(event.update))
    output.append(UInt8(Bool(event.state)))
    if event.state:
        ref state = event.state.value()
        buffer_summary(
            output,
            state.items,
            state.search,
            state.selected,
            state.filter_no_matches,
            state.help,
            state.prompt,
        )
    output.append(UInt8(Bool(event.search)))
    if event.search:
        append_text(output, event.search.value().query)
        output.append(UInt8(event.search.value().reverse))
    append_uint(output, UInt64(event.selected))
    return output^


def apply(mut dialogs: PaletteDialogs, var frame: List[UInt8]) raises:
    var event = decode_stream_frame(frame.copy())
    if event.kind == BATCH:
        var revision = dialogs.chooser_revision
        var closed = dialogs.chooser_closed_revision
        for item in event.batch:
            apply(dialogs, item.copy())
        dialogs.finish_group(revision, closed)
    elif event.event_kind == Optional[UInt64](
        11
    ) or event.event_kind == Optional[UInt64](12):
        var tree = decode_choose_event(frame^)
        dialogs.apply_chooser(tree.take())
    elif event.event_kind == Optional[UInt64](
        13
    ) or event.event_kind == Optional[UInt64](14):
        var buffer = decode_choose_buffer(frame^)
        dialogs.apply_buffer(buffer.take())
    elif event.event_kind == Optional[UInt64](51):
        var plane = decode_chooser_plane(frame^)
        dialogs.apply_plane(plane.take())
    else:
        raise Error("unexpected chooser state fixture")


def summary(mut output: List[UInt8], dialogs: PaletteDialogs):
    append_uint(output, dialogs.chooser_revision)
    append_uint(output, dialogs.chooser_closed_revision)
    append_uint(output, dialogs.buffer_revision)
    output.append(UInt8(Bool(dialogs.chooser)))
    if dialogs.chooser:
        ref tree = dialogs.chooser.value()
        append_uint(output, UInt64(len(tree.items[].values)))
        append_uint(output, UInt64(tree.selected))
        output.append(UInt8(Bool(tree.search)))
        if tree.search:
            append_text(output, tree.search.value().query)
            output.append(UInt8(tree.search.value().reverse))
    output.append(UInt8(Bool(dialogs.buffer)))
    if dialogs.buffer:
        ref buffer = dialogs.buffer.value()
        append_uint(output, UInt64(buffer.selected))
        output.append(UInt8(buffer.filter_no_matches))
        output.append(UInt8(buffer.help))
        append_text(output, buffer.prompt)
        append_uint(output, UInt64(len(buffer.items[].values)))
        for item in buffer.items[].values:
            append_text(output, item.name)
            append_text(output, item.preview)
            append_uint(output, item.size_bytes)
            append_uint(output, item.created_unix_seconds)
            append_text(output, item.key)
            append_text(output, item.text)
            output.append(UInt8(item.tagged))
        output.append(UInt8(Bool(buffer.search)))
        if buffer.search:
            append_text(output, buffer.search.value().query)
            output.append(UInt8(buffer.search.value().reverse))
    output.append(UInt8(Bool(dialogs.chooser_plane)))
    if dialogs.chooser_plane:
        ref plane = dialogs.chooser_plane.value()[]
        append_uint(output, UInt64(plane.selected))
        append_uint(output, UInt64(len(plane.rows)))
        append_text(output, plane.sort)
        append_text(output, plane.view)
        append_text(output, plane.prompt_style)
        append_text(output, plane.prompt_line)


def retained() raises:
    var dialogs = PaletteDialogs()
    var open = ChooseEvent()
    var state = ChooseState()
    state.items.append(ChooseItem())
    open.state = state^
    dialogs.apply_chooser(open^)
    var plane = ChooserPlane()
    plane.rows.append(ChooserPlaneRow("row", String("x") * (1024 * 1024), True))
    var event = ChooserPlaneEvent()
    event.plane = plane^
    dialogs.apply_plane(event^)
    var buffer = ChooseBufferState()
    for _ in range(65536):
        buffer.items.append(ChooseBufferItem())
    var buffer_event = ChooseBufferEvent()
    buffer_event.state = buffer^
    dialogs.apply_buffer(buffer_event^)
    assert_equal(Bool(dialogs.chooser_plane), False)
    var buffer_address = Int(dialogs.buffer.value().items[].values.unsafe_ptr())
    var plane2 = ChooserPlane()
    plane2.rows.append(
        ChooserPlaneRow("row", String("x") * (1024 * 1024), True)
    )
    var event2 = ChooserPlaneEvent()
    event2.plane = plane2^
    dialogs.apply_plane(event2^)
    var address = Int(dialogs.chooser_plane.value()[].rows.unsafe_ptr())
    var bytes = Int(
        dialogs.chooser_plane.value()[].rows[0].text.as_bytes().unsafe_ptr()
    )
    for index in range(10000):
        var snapshot = dialogs.copy()
        assert_equal(
            Int(snapshot.chooser_plane.value()[].rows.unsafe_ptr()), address
        )
        assert_equal(
            Int(
                snapshot.chooser_plane.value()[]
                .rows[0]
                .text.as_bytes()
                .unsafe_ptr()
            ),
            bytes,
        )
        assert_equal(
            Int(snapshot.buffer.value().items[].values.unsafe_ptr()),
            buffer_address,
        )
        var update = ChooseBufferEvent()
        update.update = True
        update.selected = UInt32(index)
        dialogs.apply_buffer(update^)
    var model = AppModel()
    model.dialogs = dialogs.copy()
    var stream = StreamModel()
    stream.dialogs = dialogs.copy()
    assert_equal(
        Int(model.dialogs.chooser_plane.value()[].rows.unsafe_ptr()), address
    )
    assert_equal(
        Int(
            stream.dialogs.chooser_plane.value()[]
            .rows[0]
            .text.as_bytes()
            .unsafe_ptr()
        ),
        bytes,
    )
    dialogs.chooser_revision = UInt64.MAX
    dialogs.buffer_revision = UInt64.MAX
    dialogs.prompt_revision = UInt64.MAX
    dialogs.reset()
    assert_equal(dialogs.chooser_revision, UInt64(1))
    assert_equal(dialogs.chooser_closed_revision, UInt64(1))
    assert_equal(dialogs.buffer_revision, UInt64(1))
    assert_equal(dialogs.prompt_revision, UInt64(1))


def main() raises:
    retained()
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var codec_count = Int(reader.uint())
    for index in range(codec_count):
        var frame = reader.bytes()
        var expected_status = reader.byte()
        var expected = reader.bytes()
        var actual = List[UInt8]()
        var status: UInt8 = 0
        try:
            var event = decode_choose_buffer(frame^)
            status = 2
            if event:
                status = 1
                actual = buffer_event_summary(event.value())
        except:
            pass
        assert_equal(
            status, expected_status, "buffer codec status " + String(index)
        )
        if status == 1:
            assert_equal(len(actual), len(expected))
            for offset in range(len(actual)):
                assert_equal(
                    actual[offset],
                    expected[offset],
                    "buffer codec metadata " + String(index),
                )
    var traces = Int(reader.uint())
    var output = List[UInt8]()
    for _ in range(traces):
        var initial = reader.uint()
        var dialogs = PaletteDialogs()
        dialogs.chooser_revision = initial
        dialogs.chooser_closed_revision = initial
        dialogs.buffer_revision = initial
        var transport = Connection(
            -1, 1, 0, 0, BendGate(args[3]), ClientFiles()
        )
        var connection = InteractiveConnection(transport^)
        connection.model.dialogs = dialogs.copy()
        var model = AppModel()
        var count = Int(reader.uint())
        for _ in range(count):
            var frame = reader.bytes()
            apply(dialogs, frame.copy())
            connection.consume(frame^)
            model.refresh(connection)
            summary(output, dialogs)
            summary(output, connection.model.dialogs)
            summary(output, model.dialogs)
            assert_equal(len(connection.model.unhandled), 0)
            if dialogs.chooser_plane:
                assert_true(Bool(model.dialogs.chooser_plane))
                assert_equal(
                    Int(
                        model.dialogs.chooser_plane.value()[].rows.unsafe_ptr()
                    ),
                    Int(
                        connection.model.dialogs.chooser_plane.value()[].rows.unsafe_ptr()
                    ),
                )
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_all(Span(output))
