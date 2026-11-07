comptime TMUX_USAGE = (
    "usage: zz [-2CDhlNuVv] [-c shell-command] [-f file] [-L socket-name]\n    "
    "        [-S socket-path] [-T features] [command [flags]]"
)
comptime TMUX_VERSION = "tmux 3.8-zz"
comptime CLI_VERSION = "zz 0.22.0"
comptime FOREIGN_TMUX_ERROR = (
    "zz: TMUX is set but ZZ_SOCKET is not; refusing to treat a tmux server as"
    " zz\nUse `zz app` to open the GUI, or pass `-S` / set `ZZ_SOCKET` to"
    " target a zz daemon."
)
comptime MAX_CLI_WORDS = 4096
comptime MAX_CLI_BYTES = 4 * 1024 * 1024


struct CliArguments(Copyable, Movable):
    var socket: List[UInt8]
    var socket_source: UInt8
    var host: Optional[String]
    var remaining: List[List[UInt8]]
    var configs: List[List[UInt8]]
    var no_start: Bool
    var control: UInt8
    var shell: Optional[String]
    var login: Bool
    var early: Optional[String]
    var utf8: Bool
    var features: List[String]
    var error_kind: UInt8
    var error: String

    def __init__(out self, path: Span[UInt8, _]):
        self.socket = List[UInt8](path)
        self.socket_source = 0
        self.host = None
        self.remaining = List[List[UInt8]]()
        self.configs = List[List[UInt8]]()
        self.no_start = False
        self.control = 0
        self.shell = None
        self.login = False
        self.early = None
        self.utf8 = False
        self.features = List[String]()
        self.error_kind = 0
        self.error = String()

    def fail(mut self, kind: UInt8, message: String):
        self.error_kind = kind
        self.error = message


def application_words(words: List[List[UInt8]]) raises -> List[List[UInt8]]:
    if len(words) > MAX_CLI_WORDS:
        raise Error("command line exceeds argument limit")
    var total = 0
    for word in words:
        if len(word) > MAX_CLI_BYTES - total:
            raise Error("command line exceeds byte limit")
        total += len(word)
    var output = List[List[UInt8]]()
    var index = 0
    while index < len(words):
        var text = String(from_utf8_lossy=Span(words[index]))
        if text == "--":
            for remaining in range(index, len(words)):
                output.append(words[remaining].copy())
            break
        if text == "--verbose":
            index += 1
            continue
        if text == "--zz-verbose-log":
            index += 2
            continue
        if text.startswith("--zz-verbose-log="):
            index += 1
            continue
        output.append(words[index].copy())
        index += 1
        if text == "--bootstrap-client-cwd" and index < len(words):
            output.append(words[index].copy())
            index += 1
    return output^


def parse_globals(
    words: List[List[UInt8]], default_path: Span[UInt8, _]
) raises -> CliArguments:
    var input = application_words(words)
    var result = CliArguments(default_path)
    var parsing = True
    var verb_socket = False
    var foreground = False
    var index = 0
    while index < len(input):
        var text = String(from_utf8_lossy=Span(input[index]))
        var socket_flag = parsing or verb_socket
        index += 1
        if socket_flag and text == "--socket":
            if index >= len(input):
                result.fail(1, "--socket requires a path")
                return result^
            result.socket = input[index].copy()
            index += 1
            if len(result.socket) == 0:
                result.fail(1, "--socket requires a non-empty path")
                return result^
            result.socket_source = 1
        elif socket_flag and text.startswith("--socket="):
            result.socket = List[UInt8](text.as_bytes()[9:])
            if len(result.socket) == 0:
                result.fail(1, "--socket requires a non-empty path")
                return result^
            result.socket_source = 1
        elif parsing and text == "--host":
            if index >= len(input):
                result.fail(1, "--host requires a name")
                return result^
            var name = String(from_utf8_lossy=Span(input[index]))
            index += 1
            if name.byte_length() == 0:
                result.fail(1, "--host requires a non-empty name")
                return result^
            result.host = name^
        elif parsing and text.startswith("--host="):
            var name = String(from_utf8=text.as_bytes()[7:])
            if name.byte_length() == 0:
                result.fail(1, "--host requires a non-empty name")
                return result^
            result.host = name^
        elif parsing and text == "--":
            parsing = False
        elif parsing and (
            text == "--version" or text == "--kill-server" or text == "--help"
        ):
            parsing = False
            result.remaining.append(input[index - 1].copy())
        elif parsing and text.startswith("--"):
            result.fail(3, String(TMUX_USAGE))
            return result^
        elif parsing and text.startswith("-") and text != "-":
            var position = 1
            var options = String(from_utf8=text.as_bytes()[1:])
            for character in options.codepoint_slices():
                var option = String(character)
                position += option.byte_length()
                var takes_value = (
                    option == "c"
                    or option == "f"
                    or option == "L"
                    or option == "S"
                    or option == "T"
                )
                var value = List[UInt8]()
                if takes_value:
                    if position < text.byte_length():
                        value = List[UInt8](text.as_bytes()[position:])
                    elif index < len(input):
                        value = input[index].copy()
                        index += 1
                    else:
                        result.fail(
                            2,
                            String(
                                "zz: option requires an argument -- ",
                                option,
                                "\n",
                                TMUX_USAGE,
                            ),
                        )
                        return result^
                if option == "2":
                    result.features.append("256")
                elif option == "q" or option == "v":
                    pass
                elif option == "u":
                    result.utf8 = True
                elif option == "c":
                    result.shell = String(from_utf8_lossy=Span(value))
                elif option == "C":
                    if result.control < 255:
                        result.control += 1
                elif option == "D":
                    foreground = True
                elif option == "f":
                    result.configs.append(value.copy())
                elif option == "h" or option == "V":
                    result = CliArguments(default_path)
                    result.early = String(
                        TMUX_USAGE if option == "h" else TMUX_VERSION
                    )
                    return result^
                elif option == "l":
                    result.login = True
                elif option == "L" or option == "S":
                    result.socket = (
                        List[UInt8](
                            String(from_utf8_lossy=Span(value)).as_bytes()
                        ) if option
                        == "L" else value.copy()
                    )
                    result.socket_source = 2 if option == "L" else 1
                elif option == "N":
                    result.no_start = True
                elif option == "T":
                    result.features.append(String(from_utf8_lossy=Span(value)))
                else:
                    result.fail(
                        2,
                        String(
                            "zz: unknown option -- ", option, "\n", TMUX_USAGE
                        ),
                    )
                    return result^
                if takes_value:
                    break
        else:
            if parsing:
                verb_socket = (
                    text == "daemon" or text == "proxy" or text == "app"
                )
            elif text == "--":
                verb_socket = False
            parsing = False
            result.remaining.append(input[index - 1].copy())
    if (result.shell or foreground) and len(result.remaining) > 0:
        result.fail(3, String(TMUX_USAGE))
    elif foreground:
        result.fail(
            1, "-D foreground server mode is not supported; use `zz daemon`"
        )
    elif result.host and result.host.value().startswith("-"):
        result.fail(
            1,
            (
                "--host takes a host name or ssh destination, and it must not"
                " start with `-`"
            ),
        )
    elif result.socket_source != 0 and result.host:
        result.fail(1, "--host cannot be used together with a socket selector")
    return result^


