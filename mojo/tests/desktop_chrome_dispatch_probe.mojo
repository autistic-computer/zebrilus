from std.sys import argv
from std.testing import assert_equal
from zzmojo.chrome_actions import ChromeActions
from zzmojo.desktop_chrome_dispatch import desktop_chrome_dispatch


def main() raises:
    var arguments = argv()
    var bytes: List[UInt8]
    with open(arguments[1], "r") as source:
        bytes = source.read_bytes()
    assert_equal(len(bytes), 561 * 6)
    var catalog = ChromeActions()
    for index in range(561):
        var offset = index * 6
        var code: UInt32 = 0
        for byte in range(4):
            code |= UInt32(bytes[offset + byte]) << UInt32(byte * 8)
        var action = catalog.decode(code)
        var actual = desktop_chrome_dispatch(action.value())
        assert_equal(actual.kind, bytes[offset + 4])
        assert_equal(actual.index, bytes[offset + 5])
    print("561 actual desktop chrome dispatch cases passed")
