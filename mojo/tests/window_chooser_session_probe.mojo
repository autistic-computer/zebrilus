from std.testing import assert_true, assert_false, assert_equal
from zzmojo.actions import ActionQueue
from zzmojo.chooser_protocol import ChooseState, ChooseItem, ChooseEvent
from zzmojo.chooser_session import ChooserSource
from zzmojo.palette_dialog_state import PaletteDialogs
from zzmojo.window_chooser_session import WindowChooserSession
from zzmojo.window_chooser_controller import WindowChooserOperation


def opened(mut dialogs: PaletteDialogs):
    var state = ChooseState()
    var item = ChooseItem()
    item.label = "dev"
    state.items.append(item^)
    var event = ChooseEvent()
    event.state = state^
    dialogs.apply_chooser(event^)


def main() raises:
    var session = WindowChooserSession()
    var dialogs = PaletteDialogs()
    var queue = ActionQueue()
    var source = ChooserSource(1, 2, 3)
    opened(dialogs)
    var observed = session.observe(queue, dialogs, source, False, False, 0, 10)
    assert_false(session.active)
    observed = session.observe(queue, dialogs, source, True, False, 0, 10)
    assert_true(observed.owner_changed)
    assert_true(session.active)
    var owner = session.owner
    var operation = WindowChooserOperation(1)
    operation.text = "dev"
    operation.cursor = 3
    _ = session.apply(queue, operation, owner, 0, 10)
    assert_equal(session.controller.data.query, "dev")
    _ = session.return_default(queue, owner, 0, 10)
    assert_false(session.active)
    assert_true(Bool(session.pending_close))
    observed = session.observe(queue, dialogs, source, True, True, 0, 10)
    assert_false(session.active)
    assert_false(observed.owner_changed)
    dialogs.apply_chooser(ChooseEvent())
    observed = session.observe(queue, dialogs, source, True, True, 0, 10)
    assert_false(Bool(session.pending_close))
    session.local_opened(dialogs.chooser_revision)
    opened(dialogs)
    observed = session.observe(queue, dialogs, source, True, True, 0, 10)
    assert_true(observed.owner_changed)
    assert_true(observed.replaced_local)
    assert_true(session.owner.scope > owner.scope)
    assert_equal(session.controller.data.query, "")
    var refused = False
    try:
        _ = session.apply(queue, WindowChooserOperation(4), owner, 0, 10)
    except:
        refused = True
    assert_true(refused)
    session.local_opened(dialogs.chooser_revision)
    observed = session.observe(queue, dialogs, source, True, True, 0, 10)
    assert_true(observed.closed)
    assert_false(session.active)
    source.epoch = 4
    observed = session.observe(queue, dialogs, source, True, True, 0, 10)
    assert_true(observed.owner_changed)
    assert_true(session.active)
    print(
        "Windows chooser explicit opt-in, close fence, local revision and"
        " source ownership passed"
    )
