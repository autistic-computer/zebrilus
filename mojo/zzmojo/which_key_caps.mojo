from mjui.widgets.penna_key_symbols import key_symbol_scalar
from mjui.widgets.penna_native import _hex
from .chrome_keymap import chrome_key
from .palette_unicode import LOWERCASE
from .which_key_caps_data import WHICH_KEY_ALPHANUMERIC, WHICH_KEY_CASED, WHICH_KEY_IGNORABLE
from .prefix_bindings import MAX_PREFIX_BINDINGS, MAX_PREFIX_BYTES


@fieldwise_init
struct WhichKeyStroke(Copyable, Movable):
    var key: String
    var modifiers: UInt8
    var key_char: Optional[String]


def _ascii_lower(text: String) -> String:
    var bytes = List[UInt8](text.as_bytes())
    for index in range(len(bytes)):
        if bytes[index] >= 65 and bytes[index] <= 90:
            bytes[index] += 32
    return String(from_utf8_lossy=Span(bytes))


def _contains(data: Span[UInt8, _], point: UInt32) -> Bool:
    var low = 0
    var high = len(data) // 16
    while low < high:
        var middle = low + (high - low) // 2
        if _hex(data, middle * 16 + 8, 8) < point:
            low = middle + 1
        else:
            high = middle
    return low < len(data) // 16 and _hex(data, low * 16, 8) <= point


def _lower_point(point: UInt32) -> String:
    var data = LOWERCASE.as_bytes()
    var low = 0
    var high = len(data) // 32
    while low < high:
        var middle = low + (high - low) // 2
        if _hex(data, middle * 32, 8) < point:
            low = middle + 1
        else:
            high = middle
    if low >= len(data) // 32 or _hex(data, low * 32, 8) != point:
        return String(chr(Int(point)))
    var result = String()
    for index in range(1, 4):
        var value = _hex(data, low * 32 + index * 8, 8)
        if value == 0:
            break
        result += chr(Int(value))
    return result^


def _rust_lower(text: String) -> String:
    var points = List[UInt32]()
    for point in text.codepoints():
        points.append(UInt32(Int(point)))
    var following = List[Bool](length=len(points), fill=False)
    var next = False
    for index in range(len(points) - 1, -1, -1):
        following[index] = next
        if not _contains(WHICH_KEY_IGNORABLE.as_bytes(), points[index]):
            next = _contains(WHICH_KEY_CASED.as_bytes(), points[index])
    var before = False
    var result = String()
    for index in range(len(points)):
        var point = points[index]
        if point == 931 and before and not following[index]:
            result += "ς"
        else:
            result += _lower_point(point)
        if not _contains(WHICH_KEY_IGNORABLE.as_bytes(), point):
            before = _contains(WHICH_KEY_CASED.as_bytes(), point)
    return result^


def _symbol(text: String) -> String:
    if text == "ctrl":
        return "ctrl"
    if text == "cmd":
        return "win"
    if text == "escape":
        return "esc"
    if text == "pageup":
        return "page up"
    if text == "pagedown":
        return "page down"
    var scalar: Optional[UInt32] = None
    var first_bytes = 0
    for point in text.codepoints():
        if scalar:
            return _rust_lower(key_symbol_scalar(scalar.value()) + String(from_utf8_lossy=text.as_bytes()[first_bytes:]))
        scalar = UInt32(Int(point))
        first_bytes = 1 if scalar.value() < 128 else 2 if scalar.value() < 2048 else 3 if scalar.value() < 65536 else 4
    return key_symbol_scalar(scalar.value()) if scalar else String()


def which_key_format(stroke: WhichKeyStroke) -> String:
    return String("Ctrl+" if stroke.modifiers & 2 != 0 else "", "Alt+" if stroke.modifiers & 4 != 0 else "", "Shift+" if stroke.modifiers & 1 != 0 else "", "Win+" if stroke.modifiers & 8 != 0 else "", _symbol(stroke.key))


