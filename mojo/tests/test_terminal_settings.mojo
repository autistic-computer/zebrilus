from std.testing import TestSuite, assert_equal
from zzmojo.appearance_protocol import TerminalAppearance
from zzmojo.terminal_settings import localized_appearance, display_size


def test_default_font_provenance_uses_local_fallback() raises:
    var appearance = TerminalAppearance()
    var available: List[String] = ["Noto Sans Mono", "Menlo", "Local Mono"]
    var provenance = Dict[UInt8, UInt8]()
    var local = localized_appearance(
        appearance, provenance, available, "Local Mono"
    )
    var expected: List[String] = ["Local Mono"]
    assert_equal(local.font_families, expected)
    assert_equal(len(local.font_families_bold), 0)
    assert_equal(display_size(local), Float32(13))


def test_custom_stacks_use_ascii_case_matching_and_preserve_order() raises:
    var appearance = TerminalAppearance()
    appearance.font_families = ["Missing", "aLPHa", "Beta"]
    appearance.font_families_bold = ["É"]
    appearance.font_families_italic = ["İ"]
    appearance.font_families_bold_italic = ["Alpha", "é"]
    var available: List[String] = ["ALPHA", "beta", "é", "i̇"]
    var provenance = Dict[UInt8, UInt8]()
    for key in range(7, 11):
        provenance[UInt8(key)] = 3
    var local = localized_appearance(
        appearance, provenance, available, "Fallback"
    )
    var regular: List[String] = ["aLPHa", "Beta"]
    var combined: List[String] = ["Alpha", "é"]
    assert_equal(local.font_families, regular)
    assert_equal(len(local.font_families_bold), 0)
    assert_equal(len(local.font_families_italic), 0)
    assert_equal(local.font_families_bold_italic, combined)
    local = localized_appearance(
        appearance, provenance, available, "Fallback", "BeTa", Float32(10)
    )
    var preferred: List[String] = ["BeTa"]
    assert_equal(local.font_families, preferred)
    assert_equal(len(local.font_families_bold_italic), 0)
    assert_equal(local.font_size_points, Float32(48))


def test_render_zoom_clamps_after_local_preferences() raises:
    var appearance = TerminalAppearance()
    appearance.font_size_points = 256
    var available: List[String] = ["Local"]
    var provenance = Dict[UInt8, UInt8]()
    var local = localized_appearance(
        appearance, provenance, available, "Local", scale=Float32(1)
    )
    assert_equal(local.font_size_points, Float32(48))
    assert_equal(display_size(local, -10), Float32(38))
    local = localized_appearance(appearance, provenance, available, "Local")
    assert_equal(local.font_size_points, Float32(256))
    assert_equal(display_size(local, -10), Float32(48))


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
