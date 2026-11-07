from std.sys import argv
from zzmojo.protocol import Decoder, append_bytes, append_uint
from zzmojo.tree_wire import read_count
from zzmojo.client_attachment import decode_detached, encode_detached, decode_server_stopping, server_stopping_event


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = read_count(reader, 100000)
    var output = List[UInt8]()
    append_uint(output, UInt64(count))
    for _ in range(count):
        var kind = reader.byte()
        var frame = reader.bytes()
        var result = List[UInt8]()
        var status: UInt8 = 2
        try:
            if kind == 0:
                var event = decode_detached(frame^)
                if event:
                    result = encode_detached(event.value())
                    status = 1
            elif kind == 1:
                var sequence = decode_server_stopping(frame^)
                if sequence:
                    result = server_stopping_event(sequence.value())
                    status = 1
            else:
                raise Error("unknown attachment probe kind")
        except:
            status = 0
        output.append(status)
        if status == 1:
            append_bytes(output, Span(result))
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_all(Span(output))
