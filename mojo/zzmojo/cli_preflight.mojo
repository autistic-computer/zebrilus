from .cli_catalog import CLI_CATALOG
from .exec_protocol import Invocation
from .cli_parse import MAX_CLI_WORDS, MAX_CLI_BYTES


struct CliPreflight(Copyable, Movable):
    var canonical: String
    var starts_server: Bool
    var error: Optional[String]
    var code: UInt8

    def __init__(out self):
        self.canonical = String()
        self.starts_server = False
        self.error = None
        self.code = 0

    def fail(mut self, message: String, code: UInt8 = 1):
        self.error = message
        self.code = code


@fieldwise_init
struct CatalogEntry(Copyable, Movable):
    var name: String
    var aliases: String
    var native: Bool
    var minimum: Int
    var maximum: Int
    var usage: String
    var options: String


@fieldwise_init
struct CatalogOption(Copyable, Movable):
    var name: String
    var flags: UInt8


def _integer(text: StringSpan) raises -> Int:
    if text == "-1":
        return -1
    var number = 0
    if text.byte_length() == 0 or text.byte_length() > 5:
        raise Error("invalid command catalog integer")
    for value in text.as_bytes():
        if value < 48 or value > 57:
            raise Error("invalid command catalog integer")
        number = number * 10 + Int(value - 48)
    return number


@inline(.never)
def _catalog() raises -> List[CatalogEntry]:
    var entries = List[CatalogEntry]()
    var data = String(CLI_CATALOG)
    if data.byte_length() > 128 * 1024:
        raise Error("command catalog exceeds limit")
    for line in data.split("\n"):
        var fields = line.split("\t")
        if len(fields) != 7 or len(entries) >= 512:
            raise Error("invalid command catalog record")
        entries.append(CatalogEntry(String(fields[0]), String(fields[1]), fields[2] == "1", _integer(fields[3]), _integer(fields[4]), String(fields[5]), String(fields[6])))
    return entries^


@inline(.never)
def _resolve(entries: List[CatalogEntry], name: String, mut result: CliPreflight) -> Int:
    if name.byte_length() == 0:
        result.fail("unknown command: " + name)
        return -1
    for index in range(len(entries)):
        if entries[index].name == name:
            return index
        for alias in entries[index].aliases.split(","):
            if alias == name:
                return index
    for native in range(2):
        var matches = 0
        var matched = -1
        var names = String()
        for index in range(len(entries)):
            if entries[index].native == Bool(native) and entries[index].name.startswith(name):
                if matches > 0:
                    names += ", "
                names += entries[index].name
                matches += 1
                matched = index
        if matches == 1:
            return matched
        if matches > 1:
            result.fail("ambiguous command: " + name + ", could be: " + names)
            return -1
    result.fail("unknown command: " + name)
    return -1


@inline(.never)
def _options(spec: CatalogEntry) raises -> List[CatalogOption]:
    var options = List[CatalogOption]()
    if spec.options.byte_length() == 0:
        return options^
    for value in spec.options.split(","):
        var parts = value.split(":")
        if len(parts) != 2 or len(options) >= 128:
            raise Error("invalid command catalog option")
        options.append(CatalogOption(String(parts[0]), UInt8(_integer(parts[1]))))
    return options^


def _option(options: List[CatalogOption], name: StringSpan) -> Int:
    for index in range(len(options)):
        if options[index].name == name:
            return index
    return -1


def _optional_starts_option(bytes: List[UInt8]) -> Bool:
    if len(bytes) < 2 or bytes[0] != 45:
        return False
    var value = bytes[1]
    return value == 45 or value >= 65 and value <= 90 or value >= 97 and value <= 122


def _option_code(options: List[CatalogOption], index: Int) -> UInt8:
    return UInt8(2 if index >= 0 and options[index].flags & 8 != 0 else 1)


