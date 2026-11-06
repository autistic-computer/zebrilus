from .protocol import (
    append_uint,
    append_text,
    envelope,
    validate_envelope,
    Decoder,
)
from .tree_wire import (
    read_uint,
    read_enum,
    read_bool,
    read_text,
    read_optional_text,
    read_count,
)
from .terminal_view import PointerCell, PointerGrid


struct KeyInput(Copyable, Movable):
    var action: UInt8
    var key: UInt8
    var codepoint: UInt32
    var value: UInt16
    var modifiers: UInt8
    var text: Optional[String]
    var unshifted: Optional[UInt32]

    def __init__(out self):
        self.action = 0
        self.key = 16
        self.codepoint = 0
        self.value = 0
        self.modifiers = 0
        self.text = None
        self.unshifted = None


@fieldwise_init
struct MouseInput(Copyable, Movable):
    var phase: UInt8
    var button: Optional[UInt8]
    var pointer: PointerCell
    var modifiers: UInt8
    var force_selection: Bool


struct InputFrame(Copyable, Movable):
    var kind: UInt8
    var pane: UInt64
    var key: Optional[KeyInput]
    var text_follows: Bool
    var view_action: UInt8
    var mouse: Optional[MouseInput]
    var pointer: Optional[PointerCell]
    var lines: Int32
    var value: UInt32
    var request_id: UInt64
    var target: UInt8
    var text: String
    var columns: UInt16
    var rows: UInt16
    var cell_width: UInt32
    var cell_height: UInt32

    def __init__(out self):
        self.kind = 1
        self.pane = 0
        self.key = None
        self.text_follows = False
        self.view_action = 0
        self.mouse = None
        self.pointer = None
        self.lines = 0
        self.value = 0
        self.request_id = 0
        self.target = 0
        self.text = String()
        self.columns = 0
        self.rows = 0
        self.cell_width = 0
        self.cell_height = 0


def wire_modifiers(mods: UInt32) -> UInt8:
    return UInt8((mods >> 17) & 15)


def _scalar(value: UInt32) -> Bool:
    return value <= 0x10FFFF and not (value >= 0xD800 and value <= 0xDFFF)


def _control(value: UInt32) -> Bool:
    return value < 32 or (value >= 127 and value <= 159)


def _character(value: UInt32) raises -> String:
    if not _scalar(value):
        raise Error("key character is not a Unicode scalar")
    return String(chr(Int(value)))


def mjui_key_input(
    key: UInt16,
    ch: UInt32,
    unshifted: UInt32,
    mods: UInt32,
    action: UInt8,
    text: Optional[String] = None,
) raises -> KeyInput:
    if action > 2:
        raise Error("unknown key action")
    var input = KeyInput()
    input.action = action
    input.modifiers = wire_modifiers(mods)
    if key >= 0x100 and key <= 0x10D:
        var named: Array[UInt8, 14] = [
            2,
            3,
            1,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            13,
            14,
            11,
            12,
        ]
        input.key = named[Int(key - 0x100)]
    elif key >= 0x180 and key <= 0x197:
        input.key = 15
        input.value = key - 0x180 + 1
    elif key == 0x124:
        input.key = 2
    else:
        var character: UInt32 = 0
        if unshifted != 0 and _scalar(unshifted) and not _control(unshifted):
            character = unshifted
        elif key >= 32 and key <= 126:
            character = UInt32(key)
        elif ch != 0 and _scalar(ch) and not _control(ch):
            character = ch
        elif key >= 0x1A0 and key <= 0x1A9:
            character = UInt32(key - 0x1A0 + 48)
        if character != 0:
            if input.modifiers & 1 != 0 and not (
                character >= 97 and character <= 122
            ):
                if ch != 0 and _scalar(ch) and not _control(ch):
                    character = ch
                input.modifiers &= ~UInt8(1)
            input.key = 0
            input.codepoint = character
            input.unshifted = character
    var held_text = text.copy()
    if not held_text and ch != 0 and _scalar(ch):
        held_text = _character(ch)
    if not held_text and input.key == 0:
        held_text = _character(input.codepoint)
    if held_text:
        var valid = True
        for character in held_text.value().codepoints():
            if _control(UInt32(Int(character))):
                valid = False
        if valid:
            input.text = held_text^
    return input^


def should_send_key(
    input: KeyInput, kitty_keyboard: Bool = False, claims_prefix: Bool = False
) -> Bool:
    return input.key != 16 and (
        kitty_keyboard
        or claims_prefix
        or input.key != 0
        or input.modifiers & 14 != 0
    )


