from .palette_places_model import PalettePlace, PaletteFleet
from .palette_text import (
    palette_trim,
    palette_space,
    scalar_width,
    palette_words,
    palette_contains_space,
)
from .palette_fuzzy import PALETTE_QUERY_BYTES
from .protocol import parse_id


@fieldwise_init
struct PaletteLine(Copyable, Movable):
    var command: Optional[String]
    var search: Optional[String]


def parse_palette_line(text: String) raises -> PaletteLine:
    if text.byte_length() > PALETTE_QUERY_BYTES:
        raise Error("palette query exceeds its text limit")
    var leading = palette_trim(text, False)
    if leading.startswith(":"):
        return PaletteLine(String(from_utf8_lossy=leading.as_bytes()[1:]), None)
    var search = palette_trim(leading)
    if search.byte_length() == 0:
        return PaletteLine(None, None)
    return PaletteLine(None, search^)


def split_place_text(
    text: String, separator: UInt8, last: Bool = False
) -> Optional[Tuple[String, String]]:
    var found = -1
    for index in range(text.byte_length()):
        if text.as_bytes()[index] == separator:
            found = index
            if not last:
                break
    if found < 0:
        return None
    return (
        String(from_utf8_lossy=text.as_bytes()[:found]),
        String(from_utf8_lossy=text.as_bytes()[found + 1 :]),
    )


def place_digits(text: String) -> Bool:
    if text.byte_length() == 0:
        return False
    for byte in text.as_bytes():
        if byte < 48 or byte > 57:
            return False
    return True


def place_index(text: String) -> Optional[UInt32]:
    if not place_digits(text):
        return None
    var value: UInt32 = 0
    for byte in text.as_bytes():
        var digit = UInt32(byte - 48)
        if value > (UInt32.MAX - digit) // 10:
            return None
        value = value * 10 + digit
    return value


def place_id(text: String, prefix: UInt8) -> Optional[UInt64]:
    try:
        return parse_id(text, prefix)
    except:
        return None


def unescape_place(text: String) -> String:
    var result = String()
    var escaped = False
    for character in text.codepoints():
        if not escaped and Int(character) == 92:
            escaped = True
        else:
            result += String(character)
            escaped = False
    if escaped:
        result += "\\"
    return result^


@fieldwise_init
struct PaletteWritten(Copyable, Movable):
    var place: PalettePlace
    var path: Optional[String]
    var line: Optional[UInt32]


def written_place(fleet: PaletteFleet, place: PalettePlace) -> Optional[String]:
    if not place.host:
        return None
    var host = fleet.host_index(place.host.value())
    if not host:
        return None
    var label = "@" + fleet.hosts[host.value()].name
    if place.pane:
        return label + ":%" + String(place.pane.value())
    if place.window:
        return label + ":@" + String(place.window.value())
    if place.session:
        return label + ":$" + String(place.session.value())
    return label^


def _direct_target(
    fleet: PaletteFleet, host: Int, target: String, pane: Bool
) -> Optional[PalettePlace]:
    var id = place_id(target, UInt8(37 if pane else 64))
    if not id:
        return None
    ref source = fleet.hosts[host]
    for session in source.sessions:
        for window in session.windows:
            if pane:
                for record in window.panes:
                    if record.id == id.value():
                        return PalettePlace(
                            source.id, session.id, window.id, record.id
                        )
            elif window.id == id.value():
                return PalettePlace(source.id, session.id, window.id)
    return None


