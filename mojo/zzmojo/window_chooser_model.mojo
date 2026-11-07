from .chooser_protocol import ChooseItem, CHOOSER_MAX_ITEMS
from .chooser_presentation import CHOOSER_DISPLAY_BYTES
from .palette_fuzzy import PaletteMatcher
from .palette_text import palette_trim
from .palette_presentation import (
    PaletteRow,
    palette_row_bytes,
    MAX_PALETTE_PRESENTATION_BYTES,
)
from .palette_navigation import PaletteNavRow, initial_selection

comptime WINDOW_CHOOSE_ACTION: UInt8 = 7


struct WindowChooserRows(Copyable, Movable):
    var rows: List[PaletteRow]
    var scores: List[Int]
    var current_window: Optional[UInt32]
    var initial: Optional[Int]

    def __init__(out self):
        self.rows = List[PaletteRow]()
        self.scores = List[Int]()
        self.current_window = None
        self.initial = None

    def navigation(self) -> List[PaletteNavRow]:
        var rows = List[PaletteNavRow]()
        for row in self.rows:
            rows.append(
                PaletteNavRow(
                    2,
                    row.right == "current",
                    row.indent,
                    row.expanded,
                    UInt32(row.index),
                )
            )
        return rows^


def _numeric_prefix(value: Span[UInt8, _]) -> Bool:
    var start = 1 if len(value) > 0 and value[0] == 43 else 0
    if start == len(value):
        return False
    var result: UInt64 = 0
    for index in range(start, len(value)):
        var byte = value[index]
        if byte < 48 or byte > 57:
            return False
        result = result * 10 + UInt64(byte - 48)
        if result > UInt64(UInt32.MAX):
            return False
    return True


def _name(item: ChooseItem) raises -> String:
    if item.text.byte_length() > 0:
        return item.text.copy()
    if item.target_kind == 1:
        var bytes = item.label.as_bytes()
        for index in range(len(bytes)):
            if bytes[index] == 58:
                if _numeric_prefix(bytes[:index]):
                    return String(from_utf8=bytes[index + 1 :])
                break
    return item.label.copy()


def _target(item: ChooseItem) -> String:
    return (
        "$" if item.target_kind == 0 else "@" if item.target_kind == 1 else "%"
    ) + String(item.target)


def _order(scores: Span[Int, _]) -> List[Int]:
    var order = List[Int]()
    var scratch = List[Int]()
    for index in range(len(scores)):
        order.append(index)
        scratch.append(0)
    var width = 1
    while width < len(order):
        var start = 0
        while start < len(order):
            var middle = min(start + width, len(order))
            var end = min(start + 2 * width, len(order))
            var left = start
            var right = middle
            for output in range(start, end):
                if left < middle and (
                    right == end or scores[order[left]] <= scores[order[right]]
                ):
                    scratch[output] = order[left]
                    left += 1
                else:
                    scratch[output] = order[right]
                    right += 1
            start = end
        swap(order, scratch)
        width *= 2
    return order^


struct WindowChooserBuilder(Movable):
    var matcher: PaletteMatcher

    def __init__(out self):
        self.matcher = PaletteMatcher()

    def build(
        self,
        items: Span[ChooseItem, _],
        query: String,
        grouped: Bool = True,
    ) raises -> WindowChooserRows:
        if len(items) > CHOOSER_MAX_ITEMS:
            raise Error("window chooser exceeds 65,536 rows")
        if query.byte_length() > CHOOSER_DISPLAY_BYTES:
            raise Error("window chooser query exceeds 64 KiB")
        var text = palette_trim(query)
        var tree = grouped and text.byte_length() == 0
        var session = String()
        var window = String()
        var session_active = False
        var window_active = False
        var result = WindowChooserRows()
        var total = 0
        var sorted = True
        for index in range(len(items)):
            ref item = items[index]
            if item.target_kind > 3 or (
                item.pane_kind and item.pane_kind.value() > 3
            ):
                raise Error("window chooser item has an unknown kind")
            if item.target_kind == 3:
                continue
            var name = _name(item)
            var path = String()
            var active: Bool
            var row = PaletteRow()
            if item.target_kind == 0:
                session = name.copy()
                window = String()
                session_active = item.flags & 4 != 0
                window_active = False
                active = session_active
                row.icon = "Layers"
            elif item.target_kind == 1:
                window = name.copy()
                window_active = session_active and item.flags & 4 != 0
                active = window_active
                if window_active:
                    result.current_window = UInt32(index)
                path = session.copy()
                row.icon = "AppWindow"
            elif item.target_kind == 2:
                path = (
                    session + " / " + window if session.byte_length() > 0
                    and window.byte_length()
                    > 0 else session.copy() if session.byte_length()
                    > 0 else window.copy()
                )
                active = window_active and item.flags & 4 != 0
                row.icon = (
                    "SquareTerminal" if item.pane_kind
                    == Optional[UInt8](0) else "Globe" if item.pane_kind
                    == Optional[UInt8](1) else "File" if item.pane_kind
                    == Optional[UInt8](2) else "Bot" if item.pane_kind
                    == Optional[UInt8](3) else "Plus"
                )
            else:
                continue
            var prefix = (
                path + " / " if not tree
                and path.byte_length() > 0 else String()
            )
            row.label = prefix + name
            row.detail = (
                item.detail.copy() if item.text.byte_length() == 0 else String()
            )
            if (
                row.label.byte_length() > CHOOSER_DISPLAY_BYTES
                or row.detail.byte_length() > CHOOSER_DISPLAY_BYTES
            ):
                raise Error("window chooser displayed row exceeds 64 KiB")
            row.muted_prefix = prefix.byte_length()
            row.right = "current" if active else ""
            row.expanded = Optional[Bool](
                item.flags & 1 != 0
            ) if tree and item.flags & 2 != 0 else Optional[Bool](None)
            row.indent = Float32(item.depth) * 18 if tree else 0
            row.action = WINDOW_CHOOSE_ACTION
            row.index = index
            var score: Int
            var matched = self.matcher.find(text, row.label)
            if matched:
                var value = matched.take()
                score = value.score
                swap(row.hits, value.hits)
            elif text.byte_length() > 0:
                var metadata = (
                    path
                    + " "
                    + item.label
                    + " "
                    + item.detail
                    + " "
                    + _target(item)
                )
                matched = self.matcher.find(text, metadata)
                if not matched:
                    continue
                score = matched.value().score + 1000
            else:
                continue
            total += palette_row_bytes(row) + len(row.hits) * 16
            if total > MAX_PALETTE_PRESENTATION_BYTES:
                raise Error("window chooser presentation exceeds 16 MiB")
            if len(result.scores) > 0:
                sorted = (
                    sorted and result.scores[len(result.scores) - 1] <= score
                )
            result.rows.append(row^)
            result.scores.append(score)
        if not tree and not sorted:
            var order = _order(Span(result.scores))
            var rows = List[PaletteRow]()
            var scores = List[Int]()
            for index in order:
                rows.append(result.rows[index].copy())
                scores.append(result.scores[index])
            result.rows = rows^
            result.scores = scores^
        var navigation = result.navigation()
        result.initial = initial_selection(
            Span(navigation), result.current_window
        )
        return result^
