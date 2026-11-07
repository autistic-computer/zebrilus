@fieldwise_init
struct PaletteUiKey(Copyable, Movable):
    var key: String
    var modifiers: UInt8
    var key_char: Optional[String]


def _ascii_lower(text: String) -> String:
    var bytes = List[UInt8](text.as_bytes())
    for index in range(len(bytes)):
        if bytes[index] >= 65 and bytes[index] <= 90:
            bytes[index] += 32
    return String(unsafe_from_utf8=Span(bytes))


def palette_ui_key(
    text: String, apple: Bool = False
) raises -> Optional[PaletteUiKey]:
    if text.byte_length() > 4096:
        raise Error("palette key spelling exceeds 4 KiB")
    var parts = List[String]()
    var first = 0
    for index in range(text.byte_length() + 1):
        if index == text.byte_length() or text.as_bytes()[index] == 45:
            if len(parts) >= 256:
                raise Error("palette key spelling exceeds 256 parts")
            parts.append(String(from_utf8_lossy=text.as_bytes()[first:index]))
            first = index + 1
    var modifiers = UInt8(0)
    var key: Optional[String] = None
    var key_char: Optional[String] = None
    var index = 0
    while index < len(parts):
        var component = parts[index].copy()
        var lowered = _ascii_lower(component)
        index += 1
        if lowered == "ctrl":
            modifiers |= 2
            continue
        if lowered == "alt":
            modifiers |= 4
            continue
        if lowered == "shift":
            modifiers |= 1
            continue
        if lowered == "fn":
            modifiers |= 16
            continue
        if lowered == "secondary":
            modifiers |= UInt8(8 if apple else 2)
            continue
        if lowered == "cmd" or lowered == "super" or lowered == "win":
            modifiers |= 8
            continue
        if index < len(parts):
            ref next = parts[index]
            if next.byte_length() == 0 and text.endswith("-"):
                key = String("-")
                break
            if next.byte_length() > 1 and next.startswith(">"):
                key = component^
                key_char = String(from_utf8_lossy=next.as_bytes()[1:])
                index += 1
                continue
            return None
        if (
            component.byte_length() == 1
            and component.as_bytes()[0] >= 65
            and component.as_bytes()[0] <= 90
        ):
            modifiers |= 1
        key = lowered^
    if not key:
        if modifiers & 1 != 0:
            key = String("shift")
            modifiers &= ~UInt8(1)
        elif modifiers & 2 != 0:
            key = String("control")
            modifiers &= ~UInt8(2)
        elif modifiers & 4 != 0:
            key = String("alt")
            modifiers &= ~UInt8(4)
        elif modifiers & 8 != 0:
            key = String("platform")
            modifiers &= ~UInt8(8)
        elif modifiers & 16 != 0:
            key = String("function")
            modifiers &= ~UInt8(16)
        else:
            return None
    return PaletteUiKey(key.take(), modifiers, key_char^)
