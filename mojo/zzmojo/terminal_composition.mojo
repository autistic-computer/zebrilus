from mjui.term.grid import GridCursor
from .terminal_model import TerminalViewport
from .terminal_projection import RowProjection


def composition_cursor(viewport: TerminalViewport, projection: RowProjection, visible_columns: Int, extra_row: Int = 0) -> Optional[GridCursor]:
    if not viewport.metadata.cursor:
        return None
    var cursor = viewport.metadata.cursor.value()
    var row = projection.display_row(cursor.row)
    var column = Int(cursor.column)
    var columns = min(visible_columns, Int(viewport.columns))
    if not row or column >= columns:
        return None
    var index = Int(cursor.row) * Int(viewport.columns) + column
    if index >= len(viewport.cells):
        return None
    var cell = viewport.cells[index]
    if (cursor.wide_tail != 0 or cell.flags & 3 == 2) and column > 0:
        column -= 1
        cell = viewport.cells[index - 1]
    var width = min(2 if cell.flags & 3 == 1 else 1, columns - column)
    return GridCursor(cursor.color, UInt16(column), UInt16(row.value() + extra_row), cursor.style, cursor.visible, cursor.blinking, UInt8(width > 1))
