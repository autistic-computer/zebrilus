from std.math import floor, isfinite
from mjui.ui import Ui, Style, TextStyle, START, CENTER, CURSOR_HAND
from mjui.box import SIDE_TOP
from mjui.native_color import NativeColor, color_float
from mjui.native_quad import NativeSurface
from mjui.text.paragraph import TextOpts
from mjui.text.native_truncate import native_end_truncate
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_native import root_color
from mjui.widgets.penna_contexts import appearance, recipe, state, inherited, control
from mjui.widgets.penna_glass import material
from mjui.widgets.penna_chooser_data import chooser_color
from mjui.widgets.penna_geometry import length, text_width
from mjui.widgets.penna_icon_names import named_icon
from mjui.widgets.penna_palette_shortcuts import PaletteKeystroke, PaletteKeyPart, key_parts, _packed
from mjui.widgets.penna_palette_surface_data import surface_metric, surface_rule
from mjui.widgets.penna_roots import colors as penna_root_values
from mjui.widgets.controls import _input, _nav, _paint_state
from mjui.widgets.key import sub
from .chooser_presentation import ChooserPresentation, ChooserRow


@fieldwise_init
struct ChooserBounds(TrivialRegisterPassable):
    var x: Float32
    var y: Float32
    var width: Float32
    var height: Float32


@fieldwise_init
struct ChooserResult(Copyable, Movable):
    var selected: Optional[UInt32]
    var activate: Bool
    var close: Bool
    var panel: ChooserBounds
    var viewport: ChooserBounds


@fieldwise_init
struct ChooserGeometry(Copyable, Movable):
    var panel: ChooserBounds
    var viewport: ChooserBounds
    var row_height: Float32
    var line_height: Float32
    var list_height: Float32
    var inset: Float32
    var border: Float32
    var notices: List[String]
    var notice_heights: List[Float32]
    var edge: Float32

    def content_width(self) -> Float32:
        return max(self.panel.width - 2 * self.edge, Float32(0))


def _label(mut ui: Ui, text: StringSpan, font: Int32, size: Float32, line: Float32, color: NativeColor, ellipsis: Bool = False, width: Float32 = 0, fallback: Int32 = -1) raises:
    var options = TextOpts().line_height(line)
    var value = String(text)
    if ellipsis:
        value = native_end_truncate(ui, text, font, size, width, [fallback] if fallback >= 0 else List[Int32]())
    ui.text_opts(value.unsafe_ptr(), value.byte_length(), TextStyle(font, size, _packed(color), False, color), options)


def _icon(mut ui: Ui, th: PennaTheme, name: StringSpan, size: Float32, color: NativeColor, drop: Float32 = 0) raises:
    ui.open(Style.col().width(length(ui, size)).height(size))
    ui.open(Style.col().width(length(ui, size)).height(length(ui, size)).at(0, drop))
    named_icon(ui, th.flat, name, length(ui, size), _packed(color), color, physical_layout=True)
    ui.close()
    ui.close()


def _width(mut ui: Ui, text: StringSpan, font: Int32, size: Float32, line: Float32) -> Float32:
    return text_width(ui, text, TextStyle(font, size, 0, False), line)


def _close(mut ui: Ui, th: PennaTheme, key: UInt64) raises -> Bool:
    var response = _input(ui, key, False, True)
    var navigation = _nav(ui, key, False)
    var context = th.flat.on("picker")
    var paint = control(th.theme, 4, _paint_state(False, response.held, response.hovered), Int(context.penna_profile), context.penna_reduced)
    var ink = paint.color(1)
    var panel = th.paint("picker")
    if not ui.native_window_active and panel.glass:
        var glass = material(Int(panel.native_id))
        ink = ink.multiply_alpha(1 - 0.65 * glass.inactive_flattening.clamp(0, 1))
    ui.open(paint.style(Style.row().width(length(ui, 24)).height(length(ui, 24)).align(CENTER, CENTER).id(key)))
    paint.decorations(ui, ui.focus == key)
    _icon(ui, th, "xmark", 14, ink, 0.5)
    ui.close()
    return response.clicked or navigation.activate()


