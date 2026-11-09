from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.actions import Action, ActionQueue, MAX_PENDING_ACTIONS, MAX_PENDING_INPUT_BYTES
from zzmojo.geometry import GridSize
from zzmojo.chrome_keymap import ChromeKeymap
from zzmojo.input_protocol import KeyInput, InputFrame, InputRoutingState, encode_input_frame, decode_input_frame
from zzmojo.verified_input_router import RouterState, RouterKey, RouterPayload, RouterInput, RouterHeld, RouterOperation, RouterOutcome, RouterAnswer, pane_owner
from zzmojo.input_router_gate import InputRouterGate, InputRouterSession, RouterPayloadArena
from zzmojo.input_router_codec import encode_outcome
from zzmojo.desktop_router_delivery import DesktopDelivery, DesktopEnvelope, DesktopKeyTarget
from zzmojo.desktop_input import desktop_router_map
from zzmojo.chrome_actions import ChromeAction
from zzmojo.prefix_bindings import PrefixBindings, PrefixTable, PrefixBinding
from zzmojo.exec_protocol import Invocation


def same(actual: RouterState, expected: RouterState) raises:
    var left = encode_outcome(RouterOutcome(actual.copy(), RouterAnswer()))
    var right = encode_outcome(RouterOutcome(expected.copy(), RouterAnswer()))
    assert_equal(len(left), len(right))
    for index in range(len(left)):
        assert_equal(left[index], right[index])


def raw(action: UInt8 = 0) -> KeyInput:
    var key = KeyInput()
    key.key = 0
    key.codepoint = 120
    key.action = action
    key.text = "x"
    key.unshifted = UInt32(120)
    return key^


def payloads(map: ChromeKeymap) raises -> RouterPayloadArena:
    var inputs = List[RouterInput]()
    var bodies = List[List[UInt8]]()
    var state = RouterState(map.version())
    for action in range(3):
        var key = raw(UInt8(action))
        var operation = map.key_operation(state, key, RouterPayload(UInt64(action + 1)), False, False)
        inputs.append(operation.input.copy())
        var frame = InputFrame()
        frame.key = key^
        bodies.append(encode_input_frame(frame))
    var keys: List[RouterKey] = [RouterKey(120)]
    return RouterPayloadArena(10, keys^, inputs^, bodies^)


def sequence(map: ChromeKeymap, session: InputRouterSession, action: UInt8, armed: Bool = False) raises -> List[RouterOperation]:
    var operation = map.key_operation(session.snapshot(), raw(action), RouterPayload(UInt64(action + 1)), armed, action != 2)
    var operations: List[RouterOperation] = [operation^, RouterOperation(4)]
    return operations^


def near_capacity(mut gate: InputRouterGate) raises:
    var map = ChromeKeymap(0, 40)
    var state = RouterState(map.version())
    state.active = pane_owner(77, 0)
    var keys = List[RouterKey]()
    for index in range(4096):
        keys.append(RouterKey(UInt64(index)))
        if index < 4095:
            state.held.append(RouterHeld(RouterKey(UInt64(index)), 77))
    var key = raw()
    key.codepoint = 4095
    key.unshifted = None
    key.text = None
    var press = map.key_operation(state, key, RouterPayload(1), False, True)
    key.action = 1
    var repeat = map.key_operation(state, key, RouterPayload(2), True, True)
    var inputs: List[RouterInput] = [press.input.copy(), repeat.input.copy()]
    var bodies: List[List[UInt8]] = [List[UInt8](), List[UInt8]()]
    var session = InputRouterSession(state^, map.action_arena(), RouterPayloadArena(50, keys^, inputs^, bodies^))
    var operations: List[RouterOperation] = [press.copy(), repeat^, RouterOperation(4)]
    var before = session.snapshot()
    with assert_raises():
        _ = session.prepare_batch(gate, operations)
    same(session.snapshot(), before)
    assert_equal(session.generation(), UInt64(0))
    operations = [press^, RouterOperation(4)]
    var batch = session.prepare_batch(gate, operations)
    assert_equal(len(batch.state.held), 4096)
    assert_equal(len(batch.delivery), 1)
    session.commit_batch(batch)
    assert_equal(session.generation(), UInt64(1))
    var empty = List[RouterOperation]()
    batch = session.prepare_batch(gate, empty)
    assert_equal(len(session.take_effects()), 1)
    with assert_raises():
        session.commit_batch(batch)
    assert_equal(len(session.snapshot().held), 4096)


