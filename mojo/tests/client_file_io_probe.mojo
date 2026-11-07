from std.sys import argv
from std.ffi import external_call
from zzmojo.client_files import ClientFiles
from zzmojo.file_protocol import (
    decode_file_request,
    encode_file_response,
)


def main() raises:
    var files = ClientFiles.capture_stdin()
    var args = argv()
    var restore = args[2] == "--restore-signal"
    if restore:
        _ = external_call["signal", UInt](Int32(15), UInt(1))
    var result = List[UInt8]()
    for index in range(3 if restore else 2, len(args)):
        var bytes: List[UInt8]
        with open(args[index], "r") as source:
            bytes = source.read_bytes()
        var request = decode_file_request(bytes^)
        if not request:
            raise Error("expected client file request")
        var response = files.answer(request.value())
        var frame = encode_file_response(response)
        result.extend(Span(frame))
    with open(args[1], "w") as destination:
        destination.write_bytes(Span(result))
    if restore:
        _ = external_call["raise", Int32](Int32(15))
