from std.ffi import external_call
from std.sys import argv
from std.sys.info import CompilationTarget
from zzmojo.client_files import ClientFiles
from zzmojo.exec_context import capture_exec_context
from zzmojo.exec_protocol import (
    split_invocations,
    Invocation,
    EXEC_UTF8,
    EXEC_RESUME,
    EXEC_NESTED,
)
from zzmojo.exec_client import execute_connected
from zzmojo.cli_output import stderr_line, write_bytes
from zzmojo.protocol import parse_id, PROTOCOL_VERSION
from zzmojo.cli_parse import (
    parse_globals,
    implicit_tmux_conflict,
    terminal_feature_mask,
    CLI_VERSION,
    FOREIGN_TMUX_ERROR,
)
from zzmojo.cli_native import (
    local_default_socket,
    environment_value,
    label_socket,
    connect_error,
    parse_connect_errno,
)
from zzmojo.cli_help import CLI_HELP
from zzmojo.cli_startup import prepare_startup
from zzmojo.cli_application import launch_resume
from zzmojo.cli_default_startup import (
    default_client_commands,
    fallback_commands,
)
from zzmojo.cli_attach import parse_attach


def _line(message: String, code: UInt8) -> UInt8:
    stderr_line(message.as_bytes())
    return code


def _unsupported(name: String) -> UInt8:
    return _line("zz: " + name + " is not implemented by this client", 1)


def _run(var files: ClientFiles) raises -> UInt8:
    var args = argv()
    var words = List[List[UInt8]]()
    for index in range(1, len(args)):
        words.append(List[UInt8](args[index].as_bytes()))
    for word in words:
        var flag = String(from_utf8_lossy=Span(word))
        if flag == "--":
            break
        if (
            flag == "--verbose"
            or flag == "--zz-verbose-log"
            or flag.startswith("--zz-verbose-log=")
        ):
            return _unsupported("verbose diagnostics")
    var path_bytes = local_default_socket()
    var parsed = parse_globals(words, Span(path_bytes))
    if parsed.error_kind != 0:
        return _line(
            ("zz: " if parsed.error_kind == 1 else "") + parsed.error,
            UInt8(2 if parsed.error_kind == 1 else 1),
        )
    if parsed.early:
        var output = parsed.early.value() + "\n"
        _ = write_bytes(1, output.as_bytes())
        return 0
    if parsed.socket_source == 2:
        try:
            path_bytes = label_socket(
                Span(parsed.socket), environment_value("TMUX_TMPDIR")
            )
        except error:
            return _line(String(error), 1)
    else:
        path_bytes = parsed.socket.copy()
    var path: String
    try:
        path = String(from_utf8=Span(path_bytes))
    except:
        return _line(
            "zz: non-UTF-8 socket paths are not implemented by this client", 1
        )
    var conflict = implicit_tmux_conflict(
        parsed.socket_source,
        environment_value("ZZ_SOCKET"),
        environment_value("TMUX"),
    )
    if parsed.control != 0 and not parsed.shell:
        if conflict and not parsed.host:
            return _line(String(FOREIGN_TMUX_ERROR), 1)
        return _unsupported("control mode")
    if parsed.shell:
        if conflict and not parsed.host:
            return _line(String(FOREIGN_TMUX_ERROR), 1)
        return _unsupported("shell startup")
    var commands = split_invocations(Span(parsed.remaining))
    if len(commands) == 0:
        if conflict and not parsed.host:
            return _line(String(FOREIGN_TMUX_ERROR), 1)
        if parsed.host:
            return _unsupported("remote host routing")
        commands = default_client_commands(
            path, parsed.configs, parsed.no_start, parsed.socket_source
        )
    var application = False
    var command = commands[0].name.copy()
    if command == "--help" or command == "help":
        _ = write_bytes(1, String(CLI_HELP).as_bytes())
        return 0
    for argument in commands[0].args:
        var text = String(from_utf8_lossy=Span(argument))
        if text == "--":
            break
        if text == "--help":
            return _unsupported("command help")
    if command == "app":
        if parsed.host or len(commands[0].args) > 0 or len(commands) != 1:
            return _line("zz: usage: zz app", 2)
        application = True
        commands = fallback_commands()
        command = "new-session"
    if parsed.host:
        if command == "daemon" or command == "proxy":
            return _line("zz: --host is not supported for `" + command + "`", 2)
        return _unsupported("remote host routing")
    if command == "--version":
        _ = write_bytes(1, String(CLI_VERSION, "\n").as_bytes())
        return 0
    if command == "protocol-version":
        if parsed.socket_source != 0 or len(commands[0].args) != 0:
            return _line("zz: usage: zz protocol-version", 2)
        _ = write_bytes(1, String(PROTOCOL_VERSION, "\n").as_bytes())
        return 0
    if (
        command == "daemon"
        or command == "proxy"
        or command == "fleet"
        or command == "list-hosts"
        or command == "tools"
        or command == "events"
    ):
        return _unsupported(command + " frontend")
    if conflict and not application:
        return _line(String(FOREIGN_TMUX_ERROR), 1)
    if command == "--kill-server":
        var first = commands[0].copy()
        first.name = "kill-server"
        commands.clear()
        commands.append(first^)
    if path.byte_length() == 0:
        return _line(connect_error(path, 22), 1)
    comptime PATH_LIMIT = 108 if CompilationTarget.is_linux() else 104
    if path.byte_length() >= PATH_LIMIT:
        return _line(
            "zz: socket path exceeds this client's Unix socket limit", 1
        )
    var startup = prepare_startup(
        path,
        Span(commands),
        parsed.configs,
        parsed.no_start,
        parsed.socket_source,
    )
    if startup.error:
        return _line(startup.error.value(), startup.code)
    var origin: Optional[UInt64] = None
    var pane = environment_value("ZZ_PANE")
    if pane and len(pane.value()) > 1 and pane.value()[0] == 37:
        try:
            origin = parse_id(String(from_utf8=Span(pane.value())), 37)
        except:
            pass
    var request = capture_exec_context(origin)
    request.commands = commands^
    request.spawned_server = startup.spawned
    request.flags |= EXEC_RESUME
    if application:
        request.flags &= ~EXEC_NESTED
        request.origin = None
    if parsed.utf8:
        request.flags |= EXEC_UTF8
    request.features = terminal_feature_mask(parsed.features)
    try:
        var result = execute_connected(
            startup.connection.take(), request, files^
        )
        if result.exit.outcome == 1:
            if not result.connection:
                raise Error(
                    "native resume did not retain its command connection"
                )
            var empty_attach = False
            if result.exit.resume_kind == 1:
                var attached = parse_attach(Span(request.commands[0].args))
                if attached.error:
                    return _line(attached.error.value(), attached.code)
                if attached.restart:
                    return _unsupported("daemon restart")
                empty_attach = (
                    attached.default_attach and len(request.commands) == 1
                )
                result.exit.commands[0].invocation = attached.command.copy()
            var status = launch_resume(
                path, result.connection.value(), result.exit, empty_attach
            )
            return status if status != 0 else result.code
        return result.code
    except error:
        var message = String(error)
        var number = parse_connect_errno(message)
        if number:
            return _line(connect_error(path, number.value()), 1)
        raise Error(message)


def run() raises:
    var files = ClientFiles.capture_stdin()
    var code: UInt8
    try:
        code = _run(files^)
    except error:
        code = _line("zz: " + String(error), 1)
    if code != 0:
        external_call["exit", NoneType](Int32(code))
