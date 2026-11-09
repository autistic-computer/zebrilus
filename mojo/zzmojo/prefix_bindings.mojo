from std.collections import Dict
from .protocol import Decoder, validate_envelope, append_uint, append_text, envelope
from .tree_wire import read_count, read_text, read_bool, read_optional_text, read_uint
from .exec_protocol import Invocation, CommandSource, append_invocation
from .input_protocol import KeyInput, input_key_name, canonical_key, _control
from .cli_preflight import CatalogEntry, CliPreflight, _catalog, _resolve
from .stream_protocol import read_host_platform

comptime MAX_PREFIX_BYTES = 16 * 1024 * 1024
comptime MAX_PREFIX_BINDINGS = 4096
comptime MAX_PREFIX_TABLES = 256
comptime MAX_PREFIX_COMMANDS = 4096
comptime MAX_PREFIX_WORDS = 16384
comptime MAX_PREFIX_TEXT = 65536


@fieldwise_init
struct PrefixBinding(Copyable, Movable):
    var key: String
    var commands: List[Invocation]
    var repeat: Bool
    var note: Optional[String]


@fieldwise_init
struct PrefixTable(Copyable, Movable):
    var name: String
    var bindings: List[PrefixBinding]


@fieldwise_init
struct PrefixQuery(TrivialRegisterPassable):
    var index: Int
    var all_keys: Bool

    def bound(self) -> Bool:
        return self.index >= 0


def _all_keys(commands: List[Invocation], catalog: List[CatalogEntry]) -> Bool:
    if len(commands) != 1 or len(commands[0].args) != 1:
        return False
    ref argument = commands[0].args[0]
    if len(argument) != 2 or argument[0] != 45 or argument[1] != 78:
        return False
    var result = CliPreflight()
    var index = _resolve(catalog, commands[0].name, result)
    return index >= 0 and catalog[index].name == "list-keys"


struct PrefixBudget:
    var bytes: Int
    var bindings: Int
    var commands: Int
    var words: Int

    def __init__(out self):
        self.bytes = 0
        self.bindings = 0
        self.commands = 0
        self.words = 0

    def add(mut self, amount: Int) raises:
        if amount < 0 or amount > MAX_PREFIX_BYTES - self.bytes:
            raise Error("prefix metadata exceeds byte limit")
        self.bytes += amount

    def text(mut self, value: String) raises:
        if value.byte_length() > MAX_PREFIX_TEXT:
            raise Error("prefix text exceeds byte limit")
        self.add(value.byte_length())

    def command(mut self, command: Invocation) raises:
        if self.commands >= MAX_PREFIX_COMMANDS:
            raise Error("prefix commands exceed count limit")
        self.commands += 1
        self.text(command.name)
        if len(command.args) + len(command.blocks) > MAX_PREFIX_WORDS - self.words:
            raise Error("prefix command words exceed count limit")
        self.words += len(command.args) + len(command.blocks)
        self.add(len(command.blocks) * 4)
        for arg in command.args:
            self.add(len(arg))
        if command.source:
            self.text(command.source.value().source)
        if command.stdin:
            self.add(len(command.stdin.value()))

    def table(mut self, table: PrefixTable) raises:
        self.text(table.name)
        if len(table.bindings) > MAX_PREFIX_BINDINGS - self.bindings:
            raise Error("prefix bindings exceed count limit")
        self.bindings += len(table.bindings)
        for binding in table.bindings:
            self.text(binding.key)
            if binding.note:
                self.text(binding.note.value())
            for command in binding.commands:
                self.command(command)


def _read_bytes(mut reader: Decoder, maximum: Int) raises -> List[UInt8]:
    var count = read_count(reader, maximum)
    var result = List[UInt8](Span(reader.data)[reader.offset:reader.offset + count])
    reader.offset += count
    return result^


