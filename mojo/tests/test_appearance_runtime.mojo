from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.box import TextStyle
from mjui.text.hb import HbFeature
from zzmojo.appearance_protocol import TerminalAppearance, FontFeature
from zzmojo.appearance_fonts import terminal_font_specs
from zzmojo.appearance_runtime import AppearanceRuntime


def test_protocol_features_reach_native_shaping() raises:
    var ui = Ui(1)
    var regular = ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    var reference_features: List[HbFeature] = [HbFeature.parse("liga=1"), HbFeature.parse("calt=0")]
    var reference = ui.add_font_variant(regular, reference_features)
    var appearance = TerminalAppearance()
    appearance.font_features.append(FontFeature(0x746C6163, 0))
    var specs = terminal_font_specs(appearance, String("monospace"), 13)
    for index in range(len(specs)):
        specs[index].path = String("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    var prepared = ui.prepare_fonts(specs)
    ui.begin_frame(32, 18)
    assert_true(ui.apply_fonts(prepared))
    var text = String("!=->=>ffi")
    var bytes = text.as_bytes()
    var actual = ui.shaper.word(Int(ui.font_group().regular), bytes.unsafe_ptr(), len(bytes))
    var expected = ui.shaper.word(reference, bytes.unsafe_ptr(), len(bytes))
    var baseline = ui.shaper.word(regular, bytes.unsafe_ptr(), len(bytes))
    var a = ui.shaper.words[actual]
    var b = ui.shaper.words[expected]
    var c = ui.shaper.words[baseline]
    assert_equal(a.glyph_count, b.glyph_count)
    var differs = a.glyph_count != c.glyph_count
    for index in range(Int(a.glyph_count)):
        var ai = Int(a.glyph_start) + index
        var bi = Int(b.glyph_start) + index
        assert_equal(ui.shaper.gids[ai], ui.shaper.gids[bi])
        assert_equal(ui.shaper.adv[ai], ui.shaper.adv[bi])
        if index < Int(c.glyph_count):
            differs |= ui.shaper.gids[ai] != ui.shaper.gids[Int(c.glyph_start) + index]
    assert_true(differs)
    ui.end_frame()


def finish_frame(mut ui: Ui, runtime: AppearanceRuntime):
    ui.label("ABCD", TextStyle(runtime.slots[0], runtime.active.font_size_points, 0xDDEEFFFF, False))
    ui.end_frame()


def test_color_updates_reuse_fonts_and_supersede_deferred_request() raises:
    var ui = Ui(1)
    _ = ui.add_font("bench/assets/fonts/Inter-Regular.ttf")
    var runtime = AppearanceRuntime(0)
    var appearance = TerminalAppearance()
    var provenance = Dict[UInt8, UInt8]()
    for attempt in range(500):
        ui.set_time(Float64(attempt) / 100)
        ui.begin_frame(64, 64)
        _ = runtime.update(ui, appearance.copy(), provenance, 1, 71, 72)
        finish_frame(ui, runtime)
        ui.atlas.gpu_done(ui.atlas.frame)
        if runtime.fonts_ready:
            break
        _ = external_call["usleep", Int32](UInt32(10000))
    assert_true(runtime.fonts_ready)
    assert_equal(runtime.error, String())
    var generation = ui.font_generation
    appearance.selection_background = 0xE53F99A7
    ui.begin_frame(64, 64)
    assert_true(runtime.update(ui, appearance.copy(), provenance, 2, 71, 72))
    assert_equal(ui.font_generation, generation)
    assert_equal(runtime.options(ui, True).selection, UInt32(0xE53F99A7))
    finish_frame(ui, runtime)
    runtime.preferences(None, 1.25)
    ui.begin_frame(64, 64)
    _ = runtime.update(ui, appearance.copy(), provenance, 2, 71, 72)
    assert_equal(ui.font_generation, generation + 1)
    finish_frame(ui, runtime)
    runtime.preferences(None, 1.5)
    ui.begin_frame(64, 64)
    _ = runtime.update(ui, appearance.copy(), provenance, 2, 71, 72)
    assert_true(Bool(runtime.prepared))
    assert_equal(ui.font_generation, generation + 1)
    finish_frame(ui, runtime)
    runtime.preferences(None, 1)
    ui.atlas.gpu_done(ui.atlas.frame)
    ui.begin_frame(64, 64)
    _ = runtime.update(ui, appearance.copy(), provenance, 2, 71, 72)
    assert_false(Bool(runtime.prepared))
    assert_equal(ui.font_generation, generation + 2)
    assert_equal(runtime.options(ui, True).size, appearance.font_size_points)
    assert_equal(runtime.error, String())
    finish_frame(ui, runtime)


def main() raises:
    test_protocol_features_reach_native_shaping()
    test_color_updates_reuse_fonts_and_supersede_deferred_request()
    print("Native appearance: protocol font features, color-only reuse and latest deferred settings passed")
