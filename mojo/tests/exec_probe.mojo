from std.sys import argv
from std.testing import assert_true, assert_equal
from zzmojo.exec_protocol import (
    decode_exec,
    encode_exec,
    decode_exec_exit,
    encode_exec_exit,
    split_invocations,
    decode_exec_notice,
    encode_exec_notice,
)
from zzmojo.protocol import Decoder


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[2], "r") as source:
        bytes = source.read_bytes()
    var rejected = False
    var output: List[UInt8]
    try:
        if args[1] == "notice":
            var notice = decode_exec_notice(bytes^)
            assert_true(Bool(notice))
            output = encode_exec_notice(notice.value())
        elif args[1] == "exit":
            var exit = decode_exec_exit(bytes^)
            assert_true(Bool(exit))
            output = encode_exec_exit(exit.value())
        elif args[1] == "words":
            var request = decode_exec(bytes^)
            assert_true(Bool(request))
            var words: List[UInt8]
            with open(args[5], "r") as source:
                words = source.read_bytes()
            var reader = Decoder(words^)
            var count = Int(reader.uint())
            var values = List[List[UInt8]]()
            for _ in range(count):
                values.append(reader.bytes())
            reader.finish()
            request.value().commands = split_invocations(Span(values))
            output = encode_exec(request.value())
        else:
            var request = decode_exec(bytes^)
            assert_true(Bool(request))
            output = encode_exec(request.value())
    except error:
        if args[4] != "reject":
            raise error^
        rejected = True
        output = List[UInt8]()
    assert_equal(rejected, args[4] == "reject")
    if not rejected:
        with open(args[3], "w") as destination:
            destination.write_bytes(Span(output))
