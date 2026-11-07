from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.protocol import Decoder
from zzmojo.chrome_keymap import ChromeKeymap, ChromeBinding
from zzmojo.chrome_actions import ChromeActions
from zzmojo.desktop_ui_zoom import DesktopUiZoom
from zzmojo.desktop_native_keymap import (
    NativeKeystroke,
    DesktopNativeKeymap,
    native_keystroke,
    native_bound_chords,
    native_dropped_chords,
    MAX_NATIVE_CHROME_RECORDS,
    MAX_NATIVE_CHROME_BYTES,
)


def read_key(mut reader: Decoder) raises -> NativeKeystroke:
    var key = reader.text()
    var modifiers = reader.byte()
    var function = reader.byte() != 0
    var character: Optional[String] = None
    if reader.byte() != 0:
        character = reader.text()
    return NativeKeystroke(key^, modifiers, function, character^)


def check_bound(actual: List[ChromeBinding], mut reader: Decoder) raises:
    assert_equal(len(actual), Int(reader.uint()))
    for value in actual:
        assert_equal(value.table, reader.text())
        assert_equal(value.key, reader.text())
        assert_equal(value.action.code(), UInt32(reader.uint()))


def bounds() raises:
    var profile_a = ChromeKeymap(0)
    var profile_b = ChromeKeymap(1)
    assert_equal(profile_a.version(), profile_b.version())
    var profile_owner = DesktopNativeKeymap()
    var profile_plan = profile_owner.prepare(profile_a)
    with assert_raises():
        profile_owner.commit(profile_b, profile_plan^)
    assert_equal(profile_owner.generation, UInt64(0))
    assert_equal(len(profile_owner.records), 0)
    var map_a = ChromeKeymap(1, 41)
    var map_b = ChromeKeymap(1, 41)
    _ = map_a.bind("terminal", "C-x", "terminal-copy")
    _ = map_b.bind("terminal", "C-x", "terminal-paste")
    assert_equal(map_a.version(), map_b.version())
    var owner_a = DesktopNativeKeymap()
    var owner_b = DesktopNativeKeymap()
    var wrong_map = owner_a.prepare(map_a)
    with assert_raises():
        owner_a.commit(map_b, wrong_map^)
    assert_equal(owner_a.generation, UInt64(0))
    assert_equal(len(owner_a.records), 0)
    var wrong_owner = owner_a.prepare(map_a)
    with assert_raises():
        owner_b.commit(map_a, wrong_owner^)
    assert_equal(owner_b.generation, UInt64(0))
    assert_equal(len(owner_b.records), 0)
    var install_a = owner_a.prepare(map_a)
    var install_b = owner_b.prepare(map_a)
    owner_a.commit(map_a, install_a^)
    owner_b.commit(map_a, install_b^)
    var owned_input = List[NativeKeystroke]()
    var owned_key = native_keystroke("ctrl-x", False)
    owned_input.append(owned_key.take())
    var owned_query = owner_a.query(0, owned_input)
    assert_true(len(owned_query.matches) > 0)
    with assert_raises():
        _ = owner_b.resolve(owned_query, 0)
    assert_equal(owner_b.generation, UInt64(1))
    var map = ChromeKeymap(1)
    var registry = DesktopNativeKeymap()
    var prepared = registry.prepare(map)
    _ = map.bind("ui", "C-z", "ui-zoom-in")
    with assert_raises():
        registry.commit(map, prepared^)
    assert_equal(registry.generation, UInt64(0))
    assert_equal(len(registry.records), 0)
    var next = registry.prepare(map)
    registry.commit(map, next^)
    var input = List[NativeKeystroke]()
    var stroke = native_keystroke("ctrl-z", False)
    input.append(stroke.take())
    var query = registry.query(3, input)
    assert_true(len(query.matches) > 0)
    var replace = registry.prepare(map)
    registry.commit(map, replace^)
    with assert_raises():
        _ = registry.resolve(query, 0)
    var refused = False
    for _ in range(512):
        var generation = registry.generation
        var count = len(registry.records)
        var bytes = registry.retained_bytes
        try:
            var plan = registry.prepare(map)
            registry.commit(map, plan^)
        except:
            assert_equal(registry.generation, generation)
            assert_equal(len(registry.records), count)
            assert_equal(registry.retained_bytes, bytes)
            refused = True
            break
    assert_true(refused)
    assert_true(len(registry.records) <= MAX_NATIVE_CHROME_RECORDS)
    assert_true(registry.retained_bytes <= MAX_NATIVE_CHROME_BYTES)
    print("native binding capacity refusal preserved", len(registry.records), registry.retained_bytes, registry.generation)
    var fresh = DesktopNativeKeymap()
    var sequence = String()
    for _ in range(33):
        sequence += "x "
    _ = map.bind("terminal", sequence, "terminal-copy")
    with assert_raises():
        _ = fresh.prepare(map)
    assert_equal(fresh.generation, UInt64(0))
    assert_equal(len(fresh.records), 0)
    with assert_raises():
        _ = fresh.query(5, input)
    fresh.generation = UInt64.MAX
    with assert_raises():
        _ = fresh.prepare(map)
    var zoom = DesktopUiZoom()
    with assert_raises():
        _ = zoom.action(0)
    assert_equal(zoom.zoom, Float32(1))
    var nan_bits: UInt32 = 0x7FC00000
    var nan = Pointer(to=nan_bits).unsafe_bitcast[Float32]()[]
    with assert_raises():
        _ = zoom.set_percent(nan)
    assert_equal(zoom.zoom, Float32(1))


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    assert_true(len(bytes) <= 16 * 1024 * 1024)
    var reader = Decoder(bytes^)
    reader.offset = 0
    var macos = reader.byte() != 0
    var windows = reader.byte() != 0
    var parsed = Int(reader.uint())
    for _ in range(parsed):
        var source = reader.text()
        var actual = native_keystroke(source, macos)
        var valid = reader.byte() != 0
        assert_equal(Bool(actual), valid, source)
        if valid:
            assert_equal(actual.value(), read_key(reader))
    var installs = 0
    var queries = 0
    var emissions = 0
    for _ in range(Int(reader.uint())):
        var map = ChromeKeymap(reader.byte())
        var registry = DesktopNativeKeymap(macos, windows)
        for _ in range(Int(reader.uint())):
            var operation = reader.byte()
            if operation != 0:
                var table = reader.text()
                var key = reader.text()
                if operation == 1:
                    var error = map.bind(table, key, reader.text())
                    assert_equal(Bool(error), False)
                else:
                    _ = map.unbind(table, key)
            var bound = native_bound_chords(map)
            var dropped = native_dropped_chords(registry.bound, bound)
            check_bound(bound, reader)
            check_bound(dropped, reader)
            var previous = len(registry.records)
            var generation = registry.generation
            var prepared = registry.prepare(map)
            assert_equal(registry.generation, generation)
            assert_equal(len(registry.records), previous)
            registry.commit(map, prepared^)
            var emitted = Int(reader.uint())
            assert_equal(len(registry.records), previous + emitted)
            for index in range(emitted):
                ref record = registry.records[previous + index]
                assert_equal(record.context, reader.byte())
                assert_equal(record.chord, reader.text())
                var action = reader.byte() != 0
                assert_equal(Bool(record.action), action)
                if action:
                    assert_equal(record.action.value().code(), UInt32(reader.uint()))
                assert_equal(record.binding.kind, reader.byte())
                assert_equal(record.binding.index, reader.byte())
                assert_equal(len(record.strokes), Int(reader.uint()))
                for stroke in record.strokes:
                    assert_equal(stroke, read_key(reader))
            emissions += emitted
            for _ in range(Int(reader.uint())):
                var context = reader.byte()
                var input = List[NativeKeystroke]()
                for _ in range(Int(reader.uint())):
                    input.append(read_key(reader))
                var query = registry.query(context, input)
                assert_equal(query.pending, reader.byte() != 0)
                assert_equal(len(query.matches), Int(reader.uint()))
                for index in range(len(query.matches)):
                    assert_equal(query.matches[index], Int(reader.uint()))
                    var result = registry.resolve(query, index)
                    assert_true(Bool(result.action))
                queries += 1
            installs += 1
    var zoom = DesktopUiZoom()
    var zooms = Int(reader.uint())
    for _ in range(zooms):
        var operation = reader.byte()
        var expected = reader.byte() != 0
        var bits = UInt32(reader.uint())
        var value = Pointer(to=bits).unsafe_bitcast[Float32]()[]
        var changed = zoom.set_percent(value) if operation == 0 else zoom.action(operation)
        assert_equal(changed, expected)
        var actual = zoom.zoom
        assert_equal(Pointer(to=actual).unsafe_bitcast[UInt32]()[], UInt32(reader.uint()))
    reader.finish()
    bounds()
    print("actual native chord, matcher, pending, installation and zoom cases passed", parsed, installs, emissions, queries, zooms)
