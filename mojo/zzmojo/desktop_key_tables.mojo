from std.memory import ArcPointer
from .protocol import Decoder, validate_envelope, append_uint, append_text, envelope
from .tree_wire import read_count, read_text, read_optional_text, read_bool, read_uint
from .exec_protocol import append_invocation
from .prefix_bindings import PrefixBindings, PrefixTable, PrefixBinding, PrefixBudget, read_prefix_tables, MAX_PREFIX_BYTES, MAX_PREFIX_TABLES
from .key_table_hash import key_table_hash

comptime KEY_TABLES_FULL: UInt64 = 25
comptime KEY_TABLES_PATCH: UInt64 = 56
comptime KEY_TABLES_HASH: UInt64 = 59
comptime KEY_TABLE_ACTIVE: UInt64 = 54
comptime MAX_MOUSE_WORDS = 658


def encode_key_tables(tables: List[PrefixTable]) raises -> List[UInt8]:
    if len(tables) > MAX_PREFIX_TABLES:
        raise Error("key table vector exceeds count limit")
    var budget = PrefixBudget()
    for table in tables:
        budget.table(table)
    var result = List[UInt8]()
    append_uint(result, UInt64(len(tables)))
    for table in tables:
        append_text(result, table.name)
        append_uint(result, UInt64(len(table.bindings)))
        for binding in table.bindings:
            append_text(result, binding.key)
            append_uint(result, UInt64(len(binding.commands)))
            for command in binding.commands:
                append_invocation(result, command)
            result.append(UInt8(binding.repeat))
            result.append(UInt8(Bool(binding.note)))
            if binding.note:
                append_text(result, binding.note.value())
    if len(result) > MAX_PREFIX_BYTES:
        raise Error("encoded key tables exceed byte limit")
    return result^


def _partition(tables: List[PrefixTable], name: String) -> Int:
    var size = len(tables)
    if size == 0:
        return 0
    var base = 0
    while size > 1:
        var half = size // 2
        var middle = base + half
        if tables[middle].name < name:
            base = middle
        size -= half
    return base + Int(tables[base].name < name)


def _mouse_index(key: String) raises -> Optional[Int]:
    var bytes = key.as_bytes()
    var start = 0
    var modifiers = 0
    for index in range(len(bytes)):
        if bytes[index] == 45:
            if index - start != 1:
                return None
            if bytes[start] == 67:
                modifiers |= 1
            elif bytes[start] == 77:
                modifiers |= 2
            elif bytes[start] == 83:
                modifiers |= 4
            else:
                return None
            start = index + 1
    var text = String(from_utf8=bytes[start:])
    var kinds: List[String] = ["MouseDown1", "MouseDown2", "MouseDown3", "MouseUp1", "MouseUp2", "MouseUp3", "MouseDrag1", "MouseDrag2", "MouseDrag3", "MouseDragEnd1", "MouseDragEnd2", "MouseDragEnd3", "DoubleClick1", "DoubleClick2", "DoubleClick3", "TripleClick1", "TripleClick2", "TripleClick3", "WheelUp", "WheelDown"]
    var locations: List[String] = ["Pane", "Border", "Status", "StatusLeft", "StatusRight", "StatusDefault", "Empty"]
    for kind in range(len(kinds)):
        if text.startswith(kinds[kind]):
            var rest = String(from_utf8=text.as_bytes()[kinds[kind].byte_length():])
            for location in range(len(locations)):
                if rest == locations[location]:
                    return (modifiers * 263 + location) * 20 + kind
            if not rest.startswith("Control"):
                return None
            var digits = rest.as_bytes()[7:]
            var offset = 1 if len(digits) > 0 and digits[0] == 43 else 0
            if offset == len(digits):
                return None
            var value = 0
            for index in range(offset, len(digits)):
                if digits[index] < 48 or digits[index] > 57 or value > (255 - Int(digits[index] - 48)) // 10:
                    return None
                value = value * 10 + Int(digits[index] - 48)
            return (modifiers * 263 + value + 7) * 20 + kind
    return None


