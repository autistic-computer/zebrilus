from std.math import floor, isfinite
from mjui.ui import Ui, Style, TextStyle, START, CENTER, CURSOR_HAND
from mjui.native_color import NativeColor
from mjui.text.paragraph import TextOpts, TEXT_END_KEEP_SPACES
from mjui.text.native_context import NativeTextContext
from mjui.text.native_text import NativeTextRequest, native_text_context, native_text_prepare
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_contexts import appearance, recipe, state, inherited
from mjui.widgets.penna_geometry import length
from mjui.widgets.penna_palette_shortcuts import _packed
from mjui.widgets.key import sub
from .buffer_presentation import BufferPresentation, BufferRow
from .chooser_component import ChooserGeometry, ChooserResult, measure_chooser_frame, open_chooser_frame, close_chooser_frame, _label, _width


def measure_buffer_chooser(mut ui: Ui, th: PennaTheme, model: BufferPresentation) -> ChooserGeometry:
    return measure_chooser_frame(ui, th, model.height, 640, model.snapshot.help, model.snapshot.prompt)


def _badge_width(mut ui: Ui, th: PennaTheme, selected: Bool) raises -> Float32:
    var profile = inherited("menu-item", "selected") if selected else 0
    var paint = appearance(th.theme, recipe("badge", "primary", True), state("rest"), profile, th.flat.penna_reduced)
    return _width(ui, "TAGGED", th.flat.mono, 10, 14) + length(ui, 8) + 2 * length(ui, paint.border_width)


def _badge(mut ui: Ui, th: PennaTheme, selected: Bool, width: Float32) raises:
    var profile = inherited("menu-item", "selected") if selected else 0
    var paint = appearance(th.theme, recipe("badge", "primary", True), state("rest"), profile, th.flat.penna_reduced)
    ui.open(paint.style(Style.row().width(width).height(length(ui, 16)).pad(length(ui, 4), 0).align(START, CENTER)))
    paint.decorations(ui)
    _label(ui, "TAGGED", th.flat.mono, 10, 14, paint.color(1))
    ui.close()


def _right(mut ui: Ui, th: PennaTheme, text: String, width: Float32, ink: NativeColor, context: NativeTextContext) raises:
    ui.open(Style.col().width(width))
    ui.text_opts(text.unsafe_ptr(), text.byte_length(), TextStyle(th.flat.mono, 10, _packed(ink), True, ink), TextOpts().line_height(length(ui, 14)).native_line(context).max_lines(0).align(TEXT_END_KEEP_SPACES))
    ui.close()


@inline(.never)
def _row(mut ui: Ui, th: PennaTheme, key: UInt64, row: BufferRow, selected: Bool, gutter: Bool, width: Float32, y: Float32, context: NativeTextContext) raises -> Optional[Bool]:
    var response = ui.interact(key, focus_on_press=False)
    if response.hovered or response.held:
        ui.cursor = CURSOR_HAND
    var paint = th.paint("menu-item", "selected" if selected else "active" if response.held else "hover" if response.hovered else "rest")
    var ink = paint.color(1)
    var faded = ink.multiply_alpha(Float32(1) if selected else Float32(0.8))
    var badge = _badge_width(ui, th, selected) if row.tagged else Float32(0)
    var fixed = length(ui, 142) + length(ui, 76) + length(ui, 54) + length(ui, 12) * Float32(3 + Int(gutter) + Int(row.tagged))
    if gutter:
        fixed += length(ui, 30)
    if row.tagged:
        fixed += length(ui, 8) + badge
    var preview = max(width - 2 * length(ui, 10) - fixed, Float32(0))
    ui.open(paint.style(Style.row().width(width).height(length(ui, 40)).pad(length(ui, 10), 0).align(START, CENTER).id(key).at(0, y)))
    paint.decorations(ui)
    ui.open(Style.row().width(max(width - 2 * length(ui, 10), Float32(0))).spacing(length(ui, 12)).align(START, CENTER))
    if gutter:
        ui.open(Style.col().width(length(ui, 30)).height(14).clipped())
        _label(ui, row.key, th.flat.mono, 10, 14, faded)
        ui.close()
    ui.open(Style.col().width(length(ui, 142)).height(16).clipped())
    _label(ui, row.name, th.flat.mono, 11, 16, ink, True, length(ui, 142), th.flat.font)
    ui.close()
    ui.open(Style.col().width(preview).height(16).clipped())
    _label(ui, row.preview, th.flat.font, 11, 16, ink, True, preview, th.flat.mono)
    ui.close()
    _right(ui, th, row.size, length(ui, 76), faded, context)
    _right(ui, th, row.age, length(ui, 54), faded, context)
    if row.tagged:
        ui.open(Style.row().width(length(ui, 8) + badge).pad_sides(length(ui, 8), 0, 0, 0).align(START, CENTER))
        _badge(ui, th, selected, badge)
        ui.close()
    ui.close()
    ui.close()
    return Optional[Bool](response.clicks >= 2) if response.pressed else None


@inline(.never)
def render_buffer_chooser(mut ui: Ui, th: PennaTheme, key: UInt64, model: BufferPresentation, now: UInt64, scroll: Float32 = 0, marked: String = "", overlay: Bool = True) raises -> ChooserResult:
    if not isfinite(scroll):
        raise Error("buffer chooser scroll offset is not finite")
    var search = model.search(marked)
    var geometry = measure_buffer_chooser(ui, th, model)
    var width = geometry.panel.width
    var row_height = geometry.row_height
    var count = len(model.snapshot.items[].values)
    var offset = min(max(scroll, Float32(0)), max(Float32(count) * row_height - geometry.viewport.height, Float32(0)))
    var first = Int(floor(offset / row_height)) if row_height > 0 else 0
    var visible = List[BufferRow]()
    for index in range(first, min(count, first + 12)):
        visible.append(model.row(index, now))
    var context = native_text_context(ui, [th.flat.font, th.flat.mono])
    var requests = List[NativeTextRequest]()
    for row in visible:
        requests.append(NativeTextRequest(row.size.copy(), th.flat.mono, 10))
        requests.append(NativeTextRequest(row.age.copy(), th.flat.mono, 10))
    var receipt = native_text_prepare(ui, requests, context)
    receipt.validate(ui)
    var result = open_chooser_frame(ui, th, key, geometry, model.title, model.subtitle, th.flat.font, overlay)
    for slot in range(len(visible)):
        var index = first + slot
        var pressed = _row(ui, th, sub(key, index + 64), visible[slot], model.snapshot.selected == UInt32(index), model.show_key_gutter, max(width - 2 * geometry.inset, Float32(0)), Float32(index) * row_height - offset, context)
        if pressed:
            result.selected = UInt32(index)
            result.activate = pressed.value()
    close_chooser_frame(ui, th, search, width, overlay, True)
    return result^
