from std.sys import argv
from mjui.ui import Ui
from mjui.term.grid import TermOpts
from mjui.png import write_png
from mjui.render.sdlgpu import SdlGpuRenderer
from zzmojo.terminal_appearance import terminal_metrics
from zzmojo.terminal_model import TerminalViewport, TerminalCell, TerminalStyle, TerminalOverlay, TerminalCursor
from zzmojo.terminal_painter import render_viewport_opts


def number(data: List[UInt8], mut offset: Int) raises -> UInt32:
    if len(data) - offset < 4:
        raise Error("truncated styled terminal fixture")
    var value: UInt32 = 0
    for index in range(4):
        value |= UInt32(data[offset + index]) << UInt32(index * 8)
    offset += 4
    return value


def real(data: List[UInt8], mut offset: Int) raises -> Float32:
    var bits = number(data, offset)
    return Pointer(to=bits).unsafe_bitcast[Float32]()[]


def text(data: List[UInt8], mut offset: Int) raises -> String:
    var count = Int(number(data, offset))
    if count > 65536 or count > len(data) - offset:
        raise Error("styled terminal text exceeds limit")
    var value = String(from_utf8=Span(data)[offset : offset + count])
    offset += count
    return value^


def count(data: List[UInt8], mut offset: Int, maximum: Int) raises -> Int:
    var value = Int(number(data, offset))
    if value > maximum:
        raise Error("styled terminal count exceeds limit")
    return value


