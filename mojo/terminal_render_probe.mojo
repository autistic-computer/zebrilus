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
        raise Error("truncated terminal rendering fixture")
    var value: UInt32 = 0
    for index in range(4):
        value |= UInt32(data[offset + index]) << UInt32(index * 8)
    offset += 4
    return value


def real(data: List[UInt8], mut offset: Int) raises -> Float32:
    var bits = number(data, offset)
    return Pointer(to=bits).unsafe_bitcast[Float32]()[]


def main() raises:
    var args = argv()
    if len(args) != 4:
        raise Error("usage: terminal-render-probe INPUT.bin OUTPUT.png OUTPUT.json")
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var offset = 0
    var width = Int(number(data, offset))
    var height = Int(number(data, offset))
    var display_columns = Int(number(data, offset))
    var display_rows = Int(number(data, offset))
    var size = real(data, offset)
    var device_scale = real(data, offset)
    var viewport = TerminalViewport()
    viewport.columns = UInt16(number(data, offset))
    viewport.rows = UInt16(number(data, offset))
    viewport.metadata.foreground = number(data, offset)
    viewport.metadata.background = number(data, offset)
    viewport.styles.append(TerminalStyle(viewport.metadata.foreground, viewport.metadata.background, 0xFFFFFFFF, 0, 0, 0))
    var opts = TermOpts(0, size, focused=True, native_colors=True, native_appearance=True)
    opts.selection = number(data, offset)
    opts.selection_foreground = number(data, offset)
    opts.match_bg = number(data, offset)
    opts.match_current = number(data, offset)
    opts.link = number(data, offset)
    opts.copy_cursor = number(data, offset)
    opts.rounded_selection = number(data, offset) != 0
    for _ in range(Int(viewport.columns) * Int(viewport.rows)):
        var glyph = number(data, offset)
        var style = UInt16(number(data, offset))
        var flags = UInt16(number(data, offset))
        viewport.cells.append(TerminalCell(glyph, style, flags))
    var overlays = Int(number(data, offset))
    for _ in range(overlays):
        var row = UInt16(number(data, offset))
        var start = UInt16(number(data, offset))
        var end = UInt16(number(data, offset))
        var kind = UInt16(number(data, offset))
        viewport.metadata.overlays.append(TerminalOverlay(row, start, end, kind))
    var color = number(data, offset)
    var column = UInt16(number(data, offset))
    var row = UInt16(number(data, offset))
    var style = UInt8(number(data, offset))
    var visible = UInt8(number(data, offset))
    var blinking = UInt8(number(data, offset))
    var wide = UInt8(number(data, offset))
    viewport.metadata.cursor = TerminalCursor(color, column, row, style, visible, blinking, wide)
    if offset != len(data):
        raise Error("trailing terminal rendering fixture bytes")
    var ui = Ui(device_scale)
    _ = ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    var advance = ui.font_advance(0, size, 109)
    var font = ui.native_font_metrics(0, size)
    var metrics = terminal_metrics(advance, font.ascender, -font.descender, font.line_gap, device_scale, 0, 0)
    opts.cell_width = metrics.width
    opts.line_height = metrics.height
    opts.baseline = metrics.baseline
    ui.set_time(1)
    ui.begin_frame(Float32(width) / device_scale, Float32(height) / device_scale)
    _ = render_viewport_opts(ui, 91, viewport, opts, display_columns, display_rows, True)
    ui.end_frame()
    var renderer = SdlGpuRenderer()
    var pixels = renderer.render_offscreen(ui.dl, ui.atlas, width, height, (viewport.metadata.background << 8) | 255)
    write_png(args[2], pixels.unsafe_ptr(), width, height, width * 4)
    var result = String('{"advance":') + String(advance) + ',"ascent":' + String(font.ascender) + ',"descent":' + String(-font.descender) + ',"line_gap":' + String(font.line_gap) + ',"baseline":' + String(metrics.baseline) + ',"cell_width":' + String(metrics.width) + ',"line_height":' + String(metrics.height) + ',"glyphs":['
    for index in range(len(ui.dl.glyphs)):
        var glyph = ui.dl.glyphs[index]
        var slot = ui.atlas.slots[Int(glyph.slot)]
        if index != 0:
            result += ","
        result += String('{"bounds":[') + String(glyph.x) + "," + String(glyph.y) + "," + String(slot.w) + "," + String(slot.h) + '],"color":' + String(glyph.color) + "}"
    result += '],"metric_bits":['
    var values = SIMD[.float32, 8](advance, font.ascender, -font.descender, font.line_gap, metrics.baseline, metrics.width, metrics.height, 0)
    for index in range(7):
        if index != 0:
            result += ","
        result += String(values[index].to_bits[DType.uint32]())
    result += "]}"
    with open(args[3], "w") as output:
        output.write(result)
