from .exec_protocol import Invocation
from .cli_startup import prepare_startup
from .transport import connect
from .cli_default import parse_default
from .cli_default_home import resolve_home


def fallback_commands() -> List[Invocation]:
    var command = Invocation()
    command.name = "new-session"
    command.args = [List[UInt8](String("-A").as_bytes())]
    var commands = List[Invocation]()
    commands.append(command^)
    return commands^


def _stored_default(
    path: String,
    configs: List[List[UInt8]],
    no_start: Bool,
    socket_source: UInt8,
) raises -> Optional[String]:
    try:
        var fallback = fallback_commands()
        var startup = prepare_startup(
            path, Span(fallback), configs, no_start, socket_source
        )
        if startup.error:
            return None
        var connection = connect(path)
        var args: List[String] = ["-sqv", "default-client-command"]
        var response = connection.command(
            "show-options", Span(args), cli_errors=True
        )
        _ = startup^
        if response.exit_code != 0:
            return None
        while (
            len(response.output) > 0
            and response.output[len(response.output) - 1] == 10
        ):
            _ = response.output.pop()
        return String(from_utf8_lossy=Span(response.output))
    except:
        return None


def default_client_commands(
    path: String,
    configs: List[List[UInt8]],
    no_start: Bool,
    socket_source: UInt8,
) raises -> List[Invocation]:
    var text = _stored_default(path, configs, no_start, socket_source)
    if not text:
        return fallback_commands()
    var home: Optional[String] = None
    if "~" in text.value():
        home = resolve_home()
    var homes = Dict[String, Optional[String]]()
    var total = 0
    while True:
        var parsed = parse_default(text.value(), home, homes)
        if parsed.kind == 1 or (parsed.kind == 0 and len(parsed.commands) == 0):
            return fallback_commands()
        if parsed.kind == 0:
            if (
                len(parsed.commands) == 1
                and parsed.commands[0].name == "new-session"
                and len(parsed.commands[0].args) == 0
            ):
                return fallback_commands()
            var commands = List[Invocation]()
            swap(commands, parsed.commands)
            return commands^
        if not parsed.needed_home:
            raise Error("zz: " + parsed.error)
        var name = parsed.needed_home.take()
        if name in homes or len(homes) >= 4096:
            raise Error(
                "zz: default command home lookup exceeds its entry limit"
            )
        var value = resolve_home(name.copy())
        var size = name.byte_length() + (
            value.value().byte_length() if value else 0
        )
        if size > 16 * 1024 * 1024 - total:
            raise Error(
                "zz: default command home lookup exceeds its byte limit"
            )
        total += size
        homes[name] = value^
