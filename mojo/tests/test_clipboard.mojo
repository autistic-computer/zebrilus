from std.testing import TestSuite, assert_equal, assert_true
from mjui.input import Clipboard
from zzmojo.clipboard_protocol import (
    ClipboardEvent,
    ClipboardState,
    MAX_CLIPBOARD_REPLY_BYTES,
    MAX_CLIPBOARD_EVENTS,
    decode_clipboard_reply,
)
from zzmojo.clipboard_host import deliver_clipboard
from zzmojo.terminal_commit import terminal_commit
from zzmojo.input_protocol import decode_input_frame


struct Host(Clipboard, Movable):
    var clipboard: String
    var primary: String
    var clipboard_writes: Int
    var primary_writes: Int
    var primary_supported: Bool

    def __init__(out self, primary: Bool = True):
        self.clipboard = "original clipboard"
        self.primary = "original primary"
        self.clipboard_writes = 0
        self.primary_writes = 0
        self.primary_supported = primary

    def get_clipboard(self) -> String:
        return self.clipboard.copy()

    def set_clipboard(mut self, text: String):
        self.clipboard = text
        self.clipboard_writes += 1

    def supports_primary(self) -> Bool:
        return self.primary_supported

    def get_primary_selection(self) -> String:
        return self.primary.copy()

    def set_primary_selection(mut self, text: String):
        self.primary = text
        self.primary_writes += 1


def write(
    text: String, producer: UInt8 = 0, target: UInt8 = 1
) -> ClipboardEvent:
    var event = ClipboardEvent()
    event.pane = UInt64.MAX
    event.request_id = 0
    event.text = text
    event.producer = producer
    event.target = target
    return event^


def test_target_producer_and_empty_writes_preserve_host_ownership() raises:
    var host = Host()
    var state = ClipboardState()
    state.apply(write("server selection"))
    var delivery = deliver_clipboard(host, state)
    assert_equal(host.clipboard, "server selection")
    assert_equal(host.primary, "server selection")
    assert_equal(len(delivery.copied_panes), 1)
    assert_equal(delivery.copied_panes[0], UInt64.MAX)
    assert_true(not delivery.reply)
    state = ClipboardState()
    state.apply(write("application selection", 1))
    state.apply(write(""))
    delivery = deliver_clipboard(host, state, String("on"))
    assert_equal(host.clipboard, "server selection")
    assert_equal(host.primary, "application selection")
    assert_equal(len(delivery.copied_panes), 0)
    state = ClipboardState()
    state.apply(write("disabled mirror"))
    _ = deliver_clipboard(host, state, String("off"))
    assert_equal(host.clipboard, "server selection")
    assert_equal(host.primary, "disabled mirror")
    state = ClipboardState()
    state.apply(write("explicit clipboard", 1, 0))
    _ = deliver_clipboard(host, state, String("off"))
    assert_equal(host.clipboard, "explicit clipboard")
    assert_equal(host.clipboard_writes, 2)
    assert_equal(host.primary_writes, 3)
    var no_primary = Host(False)
    state = ClipboardState()
    state.apply(write("server mirror"))
    _ = deliver_clipboard(no_primary, state)
    assert_equal(no_primary.clipboard, "server mirror")
    assert_equal(no_primary.primary_writes, 0)


def test_query_coalesces_and_keeps_connection_identity_without_replaying_writes() raises:
    var host = Host()
    var state = ClipboardState()
    var query = ClipboardEvent()
    query.kind = 63
    state.apply(query, 10, 20)
    state.apply(query, 10, 20)
    state.apply(write("héllo 世界\n", 0, 0))
    var next = ClipboardState()
    next.apply(query, 11, 21)
    state.merge(next^)
    var taken = state.take()
    assert_true(not state.query)
    assert_equal(len(state.writes), 0)
    assert_equal(state.pending_bytes, 0)
    var delivery = deliver_clipboard(host, taken)
    assert_equal(host.clipboard_writes, 1)
    assert_true(Bool(delivery.reply))
    assert_equal(delivery.reply.value().server, UInt64(11))
    assert_equal(delivery.reply.value().client, UInt64(21))
    var bytes = decode_clipboard_reply(delivery.reply.value().frame.copy())
    assert_equal(String(from_utf8=Span(bytes)), "héllo 世界\n")
    _ = deliver_clipboard(host, state.take())
    assert_equal(host.clipboard_writes, 1)


def test_mailbox_and_host_reply_bounds_reject_atomically() raises:
    var state = ClipboardState()
    var event = write("")
    for _ in range(MAX_CLIPBOARD_EVENTS):
        state.apply(event)
    var rejected = False
    try:
        state.apply(event)
    except:
        rejected = True
    assert_true(rejected)
    assert_equal(len(state.writes), MAX_CLIPBOARD_EVENTS)
    state = ClipboardState()
    event.text = String("x") * MAX_CLIPBOARD_REPLY_BYTES
    state.apply(event)
    state.apply(event)
    event.text = "x"
    rejected = False
    try:
        state.apply(event)
    except:
        rejected = True
    assert_true(rejected)
    assert_equal(len(state.writes), 2)
    assert_equal(state.pending_bytes, 2 * MAX_CLIPBOARD_REPLY_BYTES)
    state = ClipboardState()
    event.kind = 63
    state.apply(event, 1, 2)
    var host = Host()
    host.clipboard = String("x") * MAX_CLIPBOARD_REPLY_BYTES
    var result = deliver_clipboard(host, state)
    assert_true(Bool(result.reply))
    assert_true(len(result.reply.value().frame) > MAX_CLIPBOARD_REPLY_BYTES)
    host.clipboard += "x"
    rejected = False
    try:
        _ = deliver_clipboard(host, state)
    except:
        rejected = True
    assert_true(rejected)


def test_ordinary_commits_keep_native_paste_and_composition_lanes() raises:
    assert_true(not terminal_commit(7, ""))
    for text in [String("q"), "é", "🦀"]:
        var frame = decode_input_frame(terminal_commit(7, text).value().copy())
        assert_equal(frame.kind, UInt8(0))
        assert_equal(frame.pane, UInt64(7))
        assert_equal(frame.text, text)
    for text in [String("qq"), "e\u0301", "世界", "\n", "a\rb"]:
        for output in [False, True]:
            var frame = decode_input_frame(
                terminal_commit(7, text, command_output=output).value().copy()
            )
            assert_equal(frame.kind, UInt8(7 if output else 5))
            assert_equal(frame.view_action, UInt8(24))
            assert_equal(frame.text, text)
        var composed = decode_input_frame(
            terminal_commit(7, text, composed=True).value().copy()
        )
        assert_equal(composed.kind, UInt8(0))
        assert_equal(composed.text, text)


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