def _cap_width(mut ui: Ui, th: PennaTheme, stroke: PaletteKeystroke) raises -> Float32:
    var parts = _hint_parts(stroke)
    var width = length(ui, 2) * Float32(max(len(parts) - 1, 0))
    for part in parts:
        width += length(ui, 13) if part.icon else _width(ui, part.value, th.flat.font, 11, 16)
    var paint = th.paint("keycap")
    return max(length(ui, 18), width + length(ui, 8) + 2 * length(ui, paint.border_width))


def _hint_parts(stroke: PaletteKeystroke) -> List[PaletteKeyPart]:
    var parts = key_parts(stroke)
    if stroke.key.byte_length() == 1:
        var byte = stroke.key.as_bytes()[0]
        if byte >= 97 and byte <= 122 and len(parts) > 0 and not parts[len(parts) - 1].icon:
            parts[len(parts) - 1].value = String(chr(Int(byte) - 32))
    return parts^


@inline(.never)
def _cap(mut ui: Ui, th: PennaTheme, stroke: PaletteKeystroke) raises:
    var paint = th.paint("keycap")
    var width = _cap_width(ui, th, stroke)
    ui.open(paint.style(Style.row().width(width).height(length(ui, 18)).pad(length(ui, 4), 0).align(CENTER, CENTER)))
    paint.decorations(ui)
    ui.open(Style.row().spacing(length(ui, 2)).align(CENTER, CENTER))
    var parts = _hint_parts(stroke)
    for part in parts:
        if part.icon:
            _icon(ui, th, part.value, 13, paint.color(2))
        else:
            _label(ui, part.value, th.flat.font, 11, 16, paint.color(2))
    ui.close()
    ui.close()


@fieldwise_init
struct _Hint(Copyable, Movable):
    var keys: List[String]
    var label: String


def _hints(searching: Bool, buffer: Bool = False) -> List[_Hint]:
    if searching:
        return [_Hint(["enter"], "accept"), _Hint(["escape"], "cancel")]
    if buffer:
        return [_Hint(["up", "down"], "navigate"), _Hint(["enter"], "paste"), _Hint(["d"], "delete"), _Hint(["/"], "search"), _Hint(["escape"], "close")]
    return [_Hint(["up", "down"], "navigate"), _Hint(["left", "right"], "expand"), _Hint(["enter"], "choose"), _Hint(["/"], "search"), _Hint(["escape"], "close")]


def _hint_width(mut ui: Ui, th: PennaTheme, hint: _Hint) raises -> Float32:
    var width = _width(ui, hint.label, th.flat.font, 10, 14) + length(ui, 4) * Float32(len(hint.keys))
    for key in hint.keys:
        width += _cap_width(ui, th, PaletteKeystroke(key.copy()))
    return width


def _notice_height(mut ui: Ui, text: StringSpan, font: Int32, width: Float32, border: Float32) -> Float32:
    var paragraph = ui.layout_text(text.unsafe_ptr(), text.byte_length(), TextStyle(font, 11, 0, True), max(width - 2 * length(ui, 12), Float32(0)), 16)
    return Float32(max(len(ui.paras.paras[paragraph].lines), 1)) * 16 + 2 * length(ui, 9) + border


def measure_chooser_frame(mut ui: Ui, th: PennaTheme, requested_height: Float32, max_width: Float32, help: Bool, prompt: String) raises -> ChooserGeometry:
    var width = min(length(ui, max_width), max(ui.width - 2 * length(ui, 24), Float32(0)))
    var height = min(length(ui, requested_height), max(ui.height - 44, Float32(0)))
    var x = (ui.width - width) / 2
    var y = length(ui, 22)
    var notices = List[String]()
    if help:
        notices.append("Help: the next key closes this and does nothing else")
    if prompt.byte_length() != 0:
        notices.append(prompt.copy())
    var border = length(ui, surface_metric(th.theme, 1))
    var edge = length(ui, th.paint("picker").border_width)
    var inner = max(width - 2 * edge, Float32(0))
    var notice_heights = List[Float32]()
    var notice_height: Float32 = 0
    for notice in notices:
        var measured = _notice_height(ui, notice, th.flat.mono, inner, border)
        notice_heights.append(measured)
        notice_height += measured
    var list_height = max(height - 2 * edge - length(ui, 56) - length(ui, 34) - notice_height, 2 * length(ui, 8))
    var inset = length(ui, surface_metric(th.theme, 0))
    var content = max(list_height - 2 * length(ui, 8), Float32(0))
    return ChooserGeometry(ChooserBounds(x, y, width, height), ChooserBounds(x + edge + inset, y + edge + length(ui, 56) + notice_height + length(ui, 8), max(inner - 2 * inset, Float32(0)), content), length(ui, 40), 18, list_height, inset, border, notices^, notice_heights^, edge)