def main() raises:
    var args = argv()
    if len(args) != 4:
        raise Error("usage: terminal-styled-probe INPUT OUTPUT.png OUTPUT.json")
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 1024 * 1024:
        raise Error("styled terminal fixture exceeds limit")
    var offset = 0
    var width = count(data, offset, 1536)
    var height = count(data, offset, 1536)
    var display_columns = count(data, offset, 256)
    var display_rows = count(data, offset, 256)
    var size = real(data, offset)
    var device_scale = real(data, offset)
    var viewport = TerminalViewport()
    viewport.columns = UInt16(count(data, offset, 256))
    viewport.rows = UInt16(count(data, offset, 256))
    viewport.metadata.foreground = number(data, offset)
    viewport.metadata.background = number(data, offset)
    var opts = TermOpts(0, size, bold=1, italic=2, bold_italic=3, focused=True, native_colors=True, native_appearance=True)
    opts.selection = number(data, offset)
    opts.selection_foreground = number(data, offset)
    opts.match_bg = number(data, offset)
    opts.match_current = number(data, offset)
    opts.link = number(data, offset)
    opts.copy_cursor = number(data, offset)
    opts.rounded_selection = number(data, offset) != 0
    opts.minimum_contrast = real(data, offset)
    opts.font_thicken = number(data, offset) != 0
    opts.font_thicken_strength = UInt8(number(data, offset))
    opts.font_synthetic_style = UInt8(number(data, offset))
    var paths = List[String]()
    for _ in range(4):
        paths.append(text(data, offset))
    var fallbacks = List[String]()
    var fallback_count = count(data, offset, 28)
    for _ in range(fallback_count):
        fallbacks.append(text(data, offset))
    var styles = count(data, offset, 1024)
    for _ in range(styles):
        var foreground = number(data, offset)
        var background = number(data, offset)
        var underline_color = number(data, offset)
        var attributes = UInt16(number(data, offset))
        var underline = UInt8(number(data, offset))
        var background_index = UInt8(number(data, offset))
        viewport.styles.append(TerminalStyle(foreground, background, underline_color, attributes, underline, background_index))
    viewport.grapheme_offsets.clear()
    var offsets = count(data, offset, 4096)
    for _ in range(offsets):
        viewport.grapheme_offsets.append(number(data, offset))
    var bytes = count(data, offset, 65536)
    if bytes > len(data) - offset:
        raise Error("truncated styled grapheme bytes")
    viewport.grapheme_bytes = List[UInt8](Span(data)[offset : offset + bytes])
    offset += bytes
    var cells = Int(viewport.columns) * Int(viewport.rows)
    if cells > 16384:
        raise Error("styled grid exceeds limit")
    for _ in range(cells):
        var glyph = number(data, offset)
        var style = UInt16(number(data, offset))
        var flags = UInt16(number(data, offset))
        viewport.cells.append(TerminalCell(glyph, style, flags))
    var overlays = count(data, offset, 1024)
    for _ in range(overlays):
        var row = UInt16(number(data, offset))
        var start = UInt16(number(data, offset))
        var end = UInt16(number(data, offset))
        var kind = UInt16(number(data, offset))
        viewport.metadata.overlays.append(TerminalOverlay(row, start, end, kind))
    if number(data, offset) != 0:
        var color = number(data, offset)
        var column = UInt16(number(data, offset))
        var row = UInt16(number(data, offset))
        var style = UInt8(number(data, offset))
        var visible = UInt8(number(data, offset))
        var blinking = UInt8(number(data, offset))
        var wide = UInt8(number(data, offset))
        viewport.metadata.cursor = TerminalCursor(color, column, row, style, visible, blinking, wide)
    var marked: Optional[String] = None
    if number(data, offset) != 0:
        marked = text(data, offset)
    var probe_words = List[String]()
    var probes = count(data, offset, 128)
    for _ in range(probes):
        probe_words.append(text(data, offset))
    if offset != len(data):
        raise Error("trailing styled terminal bytes")
    var ui = Ui(device_scale)
    for path in paths:
        _ = ui.add_font(path)
    var fallback_ids = List[Int32]()
    for path in fallbacks:
        fallback_ids.append(Int32(ui.add_font(path)))
    for style in range(4):
        if len(fallback_ids) > 0:
            ui.shaper.explicit_fallbacks[Int32(style)] = fallback_ids.copy()
    var advance = ui.font_advance(0, size, 109)
    var font = ui.native_font_metrics(0, size)
    var metrics = terminal_metrics(advance, font.ascender, -font.descender, font.line_gap, device_scale, 0, 0)
    opts.cell_width = metrics.width
    opts.line_height = metrics.height
    opts.baseline = metrics.baseline
    ui.set_time(1)
    ui.begin_frame(Float32(width) / device_scale, Float32(height) / device_scale)
    _ = render_viewport_opts(ui, 91, viewport, opts, display_columns, display_rows, True, marked)
    ui.end_frame()
    var renderer = SdlGpuRenderer()
    var pixels = renderer.render_offscreen(ui.dl, ui.atlas, width, height, (viewport.metadata.background << 8) | 255)
    write_png(args[2], pixels.unsafe_ptr(), width, height, width * 4)
    var result = String('{"glyphs":[')
    for index in range(len(ui.dl.glyphs)):
        var glyph = ui.dl.glyphs[index]
        var slot = ui.atlas.slots[Int(glyph.slot)]
        if index != 0:
            result += ","
        result += String('{"bounds":[') + String(glyph.x) + "," + String(glyph.y) + "," + String(slot.w) + "," + String(slot.h) + '],"color":' + String(glyph.color) + "}"
    result += '],"underlines":['
    for index in range(len(ui.dl.native_underlines)):
        if index != 0:
            result += ","
        var line = ui.dl.native_underlines[index]
        result += String('{"bounds":[') + String(line.x) + "," + String(line.y) + "," + String(line.w) + "," + String(line.h) + '],"thickness":' + String(line.thickness) + ',"wavy":' + String(line.wavy) + "}"
    result += '],"faces":['
    for index in range(4):
        var face = ui.term.views[0].faces[index]
        if index != 0:
            result += ","
        result += String('{"font":') + String(face.font) + ',"bold":' + String(UInt8(face.params.bold)) + ',"italic":' + String(UInt8(face.params.italic)) + "}"
    result += '],"glyph_probes":['
    if fallback_count > 0:
        var words = probe_words.copy()
        for index in range(len(words)):
            if index != 0:
                result += ","
            var bytes = words[index].as_bytes()
            var word_id = ui.shaper.word(0, bytes.unsafe_ptr(), len(bytes))
            var word = ui.shaper.words[word_id]
            var k = ui.shaper.fonts[0].scale(size)
            var ascent: Float32 = 0
            var descent: Float32 = 0
            for glyph in range(Int(word.glyph_count)):
                var font_id = Int(ui.shaper.glyph_font[Int(word.fb) + glyph]) if word.fb >= 0 else 0
                ref face = ui.shaper.fonts[font_id]
                var metrics = face.metrics.normalized(face.upem).scaled(size)
                ascent = max(ascent, metrics.ascender)
                descent = max(descent, -metrics.descender)
            result += String('{"text":"') + words[index] + '","width":' + String(Float32(word.advance) * k) + ',"ascent":' + String(ascent) + ',"descent":' + String(descent) + ',"glyphs":['
            var pen: Float32 = 0
            for glyph in range(Int(word.glyph_count)):
                if glyph != 0:
                    result += ","
                var source = Int(word.glyph_start) + glyph
                var font_id = Int(ui.shaper.glyph_font[Int(word.fb) + glyph]) if word.fb >= 0 else 0
                result += String('{"font":') + String(font_id) + ',"glyph":' + String(ui.shaper.gids[source]) + ',"index":' + String(ui.shaper.cluster[source]) + ',"x":' + String(pen + Float32(ui.shaper.xoff[source]) * k) + ',"y":' + String(Float32(ui.shaper.yoff[source]) * k) + "}"
                pen += Float32(ui.shaper.adv[source]) * k
            result += "]}"
    result += '],"metric_bits":['
    var values = SIMD[.float32, 8](advance, font.ascender, -font.descender, font.line_gap, metrics.baseline, metrics.width, metrics.height, 0)
    for index in range(7):
        if index != 0:
            result += ","
        result += String(values[index].to_bits[DType.uint32]())
    result += "]}"
    with open(args[3], "w") as output:
        output.write(result)
