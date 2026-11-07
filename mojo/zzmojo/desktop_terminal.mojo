from .chrome_actions import ChromeAction, ChromeActions

comptime TERMINAL_NATIVE: UInt8 = 0
comptime TERMINAL_FONT_INCREASE: UInt8 = 1
comptime TERMINAL_FONT_DECREASE: UInt8 = 2
comptime TERMINAL_SWALLOW: UInt8 = 3
comptime TERMINAL_SEARCH: UInt8 = 4
comptime TERMINAL_FOCUS_SEARCH: UInt8 = 5
comptime TERMINAL_COPY: UInt8 = 6
comptime TERMINAL_SELECT_ALL: UInt8 = 7
comptime TERMINAL_CLEAR_HISTORY: UInt8 = 8
comptime TERMINAL_PASTE: UInt8 = 9
comptime TERMINAL_CONSUMED: UInt8 = 10


def terminal_chrome_key_action(action: Optional[ChromeAction], swallowed: Bool, search_open: Bool, command_output: Bool) raises -> UInt8:
    if action and not ChromeActions().decode(action.value().code()):
        raise Error("terminal chrome action identity is invalid")
    var kind = Int(action.value().kind) if action else -1
    if kind == 23:
        return TERMINAL_FONT_INCREASE
    if kind == 24:
        return TERMINAL_FONT_DECREASE
    if swallowed:
        return TERMINAL_SWALLOW
    if kind == 25:
        return TERMINAL_SEARCH
    if search_open:
        return TERMINAL_FOCUS_SEARCH
    if kind == 26:
        return TERMINAL_COPY
    if kind == 27:
        return TERMINAL_SELECT_ALL
    if kind == 28:
        return TERMINAL_CONSUMED if command_output else TERMINAL_CLEAR_HISTORY
    if kind == 29:
        return TERMINAL_PASTE
    return TERMINAL_NATIVE


def adjusted_terminal_font_size(current: Float32, increase: Bool) -> Float32:
    return (current + (Float32(1) if increase else Float32(-1))).clamp(1, 256)


def terminal_font_size_offset(configured: Float32, offset: Float32) -> Tuple[Float32, Float32]:
    var effective = (configured + offset).clamp(1, 256)
    return (effective, effective - configured)


@fieldwise_init
struct TerminalFontState(TrivialRegisterPassable):
    var size: Float32
    var offset: Float32

    def adjust(mut self, increase: Bool) -> Bool:
        var next = adjusted_terminal_font_size(self.size, increase)
        var delta = next - self.size
        if abs(delta) < Float32(1.1920928955078125e-7):
            return False
        self.size = next
        self.offset += delta
        return True

    def configured(mut self, size: Float32):
        var effective = terminal_font_size_offset(size, self.offset)
        self.size = effective[0]
        self.offset = effective[1]
