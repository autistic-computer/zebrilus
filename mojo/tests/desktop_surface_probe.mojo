from std.sys import argv
from std.testing import assert_equal, assert_raises, assert_true
from zzmojo.chrome_actions import ChromeActions, ChromeAction
from zzmojo.desktop_terminal import terminal_chrome_key_action, TerminalFontState, terminal_font_size_offset
from zzmojo.desktop_native_bindings import desktop_native_binding


struct Source(Movable):
    var data: List[UInt8]
    var offset: Int

    def __init__(out self, var data: List[UInt8]):
        self.data = data^
        self.offset = 0

    def uint(mut self, count: Int) raises -> UInt32:
        if count > len(self.data) - self.offset:
            raise Error("truncated desktop surface fixture")
        var value: UInt32 = 0
        for index in range(count):
            value |= UInt32(self.data[self.offset + index]) << UInt32(index * 8)
        self.offset += count
        return value

    def float(mut self) raises -> Float32:
        var bits = self.uint(4)
        return Pointer(to=bits).unsafe_bitcast[Float32]()[]


def check_float(actual: Float32, expected: UInt32) raises:
    assert_equal(Pointer(to=actual).unsafe_bitcast[UInt32]()[], expected)


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    assert_true(len(data) <= 1024 * 1024)
    var source = Source(data^)
    var catalog = ChromeActions()
    var decisions = source.uint(4)
    for _ in range(Int(decisions)):
        var code = source.uint(4)
        var flags = source.uint(1)
        var action = catalog.decode(code)
        var decision = terminal_chrome_key_action(action, flags & 1 != 0, flags & 2 != 0, flags & 4 != 0)
        assert_equal(decision, UInt8(source.uint(1)))
    var bindings = source.uint(4)
    for _ in range(Int(bindings)):
        var action = catalog.decode(source.uint(4)).value()
        for context in range(3):
            var binding = desktop_native_binding(UInt8(context), action)
            assert_equal(binding.kind, UInt8(source.uint(1)))
            assert_equal(binding.index, UInt8(source.uint(1)) if context == 2 else UInt8(0))
    var floats = source.uint(4)
    for _ in range(Int(floats)):
        var size = source.float()
        var offset = source.float()
        var increase = source.uint(1) != 0
        var changed = source.uint(1) != 0
        var state = TerminalFontState(size, offset)
        assert_equal(state.adjust(increase), changed)
        check_float(state.size, source.uint(4))
        check_float(state.offset, source.uint(4))
        var adjusted = terminal_font_size_offset(size, offset)
        check_float(adjusted[0], source.uint(4))
        check_float(adjusted[1], source.uint(4))
    var steps = source.uint(4)
    var state = TerminalFontState(13, 0)
    for _ in range(Int(steps)):
        var operation = source.uint(1)
        var size = source.float()
        var expected = source.uint(1) != 0
        var changed = False
        if operation < 2:
            state.configured(size)
        else:
            changed = state.adjust(operation & 1 == 0)
        assert_equal(changed, expected)
        check_float(state.size, source.uint(4))
        check_float(state.offset, source.uint(4))
    assert_equal(source.offset, len(source.data))
    with assert_raises():
        _ = terminal_chrome_key_action(ChromeAction(255, 0), False, False, False)
    with assert_raises():
        _ = desktop_native_binding(3, ChromeAction(0, 0))
    with assert_raises():
        _ = desktop_native_binding(0, ChromeAction(23, 1))
    print(decisions, bindings, floats, steps)