def _mouse_words(tables: List[PrefixTable], copy: Bool) raises -> List[Tuple[UInt16, UInt64]]:
    var masks = List[UInt64](length=MAX_MOUSE_WORDS, fill=0)
    for table in tables:
        if (not copy and table.name != "root") or (copy and table.name != "copy-mode" and table.name != "copy-mode-vi"):
            continue
        for binding in table.bindings:
            var index = _mouse_index(binding.key)
            if index:
                masks[index.value() // 64] |= UInt64(1) << UInt64(index.value() % 64)
    var result = List[Tuple[UInt16, UInt64]]()
    for index in range(len(masks)):
        if masks[index] != 0:
            result.append((UInt16(index), masks[index]))
    return result^


def _read_mouse(mut reader: Decoder) raises -> List[Tuple[UInt16, UInt64]]:
    var count = read_count(reader, MAX_MOUSE_WORDS)
    var result = List[Tuple[UInt16, UInt64]]()
    for index in range(count):
        var word = UInt16(read_uint(reader, UInt64(MAX_MOUSE_WORDS - 1)))
        var mask = reader.uint()
        if mask == 0 or index > 0 and result[index - 1][0] >= word:
            raise Error("invalid key table mouse words")
        result.append((word, mask))
    return result^


struct DesktopKeyTables(Movable):
    var tables: List[PrefixTable]
    var prefix: PrefixBindings
    var hash: UInt64
    var root_mouse: List[Tuple[UInt16, UInt64]]
    var copy_mouse: List[Tuple[UInt16, UInt64]]
    var generation: UInt64
    var full: Bool

    def __init__(out self) raises:
        self.tables = List[PrefixTable]()
        self.prefix = PrefixBindings()
        self.hash = 0
        self.root_mouse = List[Tuple[UInt16, UInt64]]()
        self.copy_mouse = List[Tuple[UInt16, UInt64]]()
        self.generation = 0
        self.full = False

    def _replace(mut self, var tables: List[PrefixTable], full: Bool) raises:
        var encoded = encode_key_tables(tables)
        var prefix = PrefixBindings()
        prefix.full(tables)
        prefix.active_table = self.prefix.active_table.copy()
        prefix.repeat = self.prefix.repeat
        prefix.generation = self.generation + 1
        var root = _mouse_words(tables, False)
        var copy = _mouse_words(tables, True)
        self.hash = key_table_hash(Span(encoded))
        self.tables = tables^
        self.prefix = prefix^
        self.root_mouse = root^
        self.copy_mouse = copy^
        self.generation += 1
        self.full = full

    def fork(self) raises -> Self:
        var result = Self()
        result._replace(self.tables.copy(), self.full)
        result.prefix.active_table = self.prefix.active_table.copy()
        result.prefix.repeat = self.prefix.repeat
        result.prefix.generation = self.prefix.generation
        result.generation = self.generation
        result.hash = self.hash
        result.root_mouse = self.root_mouse.copy()
        result.copy_mouse = self.copy_mouse.copy()
        return result^

    def apply(mut self, var bytes: List[UInt8]) raises -> Bool:
        if self.generation == UInt64.MAX or len(bytes) > MAX_PREFIX_BYTES or validate_envelope(Span(bytes)) != 0:
            raise Error("key table update exceeds frame or generation limit")
        var reader = Decoder(bytes^)
        var message = reader.uint()
        if message == 51:
            _ = read_uint(reader, 65535)
            for _ in range(5):
                _ = reader.uint()
            if reader.offset != len(reader.data):
                raise Error("trailing key table welcome bytes")
            var fresh = Self()
            fresh.generation = self.generation + 1
            fresh.prefix.generation = fresh.generation
            self = fresh^
            return True
        if message != 10:
            return False
        _ = reader.uint()
        var kind = reader.uint()
        if kind == KEY_TABLES_FULL or kind == KEY_TABLES_PATCH:
            var incoming = read_prefix_tables(reader)
            var tables = List[PrefixTable]()
            if kind == KEY_TABLES_PATCH:
                var removed = List[String]()
                var count = read_count(reader, MAX_PREFIX_TABLES)
                for _ in range(count):
                    removed.append(read_text(reader, 65536))
                for table in self.tables:
                    var keep = True
                    for name in removed:
                        keep = keep and name != table.name
                    if keep:
                        tables.append(table.copy())
                for table in incoming:
                    var found = -1
                    for index in range(len(tables)):
                        if tables[index].name == table.name:
                            found = index
                            break
                    if found >= 0:
                        tables[found] = table.copy()
                    else:
                        if len(tables) >= MAX_PREFIX_TABLES:
                            raise Error("retained key tables exceed count limit")
                        tables.insert(_partition(tables, table.name), table.copy())
            else:
                tables = incoming^
            if reader.offset != len(reader.data):
                raise Error("trailing key table update bytes")
            self._replace(tables^, self.full or kind == KEY_TABLES_FULL)
            return True
        if kind == KEY_TABLES_HASH:
            var hash = reader.uint()
            var root = _read_mouse(reader)
            var copy = _read_mouse(reader)
            if reader.offset != len(reader.data):
                raise Error("trailing key table hash bytes")
            self.full = self.full and self.hash == hash
            self.hash = hash
            self.root_mouse = root^
            self.copy_mouse = copy^
            self.generation += 1
            self.prefix.generation = self.generation
            return True
        if kind == KEY_TABLE_ACTIVE:
            var table = read_optional_text(reader, 65536)
            var repeat = read_bool(reader)
            if reader.offset != len(reader.data):
                raise Error("trailing active key table bytes")
            self.prefix.active_table = table^
            self.prefix.repeat = repeat if self.prefix.active_table else False
            self.generation += 1
            self.prefix.generation = self.generation
            return True
        return False


def get_key_tables() raises -> List[UInt8]:
    var bytes: List[UInt8] = [54]
    return envelope(Span(bytes))


struct RetainedKeyTables(Copyable, Movable):
    var snapshot: Optional[ArcPointer[DesktopKeyTables]]
    var eligible: Bool
    var pending: Bool
    var error: String
    var revision: UInt64
    var full_known: Bool
    var active_known: Bool
    var subscribed: Bool

    def __init__(out self):
        self.snapshot = None
        self.eligible = False
        self.pending = False
        self.error = String()
        self.revision = 0
        self.full_known = False
        self.active_known = True
        self.subscribed = False

    def consume(mut self, var bytes: List[UInt8]) raises -> Bool:
        if validate_envelope(Span(bytes)) != 0:
            return False
        var header = Decoder(bytes.copy())
        var message = header.uint()
        var kind: UInt64 = 0
        if message == 10:
            _ = header.uint()
            kind = header.uint()
            if kind != KEY_TABLES_FULL and kind != KEY_TABLES_PATCH and kind != KEY_TABLES_HASH and kind != KEY_TABLE_ACTIVE:
                return False
        elif message != 51:
            return False
        if self.revision == UInt64.MAX:
            raise Error("retained key table revision exhausted")
        try:
            var staged = DesktopKeyTables()
            if self.snapshot and message != 51:
                staged = self.snapshot.value()[].fork()
            elif self.snapshot:
                staged.generation = self.snapshot.value()[].generation
            staged.full = self.full_known
            _ = staged.apply(bytes^)
            var known = self.active_known or kind == KEY_TABLE_ACTIVE or message == 51
            var full = staged.full
            self.snapshot = ArcPointer(staged^)
            self.full_known = full
            self.active_known = known
            self.eligible = full and known and self.subscribed
            if self.eligible or message == 51:
                self.error = String()
            if kind == KEY_TABLES_FULL or message == 51:
                self.pending = False
        except:
            if kind == KEY_TABLE_ACTIVE:
                self.active_known = False
            else:
                self.full_known = False
            self.eligible = False
            self.error = "Key bindings could not be refreshed"
        self.revision += 1
        return True

    def needs_full(self) -> Bool:
        return self.subscribed and not self.full_known and not self.pending

    def requested(mut self):
        self.pending = True

    def reset_active(mut self) raises:
        if self.revision == UInt64.MAX:
            raise Error("retained key table revision exhausted")
        if self.snapshot:
            var staged = self.snapshot.value()[].fork()
            if staged.generation == UInt64.MAX:
                raise Error("key table generation exhausted")
            staged.prefix.active_table = None
            staged.prefix.repeat = False
            staged.generation += 1
            staged.prefix.generation = staged.generation
            self.snapshot = ArcPointer(staged^)
        self.active_known = True
        self.eligible = self.full_known and self.subscribed
        if self.eligible:
            self.error = String()
        self.revision += 1
