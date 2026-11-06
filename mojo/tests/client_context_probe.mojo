from std.sys import argv
from std.testing import assert_equal
from zzmojo.client_context import ClientContext
from zzmojo.protocol import Decoder, client_hello
from zzmojo.stream_protocol import interactive_hello


def main() raises:
    var args = argv()
    var input: List[UInt8]
    with open(args[2], "r") as source:
        input = source.read_bytes()
    var reader = Decoder(input^)
    var context = ClientContext()
    if reader.byte() == 1:
        context.cwd = reader.bytes()
    var count = Int(reader.uint())
    for _ in range(count):
        context.environment.append(reader.bytes())
    reader.finish()
    var output: List[UInt8]
    var rejected = False
    try:
        if args[1] == "command":
            output = client_hello(UInt64.MAX, UInt32.MAX, UInt64.MAX, context^)
        else:
            output = interactive_hello(
                UInt64.MAX,
                UInt32.MAX,
                "$7",
                origin=UInt64.MAX,
                options=(UInt64(1) << 21) - 1,
                viewport=False,
                context=context^,
            )
    except:
        rejected = True
        output = List[UInt8]()
    assert_equal(rejected, args[4] == "reject")
    if not rejected:
        with open(args[3], "w") as destination:
            destination.write_bytes(Span(output))
