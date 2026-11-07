from .chrome_keymap import ChromeKeymap, ChromeBinding, chrome_key, MAX_CHROME_BINDINGS, MAX_CHROME_RETAINED_BYTES
from .chrome_actions import ChromeAction
from .desktop_key_config import desktop_ala_source, _lower
from .desktop_native_bindings import DesktopNativeBinding, desktop_native_binding
from .palette_text import palette_words

comptime MAX_NATIVE_CHROME_RECORDS = 4096
comptime MAX_NATIVE_CHROME_BYTES = 2 * 1024 * 1024
comptime MAX_NATIVE_CHROME_STROKES = 32
comptime MAX_NATIVE_CHROME_INPUT_BYTES = 65536


@fieldwise_init
struct NativeKeystroke(Copyable, Movable, Equatable, Writable):
    var key: String
    var modifiers: UInt8
    var function: Bool
    var character: Optional[String]

    def __eq__(self, other: Self) -> Bool:
        return self.key == other.key and self.modifiers == other.modifiers and self.function == other.function and self.character == other.character


def native_keystroke(source: String, macos: Bool) raises -> Optional[NativeKeystroke]:
    if source.byte_length() > MAX_NATIVE_CHROME_INPUT_BYTES:
        raise Error("native keystroke source exceeds byte limit")
    var modifiers: UInt8 = 0
    var function = False
    var key: Optional[String] = None
    var character: Optional[String] = None
    var parts = source.split("-")
    var index = 0
    while index < len(parts):
        var part = String(parts[index])
        index += 1
        var lowered = _lower(part)
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
            function = True
            continue
        if lowered == "secondary":
            modifiers |= UInt8(8 if macos else 2)
            continue
        if lowered == "cmd" or lowered == "super" or lowered == "win":
            modifiers |= 8
            continue
        if index < len(parts):
            var next = String(parts[index])
            if next.byte_length() == 0 and source.endswith("-"):
                key = "-"
                break
            if next.byte_length() > 1 and next.startswith(">"):
                key = part^
                character = String(from_utf8=next.as_bytes()[1:])
                index += 1
                continue
            return None
        var bytes = part.as_bytes()
        if len(bytes) == 1 and bytes[0] >= 65 and bytes[0] <= 90:
            modifiers |= 1
        key = lowered^
    if not key:
        for bit in [UInt8(1), UInt8(2), UInt8(4), UInt8(8), UInt8(16)]:
            if function if bit == 16 else Bool(modifiers & bit):
                if bit == 16:
                    function = False
                else:
                    modifiers &= ~bit
                key = "shift" if bit == 1 else "control" if bit == 2 else "alt" if bit == 4 else "platform" if bit == 8 else "function"
                break
    if not key:
        return None
    return NativeKeystroke(key.take(), modifiers, function, character^)


def native_keystroke_matches(input: NativeKeystroke, target: NativeKeystroke, windows: Bool) -> Bool:
    if input.character and input.character.value() != input.key:
        var modifiers = UInt8(0) if windows else input.modifiers & UInt8(10)
        if target.key == input.character.value() and target.modifiers == modifiers and not target.function:
            return True
    return target.key == input.key and target.modifiers == input.modifiers and target.function == input.function


@fieldwise_init
struct NativeChromeChord(Copyable, Movable):
    var key: String
    var source: String
    var action: ChromeAction
    var live: Bool


def native_bound_chords(map: ChromeKeymap) raises -> List[ChromeBinding]:
    if len(map._bindings) > MAX_CHROME_BINDINGS:
        raise Error("native chrome map exceeds binding limit")
    var size = 0
    for binding in map._bindings:
        size += binding.table.byte_length() + binding.key.byte_length()
        if size > MAX_CHROME_RETAINED_BYTES:
            raise Error("native chrome map exceeds byte limit")
    var keys = List[String]()
    var values = Dict[String, ChromeBinding]()
    for table in [String("ui"), String("sidebar"), String("browser"), String("terminal"), String("prefix")]:
        for value in map.table_bindings(table):
            var key = value.key.display()
            var lookup = table + "\0" + key
            if lookup not in values:
                keys.append(lookup.copy())
            values[lookup] = ChromeBinding(table.copy(), key^, value.action)
    sort(Span(keys))
    var result = List[ChromeBinding](capacity=len(keys))
    for key in keys:
        result.append(values[key].copy())
    return result^


