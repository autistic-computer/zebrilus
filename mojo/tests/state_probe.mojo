from zzmojo.stream_transport import connect_interactive
from zzmojo.state import AppModel
from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true


def main() raises:
    var args = argv()
    var connection = connect_interactive(args[1], "")
    var model = AppModel()
    model.refresh(connection)
    model.sync_focus(connection)
    assert_equal(len(model.sessions), 1)
    assert_equal(len(model.panes), 1)
    assert_true(connection.model.view.session == model.session)
    model.split(connection, True)
    assert_equal(len(model.panes), 2)
    model.create_session(connection)
    assert_equal(len(model.sessions), 2)
    assert_equal(model.session.or_else(999), UInt64(1))
    model.send_key(
        connection, "printf '\\x53\\x54\\x41\\x54\\x45_STREAM_OK\\n'", True
    )
    model.send_key(connection, "Enter")
    for _ in range(100):
        model.refresh(connection)
        if "STATE_STREAM_OK" in model.terminal:
            break
        _ = external_call["usleep", Int32](UInt32(10000))
    assert_true("STATE_STREAM_OK" in model.terminal)
    assert_true(len(model.terminals) > 0)
    print(
        "modern state: attach, split, create, executed input, packed text"
        " passed"
    )
