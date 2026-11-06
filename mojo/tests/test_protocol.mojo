from std.testing import TestSuite, assert_equal, assert_true, assert_raises
from zzmojo.protocol import append_uint, envelope, validate_envelope, Decoder, client_hello, command_request, decode_response, parse_id
from zzmojo.state import AppModel, Session, Window, Pane, format_rows


def test_native_postcard_golden_vectors() raises:
    var hello = client_hello(300, 1234)
    var expected: List[UInt8] = [40, 0, 0, 0, 0, 0, 118, 0, 0, 118, 172, 2, 1, 1, 7, 122, 122, 32, 77, 111, 106, 111, 1, 14, 99, 108, 105, 101, 110, 116, 45, 117, 116, 102, 56, 45, 118, 49, 0, 0, 0, 0, 210, 9]
    assert_equal(hello, expected)
    var args: List[String] = ["-p", "héllo"]
    var command = command_request(300, "display-message", Span(args))
    var expected_command: List[UInt8] = [40, 0, 0, 0, 0, 0, 118, 0, 2, 172, 2, 15, 100, 105, 115, 112, 108, 97, 121, 45, 109, 101, 115, 115, 97, 103, 101, 2, 2, 45, 112, 6, 104, 195, 169, 108, 108, 111, 0, 0, 0, 0, 0, 0]
    assert_equal(command, expected_command)
    var payload: List[UInt8] = [3, 0, 172, 2, 3, 0, 255, 10, 7, 0, 2]
    var frame = envelope(Span(payload))
    var response = decode_response(frame^)
    assert_equal(response.request, UInt64(300))
    var bytes: List[UInt8] = [0, 255, 10]
    assert_equal(response.output, bytes)
    assert_equal(response.exit_code, UInt8(7))
    assert_equal(response.stdout_claim, UInt8(2))


def test_frame_and_integer_rejections() raises:
    var empty = List[UInt8]()
    with assert_raises():
        _ = validate_envelope(Span(empty))
    var payload: List[UInt8] = [1]
    var frame = envelope(Span(payload))
    frame[5] = 1
    with assert_raises():
        _ = validate_envelope(Span(frame))
    frame[5] = 0
    frame[6] = 117
    with assert_raises():
        _ = validate_envelope(Span(frame))
    var overflow: List[UInt8] = [0, 0, 0, 0, 0, 0, 118, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 2]
    var decoder = Decoder(overflow^)
    with assert_raises():
        _ = decoder.uint()
    assert_equal(parse_id("%18446744073709551615", 37), UInt64.MAX)
    with assert_raises():
        _ = parse_id("%18446744073709551616", 37)
    assert_equal(parse_id("%+1", 37), UInt64(1))
    with assert_raises():
        _ = parse_id("%+", 37)


def test_escaped_discovery_and_focus_reconciliation() raises:
    var text = String("$1\thé\\\tllo\\\nworld\n$2\tback\\\\slash")
    var rows = format_rows(text.as_bytes(), 2)
    assert_equal(rows[0][1], "hé\tllo\nworld")
    assert_equal(rows[1][1], "back\\slash")
    var model = AppModel()
    model.sessions.append(Session(1, "first"))
    model.sessions.append(Session(2, "second"))
    model.windows.append(Window(10, 1, "a", True))
    model.windows.append(Window(20, 2, "b", True))
    model.panes.append(Pane(100, 10, "shell", "zsh", True))
    model.panes.append(Pane(200, 20, "shell", "zsh", True))
    model.reconcile()
    assert_equal(model.session.or_else(0), UInt64(1))
    assert_equal(model.window.or_else(0), UInt64(10))
    assert_equal(model.pane.or_else(0), UInt64(100))
    model.select_session(2)
    assert_equal(model.window.or_else(0), UInt64(20))
    assert_equal(model.pane.or_else(0), UInt64(200))
    model.sessions.clear()
    model.reconcile()
    assert_true(not model.session and not model.window and not model.pane)


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
