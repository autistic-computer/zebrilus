from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.actions import ActionQueue, Action, MAX_PENDING_ACTIONS
from zzmojo.desktop_workspace import DesktopWorkspace
from zzmojo.desktop_router_delivery import DesktopKeyTarget
from zzmojo.desktop_key_tables import RetainedKeyTables
from zzmojo.chrome_keymap import ChromeKeymap
from zzmojo.input_protocol import (
    KeyInput,
    InputRoutingState,
    decode_input_frame,
)
from zzmojo.verified_input_router import (
    pane_owner,
    RouterState,
    RouterOutcome,
    RouterAnswer,
    RouterEvent,
    no_owner,
)
from zzmojo.input_router_codec import encode_outcome
from zzmojo.protocol import envelope


def same(actual: RouterState, expected: RouterState) raises:
    var left = encode_outcome(RouterOutcome(actual.copy(), RouterAnswer()))
    var right = encode_outcome(RouterOutcome(expected.copy(), RouterAnswer()))
    assert_equal(len(left), len(right))
    for index in range(len(left)):
        assert_equal(left[index], right[index])


def raw(action: UInt8 = 0) -> KeyInput:
    var input = KeyInput()
    input.key = 0
    input.codepoint = 120
    input.unshifted = UInt32(120)
    input.text = "x"
    input.action = action
    return input^


def full() raises -> RetainedKeyTables:
    var tables = RetainedKeyTables()
    tables.subscribed = True
    var bytes: List[UInt8] = [10, 0, 25, 0]
    assert_true(tables.consume(envelope(Span(bytes))))
    assert_true(tables.eligible)
    return tables^


