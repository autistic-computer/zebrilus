from .chooser_buffer_protocol import ChooseBufferItem
from .palette_dialog_state import ChooseBufferSnapshot
from .chooser_presentation import CHOOSER_DISPLAY_BYTES, chooser_modal_height


@fieldwise_init
struct BufferRow(Copyable, Movable):
    var key: String
    var name: String
    var preview: String
    var size: String
    var age: String
    var tagged: Bool


def buffer_size(bytes: UInt64) -> String:
    if bytes < 1024:
        return String(bytes, " B")
    var unit: UInt64 = 1024
    var label = String("KiB")
    if bytes >= 1073741824:
        unit = 1073741824
        label = "GiB"
    elif bytes >= 1048576:
        unit = 1048576
        label = "MiB"
    var tenths = UInt64.MAX if bytes > UInt64.MAX // 10 else bytes * 10
    var half = unit // 2
    tenths = UInt64.MAX if tenths > UInt64.MAX - half else tenths + half
    tenths //= unit
    return String(tenths // 10, ".", tenths % 10, " ", label)


def buffer_age(created: UInt64, now: UInt64) -> String:
    var age = now - created if now >= created else UInt64(0)
    if age < 60:
        return String(age, "s")
    if age < 3600:
        return String(age // 60, "m")
    if age < 86400:
        return String(age // 3600, "h")
    return String(age // 86400, "d")


def buffer_row(item: ChooseBufferItem, now: UInt64) raises -> BufferRow:
    if item.key.byte_length() > 64 or item.text.byte_length() > 512:
        raise Error("buffer chooser row exceeds protocol metadata limit")
    var custom = item.text.byte_length() != 0
    if not custom and (item.name.byte_length() > CHOOSER_DISPLAY_BYTES or item.preview.byte_length() > CHOOSER_DISPLAY_BYTES):
        raise Error("buffer chooser text exceeds display limit")
    return BufferRow(String() if item.key.byte_length() == 0 else String("(", item.key, ")"), item.text.copy() if custom else item.name.copy(), String() if custom else item.preview.copy(), String() if custom else buffer_size(item.size_bytes), String() if custom else buffer_age(item.created_unix_seconds, now), item.tagged)


struct BufferPresentation(Movable):
    var snapshot: ChooseBufferSnapshot
    var title: String
    var subtitle: String
    var height: Float32
    var show_key_gutter: Bool

    def __init__(out self, var snapshot: ChooseBufferSnapshot) raises:
        if snapshot.prompt.byte_length() > 512:
            raise Error("buffer chooser prompt exceeds display limit")
        if snapshot.search and snapshot.search.value().query.byte_length() > CHOOSER_DISPLAY_BYTES:
            raise Error("buffer chooser search exceeds display limit")
        self.height = chooser_modal_height(len(snapshot.items[].values), snapshot.help, snapshot.prompt.byte_length() != 0)
        self.title = "Paste buffer"
        self.subtitle = String(len(snapshot.items[].values), " buffers", " · filter: no matches" if snapshot.filter_no_matches else "")
        self.show_key_gutter = False
        for item in snapshot.items[].values:
            if item.key.byte_length() != 0:
                self.show_key_gutter = True
                break
        self.snapshot = snapshot^

    def row(self, index: Int, now: UInt64) raises -> BufferRow:
        if index < 0 or index >= len(self.snapshot.items[].values):
            raise Error("buffer chooser row is outside retained snapshot")
        return buffer_row(self.snapshot.items[].values[index], now)

    def search(self, marked: String) raises -> Optional[Tuple[String, String]]:
        if not self.snapshot.search:
            return None
        ref state = self.snapshot.search.value()
        if marked.byte_length() > CHOOSER_DISPLAY_BYTES - state.query.byte_length():
            raise Error("buffer chooser composition exceeds display limit")
        return Tuple(String("?") if state.reverse else String("/"), String(state.query, marked))
