from std.sys import argv
from mjui.ui import Ui, Style, MOTION_REDUCED
from mjui.native_quad import NativeSurface
from mjui.native_color import NativeColor, color_float
from mjui.png import write_png
from mjui.render.sdlgpu import SdlGpuRenderer
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_native import root_color
from zzmojo.protocol import Decoder
from zzmojo.chooser_buffer_protocol import decode_choose_buffer
from zzmojo.palette_dialog_state import ChooseBufferSnapshot
from zzmojo.buffer_presentation import BufferPresentation
from zzmojo.buffer_component import render_buffer_chooser
from render import geometry


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var input = Decoder(data^)
    input.offset = 0
    var theme = Int(input.uint())
    var width = color_float(UInt32(input.uint()))
    var height = color_float(UInt32(input.uint()))
    var scale = color_float(UInt32(input.uint()))
    var now = input.uint()
    var event = decode_choose_buffer(input.bytes())
    input.finish()
    if not event or not event.value().state:
        raise Error("chooser fixture has no retained state")
    var state = event.take()
    var model = BufferPresentation(ChooseBufferSnapshot(state.state.take()))
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
    _ = render_buffer_chooser(ui, th, 9300, model, now)
    ui.close()
    ui.end_frame()
    if not ui.native_scene_complete:
        raise Error("chooser source scene replay is incomplete")
    var renderer = SdlGpuRenderer()
    var w = Int(width * scale)
    var h = Int(height * scale)
    var pixels = renderer.render_offscreen(ui.dl, ui.atlas, w, h, th.flat.bg)
    write_png(args[2], pixels.unsafe_ptr(), w, h, w * 4)
    var scene = geometry(ui).replace('"size":[1004,588],"scale":1', String('"size":[', width, ",", height, '],"scale":', scale))
    with open(args[2] + ".json", "w") as output:
        output.write(scene)
    var boxes = String("[")
    for index in range(len(ui.boxes)):
        var box = ui.boxes[index]
        if index > 0:
            boxes += ","
        boxes += String("[", box.parent, ",", box.x, ",", box.y, ",", box.w, ",", box.h, "]")
    boxes += "]"
    with open(args[2] + ".boxes.json", "w") as output:
        output.write(boxes)
