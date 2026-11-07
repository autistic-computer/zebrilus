from std.sys import argv
from std.testing import assert_true
from zzmojo.appearance_protocol import (
    decode_appearance,
    encode_appearance,
    AppearanceUpdate,
    TerminalAppearance,
)


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    if args[1] == "default":
        var provenance = Dict[UInt8, UInt8]()
        for index in range(30):
            provenance[UInt8(index)] = 0
        bytes = encode_appearance(
            AppearanceUpdate(UInt64.MAX, TerminalAppearance(), provenance^)
        )
    else:
        with open(args[2], "r") as source:
            bytes = source.read_bytes()
        if args[1] == "reject":
            var rejected = False
            try:
                _ = decode_appearance(bytes^)
            except:
                rejected = True
            assert_true(rejected)
            return
        var event = decode_appearance(bytes^)
        assert_true(Bool(event))
        bytes = encode_appearance(event.value())
    with open(args[3], "w") as destination:
        destination.write_bytes(Span(bytes))
