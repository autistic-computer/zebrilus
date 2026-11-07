from std.memory import Pointer
from std.sys import argv
from std.testing import assert_true, assert_equal
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool, read_uint, read_text
from zzmojo.chooser_protocol import ChooseItem
from zzmojo.window_chooser_model import WindowChooserBuilder, WindowChooserRows
from zzmojo.window_chooser_search import (
    WindowChooserTarget,
    WindowChooserSearch,
)
from zzmojo.palette_navigation import parent_index, child_index


def optional(mut output: List[UInt8], value: Optional[Int]):
    append_uint(output, UInt64(Bool(value)))
    if value:
        append_uint(output, UInt64(value.value()))


def target(mut reader: Decoder) raises -> WindowChooserTarget:
    return WindowChooserTarget(UInt8(read_uint(reader, 3)), reader.uint())


def append_target(mut output: List[UInt8], value: WindowChooserTarget):
    append_uint(output, UInt64(value.kind))
    append_uint(output, value.id)


def seed(mut reader: Decoder) raises -> WindowChooserSearch:
    var result = WindowChooserSearch()
    for _ in range(Int(read_uint(reader, 65536))):
        result.expanded.append(target(reader))
    if read_bool(reader):
        result.pending = (target(reader), read_bool(reader))
    if read_bool(reader):
        result.selection = target(reader)
    return result^


def append_model(mut output: List[UInt8], model: WindowChooserRows) raises:
    optional(
        output,
        Optional[Int](
            Int(model.current_window.value())
        ) if model.current_window else Optional[Int](None),
    )
    optional(output, model.initial)
    append_uint(output, UInt64(len(model.rows)))
    for index in range(len(model.rows)):
        ref row = model.rows[index]
        append_text(output, row.label)
        append_text(output, row.detail)
        append_text(output, row.right)
        append_uint(output, UInt64(len(row.hits)))
        for hit in row.hits:
            append_uint(output, UInt64(hit.start))
            append_uint(output, UInt64(hit.end))
        append_uint(output, UInt64(row.muted_prefix))
        append_uint(output, UInt64(Bool(row.status)))
        append_uint(output, UInt64(row.running))
        append_text(output, row.icon)
        var indent = row.indent
        append_uint(
            output, UInt64(Pointer(to=indent).unsafe_bitcast[UInt32]()[])
        )
        append_uint(output, UInt64(Bool(row.expanded)))
        if row.expanded:
            append_uint(output, UInt64(row.expanded.value()))
        append_uint(output, UInt64(Bool(row.shortcut)))
        append_uint(output, UInt64(row.index))
        append_uint(output, UInt64(model.scores[index]))
    var rows = model.navigation()
    for index in range(min(len(rows) + 2, 32)):
        optional(output, parent_index(Span(rows), index))
    if len(rows) + 2 > 32:
        for index in range(max(len(rows) - 1, 0), len(rows) + 2):
            optional(output, parent_index(Span(rows), index))
    for index in range(min(len(rows) + 2, 32)):
        optional(output, child_index(Span(rows), index))
    if len(rows) + 2 > 32:
        for index in range(max(len(rows) - 1, 0), len(rows) + 2):
            optional(output, child_index(Span(rows), index))


def append_search(mut output: List[UInt8], state: WindowChooserSearch):
    append_uint(output, UInt64(len(state.expanded)))
    for item in state.expanded:
        append_target(output, item)
    append_uint(output, UInt64(Bool(state.pending)))
    if state.pending:
        append_target(output, state.pending.value()[0])
        append_uint(output, UInt64(state.pending.value()[1]))
    append_uint(output, UInt64(Bool(state.selection)))
    if state.selection:
        append_target(output, state.selection.value())


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var datasets = List[List[ChooseItem]]()
    for _ in range(Int(read_uint(reader, 65536))):
        var items = List[ChooseItem]()
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
            items.append(item^)
        datasets.append(items^)
    var builder = WindowChooserBuilder()
    var output = List[UInt8]()
    var cases = Int(read_uint(reader, 65536))
    for _ in range(cases):
        var dataset = Int(read_uint(reader, UInt64(len(datasets) - 1)))
        var query = read_text(reader)
        var grouped = read_bool(reader)
        append_model(
            output, builder.build(Span(datasets[dataset]), query, grouped)
        )
    var traces = Int(read_uint(reader, 65536))
    var steps = 0
    for _ in range(traces):
        var state = seed(reader)
        for _ in range(Int(read_uint(reader, 65536))):
            var dataset = Int(read_uint(reader, UInt64(len(datasets) - 1)))
            var result = state.advance(
                Span(datasets[dataset]), read_bool(reader)
            )
            append_uint(output, UInt64(Bool(result)))
            if result:
                append_uint(output, UInt64(result.value()[0]))
                append_uint(output, UInt64(result.value()[1]))
            append_search(output, state)
            steps += 1
    assert_equal(reader.offset, len(reader.data))
    var items = List[ChooseItem]()
    items.append(ChooseItem())
    var baseline = builder.build(Span(items), "")
    var refused = False
    try:
        _ = builder.build(Span(items), String(" ") * 65537)
    except:
        refused = True
    assert_true(refused)
    items[0].label = String("x") * 65537
    refused = False
    try:
        _ = builder.build(Span(items), "")
    except:
        refused = True
    assert_true(refused)
    assert_equal(baseline.rows[0].label, String())
    items.clear()
    var label = String("x") * 65536
    for _ in range(255):
        var item = ChooseItem()
        item.label = label.copy()
        items.append(item^)
    var exact = ChooseItem()
    exact.label = String("x") * 64000
    items.append(exact^)
    var accepted = builder.build(Span(items), "")
    assert_equal(len(accepted.rows), 256)
    items.clear()
    for _ in range(65537):
        items.append(ChooseItem())
    refused = False
    try:
        _ = builder.build(Span(items), "")
    except:
        refused = True
    assert_true(refused)
    items.clear()
    for _ in range(257):
        var item = ChooseItem()
        item.label = label.copy()
        items.append(item^)
    refused = False
    try:
        _ = builder.build(Span(items), "")
    except:
        refused = True
    assert_true(refused)
    assert_equal(baseline.rows[0].label, String())
    items.clear()
    var item = ChooseItem()
    item.flags = 2
    items.append(item^)
    var full = WindowChooserSearch()
    for index in range(65536):
        full.expanded.append(WindowChooserTarget(0, UInt64(index + 10)))
    full.pending = (WindowChooserTarget(3, UInt64.MAX), False)
    full.selection = WindowChooserTarget(2, 42)
    refused = False
    try:
        _ = full.advance(Span(items), True)
    except:
        refused = True
    assert_true(refused)
    assert_equal(len(full.expanded), 65536)
    assert_equal(full.pending.value()[0].kind, UInt8(3))
    assert_equal(full.pending.value()[0].id, UInt64.MAX)
    assert_equal(full.pending.value()[1], False)
    assert_equal(full.selection.value().id, UInt64(42))
    items[0].target = 10
    var expanded = full.advance(Span(items), True)
    assert_true(Bool(expanded))
    assert_equal(expanded.value()[0], UInt32(0))
    assert_equal(expanded.value()[1], True)
    assert_equal(len(full.expanded), 65536)
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
    print(cases, traces, steps)