def measure_chooser(mut ui: Ui, th: PennaTheme, model: ChooserPresentation) raises -> ChooserGeometry:
    return measure_chooser_frame(ui, th, model.height, 600, model.snapshot.help, model.snapshot.prompt)

@inline(.never)
def _footer(mut ui: Ui, th: PennaTheme, search: Optional[Tuple[String, String]], width: Float32, buffer: Bool = False) raises:
    var color = chooser_color(th.theme, 1)
    var rule = surface_rule(th.theme)
    var border = length(ui, surface_metric(th.theme, 1))
    var hints = _hints(Bool(search), buffer)
    var total = length(ui, 10) * Float32(max(len(hints) - 1, 0))
    for hint in hints:
        total += _hint_width(ui, th, hint)
    total = min(total, max(width - 2 * length(ui, 12) - length(ui, 12) * Float32(2 if search else 1), Float32(0)))
    var query_width = max((width - 4 * length(ui, 12) - total) / 2 - _width(ui, "?", th.flat.mono, 11, 16) - length(ui, 7), Float32(0))
    ui.open(Style.row().width(width).height(length(ui, 34)).pad_sides(length(ui, 12), border, length(ui, 12), 0).spacing(length(ui, 12)).align(START, CENTER).stroke(border, _packed(rule)).border_sides(SIDE_TOP).native_paint().clipped())
    ui.native_surface(NativeSurface(NativeColor(), rule, NativeColor(), 0, border, 2, 0, False))
    if search:
        ui.open(Style.row().grow().min_width(0).spacing(length(ui, 7)).align(START, CENTER))
        _label(ui, search.value()[0], th.flat.mono, 11, 16, root_color(th.theme, 1))
        ui.open(Style.col().grow().min_width(0).height(length(ui, 16)).clipped())
        _label(ui, search.value()[1], th.flat.mono, 11, 16, root_color(th.theme, 1), True, query_width, th.flat.font)
        ui.close()
        ui.close()
    ui.open(Style.col().grow())
    ui.close()
    ui.open(Style.row().width(total).spacing(length(ui, 10)).align(START, CENTER).clipped())
    for hint in hints:
        ui.open(Style.row().width(_hint_width(ui, th, hint)).spacing(length(ui, 4)).align(START, CENTER))
        for key in hint.keys:
            _cap(ui, th, PaletteKeystroke(key.copy()))
        _label(ui, hint.label, th.flat.font, 10, 14, color)
        ui.close()
    ui.close()
    ui.close()


def _badge_width(mut ui: Ui, th: PennaTheme, medium: Int32, selected: Bool) raises -> Float32:
    var profile = inherited("menu-item", "selected") if selected else 0
    var paint = appearance(th.theme, recipe("badge", "", False), state("rest"), profile, th.flat.penna_reduced)
    return _width(ui, "Tagged", medium, 10, 14) + length(ui, 8) + 2 * length(ui, paint.border_width)


def _badge(mut ui: Ui, th: PennaTheme, medium: Int32, selected: Bool, width: Float32) raises:
    var profile = inherited("menu-item", "selected") if selected else 0
    var paint = appearance(th.theme, recipe("badge", "", False), state("rest"), profile, th.flat.penna_reduced)
    ui.open(paint.style(Style.row().width(width).height(length(ui, 16)).pad(length(ui, 4), 0).align(START, CENTER)))
    paint.decorations(ui)
    _label(ui, "Tagged", medium, 10, 14, paint.color(1))
    ui.close()


