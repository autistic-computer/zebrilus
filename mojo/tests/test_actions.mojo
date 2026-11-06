from std.testing import assert_equal, assert_raises, TestSuite
from zzmojo.actions import ActionQueue


def test_pending_input_has_an_aggregate_memory_bound() raises:
    var bytes = List[UInt8](length=1024 * 1024, fill=97)
    var text = String(from_utf8=Span(bytes))
    var queue = ActionQueue()
    for _ in range(4):
        queue.append(2, text, UInt64(7))
    with assert_raises():
        queue.append(2, "x", UInt64(7))
    var actions = queue.take()
    assert_equal(len(actions), 4)
    assert_equal(actions[3].text.byte_length(), 1024 * 1024)
    assert_equal(actions[3].pane.or_else(0), UInt64(7))
    for _ in range(4096):
        queue.append(1, "", None)
    with assert_raises():
        queue.append(1, "", None)
    _ = queue.take()
    queue.append(2, text, UInt64(8))
    var resumed = queue.take()
    assert_equal(len(resumed), 1)
    assert_equal(resumed[0].pane.or_else(0), UInt64(8))


def test_binary_input_counts_toward_the_same_queue_bound() raises:
    var queue = ActionQueue()
    var frame = List[UInt8](length=1024 * 1024, fill=0)
    for _ in range(4):
        queue.append(9, "", UInt64(0), frame=frame)
    with assert_raises():
        queue.append(2, "x", UInt64(0))
    var actions = queue.take()
    assert_equal(len(actions[3].frame), 1024 * 1024)
    assert_equal(actions[3].pane.value(), UInt64(0))


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
