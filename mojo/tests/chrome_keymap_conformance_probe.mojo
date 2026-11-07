from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text, read_bool, read_optional_text
from zzmojo.input_protocol import KeyInput, decode_input_frame
from zzmojo.chrome_actions import ChromeAction, ChromeActions
from zzmojo.chrome_keymap import ChromeKey, ChromeKeymap, chrome_key, chrome_key_identity


def reader(path: String) raises -> Decoder:
    var bytes: List[UInt8]
    with open(path, "r") as source:
        bytes = source.read_bytes()
    if len(bytes) > 64 * 1024 * 1024:
        raise Error("chrome fixture exceeds byte limit")
    var result = Decoder(bytes^)
    result.offset = 0
    return result^


def code(value: Optional[ChromeAction]) -> UInt64:
    return UInt64(value.value().code()) + 1 if value else UInt64(0)


def checked_key(mut source: Decoder, key: Optional[ChromeKey]) raises:
    var present = read_bool(source)
    assert_equal(Bool(key), present)
    if present:
        assert_equal(key.value().modifiers, UInt8(source.uint()))
        assert_equal(key.value().base, read_text(source))
        assert_equal(key.value().display(), read_text(source))


def checked_snapshot(mut source: Decoder, map: ChromeKeymap) raises:
    var values = map.bindings()
    assert_equal(len(values), Int(source.uint()))
    for value in values:
        assert_equal(value.table, read_text(source))
        assert_equal(value.key, read_text(source))
        assert_equal(value.action.code(), UInt32(source.uint()))
    var count = Int(source.uint())
    for _ in range(count):
        var table = read_text(source)
        assert_equal(map.binds(table), read_bool(source))
        var bindings = map.table_bindings(table)
        assert_equal(len(bindings), Int(source.uint()))
        for value in bindings:
            assert_equal(value.key.modifiers, UInt8(source.uint()))
            assert_equal(value.key.base, read_text(source))
            assert_equal(value.key.display(), read_text(source))
            assert_equal(value.action.code(), UInt32(source.uint()))


def main() raises:
    var args = argv()
    var directory = String(args[1])
    var catalog = ChromeActions()
    var direct = reader(directory + "/direct.bin")
    var actions = Int(direct.uint())
    assert_equal(actions, len(catalog.all()))
    for _ in range(actions):
        var value = catalog.decode(UInt32(direct.uint()))
        assert_true(Bool(value))
        assert_equal(catalog.name(value.value()), read_text(direct))
        assert_equal(code(catalog.from_name(catalog.name(value.value()))), direct.uint())
    var names = Int(direct.uint())
    for _ in range(names):
        var name = read_text(direct)
        assert_equal(code(catalog.from_name(name)), direct.uint())
    var spellings = Int(direct.uint())
    for _ in range(spellings):
        var spelling = read_text(direct)
        checked_key(direct, chrome_key(spelling))
    var normalized = Int(direct.uint())
    for _ in range(normalized):
        var flags = UInt8(direct.uint())
        var base = read_text(direct)
        var key = ChromeKey(flags, base^).normalized()
        assert_equal(key.modifiers, UInt8(direct.uint()))
        assert_equal(key.base, read_text(direct))
        assert_equal(key.display(), read_text(direct))
    direct.finish()
    var source_inputs = reader(directory + "/inputs.bin")
    var inputs = List[KeyInput]()
    var input_count = Int(source_inputs.uint())
    for _ in range(input_count):
        var frame = decode_input_frame(source_inputs.bytes())
        inputs.append(frame.key.take())
    source_inputs.finish()
    var maps = reader(directory + "/maps.bin")
    var cases = Int(maps.uint())
    var mutations = 0
    var queries = 0
    for _ in range(cases):
        var profile = UInt8(maps.uint())
        var scope = UInt32(maps.uint())
        var map = ChromeKeymap(profile, scope)
        checked_snapshot(maps, map)
        var steps = Int(maps.uint())
        for _ in range(steps):
            var kind = UInt8(maps.uint())
            var table = read_text(maps)
            var before = map.version()
            if kind == 3:
                var index = Int(maps.uint())
                assert_equal(code(map.resolve(table, inputs[index])), maps.uint())
            else:
                var key = read_text(maps)
                if kind == 0:
                    var name = read_text(maps)
                    var error = map.bind(table, key, name)
                    var expected = read_optional_text(maps)
                    assert_equal(Bool(error), Bool(expected))
                    if expected:
                        assert_equal(error.value(), expected.value())
                elif kind == 1:
                    assert_equal(map.unbind(table, key), read_bool(maps))
                else:
                    assert_equal(code(map.action_for(table, key)), maps.uint())
            var changed = read_bool(maps)
            assert_equal(map.version(), before + UInt64(changed))
            checked_snapshot(maps, map)
            mutations += 1
        checked_snapshot(maps, map)
        var tables = Int(maps.uint())
        var expected_tables = List[List[UInt64]]()
        for _ in range(tables):
            var table = read_text(maps)
            var count = Int(maps.uint())
            assert_equal(count, len(inputs))
            var values = List[UInt64]()
            for index in range(count):
                var expected = maps.uint()
                assert_equal(code(map.resolve(table, inputs[index])), expected, String("query ", index, " table ", table))
                values.append(expected)
                queries += 1
            expected_tables.append(values^)
        var arena = map.action_arena()
        assert_equal(arena.version(), map.version())
        for value in catalog.all():
            var reference = map.action_reference(value)
            assert_true(map.action_from_reference(reference) == value)
            var body = arena.body(reference)
            assert_equal(body[0], value.kind)
            assert_equal(body[1], value.index)
            assert_equal(String(from_utf8=Span(body)[2:]), catalog.name(value))
        for index in range(len(inputs)):
            var result = map.lookups(inputs[index])
            assert_equal(result.version, map.version())
            assert_equal(result.key.id, chrome_key_identity(inputs[index]).id)
            assert_equal(Bool(result.unshifted), Bool(inputs[index].unshifted))
            if result.unshifted:
                assert_equal(result.unshifted.value().id, UInt64(inputs[index].unshifted.value()))
            assert_equal(result.ui_binds, map.binds("ui"))
            assert_equal(result.terminal_binds, map.binds("terminal"))
            for table in range(4):
                var actual: UInt64 = 0
                if result.actions[table]:
                    arena.validate(result.actions[table].value())
                    actual = UInt64(map.action_from_reference(result.actions[table].value()).code()) + 1
                assert_equal(actual, expected_tables[table][index])
    maps.finish()
    print(actions, names, spellings, normalized, len(inputs), cases, mutations, queries)