def _append_key(mut output: List[UInt8], input: KeyInput) raises:
    if input.action > 2 or input.key > 17 or input.modifiers > 15:
        raise Error("key input contains an unknown action, code or modifier")
    append_uint(output, UInt64(input.action))
    append_uint(output, UInt64(input.key))
    if input.key == 0:
        append_text(output, _character(input.codepoint))
    elif input.key == 15:
        if input.value > 255:
            raise Error("function key number exceeds u8")
        output.append(UInt8(input.value))
    elif input.key == 17:
        append_uint(output, UInt64(input.value))
    output.append(input.modifiers)
    output.append(UInt8(1 if input.text else 0))
    if input.text:
        append_text(output, input.text.value())
    output.append(UInt8(1 if input.unshifted else 0))
    if input.unshifted:
        append_text(output, _character(input.unshifted.value()))


def _read_character(mut reader: Decoder) raises -> UInt32:
    var text = read_text(reader, 4)
    for character in text.codepoints():
        return UInt32(Int(character))
    raise Error("postcard character is empty")


def _read_key(mut reader: Decoder) raises -> KeyInput:
    var input = KeyInput()
    input.action = read_enum(reader, 2)
    input.key = read_enum(reader, 17)
    if input.key == 0:
        input.codepoint = _read_character(reader)
    elif input.key == 15:
        input.value = UInt16(reader.byte())
    elif input.key == 17:
        input.value = UInt16(read_uint(reader, 65535))
    input.modifiers = reader.byte()
    if input.modifiers > 15:
        raise Error("key modifier contains reserved bits")
    input.text = read_optional_text(reader)
    if read_bool(reader):
        input.unshifted = _read_character(reader)
    return input^


def _append_cell(mut output: List[UInt8], pointer: PointerCell) raises:
    if (
        pointer.column < 0
        or pointer.column > 65535
        or pointer.row < 0
        or pointer.row > 65535
        or pointer.click_count < 0
        or pointer.click_count > 255
    ):
        raise Error("pointer cell fields exceed their wire type")
    append_uint(output, UInt64(pointer.column))
    append_uint(output, UInt64(pointer.row))
    output.append(UInt8(pointer.click_count))
    output.append(UInt8(1 if pointer.rectangle else 0))


def _read_cell(mut reader: Decoder) raises -> PointerCell:
    return PointerCell(
        Int(read_uint(reader, 65535)),
        Int(read_uint(reader, 65535)),
        Int(reader.byte()),
        read_bool(reader),
        0,
        0,
        0,
        0,
        0,
        0,
    )


def _append_mouse(mut output: List[UInt8], input: MouseInput) raises:
    if (
        input.phase > 2
        or input.modifiers > 15
        or (input.button and input.button.value() > 4)
    ):
        raise Error("mouse input contains unknown routing fields")
    append_uint(output, UInt64(input.phase))
    output.append(UInt8(1 if input.button else 0))
    if input.button:
        append_uint(output, UInt64(input.button.value()))
    _append_cell(output, input.pointer)
    var metrics: Array[Int, 6] = [
        input.pointer.x,
        input.pointer.y,
        input.pointer.screen_width,
        input.pointer.screen_height,
        input.pointer.cell_width,
        input.pointer.cell_height,
    ]
    for metric in metrics:
        if metric < 0 or metric > 4294967295:
            raise Error("mouse pixel metric exceeds u32")
        append_uint(output, UInt64(metric))
    output.append(input.modifiers)
    output.append(UInt8(1 if input.force_selection else 0))


def _read_mouse(mut reader: Decoder) raises -> MouseInput:
    var phase = read_enum(reader, 2)
    var button: Optional[UInt8] = None
    if read_bool(reader):
        button = read_enum(reader, 4)
    var pointer = _read_cell(reader)
    pointer.x = Int(read_uint(reader, 4294967295))
    pointer.y = Int(read_uint(reader, 4294967295))
    pointer.screen_width = Int(read_uint(reader, 4294967295))
    pointer.screen_height = Int(read_uint(reader, 4294967295))
    pointer.cell_width = Int(read_uint(reader, 4294967295))
    pointer.cell_height = Int(read_uint(reader, 4294967295))
    var modifiers = reader.byte()
    if modifiers > 15:
        raise Error("mouse modifier contains reserved bits")
    return MouseInput(phase, button, pointer, modifiers, read_bool(reader))


def mjui_mouse_input(
    grid: PointerGrid,
    x: Float32,
    y: Float32,
    phase: UInt8,
    button: Optional[UInt8],
    clicks: Int,
    mods: UInt32,
    force_selection: Bool = False,
) -> MouseInput:
    var modifiers = wire_modifiers(mods)
    return MouseInput(
        phase,
        button,
        grid.project(x, y, clicks, modifiers & 4 != 0),
        modifiers,
        force_selection or modifiers & 1 != 0,
    )


