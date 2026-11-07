from zzmojo.terminal_model import TerminalViewport, TerminalCell, TerminalCursor
from zzmojo.terminal_projection import RowProjection
from zzmojo.terminal_composition import composition_cursor


def main() raises:
    var viewport = TerminalViewport()
    viewport.columns = 4
    viewport.rows = 2
    for _ in range(8):
        viewport.cells.append(TerminalCell(32, 0, 0))
    viewport.cells[5] = TerminalCell(0x754C, 0, 1)
    viewport.cells[6] = TerminalCell(0, 0, 2)
    viewport.metadata.cursor = TerminalCursor(0xE0F020, 2, 1, 1, 1, 0, 0)
    var live = RowProjection(0, 0, 2)
    var tail = composition_cursor(viewport, live, 4)
    if not tail or tail.value().column != 1 or tail.value().row != 1 or tail.value().wide_tail != 1:
        raise Error("native composition cursor failed to normalize a wide spacer tail")
    var clipped = composition_cursor(viewport, live, 2)
    if clipped:
        raise Error("native composition cursor escaped visible columns")
    viewport.metadata.cursor = TerminalCursor(0xE0F020, 1, 1, 1, 0, 0, 0)
    var narrow = composition_cursor(viewport, live, 2, 1)
    if not narrow or narrow.value().column != 1 or narrow.value().row != 2 or narrow.value().wide_tail != 0:
        raise Error("native composition cursor width was not clipped at the viewport edge")
    var hidden = composition_cursor(viewport, RowProjection(0, 0, 1), 4)
    if hidden:
        raise Error("native composition cursor leaked from a hidden live row")
