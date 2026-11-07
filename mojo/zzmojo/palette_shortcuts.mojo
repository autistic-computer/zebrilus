from .palette_text import palette_trim, palette_words

comptime MAX_PALETTE_SHORTCUT_BYTES = 65536
comptime MAX_PALETTE_SHORTCUT_KEYS = 256


@fieldwise_init
struct PaletteKeystroke(Copyable, Movable):
    var modifiers: UInt8
    var key: String
    var key_char: Optional[String]


struct PaletteShortcut(Copyable, Movable):
    var kind: UInt8
    var text: String
    var keys: List[PaletteKeystroke]

    def __init__(out self):
        self.kind = 0
        self.text = String()
        self.keys = List[PaletteKeystroke]()

    def byte_length(self) -> Int:
        var count = self.text.byte_length()
        for key in self.keys:
            count += key.key.byte_length()
            if key.key_char:
                count += key.key_char.value().byte_length()
        return count


def _single(text: String) -> Optional[UInt32]:
    var point: Optional[UInt32] = None
    for character in text.codepoints():
        if point:
            return None
        point = UInt32(Int(character))
    return point


def _prefix(bytes: Span[UInt8, _], offset: Int, prefix: String) -> Bool:
    if prefix.byte_length() > len(bytes) - offset:
        return False
    for index in range(prefix.byte_length()):
        if bytes[offset + index] != prefix.as_bytes()[index]:
            return False
    return True


def palette_keystroke(spelling: String) raises -> Optional[PaletteKeystroke]:
    if spelling.byte_length() > MAX_PALETTE_SHORTCUT_BYTES:
        raise Error("palette keystroke exceeds its byte limit")
    var rest = palette_trim(spelling)
    var modifiers: UInt8 = 0
    var bytes = rest.as_bytes()
    var offset = 0
    while True:
        var count = 0
        if _prefix(bytes, offset, "D-"):
            modifiers |= 8
            count = 2
        elif _prefix(bytes, offset, "Cmd-"):
            modifiers |= 8
            count = 4
        elif _prefix(bytes, offset, "Super-"):
            modifiers |= 8
            count = 6
        elif _prefix(bytes, offset, "C-"):
            modifiers |= 2
            count = 2
        elif _prefix(bytes, offset, "Ctrl-"):
            modifiers |= 2
            count = 5
        elif _prefix(bytes, offset, "M-"):
            modifiers |= 4
            count = 2
        elif _prefix(bytes, offset, "Alt-"):
            modifiers |= 4
            count = 4
        elif _prefix(bytes, offset, "S-"):
            modifiers |= 1
            count = 2
        elif _prefix(bytes, offset, "Shift-"):
            modifiers |= 1
            count = 6
        if count == 0:
            break
        offset += count
    if offset > 0:
        var tail = String(from_utf8_lossy=bytes[offset:])
        rest = tail^
    if rest == "Space" or rest.byte_length() == 0 and spelling.endswith(" "):
        rest = " "
    elif rest.byte_length() == 0:
        return None
    var single = _single(rest)
    var commanded = modifiers & 14 != 0
    if single:
        var scalar = single.value()
        var alphabetic = (
            scalar >= 65 and scalar <= 90 or scalar >= 97 and scalar <= 122
        )
        if modifiers & 1 != 0 and not commanded and alphabetic:
            if scalar >= 97:
                rest = String(chr(Int(scalar - 32)))
            modifiers &= ~UInt8(1)
        elif commanded and scalar >= 65 and scalar <= 90:
            rest = String(chr(Int(scalar + 32)))
            modifiers |= 1
    var base = rest.copy()
    if rest == " " or rest == "Space":
        base = "space"
    elif rest == "Enter":
        base = "enter"
    elif rest == "Escape":
        base = "escape"
    elif rest == "Tab":
        base = "tab"
    elif rest == "BSpace":
        base = "backspace"
    elif rest == "DC":
        base = "delete"
    elif rest == "Up":
        base = "up"
    elif rest == "Down":
        base = "down"
    elif rest == "Left":
        base = "left"
    elif rest == "Right":
        base = "right"
    elif rest == "Home":
        base = "home"
    elif rest == "End":
        base = "end"
    elif rest == "PPage" or rest == "PageUp":
        base = "pageup"
    elif rest == "NPage" or rest == "PageDown":
        base = "pagedown"
    elif rest == "-":
        base = "minus"
    elif not _single(rest):
        return None
    var key = base.copy()
    var key_char: Optional[String] = None
    single = _single(base)
    if single and single.value() >= 65 and single.value() <= 90:
        key = String(chr(Int(single.value() + 32)))
        modifiers |= 1
    if key == "minus":
        key = "-"
    if modifiers & 14 == 0 and single:
        key_char = base.copy()
    return PaletteKeystroke(modifiers, key^, key_char^)


def palette_shortcut(spelling: String) raises -> PaletteShortcut:
    if spelling.byte_length() > MAX_PALETTE_SHORTCUT_BYTES:
        raise Error("palette shortcut exceeds its byte limit")
    var words = palette_words(spelling)
    if len(words) > MAX_PALETTE_SHORTCUT_KEYS:
        raise Error("palette shortcut exceeds its key limit")
    var result = PaletteShortcut()
    for word in words:
        var key = palette_keystroke(word)
        if not key:
            result.kind = 1
            result.text = spelling
            result.keys.clear()
            return result^
        result.keys.append(key.take())
    if len(result.keys) == 0:
        result.kind = 1
        result.text = spelling
    return result^
