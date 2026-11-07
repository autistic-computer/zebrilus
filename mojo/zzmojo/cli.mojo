from std.ffi import external_call
from std.os import getenv
from std.sys import argv
from zzmojo.client_files import ClientFiles
from zzmojo.exec_context import capture_exec_context
from zzmojo.exec_protocol import split_invocations
from zzmojo.exec_client import execute_local
from zzmojo.cli_output import stderr_line
from zzmojo.protocol import parse_id


def run() raises:
    var files = ClientFiles.capture_stdin()
    var args = argv()
    var path = getenv("ZZ_SOCKET", "")
    var offset = 1
    if len(args) > 2 and args[1] == "--socket":
        path = args[2]
        offset = 3
    if path.byte_length() == 0 or offset >= len(args):
        var usage = String("usage: zz-mojo --socket PATH COMMAND [ARG ...]")
        stderr_line(usage.as_bytes())
        external_call["exit", NoneType](Int32(2))
        return
    var origin: Optional[UInt64] = None
    var pane = getenv("ZZ_PANE", "")
    if pane.byte_length() > 1 and pane.as_bytes()[0] == 37:
        try:
            origin = parse_id(pane, 37)
        except:
            pass
    var request = capture_exec_context(origin)
    var words = List[List[UInt8]]()
    for index in range(offset, len(args)):
        words.append(List[UInt8](args[index].as_bytes()))
    request.commands = split_invocations(Span(words))
    var result = execute_local(path, request, files^)
    if result.exit.outcome == 1:
        var message = String("native execution requires an interactive resume")
        stderr_line(message.as_bytes())
        external_call["exit", NoneType](Int32(1))
    elif result.code != 0:
        external_call["exit", NoneType](Int32(result.code))
