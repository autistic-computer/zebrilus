from std.math import isfinite
from .chooser_presentation import CHOOSER_DISPLAY_BYTES, chooser_key_is_text
from .chooser_protocol import CHOOSER_MAX_ITEMS


@fieldwise_init
struct ChooserSync(TrivialRegisterPassable):
    var changed: Bool
    var selection_changed: Bool


struct ChooserInput(Copyable, Movable):
    var revision: UInt64
    var selected: Optional[UInt32]
    var search_active: Bool
    var marked: Optional[String]

    def __init__(out self):
        self.revision = 0
        self.selected = None
        self.search_active = False
        self.marked = None

    def synchronize(
        mut self, revision: UInt64, selected: UInt32, search_active: Bool
    ) -> ChooserSync:
        if self.revision == revision:
            return ChooserSync(False, False)
        self.revision = revision
        self.search_active = search_active
        var changed = self.selected != Optional[UInt32](selected)
        self.selected = selected
        if not search_active:
            self.marked = None
        return ChooserSync(True, changed)

    def key_is_text(self, character: Bool, modifiers: UInt8) -> Bool:
        return chooser_key_is_text(self.search_active, character, modifiers)

    def unmark(mut self) -> Bool:
        var changed = Bool(self.marked)
        self.marked = None
        return changed

    def replace_text(
        mut self, text: String, query_bytes: Int = 0
    ) raises -> Optional[String]:
        if self.search_active and text.byte_length() != 0:
            _input_bound(text, query_bytes)
            self.marked = None
            return text.copy()
        self.marked = None
        return None

    def mark_text(mut self, text: String, query_bytes: Int = 0) raises:
        _input_bound(text, query_bytes)
        self.marked = text.copy() if text.byte_length() != 0 else Optional[
            String
        ](None)

    def marked_utf16_length(self) -> Optional[Int]:
        if not self.marked:
            return None
        var count = 0
        for character in self.marked.value().codepoints():
            count += 2 if Int(character) > 65535 else 1
        return count


def _input_bound(text: String, query_bytes: Int) raises:
    if (
        query_bytes < 0
        or query_bytes > CHOOSER_DISPLAY_BYTES
        or text.byte_length() > CHOOSER_DISPLAY_BYTES - query_bytes
    ):
        raise Error("chooser input exceeds 64 KiB display limit")


def chooser_center_scroll(
    scroll: Float32,
    row_height: Float32,
    viewport: Float32,
    count: UInt32,
    selected: UInt32,
) raises -> Float32:
    if (
        not isfinite(scroll)
        or not isfinite(row_height)
        or not isfinite(viewport)
        or row_height <= 0
        or viewport < 0
        or count > CHOOSER_MAX_ITEMS
    ):
        raise Error("invalid chooser scroll geometry")
    var maximum = max(Float32(count) * row_height - viewport, Float32(0))
    var position = scroll.clamp(Float32(0), maximum)
    var top = row_height * Float32(selected)
    var bottom = top + row_height
    if top < position or bottom > position + viewport:
        var center = top + row_height / 2
        position = (center - viewport / 2).clamp(Float32(0), maximum)
    return position


def chooser_wheel_scroll(
    scroll: Float32,
    maximum: Float32,
    line_height: Float32,
    x: Float32,
    y: Float32,
    precise: Bool,
) raises -> Float32:
    if (
        not isfinite(scroll)
        or not isfinite(maximum)
        or not isfinite(line_height)
        or not isfinite(x)
        or not isfinite(y)
        or maximum < 0
        or line_height < 0
    ):
        raise Error("invalid chooser wheel geometry")
    var delta = y if y != 0 else x
    if not precise:
        delta *= line_height
    return (scroll - delta).clamp(Float32(0), maximum)
