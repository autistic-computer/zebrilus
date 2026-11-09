from std.sys import argv
from std.testing import assert_equal, assert_raises
from mjui.ui import Ui, Style, MOTION_REDUCED
from mjui.native_quad import NativeSurface
from mjui.native_color import NativeColor, color_float
from mjui.png import write_png
from mjui.render.sdlgpu import SdlGpuRenderer
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_native import root_color
from zzmojo.protocol import Decoder, validate_envelope
from zzmojo.tree_wire import read_text, read_bool, read_optional_text
from zzmojo.prefix_bindings import read_prefix_tables
from zzmojo.which_key import WhichKeyModel
from zzmojo.which_key_sheet import which_key_sheet
from zzmojo.which_key_component import paint_which_key
from zzmojo.which_key_render_plan import prepare_which_key
from zzmojo.which_key_live import LiveWhichKey, WhichKeyIntent, WhichKeyContext
from zzmojo.desktop_router_delivery import DesktopUiEffect, DesktopEnvelope
from zzmojo.prefix_bindings import PrefixBinding
from render import geometry


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as file:
        data = file.read_bytes()
    var input = Decoder(data^)
    input.offset = 0
    var theme = Int(input.uint())
    var width = color_float(UInt32(input.uint()))
    var height = color_float(UInt32(input.uint()))
    var scale = color_float(UInt32(input.uint()))
    var table = read_text(input)
    var prefix = read_optional_text(input)
    var all = read_bool(input)
    var frame = input.bytes()
    input.finish()
    _ = validate_envelope(Span(frame))
    var reader = Decoder(frame^)
    if reader.uint() != 10 or reader.uint() != 0 or reader.uint() != 25:
        raise Error("which-key fixture requires full tables")
    var tables = read_prefix_tables(reader)
    reader.finish()
    var model = WhichKeyModel()
    var sheet = which_key_sheet(model, tables, table, prefix, all)
    if not sheet:
        raise Error("which-key fixture has no sheet")
    var ui = Ui(scale)
    ui.native_backdrop = True
    ui.native_scene = True
    ui.native_paths = True
    ui.motion = MOTION_REDUCED
    var regular = Int32(ui.add_font("bench/assets/fonts/Inter-Regular.ttf"))
    var bold = Int32(ui.add_font("bench/assets/fonts/Inter-SemiBold.ttf"))
    var mono = Int32(ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf"))
    var icons = Int32(ui.add_font("bench/assets/fonts/lucide.ttf"))
    var th = PennaTheme(theme, regular, bold, mono, icons)
    ui.set_time(1)
    ui.begin_frame(width, height)
    ui.open(Style.col().width(width).height(height).fill(th.flat.bg).native_paint())
    ui.native_surface(NativeSurface(root_color(theme, 0), NativeColor(), NativeColor(), 0, 0, 2, 0, False))
    if theme == 0 and scale == 1 and width == 720 and height == 600:
        var plan = prepare_which_key(ui, th, sheet.value(), 32, 40)
        var boxes_before = len(ui.boxes)
        ui.width += 1
        with assert_raises():
            _ = paint_which_key(ui, th, 9200, plan)
        ui.width -= 1
        ui.scale += 0.25
        with assert_raises():
            _ = paint_which_key(ui, th, 9200, plan)
        ui.scale -= 0.25
        th.theme = 1
        with assert_raises():
            _ = paint_which_key(ui, th, 9200, plan)
        th.theme = 0
        assert_equal(len(ui.boxes), boxes_before)
    var live = LiveWhichKey(9200)
    if all and table == "prefix":
        var bindings = List[PrefixBinding]()
        for item in tables:
            if item.name == "prefix":
                bindings = item.bindings.copy()
                break
        live.apply(WhichKeyIntent.all_keys(DesktopUiEffect(3, None, DesktopEnvelope(1, 1, 1, 1, None), UInt64(0), bindings^, prefix.copy())))
    else:
        live.apply(WhichKeyIntent.table_changed(600_000_000, table, False, 400))
    var shown = live.frame(ui, th, tables, WhichKeyContext(table, False, prefix.copy(), False, 32, 40))
    if not shown.visible or shown.refusal or shown.wake:
        raise Error("which-key live frame did not paint the fixture sheet")
    var measured = shown.geometry.value().copy()
    ui.close()
    ui.end_frame()
    if not ui.native_scene_complete:
        raise Error("which-key source scene replay is incomplete")
    var renderer = SdlGpuRenderer()
    var w = Int(width * scale)
    var h = Int(height * scale)
    var pixels = renderer.render_offscreen(ui.dl, ui.atlas, w, h, th.flat.bg)
    write_png(args[2], pixels.unsafe_ptr(), w, h, w * 4)
    with open(args[2] + ".json", "w") as output:
        output.write(geometry(ui).replace('"size":[1004,588],"scale":1', String('"size":[', width, ",", height, '],"scale":', scale)))
    with open(args[2] + ".layout.json", "w") as output:
        output.write(String('{"panel":[', measured.panel.x, ",", measured.panel.y, ",", measured.panel.width, ",", measured.panel.height, '],"viewport":[', measured.viewport.x, ",", measured.viewport.y, ",", measured.viewport.width, ",", measured.viewport.height, '],"cap_width":', measured.cap_width, ',"column_width":', measured.column_width, ',"per_column":', measured.fit.per_column, ',"per_line":', measured.fit.per_line, ',"content_height":', measured.content_height, "}"))
