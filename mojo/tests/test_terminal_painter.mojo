from std.testing import assert_equal, assert_false, assert_true
from mjui.ui import Ui
from mjui.debug import render_rgb
from mjui.term.grid import TermOpts
from zzmojo.terminal_model import (
    TerminalCell,
    TerminalCursor,
    TerminalOverlay,
    TerminalStyle,
    TerminalViewport,
)
from zzmojo.terminal_painter import render_viewport, render_viewport_opts


def frame(mut ui: Ui, viewport: TerminalViewport) raises:
    ui.set_time(1)
    ui.begin_frame(32, 36)
    var metrics = render_viewport(ui, 71, viewport, 0, 13, True)
    assert_equal((metrics.w, metrics.h), (8, 18))
    ui.end_frame()


def test_packed_records_and_dirty_rows() raises:
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    var viewport = TerminalViewport()
    viewport.columns = 4
    viewport.rows = 2
    viewport.metadata.foreground = 0xDDEEFF
    viewport.metadata.background = 0x112233
    viewport.styles.append(
        TerminalStyle(0xDDEEFF, 0x112233, 0xFFFFFFFF, 0, 0, 0)
    )
    viewport.styles.append(
        TerminalStyle(0x25ABCDEF, 0x010203, 0x405060, 0xA100, 3, 29)
    )
    viewport.grapheme_offsets.append(3)
    viewport.grapheme_bytes.append(97)
    viewport.grapheme_bytes.append(0xCC)
    viewport.grapheme_bytes.append(0x81)
    viewport.cells.append(TerminalCell(0x80000000, 1, 0))
    viewport.cells.append(TerminalCell(65, 1, 0))
    viewport.cells.append(TerminalCell(0x754C, 1, 1))
    viewport.cells.append(TerminalCell(0, 1, 2))
    viewport.cells.append(TerminalCell(66, 0, 0))
    viewport.cells.append(TerminalCell(67, 0, 0))
    viewport.cells.append(TerminalCell(0, 0, 0))
    viewport.cells.append(TerminalCell(0, 0, 0))
    viewport.metadata.overlays.append(TerminalOverlay(0, 1, 3, 0x100))
    viewport.metadata.cursor = TerminalCursor(0xEDCBA9, 0, 1, 3, 1, 0, 0)
    frame(ui, viewport)
    var vi = ui.term.find(71)
    assert_equal(ui.term.views[vi].built, 2)
    assert_true(ui.term.views[vi].cursor_on)
    assert_equal(ui.term.views[vi].cursor.color, UInt32(0xEDCBA9))
    assert_equal(ui.term.views[vi].cursor.row, UInt16(1))
    assert_equal(ui.term.views[vi].cursor.style, UInt8(3))
    assert_equal(ui.term.views[vi].overlays[0].kind, UInt16(0x100))
    assert_equal(ui.term.views[vi].overlays[0].end, UInt16(3))
    var style = ui.term.views[vi].tables.styles[unsafe_offset=1]
    assert_equal(style.fg, UInt32(0x25ABCDEF))
    assert_equal(style.bg, UInt32(0x010203))
    assert_equal(style.underline_color, UInt32(0x405060))
    assert_equal(style.attrs, UInt16(0xA100))
    assert_equal(style.underline, UInt8(3))
    assert_equal(style.reserved, UInt8(29))
    var expected: List[UInt8] = [
        97,
        0xCC,
        0x81,
        65,
        0xE7,
        0x95,
        0x8C,
        10,
        66,
        67,
    ]
    assert_equal(len(ui.term.views[vi].text), len(expected))
    for i in range(len(expected)):
        assert_equal(ui.term.views[vi].text[i], expected[i])
    var pixels = render_rgb(ui.dl, ui.atlas, 32, 36)
    var corner = (35 * 32 + 31) * 3
    assert_equal(
        (pixels[corner], pixels[corner + 1], pixels[corner + 2]),
        (UInt8(0x11), UInt8(0x22), UInt8(0x33)),
    )
    frame(ui, viewport)
    assert_equal(ui.term.views[vi].built, 0)
    viewport.cells[4].glyph = 68
    frame(ui, viewport)
    assert_equal(ui.term.views[vi].built, 1)
    viewport.grapheme_bytes[0] = 98
    frame(ui, viewport)
    assert_equal(ui.term.views[vi].built, 1)
    viewport.styles[1].foreground = 0x25AABBCC
    frame(ui, viewport)
    assert_equal(ui.term.views[vi].built, 1)
    viewport.styles[0].attributes = 1
    frame(ui, viewport)
    assert_equal(ui.term.views[vi].built, 2)
    viewport.metadata.cursor = None
    viewport.metadata.overlays.clear()
    frame(ui, viewport)
    assert_equal(ui.term.views[vi].built, 0)
    assert_false(ui.term.views[vi].cursor_on)
    assert_equal(len(ui.term.views[vi].overlays), 0)
    print(
        "Packed terminal painter: exact records, text, pixels, and row cache"
        " passed"
    )


