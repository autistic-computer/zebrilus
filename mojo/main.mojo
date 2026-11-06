from std.ffi import external_call
from std.os import getenv
from std.sys import argv, stderr
from zzmojo.transport import connect
from zzmojo.protocol import parse_id


def main() raises:
    var args = argv()
    var errors = stderr
    var path = getenv("ZZ_SOCKET", "")
    var offset = 1
    if len(args) > 2 and args[1] == "--socket":
        path = args[2]
        offset = 3
    if path.byte_length() == 0 or offset >= len(args):
        errors.write("usage: zz-mojo --socket PATH COMMAND [ARG ...]\n")
        external_call["exit", NoneType](Int32(2))
        return
    var origin: Optional[UInt64] = None
    var pane = getenv("ZZ_PANE", "")
    if pane.byte_length() > 1 and pane.as_bytes()[0] == 37:
        try:
            origin = parse_id(pane, 37)
        except:
            pass
    var client = connect(path, origin=origin)
    var arguments = List[String]()
    for index in range(offset + 1, len(args)):
        arguments.append(args[index])
    var response = client.command(args[offset], Span(arguments))
    var written = 0
    while written < len(response.output):
        var count = external_call["write", Int](
            Int(1),
            response.output.unsafe_ptr().unsafe_offset(written),
            len(response.output) - written,
        )
        if count <= 0:
            raise Error("stdout write failed")
        written += count
    if (
        len(response.output) > 0
        and response.stdout_claim != 2
        and response.output[len(response.output) - 1] != 10
    ):
        var newline: UInt8 = 10
        _ = external_call["write", Int](Int(1), Pointer(to=newline), 1)
    if response.error.byte_length() > 0:
        errors.write(response.error + "\n")
    if response.exit_code != 0:
        external_call["exit", NoneType](Int32(response.exit_code))