@inline(.never)
def which_key_stroke(text: String) raises -> Optional[WhichKeyStroke]:
    var parsed = chrome_key(text)
    if not parsed:
        return None
    ref chrome = parsed.value()
    var base = chrome.base.copy()
    if base == " ":
        base = "space"
    var names: List[String] = ["Enter", "Escape", "Tab", "BSpace", "Up", "Down", "Left", "Right", "Home", "End", "PPage", "NPage", "DC", "IC", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"]
    var mapped: List[String] = ["enter", "escape", "tab", "backspace", "up", "down", "left", "right", "home", "end", "pageup", "pagedown", "delete", "insert", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"]
    for index in range(len(names)):
        if base == names[index]:
            base = mapped[index].copy()
            break
    var source = String("cmd-" if chrome.modifiers & 8 != 0 else "", "ctrl-" if chrome.modifiers & 2 != 0 else "", "alt-" if chrome.modifiers & 4 != 0 else "", "shift-" if chrome.modifiers & 1 != 0 else "")
    var bytes = base.as_bytes()
    if len(bytes) == 1 and bytes[0] >= 65 and bytes[0] <= 90:
        if chrome.modifiers & 1 != 0:
            return None
        source += "shift-" + _ascii_lower(base)
    else:
        source += base
    var parts = source.split("-")
    var modifiers: UInt8 = 0
    var key = Optional[String]()
    var key_char = Optional[String]()
    var index = 0
    while index < len(parts):
        var part = String(parts[index])
        var lower = _ascii_lower(part)
        var flag: UInt8 = 0
        if lower == "ctrl" or lower == "secondary":
            flag = 2
        elif lower == "alt":
            flag = 4
        elif lower == "shift":
            flag = 1
        elif lower == "cmd" or lower == "super" or lower == "win":
            flag = 8
        elif lower == "fn":
            flag = 16
        if flag != 0:
            modifiers |= flag
            index += 1
            continue
        if index + 1 < len(parts):
            var next = parts[index + 1]
            if next.byte_length() == 0 and source.endswith("-"):
                key = String("-")
                break
            if next.byte_length() > 1 and next.startswith(">"):
                key = part^
                key_char = String(from_utf8_lossy=next.as_bytes()[1:])
                index += 2
                continue
            return None
        var part_bytes = part.as_bytes()
        if len(part_bytes) == 1 and part_bytes[0] >= 65 and part_bytes[0] <= 90:
            modifiers |= 1
        key = lower^
        index += 1
    if not key:
        var flags: List[UInt8] = [1, 2, 4, 8, 16]
        var values: List[String] = ["shift", "control", "alt", "platform", "function"]
        for slot in range(len(flags)):
            if modifiers & flags[slot] != 0:
                modifiers &= ~flags[slot]
                key = values[slot].copy()
                break
    if not key or key.value().byte_length() == 0:
        return None
    return WhichKeyStroke(key.take(), modifiers, key_char^)


def _alphanumeric(point: UInt32) -> Bool:
    return _contains(WHICH_KEY_ALPHANUMERIC.as_bytes(), point)


def _join(values: List[String], separator: String) -> String:
    var result = String()
    for index in range(len(values)):
        if index > 0:
            result += separator
        result += values[index]
    return result^


@fieldwise_init
struct WhichKeyCap(Copyable, Movable):
    var keys: List[WhichKeyStroke]
    var raw: String
    var yours: Bool

    def text(self) -> String:
        if len(self.keys) == 0:
            return self.raw.copy()
        var full = List[String]()
        var symbols = List[String]()
        var same = True
        var digits = len(self.keys) >= 3
        var punctuation = True
        var previous = -1
        for index in range(len(self.keys)):
            ref key = self.keys[index]
            full.append(which_key_format(key))
            same &= key.modifiers == self.keys[0].modifiers
            var symbol = _symbol(key.key)
            var bytes = symbol.as_bytes()
            var digit = Int(bytes[0]) - 48 if len(bytes) == 1 and bytes[0] >= 48 and bytes[0] <= 57 else -1
            digits &= digit >= 0 and (index == 0 or digit == previous + 1)
            previous = digit
            var count = 0
            for point in symbol.codepoints():
                punctuation &= not _alphanumeric(UInt32(Int(point)))
                count += 1
            punctuation &= count == 1
            symbols.append(symbol^)
        if len(self.keys) == 1:
            return full[0].copy()
        if not same:
            return _join(full, " ")
        var head = full[0].copy()
        var modifiers = String()
        if head.endswith(symbols[0]):
            modifiers = String(from_utf8_lossy=head.as_bytes()[:head.byte_length() - symbols[0].byte_length()])
        if digits:
            return String(modifiers, symbols[0], "–", symbols[len(symbols) - 1])
        if punctuation:
            return modifiers + _join(symbols, "")
        if modifiers.byte_length() == 0:
            return _join(symbols, " ")
        return _join(full, " ")


def which_key_cap(keys: List[String], yours: Bool) raises -> WhichKeyCap:
    if len(keys) > MAX_PREFIX_BINDINGS:
        raise Error("which-key cap exceeds key count limit")
    var total = 0
    for key in keys:
        if key.byte_length() > MAX_PREFIX_BYTES - total:
            raise Error("which-key cap exceeds byte limit")
        total += key.byte_length()
    var parsed = List[WhichKeyStroke]()
    var valid = True
    for key in keys:
        var stroke = which_key_stroke(key)
        if not stroke:
            valid = False
            break
        parsed.append(stroke.take())
    if not valid:
        parsed.clear()
    return WhichKeyCap(parsed^, _join(keys, " "), yours)
