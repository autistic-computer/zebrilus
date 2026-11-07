from std.math import isfinite
from std.ffi import external_call
from .chrome_keymap import ChromeKeymap, ChromeKey, chrome_key
from .chrome_actions import ChromeActions
from .palette_text import palette_space, palette_trim

comptime MAX_DESKTOP_CONFIG_BYTES = 65536


def _trim(text: String) -> String:
    return palette_trim(text)


def _lower(text: String) -> String:
    var bytes = List[UInt8](text.as_bytes())
    for index in range(len(bytes)):
        if bytes[index] >= 65 and bytes[index] <= 90:
            bytes[index] += 32
    return String(from_utf8_lossy=Span(bytes))


@fieldwise_init
struct DesktopKeystroke(Copyable, Movable):
    var key: ChromeKey
    var function: Bool

    def spelling(self) -> String:
        return String(
            "ctrl-" if self.key.modifiers & 2 else "",
            "alt-" if self.key.modifiers & 4 else "",
            "cmd-" if self.key.modifiers & 8 else "",
            "shift-" if self.key.modifiers & 1 else "",
            "fn-" if self.function else "",
            self.key.base,
        )


def desktop_keystroke(
    source: String, macos: Bool
) -> Optional[DesktopKeystroke]:
    var modifiers: UInt8 = 0
    var function = False
    var base: Optional[String] = None
    var parts = source.split("-")
    var index = 0
    while index < len(parts):
        var part = String(parts[index])
        index += 1
        var lower = _lower(part)
        if lower == "ctrl":
            modifiers |= 2
            continue
        if lower == "alt":
            modifiers |= 4
            continue
        if lower == "shift":
            modifiers |= 1
            continue
        if lower == "fn":
            function = True
            continue
        if lower == "secondary":
            modifiers |= UInt8(8 if macos else 2)
            continue
        if lower == "cmd" or lower == "super" or lower == "win":
            modifiers |= 8
            continue
        if index < len(parts):
            var next = String(parts[index])
            if next.byte_length() == 0 and source.endswith("-"):
                base = "-"
                break
            if next.byte_length() > 1 and next.startswith(">"):
                base = part^
                index += 1
                continue
            return None
        var bytes = part.as_bytes()
        if len(bytes) == 1 and bytes[0] >= 65 and bytes[0] <= 90:
            modifiers |= 1
        base = lower^
    if not base:
        for bit in [UInt8(1), UInt8(2), UInt8(4), UInt8(8), UInt8(16)]:
            if function if bit == 16 else Bool(modifiers & bit):
                if bit == 16:
                    function = False
                else:
                    modifiers &= ~bit
                base = (
                    "shift" if bit
                    == 1 else "control" if bit
                    == 2 else "alt" if bit
                    == 4 else "platform" if bit
                    == 8 else "function"
                )
                break
    if not base or base.value().byte_length() == 0:
        return None
    return DesktopKeystroke(ChromeKey(modifiers, base.take()), function)


def desktop_ala_source(key: ChromeKey, macos: Bool) -> Optional[String]:
    var base = key.base.copy()
    var names: List[String] = [
        "Enter",
        "Escape",
        "Tab",
        "BSpace",
        "Up",
        "Down",
        "Left",
        "Right",
        "Home",
        "End",
        "PPage",
        "NPage",
        "DC",
        "IC",
        "F1",
        "F2",
        "F3",
        "F4",
        "F5",
        "F6",
        "F7",
        "F8",
        "F9",
        "F10",
        "F11",
        "F12",
    ]
    var mapped: List[String] = [
        "enter",
        "escape",
        "tab",
        "backspace",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
        "delete",
        "insert",
        "f1",
        "f2",
        "f3",
        "f4",
        "f5",
        "f6",
        "f7",
        "f8",
        "f9",
        "f10",
        "f11",
        "f12",
    ]
    if base == " ":
        base = "space"
    else:
        for index in range(len(names)):
            if names[index] == base:
                base = mapped[index].copy()
                break
    var source = String(
        "cmd-" if key.modifiers & 8 else "",
        "ctrl-" if key.modifiers & 2 else "",
        "alt-" if key.modifiers & 4 else "",
        "shift-" if key.modifiers & 1 else "",
    )
    var bytes = base.as_bytes()
    if len(bytes) == 0:
        return None
    if len(bytes) == 1 and bytes[0] >= 65 and bytes[0] <= 90:
        if key.modifiers & 1:
            return None
        source += "shift-" + _lower(base)
    else:
        source += base
    return source^ if desktop_keystroke(source, macos) else None


@fieldwise_init
struct DesktopOverride(Copyable, Movable):
    var bind: Bool
    var table: String
    var key: String
    var action: String


@fieldwise_init
struct DesktopOverrideResult(Copyable, Movable):
    var value: Optional[DesktopOverride]
    var error: Optional[String]