def mjui_scroll_input(
    grid: PointerGrid, x: Float32, y: Float32, lines: Int32, mods: UInt32
) -> MouseInput:
    return mjui_mouse_input(
        grid, x, y, 0, UInt8(3 if lines < 0 else 4), 1, mods
    )


def _append_lines(mut output: List[UInt8], lines: Int32):
    var value = (
        UInt64(-Int64(lines) * 2 - 1) if lines < 0 else UInt64(lines) * 2
    )
    append_uint(output, value)


def _read_lines(mut reader: Decoder) raises -> Int32:
    var value = read_uint(reader, 4294967295)
    return Int32(-Int64(value >> 1) - 1) if value & 1 else Int32(value >> 1)


def encode_input_frame(frame: InputFrame) raises -> List[UInt8]:
    var output: List[UInt8] = [9]
    append_uint(output, UInt64(frame.kind))
    if frame.kind != 6 and frame.kind != 7:
        append_uint(output, frame.pane)
    if frame.kind == 1 or frame.kind == 3:
        if not frame.key:
            raise Error("key frame has no key input")
        _append_key(output, frame.key.value())
        output.append(UInt8(1 if frame.text_follows else 0))
    elif frame.kind == 6:
        append_uint(output, UInt64(frame.columns))
        append_uint(output, UInt64(frame.rows))
        append_uint(output, UInt64(frame.cell_width))
        append_uint(output, UInt64(frame.cell_height))
    elif frame.kind == 5 or frame.kind == 7:
        append_uint(output, UInt64(frame.view_action))
        if frame.view_action == 0 or frame.view_action == 1:
            _append_lines(output, frame.lines)
        elif frame.view_action == 4 or frame.view_action == 25:
            append_uint(output, UInt64(frame.value))
        elif frame.view_action == 22:
            if frame.target > 1:
                raise Error("unknown clipboard target")
            append_uint(output, frame.request_id)
            output.append(frame.target)
        elif frame.view_action == 23:
            if frame.value > 1:
                raise Error("unknown terminal focus value")
            output.append(UInt8(frame.value))
        elif frame.view_action == 24:
            append_text(output, frame.text)
        elif frame.view_action >= 5 and frame.view_action <= 8:
            if not frame.pointer:
                raise Error("selection frame has no pointer cell")
            if frame.view_action == 7:
                _append_lines(output, frame.lines)
            _append_cell(output, frame.pointer.value())
        elif frame.view_action == 9 or frame.view_action == 11:
            if not frame.mouse:
                raise Error("mouse frame has no mouse input")
            if frame.view_action == 11:
                _append_lines(output, frame.lines)
            _append_mouse(output, frame.mouse.value())
        elif (
            frame.view_action != 2
            and frame.view_action != 3
            and frame.view_action != 10
            and (frame.view_action < 12 or frame.view_action > 15)
        ):
            raise Error("unsupported terminal view action")
    else:
        raise Error("unsupported native input frame")
    return envelope(Span(output))


def decode_input_frame(var bytes: List[UInt8]) raises -> InputFrame:
    if validate_envelope(Span(bytes)) != 0:
        raise Error("input frame must use control lane")
    var reader = Decoder(bytes^)
    if read_enum(reader, 75) != 9:
        raise Error("frame is not a native input message")
    var frame = InputFrame()
    frame.kind = read_enum(reader, 23)
    if frame.kind != 6 and frame.kind != 7:
        frame.pane = reader.uint()
    if frame.kind == 1 or frame.kind == 3:
        frame.key = _read_key(reader)
        frame.text_follows = read_bool(reader)
    elif frame.kind == 6:
        frame.columns = UInt16(read_uint(reader, 65535))
        frame.rows = UInt16(read_uint(reader, 65535))
        frame.cell_width = UInt32(read_uint(reader, 4294967295))
        frame.cell_height = UInt32(read_uint(reader, 4294967295))
    elif frame.kind == 5 or frame.kind == 7:
        frame.view_action = read_enum(reader, 31)
        if frame.view_action == 0 or frame.view_action == 1:
            frame.lines = _read_lines(reader)
        elif frame.view_action == 4 or frame.view_action == 25:
            frame.value = UInt32(read_uint(reader, 4294967295))
        elif frame.view_action == 22:
            frame.request_id = reader.uint()
            frame.target = read_enum(reader, 1)
        elif frame.view_action == 23:
            frame.value = UInt32(1 if read_bool(reader) else 0)
        elif frame.view_action == 24:
            frame.text = reader.text()
        elif frame.view_action >= 5 and frame.view_action <= 8:
            if frame.view_action == 7:
                frame.lines = _read_lines(reader)
            frame.pointer = _read_cell(reader)
        elif frame.view_action == 9 or frame.view_action == 11:
            if frame.view_action == 11:
                frame.lines = _read_lines(reader)
            frame.mouse = _read_mouse(reader)
        elif (
            frame.view_action != 2
            and frame.view_action != 3
            and frame.view_action != 10
            and (frame.view_action < 12 or frame.view_action > 15)
        ):
            raise Error("unsupported terminal view action")
    else:
        raise Error("unsupported native input frame")
    return frame^


