from std.sys import argv
from std.testing import assert_equal
from mjui.ui import TextStyle
from mjui.input import TextInput, INSERT_COMPOSED
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool, read_uint, read_text
from zzmojo.chooser_protocol import ChooseState, ChooseItem, choose_request
from zzmojo.palette_dialog_state import ChooseSnapshot
from zzmojo.window_chooser_component import WindowChooserComponent
from zzmojo.window_chooser_controller import (
    WindowChooserController,
    WindowChooserData,
    WindowChooserEffects,
    WindowChooserOperation,
)
from zzmojo.window_chooser_search import WindowChooserTarget


def target(mut output: List[UInt8], value: WindowChooserTarget):
    append_uint(output, UInt64(value.kind))
    append_uint(output, value.id)


def optional_target(
    mut output: List[UInt8], value: Optional[WindowChooserTarget]
):
    append_uint(output, UInt64(Bool(value)))
    if value:
        target(output, value.value())


def snapshot(mut reader: Decoder) raises -> ChooseSnapshot:
    var state = ChooseState()
    state.selected = UInt32(read_uint(reader, UInt64(UInt32.MAX)))
    for _ in range(Int(read_uint(reader, 65536))):
        var item = ChooseItem()
        item.label = read_text(reader)
        item.detail = read_text(reader)
        item.target_kind = UInt8(read_uint(reader, 3))
        item.target = reader.uint()
        item.depth = UInt8(read_uint(reader, 255))
        item.flags = UInt8(read_uint(reader, 255))
        if read_bool(reader):
            item.pane_kind = UInt8(read_uint(reader, 3))
        item.key = read_text(reader)
        item.text = read_text(reader)
        state.items.append(item^)
    return ChooseSnapshot(state^)


def append_state(
    mut output: List[UInt8],
    data: WindowChooserData,
    effects: WindowChooserEffects,
    component: WindowChooserComponent,
) raises:
    append_uint(output, data.revision)
    append_uint(output, UInt64(data.local))
    append_uint(output, UInt64(Bool(data.snapshot)))
    append_uint(output, UInt64(data.finishing))
    append_uint(output, UInt64(data.navigation))
    append_uint(output, UInt64(Bool(data.selected)))
    if data.selected:
        append_uint(output, UInt64(data.selected.value()))
    append_text(output, data.query)
    append_uint(output, UInt64(data.cursor))
    append_uint(output, UInt64(len(data.rows[].rows)))
    for row in data.rows[].rows:
        append_uint(output, UInt64(row.index))
    append_uint(output, UInt64(len(data.search.expanded)))
    for value in data.search.expanded:
        target(output, value)
    append_uint(output, UInt64(Bool(data.search.pending)))
    if data.search.pending:
        target(output, data.search.pending.value()[0])
        append_uint(output, UInt64(data.search.pending.value()[1]))
    optional_target(output, data.search.selection)
    optional_target(output, data.activation)
    append_uint(output, UInt64(len(effects.actions)))
    for action in effects.actions:
        var frame = choose_request(action)
        append_uint(output, UInt64(len(frame)))
        output.extend(frame^)
    append_uint(output, UInt64(effects.returned))
    if effects.returned:
        append_uint(output, 0)
    append_uint(output, UInt64(Bool(effects.scroll)))
    if effects.scroll:
        append_uint(output, UInt64(effects.scroll.value()[0]))
        append_uint(output, UInt64(effects.scroll.value()[1]))
        append_uint(output, 0)
        append_uint(output, 0)
    var text, cursor = component.state()
    append_text(output, text)
    append_uint(output, UInt64(cursor))


def apply(
    mut controller: WindowChooserController,
    operation: WindowChooserOperation,
) raises -> WindowChooserEffects:
    var change = controller.prepare(operation)
    var effects = change.effects.copy()
    controller.commit(change^)
    return effects^


def main() raises:
    var args = argv()
    var input: List[UInt8]
    with open(args[1], "r") as source:
        input = source.read_bytes()
    var reader = Decoder(input^)
    reader.offset = 0
    var snapshots = List[ChooseSnapshot]()
    for _ in range(Int(read_uint(reader, 65536))):
        snapshots.append(snapshot(reader))
    var traces = Int(read_uint(reader, 65536))
    var output = List[UInt8]()
    var states = 0
    for _ in range(traces):
        var initial = Int(read_uint(reader, UInt64(len(snapshots) - 1)))
        var controller = WindowChooserController(read_bool(reader))
        var operation = WindowChooserOperation(0)
        operation.snapshot = snapshots[initial].copy()
        operation.revision = 1
        var effects = apply(controller, operation)
        var component = WindowChooserComponent(
            0x7800000000000000, TextStyle(0, 12, 0xFFFFFFFF, False)
        )
        component.synchronize(controller.data, effects)
        append_state(output, controller.data, effects, component)
        states += 1
        for _ in range(Int(read_uint(reader, 65536))):
            var kind = read_uint(reader, 17)
            effects = WindowChooserEffects()
            if kind == 1:
                var text = read_text(reader)
                if text != component.state()[0]:
                    component.field.set_text(text)
                    component.field.marked.clear()
                if read_bool(reader):
                    var cursor = Int(read_uint(reader, 65536))
                    component.field.select(cursor, cursor)
            elif kind == 17:
                var start = Int(read_uint(reader, 65536))
                var end = Int(read_uint(reader, 65536))
                component.field.select(start, end)
            elif kind == 14:
                var marked = read_text(reader)
                var start = Int(read_uint(reader, 65536))
                var end = Int(read_uint(reader, 65536))
                _ = component.compose(marked.as_bytes(), start, end)
            elif kind == 15:
                component.unmark()
            elif kind == 16:
                var text = read_text(reader)
                var events = TextInput()
                events.insert(text, composed=True)
                assert_equal(events.events[0].cmd, INSERT_COMPOSED)
                var empty = List[UInt8]()
                _ = component.insert(
                    Span(events.bytes)[
                        Int(events.events[0].start) : Int(events.events[0].end)
                    ],
                    True,
                )
                _ = component.compose(Span(empty), 0, 0)
            elif kind == 2:
                var negative = read_bool(reader)
                var direction = Int(read_uint(reader, 65536))
                operation = WindowChooserOperation(2)
                operation.direction = -direction if negative else direction
                effects = apply(controller, operation)
            else:
                raise Error("unknown composition operation")
            var edited = component.edited(controller.data)
            if edited:
                effects = apply(controller, edited.value())
            append_state(output, controller.data, effects, component)
            states += 1
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
    print(traces, states)