def desktop_override(
    text: String, bind: Bool, macos: Bool
) raises -> DesktopOverrideResult:
    if text.byte_length() > MAX_DESKTOP_CONFIG_BYTES:
        raise Error("desktop override exceeds text limit")
    var target = text.copy()
    var action = String()
    if bind:
        var equal = -1
        for index in range(text.byte_length()):
            if text.as_bytes()[index] == 61:
                equal = index
        if equal < 0:
            return DesktopOverrideResult(
                None, String("expected `<table>:<key>=<action>`")
            )
        target = String(from_utf8=text.as_bytes()[:equal])
        action = _trim(String(from_utf8=text.as_bytes()[equal + 1 :]))
    var colon = -1
    for index in range(target.byte_length()):
        if target.as_bytes()[index] == 58:
            colon = index
            break
    if colon < 0:
        return DesktopOverrideResult(None, String("expected `<table>:<key>`"))
    var table = _trim(String(from_utf8=target.as_bytes()[:colon]))
    if (
        table != "ui"
        and table != "prefix"
        and table != "sidebar"
        and table != "terminal"
        and table != "browser"
    ):
        return DesktopOverrideResult(
            None,
            String(
                "unknown chrome table `",
                table,
                "`; expected one of ui, sidebar, browser, terminal, prefix",
            ),
        )
    var key = String(from_utf8=target.as_bytes()[colon + 1 :])
    var chord = chrome_key(key)
    if not chord or not desktop_ala_source(chord.value(), macos):
        return DesktopOverrideResult(
            None, String("`", key, "` is not a chord zz can bind")
        )
    if bind and not ChromeActions().from_name(action):
        return DesktopOverrideResult(
            None, String("unknown chrome action `", action, "`")
        )
    return DesktopOverrideResult(
        DesktopOverride(bind, table^, chord.value().display(), action^), None
    )


def desktop_configured_map(
    overrides: List[DesktopOverride], hotkey: String, macos: Bool, scope: UInt32
) raises -> ChromeKeymap:
    if len(overrides) > 4096 or hotkey.byte_length() > MAX_DESKTOP_CONFIG_BYTES:
        raise Error("desktop configured keymap exceeds field limits")
    var map = ChromeKeymap(UInt8(2 if macos else 1), scope)
    var chord = desktop_keystroke(hotkey, macos)
    if chord and not chord.value().function:
        for binding in map.table_bindings("browser"):
            if binding.action.kind == 43 and (
                binding.key.modifiers != chord.value().key.modifiers
                or binding.key.base != chord.value().key.base
            ):
                _ = map.unbind("browser", binding.key.display())
        _ = map.bind(
            "browser", chord.value().key.display(), "browser-element-selector"
        )
    var budget = hotkey.byte_length()
    for entry in overrides:
        var bytes = (
            entry.table.byte_length()
            + entry.key.byte_length()
            + entry.action.byte_length()
        )
        if bytes > MAX_DESKTOP_CONFIG_BYTES - budget:
            raise Error(
                "desktop configured overrides exceed aggregate text limit"
            )
        budget += bytes
        if entry.bind:
            _ = map.bind(entry.table, entry.key, entry.action)
        else:
            _ = map.unbind(entry.table, entry.key)
    return map^


def _without_comment(text: String) -> String:
    var quote: UInt32 = 0
    var escaped = False
    var saw_value = False
    var previous_space = False
    var offset = 0
    for point in text.codepoints():
        var scalar = UInt32(Int(point))
        var space = palette_space(scalar)
        if escaped:
            escaped = False
            saw_value = saw_value or not space
            previous_space = space
            offset += (
                1 if scalar
                < 128 else 2 if scalar
                < 2048 else 3 if scalar
                < 65536 else 4
            )
            continue
        if scalar == 92 and quote != 0:
            escaped = True
        elif (scalar == 34 or scalar == 39) and quote == scalar:
            quote = 0
        elif (scalar == 34 or scalar == 39) and quote == 0:
            quote = scalar
        elif scalar == 35 and quote == 0 and saw_value and previous_space:
            return String(from_utf8_lossy=text.as_bytes()[:offset])
        saw_value = saw_value or not space
        previous_space = space
        offset += (
            1 if scalar
            < 128 else 2 if scalar
            < 2048 else 3 if scalar
            < 65536 else 4
        )
    return text.copy()


def _decimal_float(text: String) -> Bool:
    var bytes = text.as_bytes()
    var index = (
        1 if len(bytes) > 0 and (bytes[0] == 43 or bytes[0] == 45) else 0
    )
    var rest = _lower(String(from_utf8_lossy=bytes[index:]))
    if rest == "inf" or rest == "infinity" or rest == "nan":
        return True
    var digits = 0
    while index < len(bytes) and bytes[index] >= 48 and bytes[index] <= 57:
        digits += 1
        index += 1
    if index < len(bytes) and bytes[index] == 46:
        index += 1
        while index < len(bytes) and bytes[index] >= 48 and bytes[index] <= 57:
            digits += 1
            index += 1
    if digits == 0:
        return False
    if index < len(bytes) and (bytes[index] == 101 or bytes[index] == 69):
        index += 1
        if index < len(bytes) and (bytes[index] == 43 or bytes[index] == 45):
            index += 1
        var start = index
        while index < len(bytes) and bytes[index] >= 48 and bytes[index] <= 57:
            index += 1
        if index == start:
            return False
    return index == len(bytes)


