from std.sys import argv
from std.os import abort
from mjui.ui import Ui, Style
from mjui.input import TextInput
from mjui.testing import Driver, Scene
from mjui.png import write_png
from mjui.render.sdlgpu import SdlGpuRenderer
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_palette import PaletteRow as KitRow, palette_row
from mjui.widgets.penna_native import root_color
from mjui.native_color import NativeColor
from mjui.native_quad import NativeSurface
from zzmojo.palette_assets import palette_asset_documents
from zzmojo.palette_component import palette_kit_row
from zzmojo.palette_presentation import PaletteRow


struct AssetFixture(Scene):
    var theme: PennaTheme
    var rows: List[KitRow]

    def __init__(out self, var theme: PennaTheme, openai: Int32, claude: Int32):
        self.theme = theme^
        self.rows = List[KitRow]()
        for index in range(8):
            var row = PaletteRow()
            row.label = "Agent provider"
            row.detail = "Native asset"
            row.icon = 'Asset("icons/openai.svg")' if index % 2 == 0 else 'Asset("icons/claude.svg")'
            row.indent = 16 if index >= 4 else 0
            if index >= 4:
                row.expanded = index >= 6
            var kit_row = palette_kit_row(row, openai, claude)
            if kit_row.icon_doc != (openai if index % 2 == 0 else claude) or kit_row.icon_name.byte_length() != 0:
                abort("palette asset mapping differs")
            self.rows.append(kit_row^)

    def build(mut self, mut ui: Ui, input: TextInput) raises:
        ui.open(Style.col().grow().grow_h().fill(self.theme.flat.bg).pad(20, 20).native_paint())
        ui.native_surface(NativeSurface(root_color(self.theme.theme, 0), NativeColor(), NativeColor(), 0, 0, 0, 0, False))
        for index in range(len(self.rows)):
            ui.open(Style.col().grow().height(40))
            _ = palette_row(ui, self.theme, UInt64(index + 1), self.rows[index], index % 4 >= 2, 600)
            ui.close()
        ui.close()


def main() raises:
    var args = argv()
    var theme = 0
    for byte in args[2].as_bytes():
        theme = theme * 10 + Int(byte) - 48
    var scale = Float32(atof(args[3]))
    var ui = Ui(scale)
    var regular = Int32(ui.add_font("bench/assets/fonts/Inter-Regular.ttf"))
    var bold = Int32(ui.add_font("bench/assets/fonts/Inter-SemiBold.ttf"))
    var icons = Int32(ui.add_font("bench/assets/fonts/lucide.ttf"))
    var mono = Int32(ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf"))
    var openai, claude = palette_asset_documents(ui)
    var th = PennaTheme(theme, regular, bold, mono, icons)
    var bg = th.flat.bg
    var fixture = AssetFixture(th^, openai, claude)
    var driver = Driver[AssetFixture](ui^, fixture^, 640, 360)
    driver.frame()
    var retained_icons = len(driver.ui.native_icons)
    var retained_docs = len(driver.ui.images.docs)
    driver.frame()
    if len(driver.ui.native_icons) != retained_icons or len(driver.ui.images.docs) != retained_docs:
        abort("palette asset retention changed across frames")
    var renderer = SdlGpuRenderer()
    var width = Int(640 * scale)
    var height = Int(360 * scale)
    var pixels = renderer.render_offscreen(driver.ui.dl, driver.ui.atlas, width, height, bg)
    write_png(args[1], pixels.unsafe_ptr(), width, height, width * 4)
