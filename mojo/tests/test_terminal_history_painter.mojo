from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.debug import render_rgb
from zzmojo.terminal_model import (
    TerminalViewport,
    TerminalHistory,
    TerminalCell,
    TerminalStyle,
    TerminalScrollbar,
)
from zzmojo.terminal_history import TerminalHistoryState
from zzmojo.terminal_history_painter import render_retained_viewport


def main() raises:
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    var viewport = TerminalViewport()
    viewport.generation = 1
    viewport.view_generation = 1
    viewport.columns = 1
    viewport.rows = 3
    viewport.metadata.foreground = 0xFFFFFF
    viewport.metadata.background = 0x112233
    viewport.metadata.scrollbar = TerminalScrollbar(100, 10, 3)
    viewport.styles.append(TerminalStyle(0xFFFFFF, 0x112233, 0, 0, 0, 0))
    viewport.cells = [
        TerminalCell(65, 0, 0),
        TerminalCell(66, 0, 0),
        TerminalCell(67, 0, 0),
    ]
    var chunk = TerminalHistory()
    chunk.columns = 1
    chunk.rows = 6
    chunk.total = 100
    chunk.start = 4
    chunk.styles.append(TerminalStyle(0xFFFFFF, 0x778899, 0, 0, 0, 0))
    for glyph in range(40, 46):
        chunk.cells.append(TerminalCell(UInt32(glyph), 0, 0))
    var counter: UInt64 = 1
    var retained = TerminalHistoryState(viewport, counter)
    assert_true(retained.apply_history(viewport, chunk^, counter))
    var snapshot = retained.snapshot(viewport)
    ui.set_time(1)
    ui.begin_frame(8, 54)
    _ = render_retained_viewport(
        ui, 71, viewport, snapshot, UInt32(8), 5, 0, 13, True, 1, 3
    )
    ui.end_frame()
    var index = ui.term.find(71)
    assert_equal(ui.term.views[index].offset, 13)
    assert_equal(
        String(from_utf8=Span(ui.term.views[index].text)), String("+\n,\n-\nA")
    )
    var pixels = render_rgb(ui.dl, ui.atlas, 8, 54)
    assert_equal(
        (pixels[21], pixels[22], pixels[23]),
        (UInt8(0x77), UInt8(0x88), UInt8(0x99)),
    )
    var bottom = (53 * 8 + 7) * 3
    assert_equal(
        (pixels[bottom], pixels[bottom + 1], pixels[bottom + 2]),
        (UInt8(0x11), UInt8(0x22), UInt8(0x33)),
    )
    var revision = (
        ui.term.views[index].entries[Int(ui.term.views[index].shown[1])].rev
    )
    ui.begin_frame(8, 54)
    _ = render_retained_viewport(
        ui, 71, viewport, snapshot, UInt32(8), 6, 0, 13, True, 1, 3
    )
    ui.end_frame()
    assert_equal(ui.term.views[index].built, 0)
    assert_equal(
        ui.term.views[index].entries[Int(ui.term.views[index].shown[1])].rev,
        revision,
    )
    viewport.generation += 1
    ui.begin_frame(8, 54)
    _ = render_retained_viewport(
        ui, 71, viewport, snapshot, UInt32(8), 6, 0, 13, True, 1, 3
    )
    ui.end_frame()
    assert_equal(ui.term.views[index].offset, 0)
    assert_equal(
        String(from_utf8=Span(ui.term.views[index].text)), String("A\nB\nC")
    )
    assert_false(ui.term.views[index].cursor_on)
    print(
        "retained terminal painter exact rows, historical dictionaries,"
        " fractional pixels, identity reuse and stale fallback passed"
    )
