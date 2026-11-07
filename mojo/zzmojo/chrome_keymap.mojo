from .chrome_actions import ChromeAction, ChromeActions, MAX_CHROME_STRING_BYTES
from .chrome_catalog import CHROME_PROFILE_DEFAULTS
from .palette_text import palette_trim
from .input_protocol import KeyInput, input_key_name, _control, _scalar
from .key_spelling import canonical_protocol_key
from .verified_input_router import RouterKey, RouterAction, RouterState, RouterInput, RouterPayload, RouterOperation
from .input_router_gate import RouterMapArena

comptime MAX_CHROME_BINDINGS = 4096
comptime MAX_CHROME_TABLES = 256
comptime MAX_CHROME_RETAINED_BYTES = 1024 * 1024


@fieldwise_init
struct ChromeKey(Copyable, Movable):
    var modifiers: UInt8
    var base: String

    def normalized(self) -> Self:
        var result = self.copy()
        var single: Optional[UInt32] = None
        var count = 0
        for point in self.base.codepoints():
            single = UInt32(Int(point))
            count += 1
            if count > 1:
                return result^
        if single:
            var scalar = single.value()
            var commanded = self.modifiers & 14 != 0
            var alphabetic = scalar >= 65 and scalar <= 90 or scalar >= 97 and scalar <= 122
            if self.modifiers & 1 != 0 and not commanded and alphabetic:
                result.base = String(chr(Int(scalar - 32 if scalar >= 97 else scalar)))
                result.modifiers &= ~UInt8(1)
            elif commanded and scalar >= 65 and scalar <= 90:
                result.base = String(chr(Int(scalar + 32)))
                result.modifiers |= 1
        return result^

    def display(self) -> String:
        return String("D-" if self.modifiers & 8 != 0 else "", "C-" if self.modifiers & 2 != 0 else "", "M-" if self.modifiers & 4 != 0 else "", "S-" if self.modifiers & 1 != 0 else "", self.base)


def chrome_key(spelling: String) raises -> Optional[ChromeKey]:
    if spelling.byte_length() > MAX_CHROME_STRING_BYTES:
        raise Error("chrome key spelling exceeds byte limit")
    var rest = palette_trim(spelling)
    var flags: UInt8 = 0
    var bytes = rest.as_bytes()
    var offset = 0
    while True:
        var amount = 0
        var bit: UInt8 = 0
        for prefix in ["D-", "Cmd-", "Super-", "C-", "Ctrl-", "M-", "Alt-", "S-", "Shift-"]:
            var token = String(prefix)
            var matched = token.byte_length() <= len(bytes) - offset
            if matched:
                for index in range(token.byte_length()):
                    matched = matched and bytes[offset + index] == token.as_bytes()[index]
            if matched:
                amount = token.byte_length()
                if token == "D-" or token == "Cmd-" or token == "Super-":
                    bit = 8
                elif token == "C-" or token == "Ctrl-":
                    bit = 2
                elif token == "M-" or token == "Alt-":
                    bit = 4
                else:
                    bit = 1
                break
        if amount == 0:
            break
        flags |= bit
        offset += amount
    var base = String(from_utf8=bytes[offset:])
    if base == "Space" or base.byte_length() == 0 and spelling.endswith(" "):
        base = " "
    elif base.byte_length() == 0:
        return None
    return ChromeKey(flags, base^).normalized()


def _stored_key(key: String) raises -> String:
    var parsed = chrome_key(key)
    return canonical_protocol_key(parsed.value().display() if parsed else key)


@fieldwise_init
struct ChromeBinding(Copyable, Movable):
    var table: String
    var key: String
    var action: ChromeAction


@fieldwise_init
struct ChromeTableBinding(Copyable, Movable):
    var key: ChromeKey
    var action: ChromeAction


def _before(table: String, key: String, value: ChromeBinding) -> Bool:
    return table < value.table or table == value.table and key < value.key


def _validate_input(input: KeyInput) raises:
    if input.action > 2 or input.modifiers > 15 or input.key > 17 or input.key == 0 and not _scalar(input.codepoint) or input.key == 15 and input.value > 255 or input.unshifted and not _scalar(input.unshifted.value()):
        raise Error("chrome input cannot represent a source key")
    if input.text and input.text.value().byte_length() > MAX_CHROME_STRING_BYTES:
        raise Error("chrome input text exceeds byte limit")


