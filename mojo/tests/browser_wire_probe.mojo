from std.sys import argv
from std.testing import assert_equal, assert_raises, assert_true
from zzmojo.browser_wire import (
    browser_command_event,
    encode_browser_command_event,
)
from browser_fixture import Source


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    var source = Source(bytes^)
    var count = source.uint(4)
    for _ in range(Int(count)):
        var size = Int(source.uint(4))
        assert_true(size <= len(source.bytes) - source.offset)
        var frame = List[UInt8](
            Span(source.bytes)[source.offset : source.offset + size]
        )
        source.offset += size
        var event = browser_command_event(frame)
        assert_equal(event.pane, UInt64(0xF123456789ABCDEF))
        var encoded = encode_browser_command_event(event)
        assert_equal(len(encoded), len(frame))
        for i in range(len(frame)):
            assert_equal(encoded[i], frame[i])
        var bad = frame.copy()
        bad.append(0)
        var length = len(bad) - 4
        for i in range(4):
            bad[i] = UInt8((length >> (i * 8)) & 255)
        with assert_raises():
            _ = browser_command_event(bad)
        var short = List[UInt8](Span(frame)[0 : len(frame) - 1])
        with assert_raises():
            _ = browser_command_event(short)
    assert_equal(source.offset, len(source.bytes))
    print(count)
