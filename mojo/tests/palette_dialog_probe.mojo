from std.sys import argv
from zzmojo.protocol import Decoder, append_bytes, append_uint
from zzmojo.tree_wire import read_count
from zzmojo.prompt_protocol import decode_prompt_event, encode_prompt_event, decode_prompt_request, prompt_request
from zzmojo.chooser_protocol import decode_choose_event, encode_choose_event, decode_choose_request, choose_request


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
        var accepted = True
        try:
            if kind == 0:
                var event = decode_prompt_event(frame^)
                if not event:
                    raise Error("expected prompt event")
                result = encode_prompt_event(event.value())
            elif kind == 1:
                result = prompt_request(decode_prompt_request(frame^))
            elif kind == 2:
                var event = decode_choose_event(frame^)
                if not event:
                    raise Error("expected chooser event")
                result = encode_choose_event(event.value())
            elif kind == 3:
                result = choose_request(decode_choose_request(frame^))
            else:
                raise Error("unknown dialog probe kind")
        except:
            accepted = False
        output.append(UInt8(accepted))
        if accepted:
            append_bytes(output, Span(result))
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_all(Span(output))
