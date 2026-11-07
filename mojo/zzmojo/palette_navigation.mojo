comptime MAX_PALETTE_ROWS = 65536


@fieldwise_init
struct PaletteNavRow(Copyable, Movable):
    var kind: UInt8
    var current: Bool
    var indent: Float32
    var expanded: Optional[Bool]
    var choice: UInt32


def _bound(rows: Span[PaletteNavRow, _]) raises:
    if len(rows) > MAX_PALETTE_ROWS:
        raise Error("palette navigation exceeds its row limit")


def navigate(rows: Span[PaletteNavRow, _], selected: Optional[Int], direction: Int) raises -> Optional[Int]:
    _bound(rows)
    var indices = List[Int]()
    for index in range(len(rows)):
        if rows[index].kind != 0:
            indices.append(index)
    if len(indices) == 0:
        return None
    var current = 0
    if selected:
        for index in range(len(indices)):
            if indices[index] == selected.value():
                current = index
                break
    var next = (current - 1 if current > 0 else len(indices) - 1) if direction < 0 else (current + 1) % len(indices)
    return indices[next]


def initial_selection(rows: Span[PaletteNavRow, _], current_window: Optional[UInt32] = None) raises -> Optional[Int]:
    _bound(rows)
    for index in range(len(rows)):
        var row = rows[index].copy()
        if row.current and (row.kind == 1 or row.kind == 2 and Bool(current_window) and row.choice == current_window.value()):
            return index
    for index in range(len(rows)):
        if rows[index].kind != 0 and rows[index].current:
            return index
    for index in range(len(rows)):
        if rows[index].kind != 0:
            return index
    return None


def parent_index(rows: Span[PaletteNavRow, _], index: Int) raises -> Optional[Int]:
    _bound(rows)
    if index < 0 or index >= len(rows):
        return None
    var depth = rows[index].indent
    for previous in range(index - 1, -1, -1):
        if rows[previous].kind == 0:
            break
        if rows[previous].indent < depth:
            return previous
    return None


def child_index(rows: Span[PaletteNavRow, _], index: Int) raises -> Optional[Int]:
    _bound(rows)
    if index < 0 or index >= len(rows) - 1:
        return None
    if rows[index].expanded == Optional[Bool](True) and rows[index + 1].kind != 0 and rows[index + 1].indent > rows[index].indent:
        return index + 1
    return None


struct PaletteLayers(Copyable, Movable):
    var command: Optional[Int]
    var mode: Optional[UInt8]
    var host: Optional[UInt32]

    def __init__(out self):
        self.command = None
        self.mode = None
        self.host = None

    def pop(mut self) -> Bool:
        if self.command:
            _ = self.command.take()
            return True
        if self.mode:
            _ = self.mode.take()
            return True
        if self.host:
            _ = self.host.take()
            return True
        return False
