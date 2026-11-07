from std.sys import argv
from std.testing import assert_equal, assert_true, assert_false, assert_raises
from zzmojo.verified_input_router import (
    RouterKey, RouterPayload, RouterAction, RouterInput, RouterState,
    RouterHeld, RouterEvent, RouterEffect, RouterOperation, RouterOutcome,
    RouterAnswer, MAX_ROUTER_CLAIMS, MAX_ROUTER_EFFECTS, pane_owner,
)
from zzmojo.input_router_codec import encode_outcome, native_eligible
from zzmojo.input_router_gate import (
    RouterMapArena, RouterPayloadArena, InputRouterGate, InputRouterSession,
    MAX_ROUTER_PAYLOAD_BYTES,
)


def same(actual: RouterState, expected: RouterState) raises:
    var left = encode_outcome(RouterOutcome(actual.copy(), RouterAnswer()))
    var right = encode_outcome(RouterOutcome(expected.copy(), RouterAnswer()))
    assert_equal(len(left), len(right))
    for index in range(len(left)):
        assert_equal(left[index], right[index])


def main() raises:
    var args = argv()
    var actions: List[RouterAction] = [RouterAction(3)]
    var action_body: List[UInt8] = [120]
    var action_bodies: List[List[UInt8]] = []
    action_bodies.append(action_body^)
    var map = RouterMapArena(7, actions.copy(), action_bodies.copy())
    var input = RouterInput(RouterKey(5), None, 0, RouterPayload(9))
    var keys: List[RouterKey] = [RouterKey(5)]
    var inputs: List[RouterInput] = [input.copy()]
    var raw: List[UInt8] = [97, 0, 98]
    var bodies: List[List[UInt8]] = []
    bodies.append(raw^)
    var payloads = RouterPayloadArena(11, keys.copy(), inputs.copy(), bodies.copy())
    var original = RouterState(7)
    original.capture = UInt64.MAX
    var operation = RouterOperation(3)
    operation.input = input.copy()
    var path: Optional[String] = None
    if len(args) > 1:
        path = String(args[1])
    var gate = InputRouterGate(path)
    var session = InputRouterSession(original.copy(), map.copy(), payloads.copy())
    var prepared = session.prepare(gate, operation)
    same(session.snapshot(), original)
    var duplicate = prepared.copy()
    var foreign_plan = prepared.copy()
    var other_state = original.copy()
    other_state.capture = None
    var other = InputRouterSession(other_state.copy(), map.copy(), payloads.copy())
    with assert_raises():
        other.commit(foreign_plan^)
    same(other.snapshot(), other_state)
    session.commit(prepared^)
    with assert_raises():
        session.commit(duplicate^)
    assert_equal(session.generation(), UInt64(1))
    assert_equal(len(session.snapshot().effects), 1)
    assert_equal(len(session.take_effects()), 0)
    var drain = session.prepare(gate, RouterOperation(4))
    var stale = drain.copy()
    session.commit(drain^)
    with assert_raises():
        session.commit(stale^)
    var delivered = session.take_effects()
    assert_equal(len(delivered), 1)
    assert_equal(delivered[0].kind, UInt8(0))
    assert_equal(delivered[0].pane, UInt64.MAX)
    assert_true(delivered[0].input == input)
    assert_equal(len(session.take_effects()), 0)
    var body = session.payload_body(RouterPayload(9))
    assert_equal(body[1], UInt8(0))
    body[0] = 255
    assert_equal(session.payload_body(RouterPayload(9))[0], UInt8(97))
    var action = session.action_body(RouterAction(3))
    action[0] = 0
    assert_equal(session.action_body(RouterAction(3))[0], UInt8(120))
    var wrong = session.prepare(gate, RouterOperation(4))
    wrong.scope += 1
    var before = session.snapshot()
    with assert_raises():
        session.commit(wrong^)
    same(session.snapshot(), before)
    var budget = RouterState(7)
    for _ in range(MAX_ROUTER_CLAIMS):
        budget.held.append(RouterHeld(RouterKey(5), 99))
    assert_false(native_eligible(budget, RouterOperation(4)))
    var bounded = InputRouterSession(budget.copy(), map.copy(), payloads.copy())
    var budget_before = bounded.snapshot()
    with assert_raises():
        _ = bounded.prepare(gate, operation)
    same(bounded.snapshot(), budget_before)
    var effect = RouterEffect(2)
    effect.owner = pane_owner(19, 0)
    var queue = RouterState(7)
    for _ in range(MAX_ROUTER_EFFECTS):
        queue.effects.append(effect.copy())
    var full = InputRouterSession(queue.copy(), map.copy(), payloads.copy())
    var open = RouterOperation(0)
    open.event = RouterEvent(0, 19, 0, pane_owner(19, 0))
    with assert_raises():
        _ = full.prepare(gate, open)
    same(full.snapshot(), queue)
    var first = full.prepare(gate, RouterOperation(4))
    full.commit(first^)
    var second = full.prepare(gate, open)
    full.commit(second^)
    var queued = full.snapshot()
    with assert_raises():
        _ = full.prepare(gate, RouterOperation(4))
    same(full.snapshot(), queued)
    assert_equal(len(full.take_effects()), MAX_ROUTER_EFFECTS)
    var oversize = List[UInt8](length=MAX_ROUTER_PAYLOAD_BYTES + 1, fill=0)
    var oversized_bodies = List[List[UInt8]]()
    oversized_bodies.append(oversize^)
    with assert_raises():
        _ = RouterPayloadArena(1, keys.copy(), inputs.copy(), oversized_bodies^)
    var duplicates: List[RouterAction] = [RouterAction(3), RouterAction(3)]
    var duplicate_bodies: List[List[UInt8]] = []
    duplicate_bodies.append(List[UInt8]())
    duplicate_bodies.append(List[UInt8]())
    with assert_raises():
        _ = RouterMapArena(7, duplicates^, duplicate_bodies^)
    var foreign = input.copy()
    foreign.payload = RouterPayload(10)
    with assert_raises():
        payloads.validate(foreign)
    var modified = input.copy()
    modified.action = 1
    with assert_raises():
        payloads.validate(modified)
    for size in range(255, 257):
        var largest = RouterState(7)
        for _ in range(size):
            largest.held.append(RouterHeld(RouterKey(5), UInt64.MAX))
            largest.local.append(RouterKey(5))
        var forwarded = RouterEffect(0)
        forwarded.pane = UInt64.MAX
        forwarded.input = input.copy()
        var queued = 511 if size == 255 else 512
        for _ in range(queued):
            largest.effects.append(forwarded.copy())
        var bounded = InputRouterSession(largest^, map.copy(), payloads.copy())
        var drain = bounded.prepare(gate, RouterOperation(4))
        assert_equal(drain.native, len(args) > 1 and size == 255)
        bounded.commit(drain^)
        assert_equal(len(bounded.take_effects()), queued)
        assert_equal(len(bounded.take_effects()), 0)
    print("router atomic fallback, stale plans, delivery, arenas and outer budgets passed", gate.native_count, gate.fallback_count)