def chrome_key_identity(input: KeyInput) raises -> RouterKey:
    _validate_input(input)
    var payload = UInt64(input.codepoint) if input.key == 0 else UInt64(input.value) if input.key == 15 or input.key == 17 else UInt64(0)
    return RouterKey((UInt64(input.key) << 32) | payload)


@fieldwise_init
struct ChromeLookups(Copyable, Movable):
    var version: UInt64
    var key: RouterKey
    var unshifted: Optional[RouterKey]
    var actions: List[Optional[RouterAction]]
    var ui_binds: Bool
    var terminal_binds: Bool


struct ChromeKeymap(Movable):
    var _bindings: List[ChromeBinding]
    var _catalog: ChromeActions
    var _scope: UInt32
    var _revision: UInt32

    def __init__(out self, profile: UInt8 = 0, scope: UInt32 = 1) raises:
        if profile > 2:
            raise Error("invalid chrome profile")
        self._bindings = List[ChromeBinding]()
        self._catalog = ChromeActions()
        self._scope = scope
        self._revision = 0
        var source = String(CHROME_PROFILE_DEFAULTS)
        for line in source.split("\n"):
            var fields = List[String]()
            var text = String(line)
            for field in text.split("\t"):
                fields.append(String(field))
            if len(fields) != 4:
                raise Error("invalid chrome default catalog")
            if fields[0] == String(profile):
                var code: UInt32 = 0
                for byte in fields[3].as_bytes():
                    code = code * 10 + UInt32(byte - 48)
                var action = self._catalog.decode(code)
                if not action:
                    raise Error("invalid chrome default action")
                self._install(fields[1], fields[2], action.value())
        self._revision = 0

    def version(self) -> UInt64:
        return (UInt64(self._scope) << 32) | UInt64(self._revision)

    def binds(self, table: String) raises -> Bool:
        if table.byte_length() > MAX_CHROME_STRING_BYTES:
            raise Error("chrome table name exceeds byte limit")
        for value in self._bindings:
            if value.table == table:
                return True
        return False

    def _get(self, table: String, key: String) raises -> Optional[ChromeAction]:
        var canonical = canonical_protocol_key(key)
        for value in self._bindings:
            if value.table == table and value.key == canonical:
                return value.action
        return None

    def _install(mut self, table: String, key: String, action: ChromeAction) raises:
        var same = False
        var existing = False
        var bytes = 0
        var tables = 0
        var previous: Optional[String] = None
        for value in self._bindings:
            if not previous or previous.value() != value.table:
                tables += 1
                previous = value.table.copy()
            if value.table == table and value.key == key:
                same = value.action == action
                existing = True
            else:
                bytes += value.table.byte_length() + value.key.byte_length()
        if same:
            return
        bytes += table.byte_length() + key.byte_length()
        if not self.binds(table):
            tables += 1
        if bytes > MAX_CHROME_RETAINED_BYTES or len(self._bindings) + (0 if existing else 1) > MAX_CHROME_BINDINGS or tables > MAX_CHROME_TABLES:
            raise Error("chrome binding exceeds retained budget")
        if self._revision == UInt32.MAX:
            raise Error("chrome map revision exhausted")
        var values = List[ChromeBinding]()
        var inserted = False
        for value in self._bindings:
            if value.table == table and value.key == key:
                if not inserted:
                    values.append(ChromeBinding(table.copy(), key.copy(), action))
                    inserted = True
            else:
                if not inserted and _before(table, key, value):
                    values.append(ChromeBinding(table.copy(), key.copy(), action))
                    inserted = True
                values.append(value.copy())
        if not inserted:
            values.append(ChromeBinding(table.copy(), key.copy(), action))
        self._bindings = values^
        self._revision += 1

    def bind(mut self, table: String, key: String, name: String) raises -> Optional[String]:
        if table.byte_length() > MAX_CHROME_STRING_BYTES:
            raise Error("chrome table name exceeds byte limit")
        var action = self._catalog.from_name(name)
        if not action:
            return name.copy()
        self._install(table, _stored_key(key), action.value())
        return None

    def unbind(mut self, table: String, key: String) raises -> Bool:
        if table.byte_length() > MAX_CHROME_STRING_BYTES:
            raise Error("chrome table name exceeds byte limit")
        var canonical = _stored_key(key)
        var found = False
        for value in self._bindings:
            found = found or value.table == table and value.key == canonical
        if not found:
            return False
        if self._revision == UInt32.MAX:
            raise Error("chrome map revision exhausted")
        var values = List[ChromeBinding]()
        for value in self._bindings:
            if value.table != table or value.key != canonical:
                values.append(value.copy())
        self._bindings = values^
        self._revision += 1
        return True

    def action_for(self, table: String, key: String) raises -> Optional[ChromeAction]:
        if table.byte_length() > MAX_CHROME_STRING_BYTES:
            raise Error("chrome table name exceeds byte limit")
        var parsed = chrome_key(key)
        if not parsed:
            return None
        return self._get(table, parsed.value().display())

    def _pressed(self, input: KeyInput, shifted: Bool) raises -> String:
        var bare = input.copy()
        bare.modifiers = 0
        bare.text = None
        var base = input_key_name(bare)
        if base.byte_length() == 0 and input.text:
            base = input.text.value().copy()
        return ChromeKey((input.modifiers & 14) | UInt8(shifted), base^).normalized().display()

    def resolve(self, table: String, input: KeyInput) raises -> Optional[ChromeAction]:
        _validate_input(input)
        if not self.binds(table):
            return None
        if input.modifiers & 1 != 0:
            var shifted = self._get(table, self._pressed(input, True))
            if shifted:
                return shifted
        if input.modifiers & 8 != 0:
            return self._get(table, self._pressed(input, False))
        if input.modifiers & 14 == 0 and input.text:
            var single = 0
            var control = False
            for point in input.text.value().codepoints():
                single += 1
                control = control or _control(UInt32(Int(point)))
            if single == 1 and not control:
                var typed = self._get(table, input.text.value())
                if typed:
                    return typed
        var named = self._get(table, input_key_name(input))
        return named if named else self._get(table, "Any")

    def bindings(self) -> List[ChromeBinding]:
        return self._bindings.copy()

    def table_bindings(self, table: String) raises -> List[ChromeTableBinding]:
        if table.byte_length() > MAX_CHROME_STRING_BYTES:
            raise Error("chrome table name exceeds byte limit")
        var values = List[ChromeTableBinding]()
        for value in self._bindings:
            if value.table == table:
                var key = chrome_key(value.key)
                if key:
                    values.append(ChromeTableBinding(key.take(), value.action))
        return values^

    def action_reference(self, action: ChromeAction) raises -> RouterAction:
        if not self._catalog.decode(action.code()):
            raise Error("invalid chrome action identity")
        return RouterAction((UInt64(self._scope) << 32) | UInt64(action.code()))

    def action_from_reference(self, action: RouterAction) raises -> ChromeAction:
        if action.id >> 32 != UInt64(self._scope):
            raise Error("chrome action belongs to another map scope")
        var parsed = self._catalog.decode(UInt32(action.id & 0xFFFFFFFF))
        if not parsed:
            raise Error("invalid chrome action identity")
        return parsed.value()

    def action_arena(self) raises -> RouterMapArena:
        var references = List[RouterAction]()
        var bodies = List[List[UInt8]]()
        for action in self._catalog.all():
            references.append(self.action_reference(action))
            var body: List[UInt8] = [action.kind, action.index]
            var name = self._catalog.name(action)
            body.extend(name.as_bytes())
            bodies.append(body^)
        return RouterMapArena(self.version(), references^, bodies^)

    def lookups(self, input: KeyInput) raises -> ChromeLookups:
        var key = chrome_key_identity(input)
        var unshifted: Optional[RouterKey] = None
        if input.unshifted:
            unshifted = RouterKey(UInt64(input.unshifted.value()))
        var actions = List[Optional[RouterAction]]()
        for table in ["ui", "sidebar", "terminal", "browser"]:
            var action = self.resolve(String(table), input)
            var reference: Optional[RouterAction] = None
            if action:
                reference = self.action_reference(action.value())
            actions.append(reference)
        return ChromeLookups(self.version(), key, unshifted, actions^, self.binds("ui"), self.binds("terminal"))

    def key_operation(self, state: RouterState, input: KeyInput, payload: RouterPayload, armed: Bool, claimed: Bool) raises -> RouterOperation:
        var resolved = self.lookups(input)
        if resolved.version != state.keymap:
            raise Error("chrome lookup belongs to another installed map version")
        var operation = RouterOperation(3)
        operation.input = RouterInput(resolved.key, resolved.unshifted.copy(), input.action, payload)
        operation.armed = armed
        operation.claimed = claimed
        operation.lookups = resolved.actions.copy()
        return operation^