def authenticated_key(mut gate: InputRouterGate) raises:
    var configured = ChromeKeymap(0, 101)
    var map = desktop_router_map(configured, 102)
    var state = RouterState(map.version())
    state.active = pane_owner(77, 0)
    var session = InputRouterSession(state^, map.action_arena(), payloads(map))
    var queue = ActionQueue()
    var prefix = PrefixBindings()
    var routing = InputRoutingState()
    var delivery = DesktopDelivery()
    var context = DesktopEnvelope(10, 1, 2, 7, None)
    var target = DesktopKeyTarget(pane_owner(77, 0), None, False, False, True)
    var press = map.key_operation(session.snapshot(), raw(), RouterPayload(1), False, False)
    var forged = press.copy()
    forged.lookups[0] = map.action_reference(ChromeAction(1, 0))
    var operations: List[RouterOperation] = [forged^, RouterOperation(4)]
    var before = session.snapshot()
    with assert_raises():
        _ = delivery._prepare(session, gate, queue, map, configured, prefix, operations, context, False, False)
    same(session.snapshot(), before)
    forged = press.copy()
    _ = forged.lookups.pop()
    operations = [forged^, RouterOperation(4)]
    with assert_raises():
        _ = delivery._prepare(session, gate, queue, map, configured, prefix, operations, context, False, False)
    same(session.snapshot(), before)
    forged = press.copy()
    forged.armed = True
    operations = [forged^, RouterOperation(4)]
    with assert_raises():
        _ = delivery._prepare(session, gate, queue, map, configured, prefix, operations, context, False, False)
    same(session.snapshot(), before)
    assert_equal(len(queue), 0)
    var plan = delivery.prepare_key(session, gate, queue, map, configured, prefix, routing, press.input, target, context, False, False)
    assert_true(plan.native.value())
    assert_equal(len(plan.router.answers), 3)
    assert_equal(plan.router.answers[1].kind, UInt8(1))
    assert_true(plan.router.answers[1].value)
    var receipt = delivery.commit_key(session, queue, configured, prefix, routing, context, plan^)
    assert_equal(len(queue), 0)
    _ = delivery.acknowledge(receipt, context)
    plan = delivery.prepare_key(session, gate, queue, map, configured, prefix, routing, press.input, target, context, False, False)
    before = session.snapshot()
    routing.armed = True
    with assert_raises():
        _ = delivery.commit_key(session, queue, configured, prefix, routing, context, plan^)
    same(session.snapshot(), before)
    assert_equal(len(queue), 0)
    plan = delivery.prepare_key(session, gate, queue, map, configured, prefix, routing, press.input, target, context, False, False)
    assert_equal(plan.native.value(), False)
    receipt = delivery.commit_key(session, queue, configured, prefix, routing, context, plan^)
    assert_equal(len(queue), 1)
    _ = queue.take()
    _ = delivery.acknowledge(receipt, context)
    var release = map.key_operation(session.snapshot(), raw(2), RouterPayload(3), False, False)
    target = DesktopKeyTarget(None, UInt64(99), True, True, False)
    var newer = DesktopEnvelope(10, 1, 2, 9, None)
    plan = delivery.prepare_key(session, gate, queue, map, configured, prefix, routing, release.input, target, newer, False, False)
    assert_equal(plan.native.value(), False)
    assert_equal(len(plan.router.answers), 2)
    receipt = delivery.commit_key(session, queue, configured, prefix, routing, newer, plan^)
    assert_equal(queue.items[0].pane.value(), UInt64(77))
    assert_equal(queue.items[0].selection_epoch, UInt64(7))
    assert_equal(session.snapshot().active.value().pane, UInt64(77))
    assert_equal(Bool(session.snapshot().capture), False)
    _ = delivery.acknowledge(receipt, newer)


