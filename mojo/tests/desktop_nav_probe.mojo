from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_optional_id
from zzmojo.exec_protocol import Invocation
from zzmojo.palette_protocol import (
    palette_command_request,
    PALETTE_COMMAND_BYTES,
)
from zzmojo.desktop_nav import (
    NavTarget,
    activation_for_target,
    new_session_commands,
    new_window_command,
    select_window_command,
    select_pane_commands,
    kill_target_command,
    picker_split_command,
    detach_command,
)


def check_commands(commands: List[Invocation], mut reader: Decoder) raises:
    assert_equal(len(commands), Int(reader.uint()))
    for command in commands:
        var actual = palette_command_request(0, command)
        assert_equal(actual, reader.bytes())


def bounds() raises:
    with assert_raises():
        _ = activation_for_target(0, NavTarget(3, 0), None, None, 0, None, True)
    with assert_raises():
        _ = picker_split_command(0, 2)
    var capabilities = List[String]()
    for _ in range(4097):
        capabilities.append(String())
    with assert_raises():
        _ = new_session_commands(True, capabilities, None)
    var raw = List[UInt8](capacity=PALETTE_COMMAND_BYTES)
    for _ in range(PALETTE_COMMAND_BYTES):
        raw.append(120)
    var directory = String(from_utf8=Span(raw))
    with assert_raises():
        _ = new_session_commands(True, List[String](), directory)
    var remote = new_session_commands(False, capabilities, directory)
    assert_equal(len(remote), 1)
    assert_equal(remote[0].name, String("new-session"))
    assert_equal(len(remote[0].args), 1)
    assert_equal(String(from_utf8=Span(remote[0].args[0])), String("-d"))


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    assert_true(len(bytes) <= 16 * 1024 * 1024)
    var reader = Decoder(bytes^)
    reader.offset = 0
    var activations = Int(reader.uint())
    for _ in range(activations):
        var host = UInt32(reader.uint())
        var kind = reader.byte()
        var id = reader.uint()
        var owner_session = read_optional_id(reader)
        var owner_window = read_optional_id(reader)
        var attached_host = UInt32(reader.uint())
        var attached_session = read_optional_id(reader)
        var connected = reader.byte() != 0
        var actual = activation_for_target(
            host,
            NavTarget(kind, id),
            owner_session,
            owner_window,
            attached_host,
            attached_session,
            connected,
        )
        var present = reader.byte() != 0
        assert_equal(Bool(actual), present)
        if present:
            var value = actual.take()
            assert_equal(value.kind, reader.byte())
            assert_equal(value.host, UInt32(reader.uint()))
            assert_equal(value.session, read_optional_id(reader))
            assert_equal(value.target, reader.text())
            check_commands(value.commands, reader)
    var sessions = Int(reader.uint())
    for _ in range(sessions):
        var local = reader.byte() != 0
        var capabilities = List[String]()
        for _ in range(Int(reader.uint())):
            capabilities.append(reader.text())
        var directory: Optional[String] = None
        if reader.byte() != 0:
            var raw = reader.bytes()
            directory = String(from_utf8_lossy=Span(raw))
            assert_equal(directory.value(), reader.text())
        check_commands(
            new_session_commands(local, capabilities, directory), reader
        )
    var standalone = Int(reader.uint())
    for _ in range(standalone):
        var kind = reader.byte()
        var id = reader.uint()
        var commands = List[Invocation]()
        if kind == 0:
            commands.append(new_window_command(id))
        elif kind == 1:
            commands.append(select_window_command(id))
        elif kind == 2:
            commands = select_pane_commands(id)
        elif kind <= 5:
            commands.append(kill_target_command(NavTarget(kind - 3, id)))
        elif kind <= 7:
            commands.append(picker_split_command(id, kind - 6))
        elif kind == 8:
            commands.append(detach_command())
        else:
            raise Error("unknown desktop navigation fixture")
        check_commands(commands, reader)
    reader.finish()
    bounds()
    print(
        "desktop navigation actual activation and command bytes passed",
        activations,
        sessions,
        standalone,
    )
