from std.sys import argv
from mjui.ui import Ui, Style, MOTION_REDUCED
from mjui.native_quad import NativeSurface
from mjui.native_color import NativeColor
from mjui.png import write_png
from mjui.render.sdlgpu import SdlGpuRenderer
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_native import root_color
from zzmojo.editor_pane import editor_hint, editor_error_panel


def main() raises:
    var args = argv()
    if len(args) != 9:
        raise Error("editor scene probe takes theme, scale, variant, width, height, title, message and an output path")
    var theme = Int(atof(args[1]))
    var scale = Float32(atof(args[2]))
    var variant = Int(atof(args[3]))
    var width = Float32(atof(args[4]))
    var height = Float32(atof(args[5]))
    var title = String(args[6])
    var message = String(args[7])
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
    if variant == 0:
        editor_hint(ui, th, width, height)
    else:
        _ = editor_error_panel(ui, th, 9300, title, message, width, height)
    ui.close()
    ui.end_frame()
    var complete = ui.native_scene_complete
    var renderer = SdlGpuRenderer()
    var w = Int(width * scale)
    var h = Int(height * scale)
    var pixels = renderer.render_offscreen(ui.dl, ui.atlas, w, h, th.flat.bg)
    write_png(String(args[8]), pixels.unsafe_ptr(), w, h, w * 4)
    print(1 if complete else 0)