def main() raises:
    var arguments = argv()
    var worker: Optional[String] = None
    if len(arguments) > 1:
        worker = String(arguments[1])
    var workspace = DesktopWorkspace(0, worker)
    workspace.bind_source(1, 2)
    var queue = ActionQueue()
    var tables = full()
    var routing = InputRoutingState()
    routing.armed = True
    var target = DesktopKeyTarget(pane_owner(77, 0), None, False, False, True)
    var before = workspace.session.snapshot()
    var generation = workspace.generation()
    var serial = workspace.session.generation()
    var plan = workspace.prepare(queue, tables, routing, raw(), target, 7)
    assert_equal(plan.plan.value().native.value(), False)
    same(workspace.session.snapshot(), before)
    assert_equal(workspace.generation(), generation)
    assert_equal(workspace.session.generation(), serial)
    var stale = workspace.prepare(queue, tables, routing, raw(), target, 7)
    var receipt = workspace.commit(
        queue, tables, routing, target, UInt64(7), None, plan^
    )
    assert_equal(workspace.session.generation(), serial + 2)
    assert_equal(workspace.generation(), generation + 1)
    assert_equal(workspace._next_payload, UInt64(2))
    assert_equal(len(queue), 1)
    assert_equal(len(workspace.session._payloads._inputs), 1)
    var input = decode_input_frame(queue.items[0].frame.copy())
    assert_equal(input.pane, UInt64(77))
    assert_equal(input.text_follows, False)
    with assert_raises():
        _ = workspace.commit(
            queue, tables, routing, target, UInt64(7), None, stale^
        )
    with assert_raises():
        _ = workspace.prepare(queue, tables, routing, raw(2), target, 7)
    assert_equal(len(workspace.acknowledge(receipt, 7)), 0)
    with assert_raises():
        _ = workspace.acknowledge(receipt, 7)
    _ = queue.take()
    var configured = ChromeKeymap(1, 99)
    _ = configured.bind("ui", "C-x", "ui-zoom-in")
    var held = workspace.session.snapshot()
    workspace.install_configured(configured)
    held.keymap = workspace.map.version()
    same(workspace.session.snapshot(), held)
    assert_equal(workspace.delivery.held()[0].envelope.epoch, UInt64(7))
    assert_true(Bool(workspace.configured.action_for("ui", "C-x")))
    assert_equal(Bool(workspace.map.action_for("ui", "C-x")), False)
    plan = workspace.prepare(queue, tables, routing, raw(1), target, 9)
    assert_equal(plan.plan.value().native.value(), False)
    receipt = workspace.commit(
        queue, tables, routing, target, UInt64(9), None, plan^
    )
    assert_equal(len(queue), 0)
    _ = workspace.acknowledge(receipt, 9)
    assert_equal(workspace.delivery.held()[0].envelope.epoch, UInt64(7))
    var released = DesktopKeyTarget(None, UInt64(99), True, True, False)
    tables.eligible = False
    plan = workspace.prepare(queue, tables, routing, raw(2), released, 11)
    receipt = workspace.commit(
        queue, tables, routing, released, UInt64(11), None, plan^
    )
    assert_equal(queue.items[0].selection_epoch, UInt64(7))
    assert_equal(queue.items[0].source.value()[0], UInt64(1))
    assert_equal(len(workspace.session.snapshot().held), 0)
    _ = workspace.acknowledge(receipt, 11)
    _ = queue.take()
    with assert_raises():
        _ = workspace.prepare(queue, tables, routing, raw(), target, 11)
    tables.eligible = True
    var changed = DesktopKeyTarget(pane_owner(88, 0), None, False, False, True)
    before = workspace.session.snapshot()
    generation = workspace.generation()
    plan = workspace.prepare(queue, tables, routing, raw(), target, 12)
    with assert_raises():
        _ = workspace.commit(queue, tables, routing, changed, 12, None, plan^)
    same(workspace.session.snapshot(), before)
    assert_equal(workspace.generation(), generation)
    plan = workspace.prepare(queue, tables, routing, raw(), target, 12)
    with assert_raises():
        _ = workspace.commit(queue, tables, routing, target, 13, None, plan^)
    same(workspace.session.snapshot(), before)
    plan = workspace.prepare(queue, tables, routing, raw(), target, 12)
    with assert_raises():
        _ = workspace.commit(
            queue, tables, routing, target, 12, UInt64(9), plan^
        )
    same(workspace.session.snapshot(), before)
    for _ in range(MAX_PENDING_ACTIONS):
        queue.append(10, "", UInt64(77))
    serial = workspace.session.generation()
    with assert_raises():
        _ = workspace.prepare(queue, tables, routing, raw(), target, 12)
    same(workspace.session.snapshot(), before)
    assert_equal(workspace.session.generation(), serial)
    assert_equal(len(queue), MAX_PENDING_ACTIONS)
    _ = queue.take()
    plan = workspace.prepare(queue, tables, routing, raw(), target, 12)
    var extra: List[Action] = [
        Action(
            10,
            "",
            UInt64(77),
            12,
            None,
            List[UInt8](length=4 * 1024 * 1024, fill=1),
            None,
            None,
        )
    ]
    before = workspace.session.snapshot()
    generation = workspace.generation()
    serial = workspace.session.generation()
    with assert_raises():
        workspace.stage_actions(queue, plan, extra)
    same(workspace.session.snapshot(), before)
    assert_equal(workspace.generation(), generation)
    assert_equal(workspace.session.generation(), serial)
    assert_equal(len(queue), 0)
    queue.append(1, "other", None)
    with assert_raises():
        _ = workspace.commit(
            queue, tables, routing, target, UInt64(12), None, plan^
        )
    same(workspace.session.snapshot(), before)
    assert_equal(workspace._next_payload, UInt64(4))
    _ = queue.take()
    plan = workspace.prepare(queue, tables, routing, raw(), target, 12)
    tables.revision += 1
    with assert_raises():
        _ = workspace.commit(
            queue, tables, routing, target, UInt64(12), None, plan^
        )
    same(workspace.session.snapshot(), before)
    plan = workspace.prepare(queue, tables, routing, raw(), target, 13)
    workspace.install_configured(configured)
    with assert_raises():
        _ = workspace.commit(
            queue, tables, routing, target, UInt64(13), None, plan^
        )
    assert_equal(len(queue), 0)
    serial = workspace.session.generation()
    workspace.bind_source(3, 4)
    assert_true(workspace.session.generation() > serial)
    assert_equal(len(workspace.delivery.held()), 0)
    assert_equal(workspace.source.value()[0], UInt64(3))
    assert_true(workspace._next_payload > 1)
    plan = workspace.prepare(queue, tables, routing, raw(), target, 14)
    receipt = workspace.commit(
        queue, tables, routing, target, UInt64(14), None, plan^
    )
    _ = workspace.acknowledge(receipt, 14)
    assert_equal(queue.items[0].source.value()[0], UInt64(3))
    assert_equal(workspace.delivery.held()[0].envelope.epoch, UInt64(14))
    _ = queue.take()
    for index in range(100):
        plan = workspace.prepare(
            queue, tables, routing, raw(1), target, UInt64(index + 15)
        )
        receipt = workspace.commit(
            queue, tables, routing, target, UInt64(index + 15), None, plan^
        )
        _ = workspace.acknowledge(receipt, UInt64(index + 15))
        assert_equal(len(workspace.session._payloads._inputs), 1)
        assert_equal(len(workspace.session._payloads._keys), 1)
    var events: List[RouterEvent] = [RouterEvent(7, 77, 0, no_owner())]
    workspace.observe(events)
    assert_equal(len(workspace.session.snapshot().held), 0)
    assert_equal(len(workspace.delivery.held()), 0)
    assert_equal(len(workspace.session._payloads._inputs), 0)
    assert_equal(len(workspace.session._payloads._bodies), 0)
    serial = workspace.session.generation()
    events = [RouterEvent(9, 0, 0, no_owner())]
    workspace.observe(events)
    assert_true(workspace.session.generation() > serial)
    events = [RouterEvent(2, 0, 0, no_owner())]
    before = workspace.session.snapshot()
    generation = workspace.generation()
    with assert_raises():
        workspace.observe(events)
    same(workspace.session.snapshot(), before)
    assert_equal(workspace.generation(), generation)
    print(workspace.gate.native_count, workspace.gate.fallback_count)
