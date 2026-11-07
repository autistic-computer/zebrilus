from std.sys import argv
from zzmojo.input_protocol import (
    resize_command_output,
    input_focus,
    input_paste,
    input_copy_selection,
    input_view,
    encode_input_frame,
    decode_input_frame,
)
from zzmojo.command_output import CommandOutputPresentation
from zzmojo.client_events import CommandOutputState
from zzmojo.terminal_model import TerminalViewport
from zzmojo.protocol import Decoder
from zzmojo.protocol import append_text
from zzmojo.command_output_chrome import mode_indicator
from zzmojo.terminal_model import TerminalMode


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[2], "r") as source:
        bytes = source.read_bytes()
    var result: List[UInt8]
    if args[1] == "mode":
        var reader = Decoder(bytes^)
        reader.offset = 0
        var mode = TerminalMode(
            UInt8(reader.uint()),
            UInt32(reader.uint()),
            UInt32(reader.uint()),
            reader.uint() != 0,
        )
        var indicator = mode_indicator(mode, UInt32(reader.uint()))
        result = List[UInt8]()
        result.append(UInt8(1 if indicator else 0))
        if indicator:
            result.append(UInt8(1 if indicator.value().label else 0))
            if indicator.value().label:
                append_text(result, indicator.value().label.value())
            append_text(result, indicator.value().detail)
        reader.finish()
    elif args[1] == "roundtrip":
        result = encode_input_frame(decode_input_frame(bytes^))
    else:
        var reader = Decoder(bytes^)
        reader.offset = 0
        var kind = reader.uint()
        if kind == 6:
            result = resize_command_output(
                UInt16(reader.uint()),
                UInt16(reader.uint()),
                UInt32(reader.uint()),
                UInt32(reader.uint()),
            )
        elif kind == 0:
            var composed = reader.uint() != 0
            var output = CommandOutputState()
            output.apply(7, 7, Optional[TerminalViewport](TerminalViewport()))
            var component = CommandOutputPresentation()
            _ = component.observe(output)
            _ = component.focus(True)
            result = component.commit(reader.text(), composed).value().copy()
        else:
            var output = reader.uint() != 0
            if kind == 23:
                result = input_focus(UInt64.MAX, reader.uint() != 0, output)
            elif kind == 24:
                result = input_paste(UInt64.MAX, reader.text(), output)
            elif kind == 22:
                var request_id = reader.uint()
                result = input_copy_selection(
                    UInt64.MAX, request_id, UInt8(reader.uint()), output
                )
            elif kind == 25:
                result = input_view(
                    UInt64.MAX, 25, UInt32(reader.uint()), command_output=output
                )
            else:
                raise Error("unknown command-output oracle request")
        reader.finish()
    with open(args[3], "w") as destination:
        destination.write_bytes(Span(result))
