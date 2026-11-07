from .chooser_protocol import ChooseItem, CHOOSER_MAX_ITEMS
from .palette_dialog_state import ChooseSnapshot

comptime CHOOSER_ROW_HEIGHT: Float32 = 40
comptime CHOOSER_DISPLAY_BYTES: Int = 65536


def chooser_modal_height(row_count: Int, help: Bool, prompt: Bool) raises -> Float32:
    if row_count < 0 or row_count > CHOOSER_MAX_ITEMS:
        raise Error("chooser row count exceeds display limit")
    return 106 + Float32(min(row_count, 10)) * CHOOSER_ROW_HEIGHT + Float32(Int(help) + Int(prompt)) * 36


def chooser_key_is_text(search: Bool, character: Bool, modifiers: UInt8) -> Bool:
    return search and character and modifiers & 14 == 0


def _display_bound(text: String) raises:
    if text.byte_length() > CHOOSER_DISPLAY_BYTES:
        raise Error("chooser display text exceeds shaping limit")


@fieldwise_init
struct ChooserRow(Copyable, Movable):
    var key: String
    var label: String
    var detail: String
    var target: String
    var depth: UInt8
    var disclosure: UInt8
    var active: Bool
    var tagged: Bool
    var pane_kind: Optional[UInt8]


def chooser_row(item: ChooseItem) raises -> ChooserRow:
    if item.target_kind > 3 or (Bool(item.pane_kind) and item.pane_kind.value() > 3):
        raise Error("unknown chooser row target")
    if item.key.byte_length() > 64 or item.text.byte_length() > 512:
        raise Error("chooser row metadata exceeds protocol limit")
    if item.text.byte_length() == 0:
        _display_bound(item.label)
        _display_bound(item.detail)
    var key = String() if item.key.byte_length() == 0 else String("(", item.key, ")")
    var prefix = String("$") if item.target_kind == 0 else String("@") if item.target_kind == 1 else String("%") if item.target_kind == 2 else String("c")
    var label = item.label.copy() if item.text.byte_length() == 0 else item.text.copy()
    var detail = item.detail.copy() if item.text.byte_length() == 0 else String()
    var disclosure: UInt8 = 0
    if item.flags & 2 != 0:
        disclosure = 1 if item.flags & 1 == 0 else 2
    return ChooserRow(key^, label^, detail^, String(prefix, item.target), item.depth, disclosure, item.flags & 4 != 0, item.flags & 8 != 0, item.pane_kind)


struct ChooserPresentation(Movable):
    var snapshot: ChooseSnapshot
    var title: String
    var subtitle: String
    var height: Float32
    var show_key_gutter: Bool

    def __init__(out self, var snapshot: ChooseSnapshot) raises:
        if snapshot.kind > 2:
            raise Error("unknown chooser kind")
        if snapshot.prompt.byte_length() > 512:
            raise Error("chooser prompt exceeds protocol limit")
        if snapshot.search:
            _display_bound(snapshot.search.value().query)
        self.height = chooser_modal_height(len(snapshot.items[].values), snapshot.help, snapshot.prompt.byte_length() != 0)
        self.title = String("Choose window") if snapshot.kind == 0 else String("Choose pane") if snapshot.kind == 1 else String("Choose client")
        var targets = String("sessions and windows") if snapshot.kind == 0 else String("sessions, windows, and panes") if snapshot.kind == 1 else String("clients")
        self.subtitle = String(len(snapshot.items[].values), " ", targets)
        if snapshot.filter_no_matches:
            self.subtitle += " · filter: no matches"
        self.show_key_gutter = False
        for item in snapshot.items[].values:
            if item.key.byte_length() != 0:
                self.show_key_gutter = True
                break
        self.snapshot = snapshot^

    def row(self, index: Int) raises -> ChooserRow:
        if index < 0 or index >= len(self.snapshot.items[].values):
            raise Error("chooser row index is outside the retained snapshot")
        return chooser_row(self.snapshot.items[].values[index])

    def search(self, marked: String) raises -> Optional[Tuple[String, String]]:
        if not self.snapshot.search:
            return None
        ref state = self.snapshot.search.value()
        if marked.byte_length() > CHOOSER_DISPLAY_BYTES - state.query.byte_length():
            raise Error("chooser composition exceeds shaping limit")
        return Tuple(String("?") if state.reverse else String("/"), String(state.query, marked))
