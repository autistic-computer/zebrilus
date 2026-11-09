from std.math import isfinite
from mjui.ui import Ui, Style, TextStyle, START, CENTER
from mjui.native_color import NativeColor
from mjui.native_geometry import native_snap
from mjui.native_quad import NativeSurface
from mjui.text.native_context import NativeTextContext
from mjui.text.paragraph import TextOpts
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_native import root_color
from mjui.widgets.penna_chooser_data import chooser_color
from mjui.widgets.penna_which_key_data import which_key_accent
from mjui.widgets.penna_geometry import length
from mjui.widgets.penna_palette_shortcuts import _packed
from .chooser_component import _icon
from .which_key import which_key_group_title
from .which_key_sheet import WhichKeySheet
from .which_key_render_plan import WhichKeyBounds, WhichKeyGeometry, WhichKeyPlan, WhichKeyRowPaint, prepare_which_key, _stack_height_pixels


def _text(mut ui: Ui, text: StringSpan, font: Int32, size: Float32, ink: NativeColor, context: NativeTextContext):
    ui.text_opts(text.unsafe_ptr(), text.byte_length(), TextStyle(font, size, _packed(ink), False, ink), TextOpts().line_height(16).native_line(context))


def _clip(mut ui: Ui, style: Style):
    ui.open(style.clipped().native_paint())
    ui.native_surface(NativeSurface(NativeColor(), NativeColor(), NativeColor(), 0, 0, 2, 0, False, True))


def measure_which_key(mut ui: Ui, th: PennaTheme, sheet: WhichKeySheet, top: Float32 = 0, bottom: Float32 = 0) raises -> WhichKeyGeometry:
    var plan = prepare_which_key(ui, th, sheet, top, bottom)
    plan.validate(ui, th)
    return plan.geometry.copy()


@inline(.never)
def _row(mut ui: Ui, th: PennaTheme, row: WhichKeyRowPaint, geometry: WhichKeyGeometry, accent: NativeColor, context: NativeTextContext) raises:
    _clip(ui, Style.col().width(geometry.cap_width).height(16))
    for cap in row.caps:
        _clip(ui, Style.col().width(cap.width).height(16).at(cap.x, 0))
        _text(ui, cap.text, th.flat.mono, 11, accent if cap.yours else chooser_color(th.theme, 1), context)
        ui.close()
    ui.close()
    _clip(ui, Style.col().width(row.label_width).height(16))
    _text(ui, row.label, th.flat.font, 12, root_color(th.theme, 1), context)
    ui.close()
    if row.repeat:
        ui.open(Style.col().width(row.repeat_width).height(16))
        _text(ui, "↻", th.flat.font, 11, chooser_color(th.theme, 1), context)
        ui.close()


@inline(.never)
def _prefix(mut ui: Ui, th: PennaTheme, plan: WhichKeyPlan) raises:
    var paint = th.paint("keycap")
    ui.open(paint.style(Style.row().width(plan.prefix_width).height(length(ui, 18)).pad(length(ui, 4), 0).align(CENTER, CENTER)))
    paint.decorations(ui, physical_layout=True)
    ui.open(Style.row().spacing(length(ui, 2)).align(CENTER, CENTER))
    for index in range(len(plan.parts)):
        ref part = plan.parts[index]
        if part.icon:
            _icon(ui, th, part.value, 13, paint.color(2))
        else:
            ui.open(Style.col().width(plan.part_widths[index]).height(16))
            _text(ui, part.value, th.flat.font, 11, paint.color(2), plan.context)
            ui.close()
    ui.close()
    ui.close()


