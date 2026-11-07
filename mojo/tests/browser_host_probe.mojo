from std.sys import argv
from std.testing import assert_equal, assert_raises, assert_true
from zzmojo.browser_host import BrowserHost
from browser_fixture import Source


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as input:
        bytes = input.read_bytes()
    assert_true(len(bytes) < 1024 * 1024)
    var source = Source(bytes^)
    var cases = source.uint(8)
    for _ in range(Int(cases)):
        var host = BrowserHost()
        var count = source.uint(8)
        for _ in range(Int(count)):
            var pane = source.uint(8)
            var tab = source.uint(8)
            var session = source.uint(8)
            host.bind(pane, tab, session)
        var event = source.uint(8)
        var expected = source.uint(1) != 0
        var actual = host.source(event)
        assert_equal(Bool(actual), expected)
        if expected:
            assert_equal(actual.value().pane, source.uint(8))
            assert_equal(actual.value().tab, source.uint(8))
            assert_equal(actual.value().session, event)
            host.validate(actual.value())
    assert_equal(source.offset, len(source.bytes))
    var host = BrowserHost()
    host.bind(1, 2, 3)
    var stale = host.source(3).value().copy()
    host.bind(1, 2, 4)
    assert_true(not host.source(3))
    with assert_raises():
        host.validate(stale)
    var current = host.source(4).value().copy()
    var before = host.generation
    with assert_raises():
        host.bind(99, 2, 4)
    assert_equal(host.generation, before)
    host.validate(current)
    host.retire(current)
    assert_true(not host.source(4))
    host.generation = UInt64.MAX
    with assert_raises():
        host.bind(1, 2, 9)
    assert_equal(len(host.sessions), 0)
    var first = BrowserHost()
    var second = BrowserHost()
    first.bind(9, 10, 11)
    second.bind(9, 10, 11)
    with assert_raises():
        second.validate(first.source(11).value())
    with assert_raises():
        second.validate_owner(first.source(11).value())
    print(cases)
