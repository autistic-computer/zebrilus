from .chrome_catalog import CHROME_ACTION_NAMES

comptime MAX_CHROME_STRING_BYTES = 65536
comptime CHROME_SELECT_WINDOW: UInt8 = 2
comptime CHROME_SELECT_TAB: UInt8 = 37


@fieldwise_init
struct ChromeAction(TrivialRegisterPassable, Equatable):
    var kind: UInt8
    var index: UInt8

    def __eq__(self, other: Self) -> Bool:
        return self.kind == other.kind and self.index == other.index

    def code(self) -> UInt32:
        return (UInt32(self.kind) << 8) | UInt32(self.index)


def _position(name: String, start: Int, maximum: UInt16) -> Optional[UInt8]:
    var bytes = name.as_bytes()
    var index = start
    if index < len(bytes) and bytes[index] == 43:
        index += 1
    if index == len(bytes):
        return None
    var value: UInt16 = 0
    while index < len(bytes):
        if bytes[index] < 48 or bytes[index] > 57:
            return None
        value = value * 10 + UInt16(bytes[index] - 48)
        if value > 255:
            return None
        index += 1
    if value == 0 or value > maximum:
        return None
    return UInt8(value - 1)


struct ChromeActions(Copyable, Movable):
    var _names: List[String]

    def __init__(out self):
        self._names = List[String]()
        var source = String(CHROME_ACTION_NAMES)
        for name in source.split("\n"):
            self._names.append(String(name))

    def decode(self, code: UInt32) -> Optional[ChromeAction]:
        var kind = code >> 8
        var index = code & 255
        if kind >= UInt32(len(self._names)):
            return None
        if kind != UInt32(CHROME_SELECT_WINDOW) and kind != UInt32(CHROME_SELECT_TAB) and index != 0:
            return None
        return ChromeAction(UInt8(kind), UInt8(index))

    def name(self, action: ChromeAction) raises -> String:
        if not self.decode(action.code()):
            raise Error("invalid chrome action identity")
        var name = self._names[Int(action.kind)].copy()
        if action.kind == CHROME_SELECT_WINDOW and action.index < 9 or action.kind == CHROME_SELECT_TAB and action.index < 8:
            name += String("-", UInt16(action.index) + 1)
        return name^

    def from_name(self, name: String) raises -> Optional[ChromeAction]:
        if name.byte_length() > MAX_CHROME_STRING_BYTES:
            raise Error("chrome action name exceeds byte limit")
        if name.startswith("select-window-"):
            var value = _position(name, 14, 9)
            return ChromeAction(CHROME_SELECT_WINDOW, value.value()) if value else None
        if name.startswith("browser-select-tab-"):
            var value = _position(name, 19, 8)
            return ChromeAction(CHROME_SELECT_TAB, value.value()) if value else None
        for index in range(len(self._names)):
            if index != Int(CHROME_SELECT_WINDOW) and index != Int(CHROME_SELECT_TAB) and name == self._names[index]:
                return ChromeAction(UInt8(index), 0)
        return None

    def all(self) -> List[ChromeAction]:
        var values = List[ChromeAction]()
        for kind in range(len(self._names)):
            if kind == Int(CHROME_SELECT_WINDOW) or kind == Int(CHROME_SELECT_TAB):
                for index in range(256):
                    values.append(ChromeAction(UInt8(kind), UInt8(index)))
            else:
                values.append(ChromeAction(UInt8(kind), 0))
        return values^