def input_key(
    pane: UInt64,
    input: KeyInput,
    text_follows: Bool = False,
    browser: Bool = False,
) raises -> List[UInt8]:
    var frame = InputFrame()
    frame.kind = UInt8(3 if browser else 1)
    frame.pane = pane
    frame.key = input.copy()
    frame.text_follows = text_follows
    return encode_input_frame(frame)


def input_view(
    pane: UInt64,
    action: UInt8,
    value: UInt32 = 0,
    lines: Int32 = 0,
    command_output: Bool = False,
) raises -> List[UInt8]:
    var frame = InputFrame()
    frame.kind = UInt8(7 if command_output else 5)
    frame.pane = pane
    frame.view_action = action
    frame.value = value
    frame.lines = lines
    return encode_input_frame(frame)


def input_mouse(
    pane: UInt64, input: MouseInput, command_output: Bool = False
) raises -> List[UInt8]:
    var frame = InputFrame()
    frame.kind = UInt8(7 if command_output else 5)
    frame.pane = pane
    frame.view_action = 9
    frame.mouse = input.copy()
    return encode_input_frame(frame)


def input_scroll(
    pane: UInt64,
    lines: Int32,
    input: MouseInput,
    command_output: Bool = False,
) raises -> List[UInt8]:
    var frame = InputFrame()
    frame.kind = UInt8(7 if command_output else 5)
    frame.pane = pane
    frame.view_action = 11
    frame.lines = lines
    frame.mouse = input.copy()
    return encode_input_frame(frame)


def input_selection(
    pane: UInt64,
    action: UInt8,
    pointer: PointerCell,
    lines: Int32 = 0,
    command_output: Bool = False,
) raises -> List[UInt8]:
    if action < 5 or action > 8:
        raise Error("unknown terminal selection action")
    var frame = InputFrame()
    frame.kind = UInt8(7 if command_output else 5)
    frame.pane = pane
    frame.view_action = action
    frame.pointer = pointer
    frame.lines = lines
    return encode_input_frame(frame)


def resize_command_output(
    columns: UInt16, rows: UInt16, cell_width: UInt32, cell_height: UInt32
) raises -> List[UInt8]:
    var frame = InputFrame()
    frame.kind = 6
    frame.columns = columns
    frame.rows = rows
    frame.cell_width = cell_width
    frame.cell_height = cell_height
    return encode_input_frame(frame)


def input_focus(pane: UInt64, focused: Bool, command_output: Bool = False) raises -> List[UInt8]:
    return input_view(pane, 23, UInt32(1 if focused else 0), command_output=command_output)


def input_paste(pane: UInt64, text: String, command_output: Bool = False) raises -> List[UInt8]:
    var frame = InputFrame()
    frame.kind = UInt8(7 if command_output else 5)
    frame.pane = pane
    frame.view_action = 24
    frame.text = text
    return encode_input_frame(frame)


def input_copy_selection(pane: UInt64, request_id: UInt64, target: UInt8 = 0, command_output: Bool = False) raises -> List[UInt8]:
    var frame = InputFrame()
    frame.kind = UInt8(7 if command_output else 5)
    frame.pane = pane
    frame.view_action = 22
    frame.request_id = request_id
    frame.target = target
    return encode_input_frame(frame)


@fieldwise_init
struct MuxOptionValue(Copyable, Movable):
    var value: String
    var source: UInt8


struct RoutingEvent(Copyable, Movable):
    var kind: UInt8
    var sequence: UInt64
    var armed: Bool
    var request_id: UInt64
    var table: Optional[String]
    var repeat: Bool
    var options: Dict[UInt8, MuxOptionValue]

    def __init__(out self):
        self.kind = 0
        self.sequence = 0
        self.armed = False
        self.request_id = 0
        self.table = None
        self.repeat = False
        self.options = Dict[UInt8, MuxOptionValue]()