def native_dropped_chords(previous: List[ChromeBinding], current: List[ChromeBinding]) -> List[ChromeBinding]:
    var result = List[ChromeBinding]()
    for old in previous:
        var present = False
        for next in current:
            if old.table == next.table and old.key == next.key:
                present = True
                break
        if not present:
            result.append(old.copy())
    return result^


def native_chrome_chords(map: ChromeKeymap, dropped: List[ChromeBinding], table: String, macos: Bool) raises -> List[NativeChromeChord]:
    var result = List[NativeChromeChord]()
    for value in map.table_bindings(table):
        var source = desktop_ala_source(value.key, macos)
        if source:
            result.append(NativeChromeChord(value.key.display(), source.take(), value.action, True))
    for old in dropped:
        if old.table != table:
            continue
        var key = chrome_key(old.key)
        if key:
            var source = desktop_ala_source(key.value(), macos)
            if source:
                result.append(NativeChromeChord(old.key.copy(), source.take(), old.action, False))
    return result^


@fieldwise_init
struct NativeChromeRecord(Copyable, Movable):
    var context: UInt8
    var chord: String
    var action: Optional[ChromeAction]
    var binding: DesktopNativeBinding
    var strokes: List[NativeKeystroke]


@fieldwise_init
struct NativeChromePrepared(Movable):
    var generation: UInt64
    var map_version: UInt64
    var bound: List[ChromeBinding]
    var records: List[NativeChromeRecord]
    var retained_bytes: Int


@fieldwise_init
struct NativeChromeQuery(Copyable, Movable):
    var generation: UInt64
    var matches: List[Int]
    var pending: Bool


def _same_strokes(left: List[NativeKeystroke], right: List[NativeKeystroke]) -> Bool:
    if len(left) != len(right):
        return False
    for index in range(len(left)):
        if left[index] != right[index]:
            return False
    return True


def _match(input: List[NativeKeystroke], target: List[NativeKeystroke], windows: Bool) -> Int:
    if len(input) > len(target):
        return -1
    for index in range(len(input)):
        if not native_keystroke_matches(input[index], target[index], windows):
            return -1
    return Int(len(input) < len(target))


def _record(context: UInt8, chord: NativeChromeChord, macos: Bool) raises -> Optional[NativeChromeRecord]:
    var binding = desktop_native_binding(context, chord.action)
    if binding.kind == 0:
        return None
    var words = palette_words(chord.source)
    if len(words) > MAX_NATIVE_CHROME_STROKES:
        raise Error("native chrome sequence exceeds stroke limit")
    var strokes = List[NativeKeystroke](capacity=len(words))
    for word in words:
        var stroke = native_keystroke(word, macos)
        if not stroke:
            raise Error("native chrome sequence contains an invalid keystroke")
        strokes.append(stroke.take())
    var suppressed = not chord.live or context == 0 and binding.kind == 1
    return NativeChromeRecord(context, chord.key.copy(), None if suppressed else Optional[ChromeAction](chord.action), binding, strokes^)


def _record_bytes(record: NativeChromeRecord) -> Int:
    var size = 256 + record.chord.byte_length() * 2
    for stroke in record.strokes:
        size += 128 + stroke.key.byte_length() * 2
        if stroke.character:
            size += stroke.character.value().byte_length() * 2
    return size


def _admit(mut records: List[NativeChromeRecord], mut size: Int, var record: NativeChromeRecord) raises:
    var bytes = _record_bytes(record)
    if len(records) == MAX_NATIVE_CHROME_RECORDS or bytes > MAX_NATIVE_CHROME_BYTES - size:
        raise Error("native chrome binding history exceeds retained limit")
    records.append(record^)
    size += bytes