def _float32(var text: String) raises -> Float32:
    if not _decimal_float(text):
        raise Error("invalid source decimal Float32")
    comptime Locale = Pointer[NoneType, MutUntrackedOrigin]
    comptime Byte = Pointer[UInt8, MutUntrackedOrigin]
    var name = String("C")
    var locale = external_call["newlocale", Optional[Locale]](
        Int32(0), name.as_c_string_span().ptr(), Optional[Locale](None)
    )
    _ = name
    if not locale:
        raise Error("could not prepare decimal numeric locale")
    try:
        var end: Optional[Byte] = None
        var value = external_call["strtof_l", Float32](
            text.as_c_string_span().ptr(), Pointer(to=end), locale.value()
        )
        if (
            not end
            or Int(end.value())
            != Int(text.as_c_string_span().ptr()) + text.byte_length()
        ):
            raise Error("invalid source decimal Float32")
        _ = text
        return value
    finally:
        external_call["freelocale", NoneType](locale.value())


struct DesktopKeyConfig(Copyable, Movable):
    var overrides: List[DesktopOverride]
    var hotkey: String
    var picker_sidebar: Bool
    var which_key_delay: Float32
    var diagnostics: List[Tuple[Int, String]]

    def __init__(out self, macos: Bool):
        self.overrides = List[DesktopOverride]()
        self.hotkey = "cmd-shift-c" if macos else "ctrl-shift-c"
        self.picker_sidebar = False
        self.which_key_delay = 400
        self.diagnostics = List[Tuple[Int, String]]()

    def map(self, macos: Bool, scope: UInt32) raises -> ChromeKeymap:
        return desktop_configured_map(self.overrides, self.hotkey, macos, scope)

    def which_key_ms(self) -> UInt64:
        var whole = UInt64(self.which_key_delay)
        return whole + UInt64(self.which_key_delay - Float32(whole) >= 0.5)


def desktop_key_config(source: String, macos: Bool) raises -> DesktopKeyConfig:
    if source.byte_length() > MAX_DESKTOP_CONFIG_BYTES:
        raise Error("desktop configuration exceeds the 65536-byte source limit")
    var result = DesktopKeyConfig(macos)
    var number = 0
    for raw in source.split("\n"):
        number += 1
        var line = _trim(String(raw))
        if line.byte_length() == 0 or line.startswith("#"):
            continue
        var equal = -1
        for index in range(line.byte_length()):
            if line.as_bytes()[index] == 61:
                equal = index
                break
        if equal < 0:
            continue
        var key = _trim(String(from_utf8=line.as_bytes()[:equal]))
        if (
            key != "chrome-keybind"
            and key != "chrome-unbind"
            and key != "browser-element-selector-hotkey"
            and key != "picker-focus-sidebar"
            and key != "which-key-delay"
        ):
            continue
        var value = _trim(
            _without_comment(String(from_utf8=line.as_bytes()[equal + 1 :]))
        )
        var error: Optional[String] = None
        if key == "chrome-keybind" or key == "chrome-unbind":
            var entry = desktop_override(value, key == "chrome-keybind", macos)
            if entry.value:
                if len(result.overrides) == 4096:
                    raise Error(
                        "desktop configuration exceeds override count limit"
                    )
                result.overrides.append(entry.value.take())
            error = entry.error.copy()
        elif key == "browser-element-selector-hotkey":
            var chord = desktop_keystroke(_trim(value), macos)
            if not chord:
                error = (
                    "expected modifiers and a key, for example `cmd-shift-c` or"
                    " `ctrl-shift-c`"
                )
            elif (
                chord.value().key.modifiers & 14 == 0
                and not chord.value().function
            ):
                error = (
                    "expected Control, Alt, Command/Super, or Function as a"
                    " modifier"
                )
            else:
                result.hotkey = chord.value().spelling()
        elif key == "picker-focus-sidebar":
            var lowered = _lower(value)
            if (
                lowered == "on"
                or lowered == "yes"
                or lowered == "1"
                or lowered == "true"
            ):
                result.picker_sidebar = True
            elif (
                lowered == "off"
                or lowered == "no"
                or lowered == "0"
                or lowered == "false"
            ):
                result.picker_sidebar = False
            else:
                error = (
                    "expected a boolean (`true`/`on`/`yes`/`1` or"
                    " `false`/`off`/`no`/`0`)"
                )
        else:
            if not _decimal_float(value):
                error = "expected a number milliseconds"
            else:
                var delay = _float32(value.copy())
                if not isfinite(delay):
                    error = "value must be finite"
                elif delay < 0 or delay > 2000:
                    error = "value must be between 0 and 2000 milliseconds"
                else:
                    result.which_key_delay = delay
        if error:
            result.diagnostics.append(
                (number, String("invalid `", key, "`: ", error.value()))
            )
    return result^