@inline(.never)
def _row(mut ui: Ui, th: PennaTheme, medium: Int32, key: UInt64, row: ChooserRow, selected: Bool, gutter: Bool, width: Float32, y: Float32) raises -> Optional[Bool]:
    var response = ui.interact(key, focus_on_press=False)
    if response.hovered or response.held:
        ui.cursor = CURSOR_HAND
    var paint = th.paint("menu-item", "selected" if selected else "active" if response.held else "hover" if response.hovered else "rest")
    var ink = paint.color(1)
    var faded = ink.multiply_alpha(Float32(1) if selected else Float32(0.8))
    var text_font = medium if row.active else th.flat.font
    var target = _width(ui, row.target, th.flat.mono, 10, 14)
    var badge = _badge_width(ui, th, medium, selected) if row.tagged else Float32(0)
    var inner = max(width - 2 * length(ui, 8), Float32(0))
    var fixed = (length(ui, 30) if gutter else Float32(0)) + length(ui, Float32(row.depth) * 16) + length(ui, 16) + length(ui, 14) + length(ui, 8) + length(ui, 12) + target + length(ui, 22)
    if row.tagged:
        fixed += length(ui, 8) + badge
    var detail = min(_width(ui, row.detail, th.flat.font, 11, 16), inner * 0.3)
    var detail_gap = length(ui, 12)
    detail = min(detail, max(inner - fixed - detail_gap, Float32(0)))
    var label = max(inner - fixed - detail_gap - detail, Float32(0))
    ui.open(paint.style(Style.row().width(width).height(length(ui, 40)).pad(length(ui, 8), 0).align(START, CENTER).id(key).at(0, y)))
    paint.decorations(ui)
    if gutter:
        ui.open(Style.col().width(length(ui, 30)).height(14).clipped())
        _label(ui, row.key, th.flat.mono, 10, 14, faded)
        ui.close()
    ui.open(Style.col().width(length(ui, Float32(row.depth) * 16)))
    ui.close()
    ui.open(Style.col().width(length(ui, 16)))
    if row.disclosure != 0:
        _icon(ui, th, "chevron-down" if row.disclosure == 2 else "chevron-right", 12, faded)
    ui.close()
    var icon = String("folder") if row.target.as_bytes()[0] == 36 else String("panels-top-left")
    if row.pane_kind:
        var names: List[String] = ["square-terminal", "globe", "file", "bot"]
        icon = names[Int(row.pane_kind.value())].copy()
    _icon(ui, th, icon, 14, ink if row.active or selected else faded)
    ui.open(Style.col().width(length(ui, 8)))
    ui.close()
    ui.open(Style.col().width(label).height(18).clipped())
    _label(ui, row.label, text_font, 13, 18, ink, True, label, th.flat.mono)
    ui.close()
    ui.open(Style.col().width(detail_gap))
    ui.close()
    ui.open(Style.col().width(detail).height(16).clipped())
    _label(ui, row.detail, th.flat.font, 11, 16, faded, True, detail, th.flat.mono)
    ui.close()
    ui.open(Style.col().width(length(ui, 12)))
    ui.close()
    ui.open(Style.col().width(target).height(14))
    _label(ui, row.target, th.flat.mono, 10, 14, faded)
    ui.close()
    if row.tagged:
        ui.open(Style.col().width(length(ui, 8)))
        ui.close()
        _badge(ui, th, medium, selected, badge)
    ui.open(Style.row().width(length(ui, 22)).align(2, CENTER))
    if row.active:
        _icon(ui, th, "check", 12, ink)
    ui.close()
    ui.close()
    return Optional[Bool](response.clicks >= 2) if response.pressed else None