struct DesktopNativeKeymap(Movable):
    var generation: UInt64
    var bound: List[ChromeBinding]
    var records: List[NativeChromeRecord]
    var retained_bytes: Int
    var macos: Bool
    var windows: Bool

    def __init__(out self, macos: Bool = False, windows: Bool = False) raises:
        if macos and windows:
            raise Error("native chrome platform is inconsistent")
        self.generation = 0
        self.bound = List[ChromeBinding]()
        self.records = List[NativeChromeRecord]()
        self.retained_bytes = 0
        self.macos = macos
        self.windows = windows

    def prepare(self, map: ChromeKeymap) raises -> NativeChromePrepared:
        if self.generation == UInt64.MAX:
            raise Error("native chrome generation exhausted")
        var bound = native_bound_chords(map)
        var dropped = native_dropped_chords(self.bound, bound)
        var records = self.records.copy()
        var size = 0
        for value in bound:
            size += 256 + 2 * (value.table.byte_length() + value.key.byte_length())
        if size > MAX_NATIVE_CHROME_BYTES:
            raise Error("native chrome bound map exceeds retained limit")
        for record in records:
            size += _record_bytes(record)
        if size > MAX_NATIVE_CHROME_BYTES:
            raise Error("native chrome binding history exceeds retained limit")
        for context in [UInt8(3), UInt8(2), UInt8(0), UInt8(1)]:
            if context == 0 and self.generation == 0:
                for spelling in [String("tab"), String("shift-tab")]:
                    var strokes = List[NativeKeystroke]()
                    var parsed = native_keystroke(spelling, self.macos)
                    strokes.append(parsed.take())
                    var record = NativeChromeRecord(0, spelling.copy(), None, DesktopNativeBinding(1, 0), strokes^)
                    _admit(records, size, record^)
            var table = String("terminal" if context == 0 else "sidebar" if context == 1 else "browser" if context == 2 else "ui")
            var chords = native_chrome_chords(map, dropped, table, self.macos)
            for phase in range(2 if context == 0 else 1):
                for chord in chords:
                    if context == 0 and chord.key.startswith("D-") != (phase == 0):
                        continue
                    var record = _record(context, chord, self.macos)
                    if record:
                        _admit(records, size, record.take())
        return NativeChromePrepared(self.generation, map.version(), bound^, records^, size)

    def commit(mut self, map: ChromeKeymap, var prepared: NativeChromePrepared) raises:
        if prepared.generation != self.generation or prepared.map_version != map.version() or self.generation == UInt64.MAX:
            raise Error("native chrome installation is stale")
        swap(self.bound, prepared.bound)
        swap(self.records, prepared.records)
        self.retained_bytes = prepared.retained_bytes
        self.generation += 1

    def query(self, context: UInt8, input: List[NativeKeystroke]) raises -> NativeChromeQuery:
        if context > 4 or len(input) > MAX_NATIVE_CHROME_STROKES:
            raise Error("native chrome input exceeds admitted scope")
        var size = 0
        for stroke in input:
            if stroke.modifiers > 15:
                raise Error("native chrome modifier bits are invalid")
            size += stroke.key.byte_length()
            if stroke.character:
                size += stroke.character.value().byte_length()
            if size > MAX_NATIVE_CHROME_INPUT_BYTES:
                raise Error("native chrome input exceeds byte limit")
        var matches = List[Int]()
        var suppressed = False
        var first: Optional[Int] = None
        if context != 4:
            for depth in range(2 if context < 3 else 1):
                var group = context if context < 3 and depth == 0 else UInt8(3)
                for reverse in range(len(self.records)):
                    var index = len(self.records) - reverse - 1
                    ref record = self.records[index]
                    if record.context != group or _match(input, record.strokes, self.windows) != 0:
                        continue
                    if not record.action:
                        suppressed = True
                        continue
                    if not suppressed:
                        matches.append(index)
                        if not first:
                            first = index
        var pending = List[Int]()
        if context != 4:
            for index in range(len(self.records)):
                ref record = self.records[index]
                if record.context != context and record.context != 3 or first and first.value() > index or _match(input, record.strokes, self.windows) != 1:
                    continue
                if not record.action:
                    var next = List[Int]()
                    for old in pending:
                        if not _same_strokes(self.records[old].strokes, record.strokes):
                            next.append(old)
                    pending = next^
                else:
                    var present = False
                    for old in pending:
                        present = present or _same_strokes(self.records[old].strokes, record.strokes)
                    if not present:
                        pending.append(index)
        return NativeChromeQuery(self.generation, matches^, len(pending) > 0)

    def resolve(self, query: NativeChromeQuery, index: Int) raises -> NativeChromeRecord:
        if query.generation != self.generation or index < 0 or index >= len(query.matches):
            raise Error("native chrome query is stale or out of bounds")
        var record = query.matches[index]
        if record < 0 or record >= len(self.records):
            raise Error("native chrome query reference is out of bounds")
        return self.records[record].copy()