def _read_command(mut reader: Decoder, mut budget: PrefixBudget) raises -> Invocation:
    if budget.commands >= MAX_PREFIX_COMMANDS:
        raise Error("prefix commands exceed count limit")
    budget.commands += 1
    var result = Invocation()
    result.name = read_text(reader, MAX_PREFIX_TEXT)
    var count = read_count(reader, MAX_PREFIX_WORDS - budget.words)
    budget.words += count
    for _ in range(count):
        result.args.append(_read_bytes(reader, MAX_PREFIX_BYTES))
    if read_bool(reader):
        result.source = CommandSource(read_text(reader, MAX_PREFIX_TEXT), UInt32(read_uint(reader, 4294967295)), UInt32(read_uint(reader, 4294967295)))
    count = read_count(reader, MAX_PREFIX_WORDS - budget.words)
    budget.words += count
    for _ in range(count):
        result.blocks.append(UInt32(read_uint(reader, 4294967295)))
    result.expanded_alias = read_bool(reader)
    if read_bool(reader):
        result.stdin = _read_bytes(reader, MAX_PREFIX_BYTES)
    result.stdin_available = read_bool(reader)
    return result^


def read_prefix_tables(mut reader: Decoder) raises -> List[PrefixTable]:
    if len(reader.data) > MAX_PREFIX_BYTES:
        raise Error("prefix frame exceeds byte limit")
    var budget = PrefixBudget()
    var count = read_count(reader, MAX_PREFIX_TABLES)
    var tables = List[PrefixTable]()
    for _ in range(count):
        var name = read_text(reader, MAX_PREFIX_TEXT)
        var bindings = List[PrefixBinding]()
        var amount = read_count(reader, MAX_PREFIX_BINDINGS - budget.bindings)
        budget.bindings += amount
        for _ in range(amount):
            var key = read_text(reader, MAX_PREFIX_TEXT)
            var commands = List[Invocation]()
            var size = read_count(reader, MAX_PREFIX_COMMANDS - budget.commands)
            for _ in range(size):
                commands.append(_read_command(reader, budget))
            var repeat = read_bool(reader)
            var note = read_optional_text(reader, MAX_PREFIX_TEXT)
            bindings.append(PrefixBinding(key^, commands^, repeat, note^))
        tables.append(PrefixTable(name^, bindings^))
    return tables^


