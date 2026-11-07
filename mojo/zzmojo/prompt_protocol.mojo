from .protocol import Decoder, append_uint, append_text, envelope, validate_envelope
from .tree_model import TmuxColour
from .tree_wire import read_uint, read_enum, read_bool, read_text, read_count, read_optional_id, read_optional_colour
from .client_events import _append_colour

comptime PROMPT_MAX_TEXT = 1024 * 1024
comptime PROMPT_MAX_HISTORY = 4096
comptime PROMPT_MAX_BYTES = 16 * 1024 * 1024


struct PromptState(Copyable, Movable):
    var prompt: String
    var input: String
    var cursor: UInt32
    var kind: UInt8
    var history: List[String]
    var prompt_type: UInt8
    var mode: UInt8
    var no_freeze: Bool
    var pane: Optional[UInt64]
    var command_mode: Bool
    var cursor_style: UInt8
    var cursor_colour: Optional[TmuxColour]

    def __init__(out self):
        self.prompt = String()
        self.input = String()
        self.cursor = 0
        self.kind = 0
        self.history = List[String]()
        self.prompt_type = 0
        self.mode = 0
        self.no_freeze = False
        self.pane = None
        self.command_mode = False
        self.cursor_style = 0
        self.cursor_colour = None

    def validate(self) raises:
        if self.kind > 1 or self.prompt_type > 1 or self.mode > 5:
            raise Error("unknown command prompt enum")
        if len(self.history) > PROMPT_MAX_HISTORY:
            raise Error("command prompt history limit")
        var total = self.prompt.byte_length() + self.input.byte_length()
        if self.prompt.byte_length() > PROMPT_MAX_TEXT or self.input.byte_length() > PROMPT_MAX_TEXT:
            raise Error("command prompt text limit")
        for value in self.history:
            if value.byte_length() > PROMPT_MAX_TEXT or total > PROMPT_MAX_BYTES - value.byte_length():
                raise Error("command prompt history byte limit")
            total += value.byte_length()


@fieldwise_init
struct PromptEvent(Copyable, Movable):
    var sequence: UInt64
    var state: Optional[PromptState]


struct PromptAction(Copyable, Movable):
    var kind: UInt8
    var input: String
    var cursor: UInt32

    def __init__(out self, kind: UInt8, var input: String = String(), cursor: UInt32 = 0):
        self.kind = kind
        self.input = input^
        self.cursor = cursor


def read_prompt(mut reader: Decoder) raises -> PromptState:
    var state = PromptState()
    state.prompt = read_text(reader, PROMPT_MAX_TEXT)
    state.input = read_text(reader, PROMPT_MAX_TEXT)
    state.cursor = UInt32(read_uint(reader, 4294967295))
    state.kind = read_enum(reader, 1)
    var count = read_count(reader, PROMPT_MAX_HISTORY)
    var total = state.prompt.byte_length() + state.input.byte_length()
    for _ in range(count):
        var value = read_text(reader, min(PROMPT_MAX_TEXT, PROMPT_MAX_BYTES - total))
        total += value.byte_length()
        state.history.append(value^)
    state.prompt_type = read_enum(reader, 1)
    state.mode = read_enum(reader, 5)
    state.no_freeze = read_bool(reader)
    state.pane = read_optional_id(reader)
    state.command_mode = read_bool(reader)
    state.cursor_style = reader.byte()
    state.cursor_colour = read_optional_colour(reader)
    return state^


def append_prompt(mut output: List[UInt8], state: PromptState) raises:
    state.validate()
    append_text(output, state.prompt)
    append_text(output, state.input)
    append_uint(output, UInt64(state.cursor))
    append_uint(output, UInt64(state.kind))
    append_uint(output, UInt64(len(state.history)))
    for value in state.history:
        append_text(output, value)
    append_uint(output, UInt64(state.prompt_type))
    append_uint(output, UInt64(state.mode))
    output.append(UInt8(state.no_freeze))
    output.append(UInt8(Bool(state.pane)))
    if state.pane:
        append_uint(output, state.pane.value())
    output.append(UInt8(state.command_mode))
    output.append(state.cursor_style)
    output.append(UInt8(Bool(state.cursor_colour)))
    if state.cursor_colour:
        _append_colour(output, state.cursor_colour.value())


def decode_prompt_event(var bytes: List[UInt8]) raises -> Optional[PromptEvent]:
    if validate_envelope(Span(bytes)) != 0:
        return None
    var reader = Decoder(bytes^)
    if read_enum(reader, 75) != 10:
        return None
    var sequence = reader.uint()
    if read_enum(reader, 65) != 9:
        return None
    var state: Optional[PromptState] = None
    if read_bool(reader):
        state = read_prompt(reader)
    reader.finish()
    return PromptEvent(sequence, state^)


def encode_prompt_event(event: PromptEvent) raises -> List[UInt8]:
    var output: List[UInt8] = [10]
    append_uint(output, event.sequence)
    append_uint(output, 9)
    output.append(UInt8(Bool(event.state)))
    if event.state:
        append_prompt(output, event.state.value())
    return envelope(Span(output))


def prompt_request(action: PromptAction) raises -> List[UInt8]:
    if action.kind > 2 or action.input.byte_length() > PROMPT_MAX_TEXT:
        raise Error("command prompt action exceeds its enum or text limit")
    var output: List[UInt8] = [9, 11]
    append_uint(output, UInt64(action.kind))
    if action.kind <= 1:
        append_text(output, action.input)
    if action.kind == 0:
        append_uint(output, UInt64(action.cursor))
    return envelope(Span(output))


def decode_prompt_request(var bytes: List[UInt8]) raises -> PromptAction:
    if validate_envelope(Span(bytes)) != 0:
        raise Error("command prompt action requires control lane")
    var reader = Decoder(bytes^)
    if read_enum(reader, 75) != 9 or read_enum(reader, 22) != 11:
        raise Error("expected command prompt action")
    var action = PromptAction(read_enum(reader, 2))
    if action.kind <= 1:
        action.input = read_text(reader, PROMPT_MAX_TEXT)
    if action.kind == 0:
        action.cursor = UInt32(read_uint(reader, 4294967295))
    reader.finish()
    return action^