@inline(.never)
def _validate(command: Invocation, spec: CatalogEntry, mut result: CliPreflight) raises:
    if spec.native:
        return
    var options = _options(spec)
    var index = 0
    var json_flag = False
    var format_value = False
    while index < len(command.args):
        var text = String(from_utf8_lossy=Span(command.args[index]))
        if not text.startswith("-") or text == "-":
            break
        index += 1
        if text == "--":
            break
        var long_option = _option(options, text)
        if text.startswith("--") and long_option >= 0:
            var flags = options[long_option].flags
            var available = index < len(command.args)
            if available and flags & 4 != 0:
                available = not _optional_starts_option(command.args[index])
            if flags & 3 != 0 and flags & 4 == 0 or flags & 4 != 0 and available:
                if not available:
                    result.fail("command " + spec.name + ": " + text + " expects an argument", _option_code(options, long_option))
                    return
                index += 1
            elif text == "--json":
                json_flag = True
            continue
        var position = 1
        var suffix = String(from_utf8=text.as_bytes()[1:])
        for character in suffix.codepoint_slices():
            var flag = String(character)
            position += flag.byte_length()
            if flag == "?":
                result.fail("usage: " + spec.name + " " + spec.usage)
                return
            var byte = flag.as_bytes()[0]
            if flag.byte_length() != 1 or not (byte >= 48 and byte <= 57 or byte >= 65 and byte <= 90 or byte >= 97 and byte <= 122):
                var extension = text.split("=")
                var extension_index = _option(options, extension[0]) if len(extension) > 1 else -1
                result.fail("command " + spec.name + ": invalid flag -" + flag, _option_code(options, extension_index))
                return
            var name = "-" + flag
            var option = _option(options, name)
            if option < 0:
                result.fail("command " + spec.name + ": unknown flag " + name)
                return
            var flags = options[option].flags
            var attached = position < text.byte_length()
            if flags & 4 != 0:
                if not attached and index < len(command.args) and not _optional_starts_option(command.args[index]):
                    index += 1
                break
            if flags & 3 != 0:
                if not attached:
                    if index >= len(command.args):
                        result.fail("command " + spec.name + ": " + name + " expects an argument", _option_code(options, option))
                        return
                    index += 1
                if name == "-F":
                    format_value = True
                break
    if json_flag and format_value:
        result.fail("command " + spec.name + ": --json cannot be combined with -F", 2)
        return
    var actual = len(command.args) - index
    if actual < spec.minimum:
        result.fail("command " + spec.name + ": too few arguments (need at least " + String(spec.minimum) + ")")
    elif spec.maximum >= 0 and actual > spec.maximum:
        result.fail("command " + spec.name + ": too many arguments (need at most " + String(spec.maximum) + ")")


@inline(.never)
def preflight(commands: Span[Invocation, _]) raises -> CliPreflight:
    var result = CliPreflight()
    if len(commands) == 0:
        return result^
    result.canonical = commands[0].name.copy()
    var entries = _catalog()
    var words = 0
    var bytes = 0
    for index in range(len(commands)):
        ref command = commands[index]
        words += 1 + len(command.args)
        bytes += command.name.byte_length()
        for arg in command.args:
            bytes += len(arg)
        if words > MAX_CLI_WORDS or bytes > MAX_CLI_BYTES:
            result.fail("command line exceeds preflight limit")
            return result^
        if len(command.blocks) != 0 or command.expanded_alias:
            result.fail("typed configuration blocks require a running server")
            return result^
        var resolved = _resolve(entries, command.name, result)
        if resolved < 0:
            return result^
        ref spec = entries[resolved]
        if index == 0:
            result.canonical = spec.name.copy()
            result.starts_server = spec.name == "attach-session" or spec.name == "list-commands" or spec.name == "list-keys" or spec.name == "new-session" or spec.name == "start-server"
        _validate(command, spec, result)
        if result.error:
            return result^
    return result^
