from std.sys import argv
from std.testing import assert_equal, assert_true
from std.time import perf_counter_ns
from std.ffi import external_call
from zzmojo.stream_transport import connect_interactive, InteractiveConnection
from zzmojo.desktop_workspace import DesktopWorkspace
from zzmojo.desktop_router_delivery import DesktopKeyTarget
from zzmojo.actions import ActionQueue
from zzmojo.chrome_keymap import ChromeKeymap
from zzmojo.input_protocol import KeyInput
from zzmojo.verified_input_router import pane_owner
from zzmojo.palette_protocol import read_palette_command
from zzmojo.protocol import parse_id
from zzmojo.state import AppModel
from zzmojo.desktop_snapshot_target import desktop_snapshot_target


def raw(codepoint: UInt32, modifiers: UInt8 = 0, action: UInt8 = 0) -> KeyInput:
    var input = KeyInput()
    input.key = 0
    input.codepoint = codepoint
    input.unshifted = codepoint
    input.modifiers = modifiers
    input.action = action
    return input^


def wait(mut connection: InteractiveConnection, armed: Bool) raises:
    var started = perf_counter_ns()
    while perf_counter_ns() - started < 3_000_000_000:
        connection.drain()
        if (
            connection.model.key_tables.eligible
            and connection.model.view.session
            and connection.model.routing.armed == armed
            and not connection.model.key_tables.snapshot.value()[]
            .prefix.query(raw(120))
            .bound()
        ):
            return
        _ = external_call["usleep", Int32](UInt32(10000))
    raise Error("desktop prefix source state did not converge")


def chooser_opened(mut connection: InteractiveConnection) raises -> Bool:
    var started = perf_counter_ns()
    while perf_counter_ns() - started < 3_000_000_000:
        connection.drain()
        if connection.model.dialogs.chooser:
            return True
        _ = external_call["usleep", Int32](UInt32(10000))
    return False


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
        "unbind-key", [String("-T"), String("prefix"), String("x")]
    )
    assert_equal(response.exit_code, UInt8(0))
    wait(connection, False)
    var model = AppModel()
    model.refresh(connection)
    var actual_attachment = connection.model.view.session
    assert_equal(model.attached_session, actual_attachment)
    var other = connection.command("new-session", [String("-d"), String("-P"), String("-F"), String("#{session_id}"), String("-s"), String("optimistic"), String("cat")])
    assert_equal(other.exit_code, UInt8(0))
    var optimistic = parse_id(String(from_utf8=Span(other.output)).strip(), 36)
    model.refresh(connection)
    model.select_session(optimistic)
    model.refresh(connection)
    assert_equal(model.session, Optional[UInt64](optimistic))
    assert_equal(model.attached_session, actual_attachment)
    var projected = desktop_snapshot_target(model.snapshot, model.attached_session)
    var pane: Optional[UInt64] = None
    for session in connection.model.snapshot.sessions:
        if connection.model.view.session == Optional[UInt64](session.id):
            var focused = connection.model.snapshot.focused_window_for(session)
            for window in session.windows:
                if window.id == focused:
                    pane = window.active_pane
    assert_true(Bool(pane))
    assert_true(Bool(projected.active))
    assert_equal(projected.active.value().pane, pane.value())
    assert_equal(projected.active.value().surface, UInt8(0))
    var workspace = DesktopWorkspace(1, String(arguments[2]))
    workspace.bind_source(connection.server_id, connection.client_id)
    var chrome = ChromeKeymap(1, 2)
    _ = chrome.bind("prefix", "x", "ui-zoom-in")
    workspace.install_configured(chrome)
    var target = DesktopKeyTarget(
        pane_owner(pane.value(), 0), None, False, False, True
    )
    var queue = ActionQueue()
    key(workspace, connection, queue, raw(98, 2), target)
    wait(connection, True)
    key(workspace, connection, queue, raw(98, 2, 2), target)
    var plan = workspace.prepare(
        queue,
        connection.model.key_tables,
        connection.model.routing,
        raw(120),
        target,
        1,
    )
    assert_equal(plan.plan.value().ui[0].kind, UInt8(5))
    assert_equal(plan.prefix.value().request, Optional[UInt64](1))
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
    assert_equal(ui[0].kind, UInt8(5))
    deliver(connection, queue)
    wait(connection, False)
    assert_equal(connection.model.routing.cancelled_request, UInt64(1))
    key(workspace, connection, queue, raw(120, 0, 2), target)
    assert_equal(len(workspace.session.snapshot().held), 0)
    assert_equal(len(workspace.session.snapshot().local), 0)
    assert_equal(Bool(connection.model.dialogs.chooser), False)
    key(workspace, connection, queue, raw(98, 2), target)
    wait(connection, True)
    key(workspace, connection, queue, raw(98, 2, 2), target)
    plan = workspace.prepare(
        queue,
        connection.model.key_tables,
        connection.model.routing,
        raw(115),
        target,
        1,
    )
    assert_equal(len(plan.plan.value().ui), 0)
    assert_equal(Bool(plan.prefix), False)
    receipt = workspace.commit(
        queue,
        connection.model.key_tables,
        connection.model.routing,
        target,
        1,
        None,
        plan^,
    )
    assert_equal(len(workspace.acknowledge(receipt, 1)), 0)
    assert_equal(len(queue), 1)
    assert_equal(queue.items[0].kind, 10)
    deliver(connection, queue)
    wait(connection, False)
    assert_true(chooser_opened(connection))
    assert_equal(connection.model.routing.cancelled_request, UInt64(1))
    key(workspace, connection, queue, raw(115, 0, 2), target)
    assert_equal(len(workspace.session.snapshot().held), 0)
    assert_equal(len(workspace.session.snapshot().local), 0)
    assert_true(workspace.gate.native_count > 0)
    assert_equal(workspace.gate.fallback_count, UInt64(0))
    print(
        (
            "actual daemon prefix, verified key routing, prefix chrome"
            " cancellation and a picker key forwarded to the pane passed"
        ),
        workspace.gate.native_count,
    )
