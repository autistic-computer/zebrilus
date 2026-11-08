from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool, read_uint, read_text
from zzmojo.chooser_protocol import ChooseState, ChooseItem, choose_request
from zzmojo.palette_dialog_state import ChooseSnapshot
from zzmojo.window_chooser_controller import (
    WindowChooserController,
    WindowChooserData,
    WindowChooserEffects,
    WindowChooserOperation,
)
from zzmojo.window_chooser_search import WindowChooserTarget


def optional(mut output: List[UInt8], value: Optional[Int]):
    append_uint(output, UInt64(Bool(value)))
    if value:
        append_uint(output, UInt64(value.value()))


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
) raises:
    append_uint(output, data.revision)
    append_uint(output, UInt64(data.local))
    append_uint(output, UInt64(Bool(data.snapshot)))
    append_uint(output, UInt64(data.finishing))
    append_uint(output, UInt64(data.navigation))
    optional(output, data.selected)
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
        var grouped = read_bool(reader)
        var controller = WindowChooserController(grouped)
        var operation = WindowChooserOperation(0)
        operation.snapshot = snapshots[initial].copy()
        operation.revision = 1
        var change = controller.prepare(operation)
        append_state(output, change.data, change.effects)
        controller.commit(change^)
        states += 1
        var operations = Int(read_uint(reader, 65536))
        for _ in range(operations):
            var kind = UInt8(read_uint(reader, 12))
            operation = WindowChooserOperation(kind)
            if kind == 0:
                var index = Int(read_uint(reader, UInt64(len(snapshots) - 1)))
                operation.snapshot = snapshots[index].copy()
                operation.revision = reader.uint()
            elif kind == 1 or kind == 10:
                operation.text = read_text(reader)
                operation.cursor = (
                    Int(read_uint(reader, 65536)) if kind
                    == 1 else operation.text.byte_length()
                )
            elif kind == 2:
                var negative = read_bool(reader)
                var direction = Int(read_uint(reader, 65536))
                operation.direction = -direction if negative else direction
            elif kind == 3 or kind == 4 or kind == 6:
                operation.flag = read_bool(reader)
            elif kind == 5:
                operation.index = Int(read_uint(reader, UInt64(UInt32.MAX)))
            change = controller.prepare(operation)
            append_state(output, change.data, change.effects)
            controller.commit(change^)
            states += 1
    assert_equal(reader.offset, len(reader.data))
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
    print(traces, states)
