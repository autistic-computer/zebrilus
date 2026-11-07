from std.sys import argv
from std.testing import assert_true, assert_equal
from zzmojo.protocol import Decoder, command_request, decode_response
from zzmojo.tree_wire import read_text
from zzmojo.file_protocol import (
    decode_file_request,
    encode_file_request,
    decode_file_response,
    encode_file_response,
)


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[2], "r") as source:
        bytes = source.read_bytes()
    var rejected = False
    var result: List[UInt8]
    try:
        if args[1] == "cli-error":
            result = bytes.copy()
            var response = decode_response(bytes^, cli_errors=True)
            assert_equal(response.error, args[5])
            assert_equal(response.exit_code, UInt8(Int(args[6])))
        elif args[1] == "command":
            var reader = Decoder(bytes^)
            assert_equal(reader.uint(), UInt64(2))
            var id = reader.uint()
            var name = read_text(reader)
            var arguments = List[String]()
            var count = reader.uint()
            for _ in range(Int(count)):
                arguments.append(read_text(reader))
            for _ in range(4):
                assert_equal(reader.byte(), UInt8(0))
            assert_equal(reader.byte(), UInt8(1))
            assert_equal(reader.byte(), UInt8(0))
            reader.finish()
            result = command_request(id, name, Span(arguments), True)
        elif args[1] == "request":
            var request = decode_file_request(bytes^)
            assert_true(Bool(request))
            result = encode_file_request(request.value())
        else:
            var response = decode_file_response(bytes^)
            assert_true(Bool(response))
            result = encode_file_response(response.value())
    except error:
        if args[4] != "reject":
            raise error^
        rejected = True
        result = List[UInt8]()
    assert_true(rejected == (args[4] == "reject"))
    if not rejected:
        with open(args[3], "w") as destination:
            destination.write_bytes(Span(result))