@inline(.never)
def open_chooser_frame(mut ui: Ui, th: PennaTheme, key: UInt64, geometry: ChooserGeometry, title: String, subtitle: String, medium: Int32, overlay: Bool) raises -> ChooserResult:
    var width = geometry.content_width()
    var height = geometry.panel.height
    var x = geometry.panel.x
    var y = geometry.panel.y
    var edge = geometry.edge
    var notices = geometry.notices.copy()
    var rule = surface_rule(th.theme)
    var border = geometry.border
    var notice_heights = geometry.notice_heights.copy()
    var list_height = geometry.list_height
    var inset = geometry.inset
    var content = geometry.viewport.height
    var backdrop_key = sub(key, 1)
    var backdrop = ui.interact(backdrop_key)
    var result = ChooserResult(None, False, backdrop.pressed, geometry.panel, geometry.viewport)
    var backdrop_style = Style.col().width(ui.width).height(ui.height).id(backdrop_key)
    if overlay:
        ui.overlay_begin(0, 0, backdrop_style)
    else:
        ui.open(backdrop_style.at(0, 0))
    var scrim = chooser_color(th.theme, 0)
    ui.open(Style.col().width(ui.width).height(ui.height).fill(_packed(scrim)).native_paint().at(0, 0))
    ui.native_surface(NativeSurface(scrim, NativeColor(), NativeColor(), 0, 0, 2, 0, False))
    ui.close()
    var panel = th.paint("picker")
    var radius = Float32(-1)
    if panel.radius > 0 and panel.radius < 4096:
        radius = Float32(40) * color_float(penna_root_values(th.theme)[20]) + 8
        panel.radius = radius
    ui.open(panel.style(Style.col().width(geometry.panel.width).height(height).pad(edge, edge).clipped().id(key).at(x, y)))
    panel.decorations(ui, radius=radius)
    ui.open(Style.row().width(width).height(length(ui, 56)).pad(length(ui, 12), 0).align(START, CENTER))
    ui.open(Style.col().grow().min_width(0).spacing(length(ui, 2)))
    _label(ui, title, medium, 13, 18, root_color(th.theme, 1))
    _label(ui, subtitle, th.flat.font, 11, 16, chooser_color(th.theme, 1), True, max(width - length(ui, 48), Float32(0)), th.flat.mono)
    ui.close()
    result.close |= _close(ui, th, sub(key, 2))
    ui.close()
    for index in range(len(notices)):
        ref notice = notices[index]
        ui.open(Style.col().width(width).height(notice_heights[index]).pad(length(ui, 12), length(ui, 9)).stroke(border, _packed(rule)).border_sides(4).native_paint())
        ui.native_surface(NativeSurface(NativeColor(), rule, NativeColor(), 0, border, 2, 0, False))
        var ink = panel.color(1)
        ui.text_opts(notice.unsafe_ptr(), notice.byte_length(), TextStyle(th.flat.mono, 11, _packed(ink), True, ink), TextOpts().line_height(16))
        ui.close()
    ui.open(Style.col().width(width).height(list_height).pad_sides(inset, length(ui, 8), inset, length(ui, 8)).clipped())
    ui.open(Style.col().width(max(width - 2 * inset, Float32(0))).height(content).clipped())
    return result^


def close_chooser_frame(mut ui: Ui, th: PennaTheme, search: Optional[Tuple[String, String]], width: Float32, overlay: Bool, buffer: Bool = False) raises:
    ui.close()
    ui.close()
    _footer(ui, th, search, width, buffer)
    ui.close()
    if overlay:
        ui.overlay_end()
    else:
        ui.close()


@inline(.never)
def render_chooser(mut ui: Ui, th: PennaTheme, key: UInt64, model: ChooserPresentation, scroll: Float32 = 0, medium_font: Int32 = -1, marked: String = "", overlay: Bool = True) raises -> ChooserResult:
    if not isfinite(scroll):
        raise Error("chooser scroll offset is not finite")
    var medium = th.flat.font if medium_font < 0 else medium_font
    var search = model.search(marked)
    var geometry = measure_chooser(ui, th, model)
    var width = geometry.content_width()
    var content = geometry.viewport.height
    var inset = geometry.inset
    var row_height = geometry.row_height
    var count = len(model.snapshot.items[].values)
    var offset = min(max(scroll, Float32(0)), max(Float32(count) * row_height - content, Float32(0)))
    var first = Int(floor(offset / row_height)) if row_height > 0 else 0
    var visible = List[ChooserRow]()
    for index in range(first, min(count, first + 12)):
        visible.append(model.row(index))
    var result = open_chooser_frame(ui, th, key, geometry, model.title, model.subtitle, medium, overlay)
    for slot in range(len(visible)):
        var index = first + slot
        var pressed = _row(ui, th, medium, sub(key, index + 64), visible[slot], model.snapshot.selected == UInt32(index), model.show_key_gutter, max(width - 2 * inset, Float32(0)), Float32(index) * row_height - offset)
        if pressed:
            result.selected = UInt32(index)
            result.activate = pressed.value()
    close_chooser_frame(ui, th, search, width, overlay)
    return result^
