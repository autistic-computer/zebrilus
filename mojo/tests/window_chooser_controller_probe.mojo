from std.testing import assert_equal, assert_true, assert_false
from zzmojo.chooser_protocol import ChooseItem, ChooseState
from zzmojo.palette_dialog_state import ChooseSnapshot
from zzmojo.window_chooser_controller import (
    WindowChooserController,
    WindowChooserOperation,
)


def main() raises:
    var source = ChooseState()
    var item = ChooseItem()
    item.label = "session"
    item.target = 10
    item.flags = 6
    source.items.append(item^)
    var snapshot = ChooseSnapshot(source^)
    var controller = WindowChooserController()
    var operation = WindowChooserOperation(0)
    operation.snapshot = snapshot.copy()
    operation.revision = 1
    var change = controller.prepare(operation)
    assert_false(Bool(controller.data.snapshot))
    assert_equal(change.data.selected, Optional[Int](0))
    var foreign = WindowChooserController()
    var foreign_refused = False
    try:
        foreign.commit(change.copy())
    except:
        foreign_refused = True
    assert_true(foreign_refused)
    assert_equal(foreign.generation, UInt64(0))
    assert_false(Bool(foreign.data.snapshot))
    controller.commit(change.copy())
    assert_true(Bool(controller.data.snapshot))
    var refused = False
    try:
        controller.commit(change^)
    except:
        refused = True
    assert_true(refused)
    operation = WindowChooserOperation(3)
    operation.flag = True
    change = controller.prepare(operation)
    assert_equal(len(change.effects.actions), 2)
    assert_equal(change.effects.actions[0].kind, UInt8(9))
    assert_equal(change.effects.actions[0].index, UInt32(0))
    assert_equal(change.effects.actions[1].kind, UInt8(7))
    assert_false(Bool(controller.data.search.pending))
    controller.commit(change^)
    assert_true(Bool(controller.data.search.pending))
    operation = WindowChooserOperation(4)
    change = controller.prepare(operation)
    assert_equal(len(change.effects.actions), 0)
    assert_true(Bool(change.data.activation))
    assert_false(Bool(controller.data.activation))
    controller.commit(change^)
    operation = WindowChooserOperation(1)
    operation.text = String("x") * 65537
    operation.cursor = operation.text.byte_length()
    var generation = controller.generation
    refused = False
    try:
        _ = controller.prepare(operation)
    except:
        refused = True
    assert_true(refused)
    assert_equal(controller.generation, generation)
    assert_equal(controller.data.query, String())
    assert_true(Bool(controller.data.activation))
    controller.generation = UInt64.MAX
    refused = False
    try:
        _ = controller.prepare(WindowChooserOperation(11))
    except:
        refused = True
    assert_true(refused)
    assert_equal(controller.generation, UInt64.MAX)
    print("Window chooser staged actions and atomic refusal checks passed")
