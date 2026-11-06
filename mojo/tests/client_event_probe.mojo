from std.sys import argv
from std.testing import assert_true
from zzmojo.protocol import Decoder, append_uint
from zzmojo.tree_wire import read_text
from zzmojo.terminal_wire import decode_terminal_frame
from zzmojo.client_events import (
    ClientEventState,
    decode_client_event,
    encode_client_event,
    valid_style,
)


def read_bytes(path: String) raises -> List[UInt8]:
    with open(path, "r") as source:
        return source.read_bytes()


def optional_uint(mut output: List[UInt8], value: Optional[UInt64]):
    output.append(UInt8(1 if value else 0))
    if value:
        append_uint(output, value.value())


def main() raises:
    var args = argv()
    var mode = args[1]
    var source = read_bytes(args[2])
    var output = List[UInt8]()
    if mode == "reject":
        var rejected = False
        try:
            _ = decode_client_event(source^)
        except:
            rejected = True
        assert_true(rejected)
        return
    if mode == "roundtrip":
        var event = decode_client_event(source^)
        assert_true(Bool(event))
        output = encode_client_event(event.value())
    elif mode == "styles":
        var reader = Decoder(source^)
        reader.offset = 0
        var count = reader.uint()
        for _ in range(Int(count)):
            output.append(UInt8(1 if valid_style(read_text(reader)) else 0))
        reader.finish()
    elif mode == "reduce":
        var reader = Decoder(source^)
        reader.offset = 0
        var state = ClientEventState()
        var count = reader.uint()
        for _ in range(Int(count)):
            var frame = reader.bytes()
            if frame[4] == 1:
                state.output.apply_terminal(decode_terminal_frame(frame^))
            else:
                var event = decode_client_event(frame^)
                assert_true(Bool(event))
                _ = state.apply(event.value())
            optional_uint(output, state.output.id)
            optional_uint(output, state.output.pane)
            var generation: Optional[UInt64] = None
            var view_generation: Optional[UInt64] = None
            if state.output.viewport:
                generation = state.output.viewport.value().generation
                view_generation = state.output.viewport.value().view_generation
            optional_uint(output, generation)
            optional_uint(output, view_generation)
            append_uint(output, state.output.revision)
            append_uint(output, state.output.watermark)
        reader.finish()
    else:
        raise Error("unknown client-event probe mode")
    with open(args[3], "w") as destination:
        destination.write_all(Span(output))
