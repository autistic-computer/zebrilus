from .cli_attach_data import NATIVE_ATTACH_USAGE, TMUX_ATTACH_USAGE
from .cli_parse import MAX_CLI_WORDS, MAX_CLI_BYTES
from .exec_protocol import Invocation


struct NativeAttach(Copyable, Movable):
    var command: Invocation
    var restart: Bool
    var default_attach: Bool
    var error: Optional[String]
    var code: UInt8

    def __init__(out self):
        self.command = Invocation()
        self.command.name = "attach-session"
        self.restart = False
        self.default_attach = False
        self.error = None
        self.code = 0

    def fail(mut self, message: String, code: UInt8 = 1):
        self.error = message
        self.code = code


def _argument(mut command: Invocation, text: String):
    command.args.append(List[UInt8](text.as_bytes()))


def parse_attach(arguments: Span[List[UInt8], _]) -> NativeAttach:
    var result = NativeAttach()
    if len(arguments) > MAX_CLI_WORDS:
        result.fail("zz: attach arguments exceed their count limit", 2)
        return result^
    var size = 0
    for argument in arguments:
        if len(argument) > MAX_CLI_BYTES - size:
            result.fail("zz: attach arguments exceed their byte limit", 2)
            return result^
        size += len(argument)
    var detach = False
    var hangup = False
    var no_environment = False
    var read_only = False
    var flags: Optional[String] = None
    var directory: Optional[String] = None
    var target: Optional[String] = None
    var positional: Optional[String] = None
    var options_done = False
    var explicit_boundary = False
    var index = 0
    while index < len(arguments):
        var argument = String(from_utf8_lossy=Span(arguments[index]))
        index += 1
        if not explicit_boundary and argument == "--restart-daemon":
            if result.restart:
                result.fail(NATIVE_ATTACH_USAGE, 2)
                return result^
            result.restart = True
            continue
        if options_done or not argument.startswith("-") or argument == "-":
            if positional:
                result.fail(NATIVE_ATTACH_USAGE)
                return result^
            positional = argument
            options_done = True
            continue
        if argument == "--":
            options_done = True
            explicit_boundary = True
            continue
        if argument.startswith("--"):
            result.fail("command attach-session: invalid flag --")
            return result^
        var options = String(from_utf8_lossy=argument.as_bytes()[1:])
        var offset = 0
        for option in options.codepoint_slices():
            var name = "-" + String(option)
            if option == "?":
                result.fail(TMUX_ATTACH_USAGE)
                return result^
            var bytes = option.as_bytes()
            var character = bytes[0]
            if len(bytes) != 1 or not (character >= 48 and character <= 57 or character >= 65 and character <= 90 or character >= 97 and character <= 122):
                result.fail("command attach-session: invalid flag " + name)
                return result^
            if option == "d":
                detach = True
            elif option == "E":
                no_environment = True
            elif option == "r":
                read_only = True
            elif option == "x":
                hangup = True
            elif option == "t" or option == "c" or option == "f":
                var value: String
                var start = offset + option.byte_length()
                if start < options.byte_length():
                    value = String(from_utf8_lossy=options.as_bytes()[start:])
                elif index < len(arguments):
                    value = String(from_utf8_lossy=Span(arguments[index]))
                    index += 1
                else:
                    result.fail("command attach-session: " + name + " expects an argument")
                    return result^
                if option == "t":
                    target = value
                elif option == "c":
                    directory = value
                else:
                    flags = value
                break
            else:
                result.fail("command attach-session: unknown flag " + name)
                return result^
            offset += option.byte_length()
    if Bool(target) and Bool(positional):
        result.fail(NATIVE_ATTACH_USAGE)
        return result^
    var session = target.copy() if target else positional.copy()
    result.default_attach = not no_environment and not Bool(directory) and not hangup and not Bool(session)
    if detach:
        _argument(result.command, "-d")
    if hangup:
        _argument(result.command, "-x")
    if no_environment:
        _argument(result.command, "-E")
    if read_only:
        _argument(result.command, "-r")
    if flags:
        _argument(result.command, "-f")
        _argument(result.command, flags.value())
    if directory:
        _argument(result.command, "-c")
        _argument(result.command, directory.value())
    if session:
        _argument(result.command, "-t")
        _argument(result.command, session.value())
    return result^
