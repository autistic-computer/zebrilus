from std.testing import assert_equal, assert_true, assert_false, assert_raises
from zzmojo.protocol import Decoder
from zzmojo.input_protocol import KeyInput
from zzmojo.terminal_search import SearchSession, SEARCH_ACCEPT_AND_CLOSE
from zzmojo.terminal_search_protocol import SearchQuery, search_request


def action(var frame: List[UInt8], pane: UInt64 = 7) raises -> UInt64:
    var reader = Decoder(frame^)
    assert_equal(reader.uint(), UInt64(9))
    assert_equal(reader.uint(), UInt64(5))
    assert_equal(reader.uint(), pane)
    return reader.uint()


def main() raises:
    var search = SearchSession()
    var opened = search.begin(7, 1)
    assert_equal(action(opened.frame.copy()), UInt64(17))
    assert_equal(opened.focus, UInt8(1))
    assert_true(opened.cancel_scroll)
    assert_true(opened.clear_marked)
    assert_equal(search.query.value().direction, UInt8(1))
    var changed = search.edit(7, "needle")
    assert_equal(action(changed.frame.copy()), UInt64(18))
    assert_true(changed.cancel_scroll)
    _ = search.append(7, "🦓")
    assert_equal(search.query.value().text, String("needle🦓"))
    assert_equal(action(search.enter(7, False).frame.copy()), UInt64(20))
    assert_equal(action(search.enter(7, True).frame.copy()), UInt64(19))
    assert_true(Bool(search.query))
    var toggled = search.toggle_case(7)
    assert_false(toggled.cancel_scroll)
    assert_equal(search.query.value().case_policy, UInt8(1))
    _ = search.toggle_case(7)
    assert_equal(search.query.value().case_policy, UInt8(2))
    _ = search.toggle_case(7)
    assert_equal(search.query.value().case_policy, UInt8(1))
    var input = KeyInput()
    input.key = 0
    input.codepoint = 99
    input.modifiers = 4
    _ = search.key_action(7, input)
    assert_equal(search.query.value().case_policy, UInt8(2))
    _ = search.key_action(7, input)
    assert_equal(search.query.value().case_policy, UInt8(0))
    assert_equal(search.query.value().direction, UInt8(1))
    assert_equal(search.query.value().text, String("needle🦓"))
    var closed = search.escape(7)
    assert_equal(action(closed.frame.copy()), UInt64(21))
    assert_equal(closed.focus, UInt8(2))
    assert_true(closed.clear_marked)
    assert_true(search.swallow_key(4, False))
    assert_true(search.swallow_key(4, True))
    assert_false(search.swallow_key(4, True))
    _ = search.begin(7, 1, SEARCH_ACCEPT_AND_CLOSE)
    _ = search.edit(7, "accepted")
    var accepted = search.enter(7, True)
    assert_equal(len(accepted.frame), 0)
    assert_equal(accepted.focus, UInt8(2))
    assert_false(Bool(search.query))
    assert_true(search.swallow_key(2, False))
    assert_true(search.swallow_key(2, False))
    assert_false(search.swallow_key(4, True))
    assert_true(search.swallow_key(2, True))
    assert_false(search.swallow_key(2, False))
    with assert_raises():
        _ = search.begin(7, 2)
    assert_false(Bool(search.query))
    with assert_raises():
        _ = search_request(7, 17, SearchQuery(mode=2))
    print("terminal search exact direction, native and keyboard case toggles, accept-without-close and swallowed release checks passed")
