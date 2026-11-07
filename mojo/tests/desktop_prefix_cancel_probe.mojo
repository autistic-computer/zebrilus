from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.actions import ActionQueue, MAX_PENDING_ACTIONS
from zzmojo.desktop_router_delivery import DesktopEnvelope
from zzmojo.desktop_prefix_cancel import (
    DesktopPrefixCancel,
    DesktopDialogPrefix,
    desktop_dialog_prefix,
    next_prefix_cancel_request,
)


struct Source(Movable):
    var data: List[UInt8]
    var offset: Int

    def __init__(out self, var data: List[UInt8]):
        self.data = data^
        self.offset = 0

    def byte(mut self) raises -> UInt8:
        if self.offset == len(self.data):
            raise Error("truncated prefix cancellation fixture")
        var value = self.data[self.offset]
        self.offset += 1
        return value

    def uint(mut self, count: Int) raises -> UInt64:
        var result: UInt64 = 0
        for index in range(count):
            result |= UInt64(self.byte()) << UInt64(index * 8)
        return result

    def optional(mut self) raises -> Optional[UInt64]:
        var present = self.byte() != 0
        var value = self.uint(8)
        return value if present else None


def main() raises:
    var arguments = argv()
    var bytes: List[UInt8]
    with open(arguments[1], "r") as file:
        bytes = file.read_bytes()
    assert_true(len(bytes) <= 1024 * 1024)
    var source = Source(bytes^)
    var cases = 0
    var context = DesktopEnvelope(1, 2, 3, 4, None)
    while source.offset < len(source.data):
        var previous = source.uint(8)
        var pending = source.optional()
        var sent = source.byte() != 0
        var active = source.byte() != 0
        var acknowledged = source.optional()
        var accepted = source.byte() != 0
        var request = source.byte() != 0
        var expected_next = source.uint(8)
        var expected_sent = source.byte() != 0
        var expected_pending = source.optional()
        var frame_size = Int(source.uint(4))
        var decision = desktop_dialog_prefix(
            DesktopDialogPrefix(sent, pending), active, acknowledged
        )
        assert_equal(decision.request, request)
        var machine = DesktopPrefixCancel()
        machine.next = previous
        machine.dialog = DesktopDialogPrefix(sent, pending)
        var queue = ActionQueue()
        var plan = machine.prepare_dialog(
            queue, context, active, acknowledged, accepted
        )
        assert_equal(machine.next, previous)
        assert_equal(len(queue), 0)
        machine.commit(queue, context, plan)
        assert_equal(machine.next, expected_next)
        assert_equal(machine.dialog.sent, expected_sent)
        assert_equal(machine.dialog.pending, expected_pending)
        assert_equal(len(queue), Int(frame_size != 0))
        if frame_size:
            assert_equal(len(queue.items[0].frame), frame_size)
            assert_equal(queue.items[0].source.value()[0], UInt64(2))
            assert_equal(queue.items[0].source.value()[1], UInt64(3))
            for index in range(frame_size):
                assert_equal(queue.items[0].frame[index], source.byte())
        with assert_raises():
            machine.commit(queue, context, plan)
        cases += 1
    var machine = DesktopPrefixCancel()
    var queue = ActionQueue()
    for _ in range(MAX_PENDING_ACTIONS):
        queue.append(10, "", None)
    with assert_raises():
        _ = machine.prepare_dialog(queue, context, True, None, True)
    assert_equal(machine.next, UInt64(0))
    assert_equal(machine.generation, UInt64(0))
    assert_equal(machine.dialog.sent, False)
    assert_equal(len(queue), MAX_PENDING_ACTIONS)
    _ = queue.take()
    var plan = machine.prepare_cancel(queue, context, True)
    queue.append(10, "", None)
    with assert_raises():
        machine.commit(queue, context, plan)
    assert_equal(machine.next, UInt64(0))
    _ = queue.take()
    plan = machine.prepare_cancel(queue, context, True)
    with assert_raises():
        machine.commit(queue, DesktopEnvelope(1, 2, 9, 4, None), plan)
    assert_equal(len(queue), 0)
    assert_equal(machine.next, UInt64(0))
    print(cases)
