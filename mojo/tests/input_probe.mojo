from std.sys import argv
from std.testing import assert_true
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool, read_optional_text
from zzmojo.terminal_view import PointerCell
from zzmojo.input_protocol import (
    KeyInput,
    MouseInput,
    InputFrame,
    InputRoutingState,
    mjui_key_input,
    should_send_key,
    input_key_name,
    decode_input_frame,
    encode_input_frame,
    input_key,
    decode_routing_event,
)


def read_bytes(path: String) raises -> List[UInt8]:
    with open(path, "r") as source:
        return source.read_bytes()


def read_cell(mut reader: Decoder) raises -> PointerCell:
    return PointerCell(
        Int(reader.uint()),
        Int(reader.uint()),
        Int(reader.uint()),
        reader.uint() != 0,
        0,
        0,
        0,
        0,
        0,
        0,
    )


def read_mouse(mut reader: Decoder) raises -> MouseInput:
    var phase = UInt8(reader.uint())
    var button = reader.uint()
    var choice: Optional[UInt8] = None
    if button:
        choice = UInt8(button - 1)
    var cell = read_cell(reader)
    cell.x = Int(reader.uint())
    cell.y = Int(reader.uint())
    cell.screen_width = Int(reader.uint())
    cell.screen_height = Int(reader.uint())
    cell.cell_width = Int(reader.uint())
    cell.cell_height = Int(reader.uint())
    return MouseInput(
        phase, choice, cell, UInt8(reader.uint()), reader.uint() != 0
    )


def read_lines(mut reader: Decoder) raises -> Int32:
    var value = reader.uint()
    return Int32(-Int64(value >> 1) - 1) if value & 1 else Int32(value >> 1)


def build(mut reader: Decoder) raises -> List[UInt8]:
    var frame = InputFrame()
    frame.kind = UInt8(reader.uint())
    frame.pane = reader.uint()
    if frame.kind == 1 or frame.kind == 3:
        var key = KeyInput()
        key.action = UInt8(reader.uint())
        key.key = UInt8(reader.uint())
        key.codepoint = UInt32(reader.uint())
        key.value = UInt16(reader.uint())
        key.modifiers = UInt8(reader.uint())
        key.text = read_optional_text(reader)
        if read_bool(reader):
            key.unshifted = UInt32(reader.uint())
        frame.key = key^
        frame.text_follows = reader.uint() != 0
    elif frame.kind == 5 or frame.kind == 7:
        frame.view_action = UInt8(reader.uint())
        var action = frame.view_action
        if action == 0 or action == 1 or action == 7 or action == 11:
            frame.lines = read_lines(reader)
        if action == 4:
            frame.value = UInt32(reader.uint())
        elif action >= 5 and action <= 8:
            frame.pointer = read_cell(reader)
        elif action == 9 or action == 11:
            frame.mouse = read_mouse(reader)
    reader.finish()
    return encode_input_frame(frame)


def main() raises:
    var args = argv()
    var mode = args[1]
    if mode == "reject" or mode == "routing-reject":
        var rejected = False
        try:
            if mode == "reject":
                _ = decode_input_frame(read_bytes(args[2]))
            else:
                _ = decode_routing_event(read_bytes(args[2]))
        except:
            rejected = True
        assert_true(rejected)
        return
    var output = List[UInt8]()
    if mode == "roundtrip":
        output = encode_input_frame(decode_input_frame(read_bytes(args[2])))
    elif mode == "build" or mode == "mapping":
        var reader = Decoder(read_bytes(args[2]))
        reader.offset = 0
        if mode == "build":
            output = build(reader)
        else:
            var key = UInt16(reader.uint())
            var ch = UInt32(reader.uint())
            var unshifted = UInt32(reader.uint())
            var mods = UInt32(reader.uint())
            var action = UInt8(reader.uint())
            var text = read_optional_text(reader)
            var input = mjui_key_input(key, ch, unshifted, mods, action, text^)
            _ = should_send_key(input)
            _ = input_key_name(input)
            output = input_key(7, input)
    elif mode == "routing-event":
        var event = decode_routing_event(read_bytes(args[2]))
        assert_true(Bool(event))
        var value = event.value().copy()
        append_uint(output, UInt64(value.kind))
        append_uint(output, value.sequence)
        append_uint(output, UInt64(1 if value.armed else 0))
        append_uint(output, value.request_id)
        output.append(UInt8(1 if value.table else 0))
        if value.table:
            append_text(output, value.table.value())
        output.append(UInt8(1 if value.repeat else 0))
        append_uint(output, UInt64(len(value.options)))
        for key in range(21):
            if UInt8(key) in value.options:
                append_uint(output, UInt64(key))
                append_text(output, value.options[UInt8(key)].value)
                append_uint(output, UInt64(value.options[UInt8(key)].source))
    elif mode == "routing":
        var state = InputRoutingState()
        var reader = Decoder(read_bytes(args[2]))
        reader.offset = 0
        var count = reader.uint()
        for _ in range(Int(count)):
            var event = decode_routing_event(reader.bytes())
            if not event:
                raise Error("routing fixture has no routing event")
            state.apply(event.value())
        output.append(UInt8(1 if state.armed else 0))
        output.append(UInt8(1 if state.table else 0))
        if state.table:
            append_text(output, state.table.value())
        output.append(UInt8(1 if state.repeat else 0))
        var queries = reader.uint()
        for _ in range(Int(queries)):
            var decoded = decode_input_frame(reader.bytes())
            append_text(output, input_key_name(decoded.key.value()))
            output.append(
                UInt8(
                    1 if state.claims_prefix_input(decoded.key.value()) else 0
                )
            )
        reader.finish()
    else:
        raise Error("unknown input probe command")
    with open(args[3], "w") as destination:
        destination.write_all(Span(output))
