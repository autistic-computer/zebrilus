from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.desktop_key_config_io import (
    DesktopConfigEnvironment,
    desktop_config_candidates,
    read_desktop_config,
    load_desktop_key_config,
)


def main() raises:
    var arguments = argv()
    var root = arguments[1]
    var environment = DesktopConfigEnvironment(
        root + "/xdg", root + "/home", None, None, None
    )
    var paths = desktop_config_candidates(0, environment)
    assert_equal(len(paths), 2)
    assert_equal(paths[0], root + "/xdg/zz/config")
    assert_equal(paths[1], root + "/home/.config/zz/config")
    var loaded = load_desktop_key_config(environment, False)
    assert_equal(loaded.path.value(), root + "/home/.config/zz/config")
    assert_equal(len(loaded.value.diagnostics), 1)
    assert_equal(loaded.value.diagnostics[0][0], 2)
    assert_equal(
        loaded.value.diagnostics[0][1],
        String(
            "`picker-focus-sidebar` was removed: Prefix + s and Prefix + w"
            " open their pickers in the pane"
        ),
    )
    assert_equal(loaded.value.which_key_delay, Float32(125))
    assert_equal(loaded.value.overrides[0].key, String("C-x"))
    assert_equal(read_desktop_config(root + "/exact").byte_length(), 65536)
    with assert_raises():
        _ = read_desktop_config(root + "/oversize")
    with assert_raises():
        _ = read_desktop_config(root + "/invalid")
    with assert_raises():
        _ = read_desktop_config(root + "/absent")
    environment = DesktopConfigEnvironment(None, None, None, None, None)
    loaded = load_desktop_key_config(environment, False)
    assert_equal(Bool(loaded.path), False)
    assert_equal(loaded.value.hotkey, String("ctrl-shift-c"))
    print("bounded desktop configuration loading passed")