def main() raises:
    var arguments = argv()
    var path: Optional[String] = None
    if len(arguments) > 1:
        path = String(arguments[1])
    var gate = InputRouterGate(path)
    var map = ChromeKeymap(0, 20)
    var state = RouterState(map.version())
    state.active = pane_owner(77, 0)
    var session = InputRouterSession(state^, map.action_arena(), payloads(map))
    var prefix = PrefixBindings()
    var queue = ActionQueue()
    var delivery = DesktopDelivery()
    var context = DesktopEnvelope(10, 1, 2, 7, None)
    var before = session.snapshot()
    var plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 0), context, False, False)
    same(session.snapshot(), before)
    assert_equal(len(queue), 0)
    var stale = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 0), context, False, False)
    var receipt = delivery.commit(session, queue, map, prefix, context, plan^)
    assert_equal(session.generation(), UInt64(1))
    assert_equal(len(queue), 1)
    assert_equal(delivery.held()[0].envelope.epoch, UInt64(7))
    with assert_raises():
        _ = delivery.commit(session, queue, map, prefix, context, stale^)
    with assert_raises():
        _ = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 2), context, False, False)
    with assert_raises():
        delivery.install_map(session, map.action_arena())
    with assert_raises():
        delivery.replace_payloads(session, payloads(map))
    assert_equal(len(delivery.acknowledge(receipt, context)), 0)
    with assert_raises():
        _ = delivery.acknowledge(receipt, context)
    var held_before = session.snapshot()
    _ = map.bind("prefix", "z", "new-window")
    delivery.install_map(session, map.action_arena())
    held_before.keymap = map.version()
    same(session.snapshot(), held_before)
    delivery.replace_payloads(session, payloads(map))
    assert_equal(delivery.held()[0].envelope.epoch, UInt64(7))
    _ = queue.take()
    var newer = DesktopEnvelope(10, 1, 2, 9, None)
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 1, True), newer, True, False)
    assert_equal(plan.native.value(), False)
    receipt = delivery.commit(session, queue, map, prefix, newer, plan^)
    assert_equal(len(queue), 0)
    assert_equal(delivery.held()[0].envelope.epoch, UInt64(7))
    _ = delivery.acknowledge(receipt, newer)
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 2), newer, False, False)
    assert_equal(plan.native.value(), False)
    receipt = delivery.commit(session, queue, map, prefix, newer, plan^)
    assert_equal(len(delivery.held()), 0)
    var actions = queue.take()
    assert_equal(len(actions), 1)
    assert_equal(actions[0].kind, 10)
    assert_equal(actions[0].selection_epoch, UInt64(7))
    assert_equal(actions[0].source.value()[0], UInt64(1))
    assert_equal(actions[0].pane.value(), UInt64(77))
    var release = decode_input_frame(actions[0].frame.copy())
    assert_equal(release.key.value().action, UInt8(2))
    assert_equal(release.pane, UInt64(77))
    assert_equal(release.text_follows, False)
    _ = delivery.acknowledge(receipt, newer)
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 0), context, False, False)
    receipt = delivery.commit(session, queue, map, prefix, context, plan^)
    _ = delivery.acknowledge(receipt, context)
    _ = queue.take()
    var capture = RouterOperation(1)
    capture.capture = UInt64(88)
    var captured: List[RouterOperation] = [capture^]
    var repeat = sequence(map, session, 1, True)
    for operation in repeat:
        captured.append(operation.copy())
    plan = delivery._prepare(session, gate, queue, map, map, prefix, captured, newer, True, False)
    receipt = delivery.commit(session, queue, map, prefix, newer, plan^)
    assert_equal(delivery.held()[0].pane, UInt64(88))
    assert_equal(delivery.held()[0].envelope.epoch, UInt64(9))
    assert_equal(queue.items[0].pane.value(), UInt64(88))
    _ = delivery.acknowledge(receipt, newer)
    _ = queue.take()
    var changed = DesktopEnvelope(10, 1, 2, 11, None)
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 2), changed, False, False)
    receipt = delivery.commit(session, queue, map, prefix, changed, plan^)
    assert_equal(queue.items[0].pane.value(), UInt64(88))
    assert_equal(queue.items[0].selection_epoch, UInt64(9))
    _ = delivery.acknowledge(receipt, changed)
    _ = queue.take()
    var reset_capture = RouterOperation(1)
    var reset_ops: List[RouterOperation] = [reset_capture^, RouterOperation(4)]
    plan = delivery._prepare(session, gate, queue, map, map, prefix, reset_ops, newer, False, False)
    receipt = delivery.commit(session, queue, map, prefix, newer, plan^)
    _ = delivery.acknowledge(receipt, newer)
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 0), newer, False, False)
    before = session.snapshot()
    var generation = session.generation()
    with assert_raises():
        _ = delivery.commit(session, queue, map, prefix, DesktopEnvelope(10, 8, 9, 9, None), plan^)
    same(session.snapshot(), before)
    assert_equal(session.generation(), generation)
    assert_equal(len(queue), 0)
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 0), newer, False, False)
    prefix.full(List[PrefixTable]())
    with assert_raises():
        _ = delivery.commit(session, queue, map, prefix, newer, plan^)
    same(session.snapshot(), before)
    assert_equal(len(queue), 0)
    var configured = ChromeKeymap(0, 99)
    plan = delivery._prepare(session, gate, queue, map, configured, prefix, sequence(map, session, 0), newer, False, False)
    _ = configured.bind("prefix", "x", "new-window")
    with assert_raises():
        _ = delivery.commit(session, queue, configured, prefix, newer, plan^)
    same(session.snapshot(), before)
    assert_equal(len(queue), 0)
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 0), newer, False, False)
    queue.append(1, "a", None)
    with assert_raises():
        _ = delivery.commit(session, queue, map, prefix, newer, plan^)
    same(session.snapshot(), before)
    assert_equal(len(queue), 1)
    _ = queue.take()
    var operations = sequence(map, session, 0)
    var batch = session.prepare_batch(gate, operations)
    session.replace_payloads(payloads(map))
    with assert_raises():
        session.commit_batch(batch)
    same(session.snapshot(), before)
    batch = session.prepare_batch(gate, operations)
    _ = map.bind("prefix", "z", "new-window")
    session.install_map(map.action_arena())
    before = session.snapshot()
    with assert_raises():
        session.commit_batch(batch)
    same(session.snapshot(), before)
    var over = List[RouterOperation]()
    for _ in range(33):
        over.append(RouterOperation(1))
    with assert_raises():
        _ = session.prepare_batch(gate, over)
    same(session.snapshot(), before)
    for _ in range(MAX_PENDING_ACTIONS - 1):
        queue.append(1, "", None)
    var two = sequence(map, session, 0)
    two.append(map.key_operation(session.snapshot(), raw(0), RouterPayload(1), False, True))
    two.append(RouterOperation(4))
    with assert_raises():
        _ = delivery._prepare(session, gate, queue, map, map, prefix, two, newer, False, False)
    same(session.snapshot(), before)
    assert_equal(len(queue), MAX_PENDING_ACTIONS - 1)
    _ = queue.take()
    var packed = List[UInt8](length=MAX_PENDING_INPUT_BYTES, fill=0)
    queue.append(1, "", None, frame=packed)
    with assert_raises():
        _ = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 0), newer, False, False)
    same(session.snapshot(), before)
    assert_equal(queue.pending_bytes, MAX_PENDING_INPUT_BYTES)
    _ = queue.take()
    var command = Invocation()
    command.name = "list-keys"
    command.args.append([45, 78])
    var commands: List[Invocation] = [command^]
    var bindings: List[PrefixBinding] = [PrefixBinding("Any", commands^, False, None)]
    var tables: List[PrefixTable] = [PrefixTable("prefix", bindings^)]
    prefix.full(tables)
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 0, True), newer, True, True, String("C-λ"))
    receipt = delivery.commit(session, queue, map, prefix, newer, plan^)
    assert_equal(len(queue), 0)
    assert_equal(len(session.snapshot().local), 1)
    prefix.reset_connection(List[PrefixTable]())
    var ui = delivery.acknowledge(receipt, newer)
    assert_equal(len(ui), 1)
    assert_equal(ui[0].kind, UInt8(3))
    assert_equal(ui[0].pane.value(), UInt64(77))
    assert_equal(ui[0].canonical_prefix.value(), "C-λ")
    assert_equal(len(ui[0].bindings), 1)
    assert_equal(ui[0].bindings[0].commands[0].name, "list-keys")
    plan = delivery._prepare(session, gate, queue, map, map, prefix, sequence(map, session, 2), newer, False, False)
    receipt = delivery.commit(session, queue, map, prefix, newer, plan^)
    assert_equal(len(queue), 0)
    assert_equal(len(session.snapshot().local), 0)
    assert_equal(len(delivery.held()), 0)
    _ = delivery.acknowledge(receipt, newer)
    var geometry = ActionQueue()
    geometry.append(5, "a", UInt64(77), geometry=GridSize(80, 24, 8, 16, 0))
    var extra: List[Action] = [Action(1, "x", None, 0, None, List[UInt8](), None, None)]
    var staged = geometry.stage(extra)
    geometry.append(5, "b", UInt64(77), geometry=GridSize(80, 24, 8, 16, 0))
    with assert_raises():
        geometry.commit_batch(staged)
    assert_equal(len(geometry), 1)
    assert_equal(geometry.items[0].text, "b")
    var invalid_payloads = payloads(map)
    invalid_payloads._bodies[0].append(0)
    invalid_payloads._bodies[0][0] += 1
    var invalid_state = RouterState(map.version())
    invalid_state.active = pane_owner(77, 0)
    var invalid_session = InputRouterSession(invalid_state^, map.action_arena(), invalid_payloads^)
    before = invalid_session.snapshot()
    with assert_raises():
        _ = delivery._prepare(invalid_session, gate, queue, map, map, prefix, sequence(map, invalid_session, 0), newer, False, False)
    same(invalid_session.snapshot(), before)
    assert_equal(invalid_session.generation(), UInt64(0))
    assert_equal(len(queue), 0)
    near_capacity(gate)
    authenticated_key(gate)
    print("desktop delivery atomic/source/held/one-shot checks passed", gate.native_count, gate.fallback_count)
