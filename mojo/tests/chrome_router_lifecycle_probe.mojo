from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.chrome_keymap import ChromeKeymap
from zzmojo.input_protocol import KeyInput
from zzmojo.verified_input_router import (
    RouterKey, RouterPayload, RouterInput, RouterState, RouterOwner,
    RouterOperation, RouterOutcome, RouterAnswer, pane_owner,
)
from zzmojo.input_router_codec import encode_outcome
from zzmojo.input_router_gate import InputRouterGate, InputRouterSession, RouterPayloadArena


def payloads(press: RouterInput, release: RouterInput, changed: Bool = False, scope: UInt64 = 10) raises -> RouterPayloadArena:
    var keys: List[RouterKey] = [press.key, RouterKey(122)]
    var inputs: List[RouterInput] = [press.copy(), release.copy()]
    var first: List[UInt8] = [112, 0, 114]
    if changed:
        first[2] = 115
    var second: List[UInt8] = [114, 0, 108]
    var bodies = List[List[UInt8]]()
    bodies.append(first^)
    bodies.append(second^)
    return RouterPayloadArena(scope, keys^, inputs^, bodies^)


def same(actual: RouterState, expected: RouterState) raises:
    var left = encode_outcome(RouterOutcome(actual.copy(), RouterAnswer()))
    var right = encode_outcome(RouterOutcome(expected.copy(), RouterAnswer()))
    assert_equal(len(left), len(right))
    for index in range(len(left)):
        assert_equal(left[index], right[index])


def main() raises:
    var args = argv()
    var map = ChromeKeymap(1, 30)
    var state = RouterState(map.version())
    state.owner = pane_owner(77, 0)
    state.pending = RouterOwner(4, 0, 0)
    state.observed = RouterOwner(2, 0, 0)
    state.restore = pane_owner(55, 1)
    state.active = pane_owner(77, 0)
    state.capture = UInt64.MAX
    state.local.append(RouterKey(122))
    var raw = KeyInput()
    raw.key = 0
    raw.codepoint = 120
    var press = map.key_operation(state, raw, RouterPayload(9), False, False)
    raw.action = 2
    var release = map.key_operation(state, raw, RouterPayload(10), False, False)
    var session = InputRouterSession(state^, map.action_arena(), payloads(press.input, release.input))
    var path: Optional[String] = None
    if len(args) > 1:
        path = String(args[1])
    var gate = InputRouterGate(path)
    var plan = session.prepare(gate, press)
    session.commit(plan^)
    assert_equal(session.generation(), UInt64(1))
    assert_equal(len(session.snapshot().held), 1)
    var stale = session.prepare(gate, RouterOperation(4))
    var before = session.snapshot()
    session.replace_payloads(payloads(press.input, release.input))
    assert_equal(session.generation(), UInt64(2))
    same(session.snapshot(), before)
    with assert_raises():
        session.commit(stale^)
    with assert_raises():
        session.replace_payloads(payloads(press.input, release.input, changed=True))
    with assert_raises():
        session.replace_payloads(payloads(press.input, release.input, scope=11))
    var no_keys = RouterPayloadArena(10, List[RouterKey](), List[RouterInput](), List[List[UInt8]]())
    with assert_raises():
        session.replace_payloads(no_keys^)
    same(session.snapshot(), before)
    assert_equal(session.generation(), UInt64(2))
    assert_true(not map.bind("terminal", "x", "terminal-copy"))
    with assert_raises():
        session.install_map(map.action_arena())
    with assert_raises():
        _ = map.key_operation(session.snapshot(), raw, RouterPayload(10), False, False)
    same(session.snapshot(), before)
    var drain = session.prepare(gate, RouterOperation(4))
    session.commit(drain^)
    assert_equal(session.generation(), UInt64(3))
    with assert_raises():
        session.install_map(map.action_arena())
    with assert_raises():
        session.replace_payloads(payloads(press.input, release.input))
    assert_equal(session.generation(), UInt64(3))
    var delivered = session.take_effects()
    assert_equal(len(delivered), 1)
    assert_equal(delivered[0].input.payload.id, UInt64(9))
    var body = session.payload_body(delivered[0].input.payload)
    assert_equal(body[1], UInt8(0))
    assert_equal(body[2], UInt8(114))
    var in_flight = session.prepare(gate, RouterOperation(1))
    before = session.snapshot()
    session.install_map(map.action_arena())
    assert_equal(session.generation(), UInt64(4))
    before.keymap = map.version()
    same(session.snapshot(), before)
    with assert_raises():
        session.commit(in_flight^)
    var capture = RouterOperation(1)
    capture.capture = UInt64(55)
    var changed = session.prepare(gate, capture)
    session.commit(changed^)
    var after = map.key_operation(session.snapshot(), raw, RouterPayload(10), False, False)
    var forwarded = session.prepare(gate, after)
    session.commit(forwarded^)
    assert_equal(session.generation(), UInt64(6))
    assert_equal(len(session.snapshot().held), 0)
    assert_equal(len(session.snapshot().local), 1)
    var final = session.prepare(gate, RouterOperation(4))
    session.commit(final^)
    delivered = session.take_effects()
    assert_equal(len(delivered), 1)
    assert_equal(delivered[0].pane, UInt64.MAX)
    assert_equal(delivered[0].input.payload.id, UInt64(10))
    assert_equal(len(session.take_effects()), 0)
    var remaining: List[RouterKey] = [RouterKey(122)]
    session.replace_payloads(RouterPayloadArena(10, remaining^, List[RouterInput](), List[List[UInt8]]()))
    assert_equal(session.generation(), UInt64(8))
    with assert_raises():
        _ = session.payload_body(RouterPayload(9))
    if len(args) > 1:
        assert_equal(gate.native_count, UInt64(7))
        assert_equal(gate.fallback_count, UInt64(0))
    else:
        assert_equal(gate.native_count, UInt64(0))
        assert_equal(gate.fallback_count, UInt64(7))
    print("chrome map installation and payload rollover preserve claims, scope and monotonic transactions", gate.native_count, gate.fallback_count)
