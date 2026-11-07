from std.testing import assert_equal, assert_true, assert_false
from zzmojo.state import AppModel
from zzmojo.terminal_model import (
    TerminalViewport,
    TerminalHistory,
    TerminalCell,
    TerminalStyle,
    TerminalScrollbar,
)
from zzmojo.terminal_history import TerminalHistoryState
from zzmojo.local_scroll_controller import TerminalScrollController


def main() raises:
    var viewport = TerminalViewport()
    viewport.generation = 1
    viewport.view_generation = 1
    viewport.columns = 1
    viewport.rows = 3
    viewport.styles.append(TerminalStyle(0, 0, 0, 0, 0, 0))
    viewport.cells = [
        TerminalCell(65, 0, 0),
        TerminalCell(66, 0, 0),
        TerminalCell(67, 0, 0),
    ]
    viewport.metadata.scrollbar = TerminalScrollbar(100, 10, 3)
    var chunk = TerminalHistory()
    chunk.columns = 1
    chunk.rows = 6
    chunk.total = 100
    chunk.start = 4
    chunk.styles.append(TerminalStyle(0, 0, 0, 0, 0, 0))
    for index in range(6):
        chunk.cells.append(TerminalCell(UInt32(40 + index), 0, 0))
    var counter: UInt64 = 1
    var retained = TerminalHistoryState(viewport, counter)
    assert_true(retained.apply_history(viewport, chunk^, counter))
    var model = AppModel()
    model.source_server = 1
    model.source_client = 2
    model.terminals[0] = viewport.copy()
    model.histories[0] = retained.snapshot(viewport)
    model.history_invalidations[0] = 0
    var controller = TerminalScrollController()
    _ = controller.observe(model, 0)
    var effects = controller.pixels(model, 0, 45, 20, 100)
    assert_true(Bool(effects))
    assert_equal(controller.target(0).value(), UInt32(8))
    assert_equal(controller.sub_row(0), Float32(5))
    assert_equal(controller.next_wake().value(), UInt64(120_000_100))
    assert_equal(len(controller.observe(model, 120_000_099)), 0)
    var synced = controller.observe(model, 120_000_100)
    assert_equal(len(synced), 1)
    assert_equal(synced[0].kind, UInt8(0))
    assert_equal(synced[0].value, UInt32(8))
    assert_equal(controller.next_wake().value(), UInt64(2_000_000_100))
    assert_false(Bool(controller.flush(0)))
    assert_true(controller.cancel(0))
    assert_false(Bool(controller.next_wake()))
    assert_equal(len(controller.observe(model, 2_000_000_100)), 0)
    _ = controller.pixels(model, 0, 45, 20, 3_000_000_000)
    retained.trim(0, counter)
    model.histories[0] = retained.snapshot(viewport)
    model.history_invalidations[0] = retained.history_invalidations
    var restored = controller.observe(model, 3_000_000_001)
    assert_equal(len(restored), 1)
    assert_equal(restored[0].value, UInt32(8))
    assert_false(Bool(controller.target(0)))
    assert_equal(controller.sub_row(0), Float32(0))
    model.source_client = 3
    _ = controller.observe(model, 3_000_000_002)
    assert_equal(len(controller.states), 0)
    assert_false(Bool(controller.pixels(model, 0, 45, 20, 3_000_000_003)))
    print(
        "local scroll controller exact timers, input cancellation, eviction"
        " restore and connection retirement passed"
    )
