from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text, read_bool, read_optional_text, read_optional_id
from zzmojo.input_protocol import KeyInput, decode_input_frame
from zzmojo.verified_input_router import RouterOwner, RouterState, RouterPayload, pane_owner, reduce_router
from zzmojo.desktop_input import desktop_intercepts, desktop_guard, desktop_key_projection, desktop_router_map
from zzmojo.chrome_keymap import ChromeKeymap
from zzmojo.chrome_actions import ChromeActions
from zzmojo.prefix_bindings import PrefixBindings, PrefixTable, PrefixBinding, encode_prefix_snapshot, MAX_PREFIX_BINDINGS
from zzmojo.exec_protocol import Invocation


def reader(path: String) raises -> Decoder:
    var bytes: List[UInt8]
    with open(path, "r") as source:
        bytes = source.read_bytes()
    if len(bytes) > 64 * 1024 * 1024:
        raise Error("desktop input fixture exceeds byte limit")
    var result = Decoder(bytes^)
    result.offset = 0
    return result^


def active(mut source: Decoder) raises -> Optional[RouterOwner]:
    if read_bool(source):
        var pane = source.uint()
        return pane_owner(pane, UInt8(source.uint()))
    return None


def same(left: List[UInt8], right: List[UInt8]) raises:
    assert_equal(len(left), len(right))
    for index in range(len(left)):
        assert_equal(left[index], right[index])


def main() raises:
    var args = argv()
    var directory = String(args[1])
    var source = reader(directory + "/guard.bin")
    var guards = Int(source.uint())
    for _ in range(guards):
        var window = read_bool(source)
        var key = read_text(source)
        var overlay = read_bool(source)
        var pending = read_bool(source)
        var drag = UInt8(source.uint())
        var modifiers = UInt8(source.uint())
        var function = read_bool(source)
        assert_equal(desktop_intercepts(window, key), read_bool(source))
        assert_equal(UInt64(desktop_guard(key, overlay, pending, drag, modifiers, function)), source.uint())
    assert_equal(source.offset, len(source.data))
    source = reader(directory + "/projection.bin")
    var projections = Int(source.uint())
    for _ in range(projections):
        var before = active(source)
        var selected = active(source)
        var capture = read_optional_id(source)
        var armed = read_bool(source)
        var claimed = read_bool(source)
        var overlay = read_bool(source)
        var modifiers = UInt8(source.uint())
        var function = read_bool(source)
        var projection = desktop_key_projection(before, selected, capture, armed, claimed, overlay, modifiers, function)
        var kind = source.uint()
        assert_equal(Bool(projection.event), kind != 0)
        if kind == 1:
            assert_equal(projection.event.value().kind, UInt8(1))
            assert_equal(projection.event.value().pane, source.uint())
            assert_equal(UInt64(projection.event.value().surface), source.uint())
        elif kind == 2:
            assert_equal(projection.event.value().kind, UInt8(8))
        assert_equal(projection.capture, read_optional_id(source))
        assert_equal(projection.armed, read_bool(source))
        assert_equal(projection.claimed, read_bool(source))
    assert_equal(source.offset, len(source.data))
    source = reader(directory + "/prefix.bin")
    var inputs = List[KeyInput]()
    var input_count = Int(source.uint())
    for _ in range(input_count):
        var frame = decode_input_frame(source.bytes())
        assert_true(Bool(frame.key))
        inputs.append(frame.key.take())
    var prefix = PrefixBindings()
    var updates = Int(source.uint())
    var queries = 0
    for _ in range(updates):
        _ = prefix.apply_event(source.bytes())
        same(encode_prefix_snapshot(prefix.snapshot()), source.bytes())
        var expected = read_optional_text(source)
        assert_equal(prefix.active_table, expected)
        assert_equal(prefix.repeat, read_bool(source) if expected else False)
        for input in inputs:
            var query = prefix.query(input)
            var flags = UInt64(query.bound()) | UInt64(query.all_keys) << 1 | UInt64(prefix.chrome_enabled(input)) << 2
            assert_equal(flags, source.uint())
            queries += 1
    assert_equal(source.offset, len(source.data))
    var before = encode_prefix_snapshot(prefix.snapshot())
    var generation = prefix.generation
    var bindings = List[PrefixBinding]()
    for _ in range(MAX_PREFIX_BINDINGS + 1):
        bindings.append(PrefixBinding("x", List[Invocation](), False, None))
    var tables: List[PrefixTable] = [PrefixTable("prefix", bindings^)]
    with assert_raises():
        prefix.full(tables)
    assert_equal(prefix.generation, generation)
    same(encode_prefix_snapshot(prefix.snapshot()), before)
    source = reader(directory + "/constructor.bin")
    var catalog = ChromeActions()
    var constructors = 0
    var maps = Int(source.uint())
    for index in range(maps):
        var profile = UInt8(source.uint())
        var custom = read_bool(source)
        var configured = ChromeKeymap(profile, UInt32(index * 2 + 100))
        if custom:
            _ = configured.bind("ui", "C-x", "ui-zoom-in")
            _ = configured.bind("prefix", "x", "ui-zoom-reset")
            _ = configured.bind("terminal", "x", "ui-zoom-out")
            _ = configured.bind("ui", "M-x", "new-window")
        var map = desktop_router_map(configured, UInt32(index * 2 + 101))
        assert_equal(Int(source.uint()), len(inputs))
        for input in inputs:
            var state = RouterState(map.version())
            var operation = map.key_operation(state, input, RouterPayload(1), False, False)
            var outcome = reduce_router(state, operation)
            assert_equal(outcome.answer.value, read_bool(source))
            assert_equal(len(outcome.state.effects), Int(source.uint()))
            for effect in outcome.state.effects:
                assert_equal(effect.kind, UInt8(1))
                assert_equal(catalog.name(map.action_from_reference(effect.action)), read_text(source))
            constructors += 1
    assert_equal(source.offset, len(source.data))
    print(guards, projections, input_count, updates, queries, constructors)