def read_place_target(
    fleet: PaletteFleet, host: Int, target: String
) -> Optional[PalettePlace]:
    if host < 0 or host >= len(fleet.hosts):
        return None
    if place_id(target, 37):
        return _direct_target(fleet, host, target, True)
    if place_id(target, 64):
        return _direct_target(fleet, host, target, False)
    var pieces = split_place_text(target, 58)
    var session_text = unescape_place(pieces.value()[0] if pieces else target)
    var session_index = -1
    for index in range(len(fleet.hosts[host].sessions)):
        ref session = fleet.hosts[host].sessions[index]
        if (
            session.name == session_text
            or "$" + String(session.id) == session_text
        ):
            session_index = index
            break
    if session_index < 0:
        return None
    ref session = fleet.hosts[host].sessions[session_index]
    var place = PalettePlace(fleet.hosts[host].id, session.id)
    if not pieces:
        return place^
    var split = split_place_text(pieces.value()[1], 46)
    var window_text = unescape_place(
        split.value()[0] if split else pieces.value()[1]
    )
    var found = -1
    for index in range(len(session.windows)):
        ref candidate = session.windows[index]
        if (
            String(candidate.index) == window_text
            or "@" + String(candidate.id) == window_text
        ):
            found = index
            break
    if found < 0:
        for index in range(len(session.windows)):
            if session.windows[index].name == window_text:
                found = index
                break
    if found < 0:
        return None
    place.window = session.windows[found].id
    if split:
        var wanted = split.value()[1]
        var matched = False
        for candidate in session.windows[found].panes:
            if (
                "%" + String(candidate.id) == wanted
                or String(candidate.index) == wanted
            ):
                place.pane = candidate.id
                matched = True
                break
        if not matched:
            return None
    return place^


def read_written_place(
    text: String, fleet: PaletteFleet
) raises -> Optional[PaletteWritten]:
    if text.byte_length() > PALETTE_QUERY_BYTES:
        raise Error("written palette place exceeds its text limit")
    var trimmed = palette_trim(text)
    if not trimmed.startswith("@"):
        return None
    var body = String(from_utf8_lossy=trimmed.as_bytes()[1:])
    if body.byte_length() == 0:
        return None
    for character in body.codepoints():
        var scalar = UInt32(Int(character))
        if scalar < 32 or scalar >= 127 and scalar <= 159:
            return None
    var host = fleet.named_host(body)
    if host:
        return PaletteWritten(
            PalettePlace(fleet.hosts[host.value()].id), None, None
        )
    var pieces = split_place_text(body, 58)
    if not pieces:
        return None
    host = fleet.named_host(pieces.value()[0])
    if not host:
        return None
    var rest = pieces.value()[1]
    if rest.startswith("/") or rest.startswith("~"):
        var path = rest.copy()
        var line: Optional[UInt32] = None
        for _ in range(2):
            var tail = split_place_text(path, 58, True)
            if not tail or not place_digits(tail.value()[1]):
                break
            line = place_index(tail.value()[1])
            path = tail.value()[0].copy()
        return PaletteWritten(
            PalettePlace(fleet.hosts[host.value()].id),
            unescape_place(path),
            line,
        )
    if palette_contains_space(rest) and "\\" not in rest:
        return None
    var target = read_place_target(fleet, host.value(), rest)
    if target:
        return PaletteWritten(target.take(), None, None)
    return None


def has_place_target(line: String) -> Bool:
    var words = palette_words(line)
    for index in range(1, len(words)):
        ref word = words[index]
        if word == ";" or word == "\\;":
            break
        if word.startswith("-t"):
            return True
        if (
            word.byte_length() > 1
            and word.startswith("-")
            and not word.startswith("--")
        ):
            var letters = True
            var target = False
            for byte in word.as_bytes()[1:]:
                letters = letters and (
                    byte >= 65 and byte <= 90 or byte >= 97 and byte <= 122
                )
                target = target or byte == 116
            if letters and target:
                return True
    return False


def with_place_target(line: String, target: String) -> String:
    if has_place_target(line):
        return line.copy()
    var started = False
    var end = line.byte_length()
    var byte = 0
    for character in line.codepoints():
        var scalar = UInt32(Int(character))
        if palette_space(scalar):
            if started:
                end = byte
                break
        else:
            started = True
        byte += scalar_width(scalar)
    var quoted = "'" + target + "'" if "$" in target else target.copy()
    return (
        String(from_utf8_lossy=line.as_bytes()[:end])
        + " -t "
        + quoted
        + String(from_utf8_lossy=line.as_bytes()[end:])
    )


def command_place_target(
    name: String, supported: Bool, kind: Optional[UInt8], place: PalettePlace
) -> Optional[String]:
    if not place.session or not supported or name == "new-session":
        return None
    var session = "$" + String(place.session.value())
    if name == "new-window" or kind == Optional[UInt8](1):
        return session^
    if kind == Optional[UInt8](2):
        if place.window:
            return "@" + String(place.window.value())
        return session^
    if place.pane:
        return "%" + String(place.pane.value())
    if place.window:
        return "@" + String(place.window.value())
    return session^
