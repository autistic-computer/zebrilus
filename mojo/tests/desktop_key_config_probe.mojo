from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.protocol import Decoder
from zzmojo.desktop_key_config_io import (
    desktop_config_candidates,
    DesktopConfigEnvironment,
)
from zzmojo.desktop_key_config import (
    desktop_keystroke,
    desktop_override,
    desktop_key_config,
    DesktopOverride,
)


def check_override(actual: DesktopOverride, mut reader: Decoder) raises:
    assert_equal(actual.bind, reader.byte() != 0)
    assert_equal(actual.table, reader.text())
    assert_equal(actual.key, reader.text())
    assert_equal(actual.action, reader.text())


def main() raises:
    var arguments = argv()
    var data: List[UInt8]
    with open(arguments[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 16 * 1024 * 1024:
        raise Error(
            "desktop key configuration source fixture exceeds byte limit"
        )
    var reader = Decoder(data^)
    reader.offset = 0
    var macos = reader.byte() != 0
    var count = Int(reader.uint())
    for index in range(count):
        var text = reader.text()
        var actual = desktop_keystroke(text, macos)
        assert_equal(
            Bool(actual),
            reader.byte() != 0,
            String("stroke ", index, " ", text),
        )
        if actual:
            assert_equal(
                actual.value().key.display(),
                reader.text(),
                String("stroke key ", index),
            )
            assert_equal(
                actual.value().function,
                reader.byte() != 0,
                String("stroke Fn ", index),
            )
            assert_equal(
                actual.value().spelling(),
                reader.text(),
                String("stroke spelling ", index),
            )
    count = Int(reader.uint())
    for index in range(count):
        var text = reader.text()
        var bind = reader.byte() != 0
        var actual = desktop_override(text, bind, macos)
        assert_equal(
            Bool(actual.value),
            reader.byte() != 0,
            String("override ", index, " ", text),
        )
        if actual.value:
            check_override(actual.value.value(), reader)
        else:
            assert_equal(
                actual.error.value(),
                reader.text(),
                String("override error ", index, " ", text),
            )
    count = Int(reader.uint())
    for index in range(count):
        var text = reader.text()
        var actual = desktop_key_config(text, macos)
        assert_equal(
            actual.hotkey, reader.text(), String("config hotkey ", index)
        )
        var delay = actual.which_key_delay
        assert_equal(
            Pointer(to=delay).unsafe_bitcast[UInt32]()[],
            UInt32(reader.uint()),
            String("config delay ", index),
        )
        var overrides = Int(reader.uint())
        assert_equal(
            len(actual.overrides), overrides, String("config overrides ", index)
        )
        for entry in actual.overrides:
            check_override(entry, reader)
        var diagnostics = Int(reader.uint())
        assert_equal(
            len(actual.diagnostics),
            diagnostics,
            String("config diagnostics ", index),
        )
        for value in actual.diagnostics:
            assert_equal(value[0], Int(reader.uint()))
            assert_equal(value[1], reader.text())
        var map = actual.map(macos, 1)
        var bindings = Int(reader.uint())
        assert_equal(len(map.bindings()), bindings)
        for _ in range(bindings):
            var table = reader.text()
            var key = reader.text()
            var expected = reader.text()
            var action = map.action_for(table, key)
            assert_true(Bool(action), String("missing ", table, ":", key))
            assert_equal(
                map._catalog.name(action.value()),
                expected,
                String("config binding ", index),
            )
    var identity = reader.text()
    var candidates = Int(reader.uint())
    for index in range(candidates):
        var platform = reader.byte()
        var values = List[Optional[String]]()
        for _ in range(5):
            var present = reader.byte() != 0
            var value: Optional[String] = None
            if present:
                value = reader.text()
            values.append(value^)
        var environment = DesktopConfigEnvironment(
            values[0].copy(),
            values[1].copy(),
            values[2].copy(),
            values[3].copy(),
            values[4].copy(),
        )
        var actual = desktop_config_candidates(platform, environment, identity)
        var count = Int(reader.uint())
        assert_equal(len(actual), count, String("candidate count ", index))
        for path in actual:
            assert_equal(path, reader.text(), String("candidate path ", index))
    assert_equal(reader.offset, len(reader.data))
    print("exact native key configuration passed")
