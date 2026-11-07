from std.sys import argv
from std.testing import assert_equal, assert_true
from std.time import perf_counter_ns
from std.ffi import external_call
from zzmojo.stream_transport import connect_interactive, InteractiveConnection
from zzmojo.desktop_workspace import DesktopWorkspace
from zzmojo.desktop_router_delivery import DesktopKeyTarget
from zzmojo.actions import ActionQueue, Action
from zzmojo.input_protocol import KeyInput
from zzmojo.verified_input_router import pane_owner
from zzmojo.exec_protocol import Invocation
from zzmojo.palette_protocol import (
    palette_command_request,
    read_palette_command,
)
from zzmojo.protocol import Decoder


def raw(codepoint: UInt32, modifiers: UInt8 = 0, action: UInt8 = 0) -> KeyInput:
    var input = KeyInput()
    input.key = 0
    input.codepoint = codepoint
    input.unshifted = codepoint
    input.modifiers = modifiers
    input.action = action
    return input^


def wait(
    mut connection: InteractiveConnection, armed: Bool, binding: Bool = True
) raises:
    var started = perf_counter_ns()
    while perf_counter_ns() - started < 3_000_000_000:
        connection.drain()
        if (
            connection.model.key_tables.eligible
            and connection.model.view.session
            and connection.model.routing.armed == armed
        ):
            if (
                not binding
                or connection.model.key_tables.snapshot.value()[]
                .prefix.query(raw(120))
                .sidebar_picker
            ):
                return
        _ = external_call["usleep", Int32](UInt32(10000))
    raise Error("desktop prefix source state did not converge")


def deliver(
    mut connection: InteractiveConnection, mut queue: ActionQueue
) raises:
    var actions = queue.take()
    for action in actions:
        if action.kind == 10:
            connection.transport.write(Span(action.frame))
        elif action.kind == 16:
            var request = read_palette_command(action.frame.copy())
            var response = connection.command_invocation(request.command)
            assert_equal(response.exit_code, UInt8(0), response.error)
        else:
            raise Error("desktop smoke encountered an unexpected action")


def key(
    mut workspace: DesktopWorkspace,
    mut connection: InteractiveConnection,
    mut queue: ActionQueue,
    input: KeyInput,
    target: DesktopKeyTarget,
) raises:
    var plan = workspace.prepare(
        queue,
        connection.model.key_tables,
        connection.model.routing,
        input,
        target,
        1,
    )
    assert_equal(plan.plan.value().native.value(), False)
    var receipt = workspace.commit(
        queue,
        connection.model.key_tables,
        connection.model.routing,
        target,
        1,
        None,
        plan^,
    )
    assert_equal(len(workspace.acknowledge(receipt, 1)), 0)
    deliver(connection, queue)


def main() raises:
    var arguments = argv()
    var connection = connect_interactive(
        arguments[1], "desktop-routing", native_keys=True
    )
    var response = connection.command(
        "bind-key",
        [
            String("-T"),
            String("prefix"),
            String("x"),
            String("choose-tree"),
            String("-s"),
        ],
    )
    assert_equal(response.exit_code, UInt8(0))
    wait(connection, False)
    var pane: Optional[UInt64] = None
    for session in connection.model.snapshot.sessions:
        if connection.model.view.session == Optional[UInt64](session.id):
            var focused = connection.model.snapshot.focused_window_for(session)
            for window in session.windows:
                if window.id == focused:
                    pane = window.active_pane
    assert_true(Bool(pane))
    var workspace = DesktopWorkspace(1, String(arguments[2]))
    workspace.bind_source(connection.server_id, connection.client_id)
    var target = DesktopKeyTarget(
        pane_owner(pane.value(), 0), None, False, False, True
    )
    var queue = ActionQueue()
    key(workspace, connection, queue, raw(98, 2), target)
    wait(connection, True)
    key(workspace, connection, queue, raw(98, 2, 2), target)
    _ = connection.model.take_events()
    var plan = workspace.prepare(
        queue,
        connection.model.key_tables,
        connection.model.routing,
        raw(120),
        target,
        1,
        picker_sidebar=True,
    )
    assert_equal(plan.plan.value().ui[0].kind, UInt8(4))
    assert_equal(plan.prefix.value().request, Optional[UInt64](1))
    var command = Invocation()
    command.name = "focus-sidebar"
    var actions: List[Action] = [
        Action(
            16,
            "",
            None,
            1,
            None,
            palette_command_request(0, command),
            (connection.server_id, connection.client_id),
            None,
        )
    ]
    workspace.stage_actions(queue, plan, actions)
    var receipt = workspace.commit(
        queue,
        connection.model.key_tables,
        connection.model.routing,
        target,
        1,
        None,
        plan^,
    )
    var ui = workspace.acknowledge(receipt, 1)
    assert_equal(len(ui), 1)
    assert_equal(ui[0].kind, UInt8(4))
    deliver(connection, queue)
    wait(connection, False, False)
    assert_equal(connection.model.routing.cancelled_request, UInt64(1))
    assert_equal(Bool(connection.model.dialogs.chooser), False)
    var expected: List[UInt8]
    with open(arguments[3], "r") as file:
        expected = file.read_bytes()
    var source = Decoder(expected^)
    assert_equal(source.uint(), UInt64(10))
    _ = source.uint()
    var focus_kind = source.uint()
    source.finish()
    var focused = False
    var events = connection.model.take_events()
    for event in events:
        var reader = Decoder(event.copy())
        if reader.uint() == 10:
            _ = reader.uint()
            if reader.uint() == focus_kind:
                reader.finish()
                focused = True
    assert_true(focused)
    key(workspace, connection, queue, raw(120, 0, 2), target)
    assert_equal(len(workspace.session.snapshot().held), 0)
    assert_equal(len(workspace.session.snapshot().local), 0)
    assert_true(workspace.gate.native_count > 0)
    assert_equal(workspace.gate.fallback_count, UInt64(0))
    print(
        (
            "actual daemon prefix, verified key routing, ordered cancel/sidebar"
            " command and suppressed release passed"
        ),
        workspace.gate.native_count,
    )
