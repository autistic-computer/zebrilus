from std.ffi import OwnedDLHandle
from std.sys.info import CompilationTarget
from std.testing import TestSuite, assert_equal
from zzmojo.client_context import MAX_CONTEXT_BYTES, MAX_CONTEXT_ENTRY
from zzmojo.exec_context import capture_exec_context
from zzmojo.exec_protocol import ExecRequest

comptime _Bytes = Pointer[UInt8, MutUntrackedOrigin]
comptime _Environment = Pointer[Optional[_Bytes], MutUntrackedOrigin]
comptime _Slot = Pointer[_Environment, MutUntrackedOrigin]


def capture(entries: List[List[UInt8]]) raises -> ExecRequest:
    var library = OwnedDLHandle()
    var slot: _Slot
    comptime if CompilationTarget.is_linux():
        slot = library.get_symbol[_Environment]("environ").value().unsafe_origin_cast[MutUntrackedOrigin]()
    else:
        var get_slot = library.get_function[_Slot]("_NSGetEnviron")
        slot = get_slot()
    var retained = List[List[UInt8]]()
    for entry in entries:
        var bytes = entry.copy()
        bytes.append(0)
        retained.append(bytes^)
    var pointers = List[Optional[_Bytes]]()
    for entry in retained:
        pointers.append(entry.unsafe_ptr().unsafe_mut_cast[True]().unsafe_origin_cast[MutUntrackedOrigin]())
    pointers.append(None)
    var previous = slot[]
    slot[] = pointers.unsafe_ptr().unsafe_origin_cast[MutUntrackedOrigin]()
    var request: ExecRequest
    try:
        request = capture_exec_context()
    except failure:
        slot[] = previous
        _ = pointers
        _ = retained
        _ = library
        raise failure^
    slot[] = previous
    _ = pointers
    _ = retained
    _ = library
    return request^


def test_order_duplicates_and_raw_entries() raises:
    var entries: List[List[UInt8]] = [
        List[UInt8]("Z=first".as_bytes()),
        List[UInt8]("A=middle".as_bytes()),
        List[UInt8]("Z=last".as_bytes()),
        [255, 61, 128],
        List[UInt8]("=invalid".as_bytes()),
        List[UInt8]("missing-separator".as_bytes()),
    ]
    var oversized = List[UInt8](length=MAX_CONTEXT_ENTRY + 1, fill=120)
    oversized[0] = 90
    oversized[1] = 61
    entries.append(oversized^)
    var request = capture(entries)
    var expected = List[UInt8]("Z=first\0A=middle\0Z=last\0".as_bytes())
    expected.extend([255, 61, 128, 0])
    assert_equal(request.environment, expected)
    assert_equal(request.flags, UInt8(34))


def test_environment_entry_and_total_byte_budgets() raises:
    var entries = List[List[UInt8]]()
    var expected = List[UInt8]()
    for _ in range(4097):
        entries.append(List[UInt8]("A=x".as_bytes()))
    var counted = capture(entries)
    for _ in range(4096):
        expected.extend("A=x\0".as_bytes())
    assert_equal(counted.environment, expected)
    entries.clear()
    expected.clear()
    var large = List[UInt8](length=MAX_CONTEXT_ENTRY, fill=120)
    large[0] = 65
    large[1] = 61
    for _ in range(256):
        entries.append(large.copy())
        expected.extend(Span(large))
        expected.append(0)
    entries.append(large.copy())
    var tail = List[UInt8](length=MAX_CONTEXT_BYTES - 256 * MAX_CONTEXT_ENTRY, fill=116)
    tail[0] = 66
    tail[1] = 61
    entries.append(tail.copy())
    entries.append(List[UInt8]("C=".as_bytes()))
    expected.extend(Span(tail))
    expected.append(0)
    var bounded = capture(entries)
    assert_equal(bounded.environment, expected)


def test_utf8_and_nested_locale_precedence() raises:
    var cases: List[List[String]] = [
        [],
        ["LANG=C.UTF-8"],
        ["LC_ALL=C", "LANG=C.UTF-8"],
        ["LC_ALL=", "LC_CTYPE=en_us.utf8", "LANG=C"],
        ["LC_CTYPE=C", "LANG=en_us.UTF-8"],
        ["TMUX=", "LC_ALL=C"],
        ["TMUX=/tmp/mux,1,0", "LC_ALL=C"],
        ["LANG=rawUTF8suffix"],
        ["LC_ALL=C", "LC_ALL=UTF8"],
        ["LC_ALL=UTF8", "LC_ALL=C"],
    ]
    var flags: List[UInt8] = [34, 35, 34, 35, 34, 35, 39, 35, 34, 35]
    for index in range(len(cases)):
        var entries = List[List[UInt8]]()
        for entry in cases[index]:
            entries.append(List[UInt8](entry.as_bytes()))
        var request = capture(entries)
        assert_equal(request.flags, flags[index])


def main() raises:
    var suite = TestSuite.discover_tests[__functions_in_module()]()
    _ = suite^.run()