def color_frame(mut ui: Ui, viewport: TerminalViewport, opts: TermOpts):
    ui.set_time(1)
    ui.begin_frame(48, 18)
    _ = render_viewport_opts(ui, 72, viewport, opts)
    ui.end_frame()


def glyph_colors(ui: Ui) raises -> List[UInt32]:
    ref view = ui.term.views[ui.term.find(72)]
    var entry = view.entries[Int(view.shown[0])]
    var colors = List[UInt32](length=6, fill=0)
    for i in range(Int(entry.g0), Int(entry.g1)):
        var column = Int(view.gcol[i] & 0x7FFF)
        if column < 6:
            colors[column] = view.glyphs[i].color
    for color in colors:
        assert_true(color != 0)
    return colors^


def test_native_color_policy() raises:
    var ui = Ui(1)
    _ = ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    var viewport = TerminalViewport()
    viewport.columns = 6
    viewport.rows = 1
    viewport.metadata.foreground = 0x282C34
    viewport.metadata.background = 0x282C34
    viewport.styles.append(
        TerminalStyle(0x282C34, 0x282C34, 0xFFFFFFFF, 0, 0, 0)
    )
    viewport.styles.append(
        TerminalStyle(0x25282C34, 0x282C34, 0xFFFFFFFF, 0xD004, 3, 29)
    )
    viewport.grapheme_offsets.append(6)
    viewport.grapheme_bytes = [UInt8(0xE2), 0x94, 0x80, 0xEF, 0xB8, 0x8F]
    var glyphs: List[UInt32] = [65, 0x2500, 0x80000000, 66, 0x2801, 0xE0CB]
    for glyph in glyphs:
        viewport.cells.append(TerminalCell(glyph, 1, 0))
    var opts = TermOpts(0, 13, sprites=False, native_colors=True)
    color_frame(ui, viewport, opts)
    var colors = glyph_colors(ui)
    var expected: List[UInt32] = [
        0x8E939CFF,
        0x282C34FF,
        0x282C34FF,
        0x8E939CFF,
        0x8E939CFF,
        0x8E939CFF,
    ]
    for column in range(6):
        assert_equal(colors[column], expected[column])
    var vi = ui.term.find(72)
    assert_equal(len(ui.term.views[vi].waves), 3)
    assert_equal(ui.term.views[vi].waves[0].color, UInt32(0x8E939CFF))
    assert_equal(ui.term.views[vi].waves[1].color, UInt32(0x282C34FF))
    assert_equal(ui.term.views[vi].waves[2].color, UInt32(0x8E939CFF))
    assert_equal(
        (ui.term.views[vi].waves[1].x0, ui.term.views[vi].waves[1].x1),
        (Float32(8), Float32(24)),
    )
    color_frame(ui, viewport, opts)
    assert_equal(ui.term.views[vi].built, 0)
    opts.sprites = True
    color_frame(ui, viewport, opts)
    colors = glyph_colors(ui)
    for column in range(6):
        assert_equal(colors[column], expected[column])
    opts.minimum_contrast = 1
    color_frame(ui, viewport, opts)
    assert_equal(ui.term.views[vi].built, 1)
    colors = glyph_colors(ui)
    for color in colors:
        assert_equal(color, UInt32(0x282C34FF))
    assert_equal(len(ui.term.views[vi].waves), 1)
    opts.minimum_contrast = 4.5
    color_frame(ui, viewport, opts)
    colors = glyph_colors(ui)
    assert_equal(colors[0], UInt32(0x8E939CFF))
    assert_equal(len(ui.term.views[vi].waves), 3)
    viewport.cells[1] = TerminalCell(0x80000000, 1, 1)
    viewport.cells[2] = TerminalCell(0, 1, 2)
    color_frame(ui, viewport, opts)
    assert_equal(len(ui.term.views[vi].waves), 6)
    assert_equal(ui.term.views[vi].waves[4].color, UInt32(0x282C34FF))
    assert_equal(
        (ui.term.views[vi].waves[4].x0, ui.term.views[vi].waves[4].x1),
        (Float32(8), Float32(24)),
    )
    viewport.cells[1] = TerminalCell(0x2500, 1, 0)
    viewport.cells[2] = TerminalCell(0x80000000, 1, 0)
    opts.native_colors = False
    color_frame(ui, viewport, opts)
    colors = glyph_colors(ui)
    for color in colors:
        assert_equal(color, UInt32(0x282C3480))
    viewport.metadata.background = 0x262626
    viewport.styles[1] = TerminalStyle(
        0x250D0D0D, 0x262626, 0xFFFFFFFF, 0xD004, 3, 29
    )
    opts.native_colors = True
    opts.minimum_contrast = 1
    color_frame(ui, viewport, opts)
    colors = glyph_colors(ui)
    for color in colors:
        assert_equal(color, UInt32(0x181818FF))
    assert_equal(viewport.styles[1].foreground, UInt32(0x250D0D0D))
    assert_equal(viewport.styles[1].attributes, UInt16(0xD004))
    print(
        "Native terminal colors: exact text/decorative transitions, graphemes,"
        " sprites, faint, and cache changes passed"
    )


def main() raises:
    test_packed_records_and_dirty_rows()
    test_native_color_policy()