@inline(.never)
def paint_which_key(mut ui: Ui, th: PennaTheme, key: UInt64, plan: WhichKeyPlan, scroll: Float32 = 0, overlay: Bool = True) raises -> WhichKeyGeometry:
    if not isfinite(scroll):
        raise Error("which-key scroll offset is not finite")
    plan.validate(ui, th)
    var geometry = plan.geometry.copy()
    var paint = th.paint("sheet")
    var accent = which_key_accent(th.theme)
    var muted = chooser_color(th.theme, 1)
    var offset = min(max(scroll, Float32(0)), max(geometry.content_height - geometry.viewport.height, Float32(0)))
    if overlay:
        ui.overlay_begin(0, 0, Style.col().width(ui.width).height(ui.height))
    else:
        ui.open(Style.col().width(ui.width).height(ui.height).at(0, 0))
    _ = ui.interact(key, focus_on_press=False)
    ui.open(paint.style(Style.col().width(geometry.panel.width).height(geometry.panel.height).id(key).at(geometry.panel.x, geometry.panel.y)))
    paint.decorations(ui, physical_layout=True)
    ui.open(Style.row().width(geometry.viewport.width).height(geometry.header_height).spacing(length(ui, 8)).align(START, CENTER).at(geometry.inset, geometry.inset))
    if plan.prefix:
        _prefix(ui, th, plan)
    else:
        ui.open(Style.col().width(plan.prefix_width).height(16))
        _text(ui, plan.prefix_raw, th.flat.mono, 11, muted, plan.context)
        ui.close()
    ui.open(Style.col().width(plan.table_width).height(16))
    _text(ui, plan.table, th.flat.font, 12, root_color(th.theme, 1), plan.context)
    ui.close()
    if plan.more:
        ui.open(Style.col().grow())
        ui.close()
        ui.open(Style.row().width(plan.more_cap_width + length(ui, 4) + plan.more_label_width).height(16).spacing(length(ui, 4)).align(START, CENTER))
        ui.open(Style.col().width(plan.more_cap_width).height(16))
        _text(ui, plan.more_text, th.flat.mono, 11, muted, plan.context)
        ui.close()
        ui.open(Style.col().width(plan.more_label_width).height(16))
        _text(ui, "all keys", th.flat.font, 12, muted, plan.context)
        ui.close()
        ui.close()
    ui.close()
    _clip(ui, Style.col().width(geometry.viewport.width).height(geometry.viewport.height).at(geometry.inset, geometry.inset + geometry.header_height + length(ui, 8)))
    var y_pixels: Float32 = 0
    var row_pixels = native_snap(22 * ui.scale)
    var title_pixels = native_snap(20 * ui.scale)
    var title_gap_pixels = native_snap(4 * ui.scale)
    var group_gap_pixels = native_snap(16 * ui.scale)
    var column_pixels = native_snap(geometry.column_width * ui.scale)
    var scroll_pixels = offset * ui.scale
    for first in range(0, len(geometry.fit.stacks), geometry.fit.per_line):
        var line_height_pixels: Float32 = 0
        for index in range(first, min(first + geometry.fit.per_line, len(geometry.fit.stacks))):
            ref stack = geometry.fit.stacks[index]
            var x_pixels = Float32(index - first) * (column_pixels + group_gap_pixels)
            var block_y_pixels: Float32 = 0
            for block_index in range(len(stack.blocks)):
                ref block = stack.blocks[block_index]
                if block_index > 0:
                    block_y_pixels += group_gap_pixels
                if block.titled:
                    ui.open(Style.col().width(geometry.column_width).height(length(ui, 20)).pad_sides(geometry.cap_width + length(ui, 8), 0, 0, 0).at(x_pixels / ui.scale, (y_pixels + block_y_pixels - scroll_pixels) / ui.scale))
                    if block.title:
                        _text(ui, which_key_group_title(block.group), th.flat.font, 11, muted, plan.context)
                    ui.close()
                    block_y_pixels += title_pixels + title_gap_pixels
                for row_index in block.rows:
                    ui.open(Style.row().width(geometry.column_width).height(length(ui, 22)).spacing(length(ui, 8)).align(START, CENTER).at(x_pixels / ui.scale, (y_pixels + block_y_pixels - scroll_pixels) / ui.scale))
                    _row(ui, th, plan.rows[row_index], geometry, accent, plan.context)
                    ui.close()
                    block_y_pixels += row_pixels
            line_height_pixels = max(line_height_pixels, _stack_height_pixels(ui, stack))
        y_pixels += line_height_pixels + group_gap_pixels
    ui.close()
    ui.close()
    if overlay:
        ui.overlay_end()
    else:
        ui.close()
    return geometry^


def render_which_key(mut ui: Ui, th: PennaTheme, key: UInt64, sheet: WhichKeySheet, top: Float32 = 0, bottom: Float32 = 0, scroll: Float32 = 0, overlay: Bool = True) raises -> WhichKeyGeometry:
    var plan = prepare_which_key(ui, th, sheet, top, bottom)
    return paint_which_key(ui, th, key, plan, scroll, overlay)