def decode_routing_event(
    var bytes: List[UInt8],
) raises -> Optional[RoutingEvent]:
    if validate_envelope(Span(bytes)) != 0:
        return None
    var reader = Decoder(bytes^)
    if read_enum(reader, 75) != 10:
        return None
    var event = RoutingEvent()
    event.sequence = reader.uint()
    event.kind = read_enum(reader, 65)
    if event.kind == 21:
        event.armed = read_bool(reader)
    elif event.kind == 34:
        event.request_id = reader.uint()
    elif event.kind == 54:
        event.table = read_optional_text(reader)
        event.repeat = read_bool(reader)
    elif event.kind == 2 or event.kind == 60:
        var count = read_count(reader)
        for _ in range(count):
            var key = read_enum(reader, 20)
            var value = read_text(reader, 65536)
            var source = read_enum(reader, 3)
            event.options[key] = MuxOptionValue(value^, source)
        if event.kind == 2 and len(event.options) != 21:
            raise Error("full mux options must contain every supported key")
    else:
        return None
    return event^


def canonical_key(value: String) raises -> String:
    var rest = String(value.strip())
    if value == " " or rest == "Space":
        return " "
    if rest.lower() == "none":
        return "None"
    var control = False
    var alt = False
    while True:
        var skip = 0
        if rest.startswith("Ctrl-"):
            control = True
            skip = 5
        elif rest.startswith("C-"):
            control = True
            skip = 2
        elif rest.startswith("Alt-"):
            alt = True
            skip = 4
        elif rest.startswith("M-"):
            alt = True
            skip = 2
        if skip == 0:
            break
        var tail = String(from_utf8=rest.as_bytes()[skip:])
        rest = tail^
    if not control and not alt:
        return rest^
    if rest == "Space" or (rest.byte_length() == 0 and value.endswith(" ")):
        rest = " "
    return String("C-" if control else "", "M-" if alt else "", rest)


def input_key_name(input: KeyInput) raises -> String:
    if input.modifiers & 8 != 0:
        return String()
    var output = String()
    if input.modifiers & 2 != 0:
        output += "C-"
    if input.modifiers & 4 != 0:
        output += "M-"
    if (
        input.modifiers & 1 != 0
        and input.key != 0
        and input.key != 3
        and input.key != 16
    ):
        output += "S-"
    if input.key == 0:
        var character = input.codepoint
        if (
            input.modifiers & 2 == 0
            and input.modifiers & 1 != 0
            and character >= 97
            and character <= 122
        ):
            character -= 32
        output += _character(character)
    elif input.key == 3:
        output += "BTab" if input.modifiers & 1 != 0 else "Tab"
    elif input.key == 15:
        output += "F" + String(input.value)
    elif input.key == 16:
        output += input.text.or_else(String())
    elif input.key == 17:
        output += "User" + String(input.value)
    elif input.key >= 1 and input.key <= 14:
        var named: List[String] = [
            "",
            "BSpace",
            "Enter",
            "Tab",
            "Escape",
            "DC",
            "IC",
            "Home",
            "End",
            "PPage",
            "NPage",
            "Up",
            "Down",
            "Left",
            "Right",
        ]
        output += named[Int(input.key)]
    return output^


struct InputRoutingState(Copyable, Movable):
    var armed: Bool
    var table: Optional[String]
    var repeat: Bool
    var cancelled_request: UInt64
    var options: Dict[UInt8, MuxOptionValue]

    def __init__(out self):
        self.armed = False
        self.table = None
        self.repeat = False
        self.cancelled_request = 0
        self.options = Dict[UInt8, MuxOptionValue]()
        self.options[0] = MuxOptionValue("C-b", 0)
        self.options[15] = MuxOptionValue("None", 0)

    def apply(mut self, event: RoutingEvent):
        if event.kind == 21:
            self.armed = event.armed
        elif event.kind == 34:
            self.cancelled_request = event.request_id
        elif event.kind == 54:
            self.table = event.table.copy()
            self.repeat = event.repeat if event.table else False
        elif event.kind == 2:
            self.options = event.options.copy()
        elif event.kind == 60:
            for entry in event.options.items():
                self.options[entry.key] = entry.value.copy()

    def claims_prefix_input(self, input: KeyInput) raises -> Bool:
        if input.modifiers & 8 != 0:
            return False
        if self.armed or self.table:
            return True
        var name = input_key_name(input)
        for key in [UInt8(0), UInt8(15)]:
            if key in self.options:
                var prefix = canonical_key(self.options[key].value)
                if prefix != "None" and prefix == name:
                    return True
        return False
