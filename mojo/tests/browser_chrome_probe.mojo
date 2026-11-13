from std.sys import argv
from mjui.ui import Ui, Style, MOTION_REDUCED
from mjui.native_quad import NativeSurface
from mjui.native_color import NativeColor
from mjui.png import write_png
from mjui.render.sdlgpu import SdlGpuRenderer
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_native import root_color
from mjui.widgets.penna_geometry import length
from zzmojo.browser_presentation import BrowserPresentation, BrowserTabLabel, BROWSER_STATE_PAGE, BROWSER_STATE_EMPTY, BROWSER_STATE_ERROR, BROWSER_PICK_STATUS
from zzmojo.browser_chrome import BrowserChromeInput, BROWSER_HEADER_HEIGHT, browser_header, browser_start_surface, browser_start_surface_end, browser_empty_hint, browser_recent_rows, browser_error_panel, browser_pick_status

comptime WIDTH: Float32 = 720
comptime HEIGHT: Float32 = 480


def presentation(variant: Int) -> BrowserPresentation:
    var view = BrowserPresentation()
    if variant == 1:
        view.tabs.append(BrowserTabLabel(1, "example.com", "Example Domain"))
        view.tabs.append(BrowserTabLabel(2, "Rust Programming Language", "https://www.rust-lang.org/"))
        view.tabs.append(BrowserTabLabel(3, "localhost", "http://localhost:8080/"))
        view.active = 1
        view.address = "rust-lang.org"
        view.can_go_back = True
        view.can_go_forward = True
    elif variant == 2 or variant == 3:
        view.tabs.append(BrowserTabLabel(1, "New Tab", ""))
        view.state = BROWSER_STATE_EMPTY
    else:
        view.tabs.append(BrowserTabLabel(1, "example.com", "Example Domain"))
        view.address = "example.com/docs"
        view.can_go_back = True
    if variant == 4:
        view.state = BROWSER_STATE_ERROR
        view.error = "net::ERR_NAME_NOT_RESOLVED"
        view.retry = True
    if variant == 5:
        view.pick_status = String(BROWSER_PICK_STATUS)
    return view^


def frame(mut ui: Ui, th: PennaTheme, variant: Int, view: BrowserPresentation) raises:
    var several = variant == 1
    var input = BrowserChromeInput(not several, not several, False, False, False, Float32(40) if several else Float32(0), variant == 5, False)
    ui.begin_frame(WIDTH, HEIGHT)
    ui.open(Style.col().width(WIDTH).height(HEIGHT).fill(th.flat.bg).native_paint())
    ui.native_surface(NativeSurface(root_color(th.theme, 0), NativeColor(), NativeColor(), 0, 0, 2, 0, False))
    _ = browser_header(ui, th, 9400, WIDTH, view, input)
    var height = HEIGHT - length(ui, BROWSER_HEADER_HEIGHT)
    ui.open(Style.col().width(WIDTH).height(height))
    if variant == 2:
        _ = browser_start_surface(ui, 9500, WIDTH, height)
        browser_empty_hint(ui, th, WIDTH - 2 * length(ui, 12))
        browser_start_surface_end(ui)
    elif variant == 3:
        _ = browser_start_surface(ui, 9500, WIDTH, height)
        _ = browser_recent_rows(ui, th, 9600, ["example.com/docs", "autistic.computer", "localhost:8080/admin"], WIDTH - 2 * length(ui, 12))
        browser_start_surface_end(ui)
    elif variant == 4:
        _ = browser_start_surface(ui, 9500, WIDTH, height)
        _ = browser_error_panel(ui, th, 9700, view.error, view.retry, WIDTH - 2 * length(ui, 12))
        browser_start_surface_end(ui)
    elif variant == 5:
        browser_pick_status(ui, th, view.pick_status, WIDTH, height)
    ui.close()
    ui.close()
    ui.end_frame()


def main() raises:
    var args = argv()
    if len(args) != 5:
        raise Error("browser chrome probe takes theme, scale, variant and an output path")
    var theme = Int(atof(args[1]))
    var scale = Float32(atof(args[2]))
    var variant = Int(atof(args[3]))
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
    var view = presentation(variant)
    ui.set_time(1)
    ui.pointer_move(0, 0)
    frame(ui, th, variant, view)
    frame(ui, th, variant, view)
    var complete = ui.native_scene_complete
    var renderer = SdlGpuRenderer()
    var w = Int(WIDTH * scale)
    var h = Int(HEIGHT * scale)
    var pixels = renderer.render_offscreen(ui.dl, ui.atlas, w, h, th.flat.bg)
    write_png(String(args[4]), pixels.unsafe_ptr(), w, h, w * 4)
    print(1 if complete else 0)
