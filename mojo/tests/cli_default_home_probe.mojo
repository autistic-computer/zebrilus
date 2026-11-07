from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.cli_default_home import resolve_home


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 1024:
        raise Error("home fixtures exceed their item limit")
    var output = List[UInt8]()
    for _ in range(Int(count)):
        var user: Optional[String] = None
        if reader.uint() != 0:
            user = reader.text()
        var home = resolve_home(user)
        append_uint(output, UInt64(Bool(home)))
        if home:
            append_text(output, home.value())
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