def implicit_tmux_conflict(
    source: UInt8, socket: Optional[List[UInt8]], tmux: Optional[List[UInt8]]
) -> Bool:
    return (
        source == 0
        and (not socket or len(socket.value()) == 0)
        and Bool(tmux)
        and len(tmux.value()) > 0
    )


def path_join(base: Span[UInt8, _], child: Span[UInt8, _]) -> List[UInt8]:
    if len(child) > 0 and child[0] == 47:
        return List[UInt8](child)
    var result = List[UInt8](base)
    if len(result) > 0 and result[len(result) - 1] != 47:
        result.append(47)
    result.extend(child)
    return result^


def default_socket(
    socket: Optional[List[UInt8]],
    runtime: Optional[List[UInt8]],
    temporary: Span[UInt8, _],
    user: Optional[String],
    directory: String = "zz",
) -> List[UInt8]:
    if socket:
        return socket.value().copy()
    var base: List[UInt8]
    if runtime:
        base = path_join(Span(runtime.value()), directory.as_bytes())
    else:
        var name = directory + "-" + user.or_else(String("user"))
        base = path_join(temporary, name.as_bytes())
    return path_join(Span(base), String("default.sock").as_bytes())


def terminal_feature_mask(specs: List[String]) raises -> UInt32:
    var names: List[String] = [
        "256",
        "appesc",
        "bpaste",
        "ccolour",
        "clipboard",
        "hyperlinks",
        "cstyle",
        "extkeys",
        "focus",
        "ignorefkeys",
        "margins",
        "mouse",
        "osc7",
        "overline",
        "progressbar",
        "rectfill",
        "RGB",
        "sixel",
        "strikethrough",
        "sync",
        "title",
        "usstyle",
        "utf8",
    ]
    var features: UInt32 = 0
    var removed: UInt32 = 0
    var count = 0
    for spec in specs:
        if spec.byte_length() > 200:
            continue
        if count == 16:
            break
        count += 1
        var start = 0
        for end in range(spec.byte_length() + 1):
            if (
                end != spec.byte_length()
                and spec.as_bytes()[end] != 58
                and spec.as_bytes()[end] != 44
            ):
                continue
            var name = String(from_utf8=spec.as_bytes()[start:end])
            start = end + 1
            var remove = name.byte_length() > 1 and name.endswith("@")
            if remove:
                var trimmed = String(
                    from_utf8=name.as_bytes()[: name.byte_length() - 1]
                )
                name = trimmed^
            var folded = List[UInt8](name.as_bytes())
            for position in range(len(folded)):
                if folded[position] >= 65 and folded[position] <= 90:
                    folded[position] += 32
            var normalized = String(from_utf8=Span(folded))
            var bit: UInt32 = 0
            for index in range(len(names)):
                if normalized == names[index].lower():
                    bit = UInt32(1) << UInt32(index)
                    break
            if bit == 0:
                break
            if remove:
                features &= ~bit
                removed |= bit
            elif removed & bit == 0:
                features |= bit
    return features