struct PrefixBindings(Movable):
    var _bindings: List[PrefixBinding]
    var _keys: Dict[String, Int]
    var _flags: List[UInt8]
    var _catalog: List[CatalogEntry]
    var _present: Bool
    var generation: UInt64
    var active_table: Optional[String]
    var repeat: Bool

    def __init__(out self) raises:
        self._bindings = List[PrefixBinding]()
        self._keys = Dict[String, Int]()
        self._flags = List[UInt8]()
        self._catalog = _catalog()
        self._present = False
        self.generation = 0
        self.active_table = None
        self.repeat = False

    def snapshot(self) -> List[PrefixBinding]:
        return self._bindings.copy()

    def _install(mut self, bindings: List[PrefixBinding], present: Bool) raises:
        if self.generation == UInt64.MAX:
            raise Error("prefix generation exhausted")
        var budget = PrefixBudget()
        budget.table(PrefixTable("prefix", bindings.copy()))
        var keys = Dict[String, Int]()
        var flags = List[UInt8]()
        for index in range(len(bindings)):
            keys[canonical_key(bindings[index].key)] = index
            flags.append(UInt8(_all_keys(bindings[index].commands, self._catalog)))
        self._bindings = bindings.copy()
        self._keys = keys^
        self._flags = flags^
        self._present = present
        self.generation += 1

    def full(mut self, tables: List[PrefixTable]) raises:
        if len(tables) > MAX_PREFIX_TABLES:
            raise Error("prefix tables exceed count limit")
        var budget = PrefixBudget()
        var selected = -1
        for index in range(len(tables)):
            budget.table(tables[index])
            if selected < 0 and tables[index].name == "prefix":
                selected = index
        var bindings = List[PrefixBinding]()
        if selected >= 0:
            bindings = tables[selected].bindings.copy()
        self._install(bindings, selected >= 0)

    def reset_connection(mut self, tables: List[PrefixTable]) raises:
        self.full(tables)
        self.active_table = None
        self.repeat = False

    def patch(mut self, tables: List[PrefixTable], removed: List[String]) raises:
        if len(tables) > MAX_PREFIX_TABLES or len(removed) > MAX_PREFIX_TABLES:
            raise Error("prefix patch tables exceed count limit")
        var budget = PrefixBudget()
        var remove = False
        for name in removed:
            budget.text(name)
            remove = remove or name == "prefix"
        var selected = -1
        for index in range(len(tables)):
            budget.table(tables[index])
            if tables[index].name == "prefix":
                selected = index
        if selected >= 0:
            self._install(tables[selected].bindings, True)
        elif remove:
            self._install(List[PrefixBinding](), False)
        else:
            if self.generation == UInt64.MAX:
                raise Error("prefix generation exhausted")
            self.generation += 1

    def apply_event(mut self, var bytes: List[UInt8]) raises -> Bool:
        if len(bytes) > MAX_PREFIX_BYTES or validate_envelope(Span(bytes)) != 0:
            raise Error("invalid prefix event frame")
        var reader = Decoder(bytes^)
        var message = reader.uint()
        if message == 51:
            _ = read_uint(reader, 65535)
            for _ in range(5):
                _ = reader.uint()
            _ = read_host_platform(reader)
            if reader.offset != len(reader.data):
                raise Error("trailing prefix welcome bytes")
            self.reset_connection(List[PrefixTable]())
            return True
        if message != 10:
            return False
        _ = reader.uint()
        var kind = reader.uint()
        if kind == 25 or kind == 56:
            var tables = read_prefix_tables(reader)
            var removed = List[String]()
            if kind == 56:
                var count = read_count(reader, MAX_PREFIX_TABLES)
                for _ in range(count):
                    removed.append(read_text(reader, MAX_PREFIX_TEXT))
            if reader.offset != len(reader.data):
                raise Error("trailing prefix table bytes")
            if kind == 25:
                self.full(tables)
            else:
                self.patch(tables, removed)
            return True
        if kind == 54:
            var table = read_optional_text(reader, MAX_PREFIX_TEXT)
            var repeat = read_bool(reader)
            if reader.offset != len(reader.data) or self.generation == UInt64.MAX:
                raise Error("invalid prefix table activation")
            self.active_table = table^
            self.repeat = repeat if self.active_table else False
            self.generation += 1
            return True
        return False

    def query(self, input: KeyInput) raises -> PrefixQuery:
        var index = -1
        if input.modifiers & 14 == 0 and input.text:
            var count = 0
            var control = False
            for scalar in input.text.value().codepoints():
                count += 1
                control = control or _control(UInt32(Int(scalar)))
            if count == 1 and not control:
                var key = canonical_key(input.text.value())
                if key in self._keys:
                    index = self._keys[key]
        if index < 0:
            var named = canonical_key(input_key_name(input))
            if named in self._keys:
                index = self._keys[named]
            elif "Any" in self._keys:
                index = self._keys["Any"]
        return PrefixQuery(index, index >= 0 and self._flags[index] & 1 != 0)

    def chrome_enabled(self, input: KeyInput) raises -> Bool:
        return Bool(self.active_table) and self.active_table.value() == "prefix" and not self.repeat and not self.query(input).bound()


def encode_prefix_snapshot(bindings: List[PrefixBinding]) raises -> List[UInt8]:
    var budget = PrefixBudget()
    budget.table(PrefixTable("prefix", bindings.copy()))
    var output: List[UInt8] = [10, 0, 25, 1]
    append_text(output, "prefix")
    append_uint(output, UInt64(len(bindings)))
    for binding in bindings:
        append_text(output, binding.key)
        append_uint(output, UInt64(len(binding.commands)))
        for command in binding.commands:
            append_invocation(output, command)
        output.append(UInt8(binding.repeat))
        output.append(UInt8(Bool(binding.note)))
        if binding.note:
            append_text(output, binding.note.value())
    return envelope(Span(output))
