from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.protocol import Decoder, validate_envelope
from zzmojo.tree_wire import read_snapshot
from zzmojo.desktop_snapshot_target import desktop_active_pane, desktop_snapshot_target


struct Source(Movable):
    var data: List[UInt8]
    var offset: Int

    def __init__(out self, var data: List[UInt8]):
        self.data = data^
        self.offset = 0

    def uint(mut self, count: Int) raises -> UInt64:
        if count > len(self.data) - self.offset:
            raise Error("truncated desktop target fixture")
        var value: UInt64 = 0
        for index in range(count):
            value |= UInt64(self.data[self.offset + index]) << UInt64(index * 8)
        self.offset += count
        return value

    def optional(mut self) raises -> Optional[UInt64]:
        var present = self.uint(1) != 0
        var value = self.uint(8)
        return value if present else None


def main() raises:
    var arguments = argv()
    var bytes: List[UInt8]
    with open(arguments[1], "r") as file:
        bytes = file.read_bytes()
    assert_true(len(bytes) <= 16 * 1024 * 1024)
    var source = Source(bytes^)
    var cases = 0
    while source.offset < len(source.data):
        var size = Int(source.uint(4))
        if size > 65536 or size > len(source.data) - source.offset:
            raise Error("desktop target frame exceeds fixture limit")
        var frame = List[UInt8](Span(source.data)[source.offset : source.offset + size])
        source.offset += size
        _ = validate_envelope(Span(frame))
        var reader = Decoder(frame^)
        assert_equal(reader.uint(), UInt64(10))
        assert_equal(reader.uint(), UInt64(0))
        assert_equal(reader.uint(), UInt64(0))
        var snapshot = read_snapshot(reader)
        reader.finish()
        var attached = source.optional()
        assert_equal(desktop_active_pane(snapshot, attached), source.optional())
        var target = desktop_snapshot_target(snapshot, attached)
        var expected = source.optional()
        var surface = UInt8(source.uint(1))
        assert_equal(Bool(target.active), Bool(expected))
        if target.active:
            assert_equal(target.active.value().kind, UInt8(1))
            assert_equal(target.active.value().pane, expected.value())
            assert_equal(target.active.value().surface, surface)
        assert_equal(target.capture, source.optional())
        cases += 1
    print(cases)
