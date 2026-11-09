from std.testing import assert_equal, assert_true, assert_false
from zzmojo.actions import ActionQueue, MAX_PENDING_ACTIONS
from zzmojo.chooser_protocol import (
    ChooseItem,
    ChooseState,
    choose_request,
    ChooseAction,
)
from zzmojo.chooser_session import ChooserSource
from zzmojo.palette_dialog_state import ChooseSnapshot
from zzmojo.window_chooser_controller import (
    WindowChooserController,
    WindowChooserOperation,
)
from zzmojo.window_chooser_admission import (
    WindowChooserOwner,
    stage_window_chooser,
    commit_window_chooser,
)


def main() raises:
    var controller = WindowChooserController()
    var source = ChooseState()
    var item = ChooseItem()
    item.label = "session"
    item.target = 1
    item.flags = 2
    source.items.append(item^)
    var operation = WindowChooserOperation(0)
    operation.snapshot = ChooseSnapshot(source^)
    operation.revision = 1
    controller.commit(controller.prepare(operation))
    var queue = ActionQueue()
    for _ in range(MAX_PENDING_ACTIONS - 1):
        queue.append(10, "", None)
    operation = WindowChooserOperation(3)
    operation.flag = True
    var owner = WindowChooserOwner(ChooserSource(11, 12, 13), 14)
    var refused = False
    try:
        _ = stage_window_chooser(controller, queue, operation, owner, 15, 10)
    except:
        refused = True
    assert_true(refused)
    assert_false(Bool(controller.data.search.pending))
    assert_equal(len(queue), MAX_PENDING_ACTIONS - 1)
    _ = queue.take()
    var admission = stage_window_chooser(
        controller, queue, operation, owner, 15, 10
    )
    assert_equal(len(queue), 0)
    assert_false(Bool(controller.data.search.pending))
    queue.append(10, "intervening", None)
    refused = False
    try:
        commit_window_chooser(controller, queue, admission^, owner)
    except:
        refused = True
    assert_true(refused)
    assert_false(Bool(controller.data.search.pending))
    assert_equal(len(queue), 1)
    admission = stage_window_chooser(
        controller, queue, operation, owner, 15, 10
    )
    refused = False
    try:
        commit_window_chooser(
            controller,
            queue,
            admission^,
            WindowChooserOwner(ChooserSource(11, 12, 99), 14),
        )
    except:
        refused = True
    assert_true(refused)
    assert_equal(len(queue), 1)
    assert_false(Bool(controller.data.search.pending))
    admission = stage_window_chooser(
        controller, queue, operation, owner, 15, 10
    )
    commit_window_chooser(controller, queue, admission^, owner)
    assert_true(Bool(controller.data.search.pending))
    assert_equal(len(queue), 3)
    assert_equal(queue.items[1].kind, 10)
    assert_equal(
        queue.items[1].source,
        Optional[Tuple[UInt64, UInt64]]((UInt64(11), UInt64(12))),
    )
    assert_equal(queue.items[1].selection_epoch, UInt64(15))
    var select = ChooseAction(9)
    assert_equal(queue.items[1].frame, choose_request(select))
    assert_equal(queue.items[2].frame, choose_request(ChooseAction(7)))
    print(
        "Window chooser batch capacity, stale queue, source retirement and"
        " ordered admission passed"
    )
