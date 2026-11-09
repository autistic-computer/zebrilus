from std.math import ceil, isfinite
from std.sys import size_of
from mjui.ui import Ui
from mjui.native_quad import NativeSurface, NATIVE_RECORD_COUNT, NATIVE_RECORD_BYTES
from mjui.native_geometry import native_snap
from mjui.text.native_context import NativeTextContext
from mjui.text.native_text import NativeTextRequest, NativeTextPrepared, native_text_context, native_text_prepare
from mjui.text.native_truncate import native_end_truncate
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_geometry import length
from mjui.widgets.penna_palette_shortcuts import PaletteKeystroke, PaletteKeyPart, key_parts, _key_icon
from .which_key import which_key_group_title
from .which_key_caps import WhichKeyStroke, which_key_format
from .which_key_sheet import WhichKeySheet
from .which_key_layout import WhichKeyFit, WhichKeyStack, which_key_fit


@fieldwise_init
struct WhichKeyBounds(TrivialRegisterPassable):
    var x: Float32
    var y: Float32
    var width: Float32
    var height: Float32


@fieldwise_init
struct WhichKeyGeometry(Copyable, Movable):
    var panel: WhichKeyBounds
    var viewport: WhichKeyBounds
    var cap_width: Float32
    var column_width: Float32
    var header_height: Float32
    var content_height: Float32
    var inset: Float32
    var fit: WhichKeyFit


@fieldwise_init
struct WhichKeyCapPaint(Copyable, Movable):
    var text: String
    var width: Float32
    var x: Float32
    var yours: Bool


@fieldwise_init
struct WhichKeyRowPaint(Copyable, Movable):
    var caps: List[WhichKeyCapPaint]
    var label: String
    var label_width: Float32
    var repeat: Bool
    var repeat_width: Float32


@fieldwise_init
struct WhichKeyPlan(Movable):
    var geometry: WhichKeyGeometry
    var context: NativeTextContext
    var receipt: NativeTextPrepared
    var rows: List[WhichKeyRowPaint]
    var parts: List[PaletteKeyPart]
    var part_widths: List[Float32]
    var prefix_width: Float32
    var table: String
    var table_width: Float32
    var prefix_raw: String
    var prefix: Bool
    var more: Bool
    var more_text: String
    var more_cap_width: Float32
    var more_label_width: Float32
    var canvas_width: Float32
    var canvas_height: Float32
    var scale: Float32
    var theme: Int
    var font: Int32
    var mono: Int32
    var profile: Int32
    var reduced: Bool
    var surfaces: Int

    def validate(self, ui: Ui, th: PennaTheme) raises:
        if self.canvas_width != ui.width or self.canvas_height != ui.height or self.scale != ui.scale or self.theme != th.theme or self.font != th.flat.font or self.mono != th.flat.mono or self.profile != th.flat.penna_profile or self.reduced != th.flat.penna_reduced:
            raise Error("which-key plan belongs to a stale viewport or appearance")
        self.receipt.validate(ui)
        var limit = min(NATIVE_RECORD_COUNT, NATIVE_RECORD_BYTES // size_of[NativeSurface]())
        if self.surfaces > limit - len(ui.native_surfaces):
            raise Error("which-key scene exceeds native surface budget")


def _auto(ui: Ui, width: Float32) -> Float32:
    return ceil(ceil(width) * ui.scale) / ui.scale


def _stack_height_pixels(ui: Ui, stack: WhichKeyStack) -> Float32:
    var result = Float32(max(len(stack.blocks) - 1, 0)) * native_snap(16 * ui.scale)
    for block in stack.blocks:
        result += Float32(len(block.rows)) * native_snap(22 * ui.scale)
        if block.titled:
            result += native_snap(20 * ui.scale) + native_snap(4 * ui.scale)
    return result


def _content_height_pixels(ui: Ui, fit: WhichKeyFit) -> Float32:
    var result: Float32 = 0
    for first in range(0, len(fit.stacks), fit.per_line):
        var height: Float32 = 0
        for index in range(first, min(first + fit.per_line, len(fit.stacks))):
            height = max(height, _stack_height_pixels(ui, fit.stacks[index]))
        if first > 0:
            result += native_snap(16 * ui.scale)
        result += height
    return result


def _add(mut requests: List[NativeTextRequest], text: String, font: Int32, size: Float32) -> Int:
    var index = len(requests)
    requests.append(NativeTextRequest(text, font, size))
    return index


def _parts(sheet: WhichKeySheet) -> List[PaletteKeyPart]:
    if not sheet.prefix:
        return List[PaletteKeyPart]()
    ref stroke = sheet.prefix.value()
    var key = stroke.key.copy()
    if _key_icon(key).byte_length() == 0:
        key = which_key_format(WhichKeyStroke(key.copy(), 0, None))
    return key_parts(PaletteKeystroke(key^, stroke.modifiers))


@inline(.never)
def prepare_which_key(mut ui: Ui, th: PennaTheme, sheet: WhichKeySheet, top: Float32 = 0, bottom: Float32 = 0) raises -> WhichKeyPlan:
    if not isfinite(ui.width) or not isfinite(ui.height) or not isfinite(ui.scale) or ui.scale <= 0 or not isfinite(top) or not isfinite(bottom) or top < 0 or bottom < 0 or len(sheet.rows) == 0:
        raise Error("which-key viewport is not valid")
    var context = native_text_context(ui, [th.flat.font, th.flat.mono])
    var requests = List[NativeTextRequest]()
    var offsets = List[Int]()
    var label_indices = List[Int]()
    var surfaces = 3 + Int(Bool(sheet.prefix))
    for row in sheet.rows:
        surfaces += 2 + len(row.caps)
        offsets.append(len(requests))
        for cap in row.caps:
            _ = _add(requests, cap.text(), th.flat.mono, 11)
        label_indices.append(_add(requests, row.label.copy(), th.flat.font, 12))
    var repeats = _add(requests, "↻", th.flat.font, 11)
    var table_index = _add(requests, sheet.table.copy(), th.flat.font, 12)
    var more_text = sheet.more.value().text() if sheet.more else String()
    var more_index = _add(requests, more_text.copy(), th.flat.mono, 11)
    var more_label_index = _add(requests, "all keys", th.flat.font, 12)
    var prefix_index = _add(requests, sheet.prefix_raw.copy(), th.flat.mono, 11)
    var parts = _parts(sheet)
    var part_indices = List[Int]()
    for part in parts:
        part_indices.append(-1 if part.icon else _add(requests, part.value.copy(), th.flat.font, 11))
    var measured = native_text_prepare(ui, requests, context)
    measured.validate(ui)
    var cap_width: Float32 = 24
    for index in range(len(sheet.rows)):
        var width: Float32 = 0
        for slot in range(len(sheet.rows[index].caps)):
            width += measured.widths[offsets[index] + slot]
        width += Float32(max(len(sheet.rows[index].caps) - 1, 0)) * 4
        cap_width = max(cap_width, width)
    cap_width = ceil(min(cap_width, Float32(112)))
    var raw_column = cap_width + 8 + 150
    var available_width = max(min(ui.width - 32, Float32(1360)), Float32(0))
    var fit = which_key_fit(sheet, raw_column, available_width, ui.height - 16)
    var width_pixels = native_snap(min(available_width, fit.width) * ui.scale)
    var header_pixels = native_snap(18 * ui.scale) if sheet.prefix else ceil(16 * ui.scale)
    var content_pixels = _content_height_pixels(ui, fit)
    var inset_pixels = native_snap(12 * ui.scale) + native_snap(th.paint("sheet").border_width * ui.scale)
    var header_gap_pixels = native_snap(8 * ui.scale)
    var height_pixels = min(2 * inset_pixels + header_pixels + header_gap_pixels + content_pixels, max(native_snap((ui.height - 16) * ui.scale), Float32(0)))
    var x_pixels = (ui.width * ui.scale - width_pixels) / 2
    var y_pixels = ui.height * ui.scale - native_snap((bottom + 8) * ui.scale) - height_pixels
    var geometry = WhichKeyGeometry(WhichKeyBounds(x_pixels / ui.scale, y_pixels / ui.scale, width_pixels / ui.scale, height_pixels / ui.scale), WhichKeyBounds((x_pixels + inset_pixels) / ui.scale, (y_pixels + inset_pixels + header_pixels + header_gap_pixels) / ui.scale, max(width_pixels - 2 * inset_pixels, Float32(0)) / ui.scale, max(height_pixels - 2 * inset_pixels - header_pixels - header_gap_pixels, Float32(0)) / ui.scale), length(ui, cap_width), length(ui, raw_column), header_pixels / ui.scale, content_pixels / ui.scale, inset_pixels / ui.scale, fit^)
    var rows = List[WhichKeyRowPaint]()
    var paint_requests = List[NativeTextRequest]()
    for index in range(len(sheet.rows)):
        ref row = sheet.rows[index]
        var widths = List[Float32]()
        var total: Float32 = 0
        for slot in range(len(row.caps)):
            var actual = ceil(ceil(measured.widths[offsets[index] + slot]) * ui.scale)
            widths.append(actual)
            total += actual
        var gap = Float32(max(len(row.caps) - 1, 0)) * native_snap(4 * ui.scale)
        var cap_pixels = native_snap(cap_width * ui.scale)
        var free_space = cap_pixels - (gap + total)
        var targets = List[Float32]()
        var target_sum: Float32 = 0
        for slot in range(len(row.caps)):
            var target = max(widths[slot] + free_space * (widths[slot] / total), Float32(0)) if free_space < 0 and total > 0 else widths[slot]
            targets.append(target)
            target_sum += target
        var remaining = cap_pixels - (gap + target_sum)
        var position: Float32 = 0
        var caps = List[WhichKeyCapPaint]()
        for slot in range(len(row.caps)):
            var item_width = targets[slot]
            var offset = remaining if slot == 0 else native_snap(4 * ui.scale)
            var location = position + offset
            var budget = item_width / ui.scale
            var text = row.caps[slot].text() if measured.widths[offsets[index] + slot] <= budget else native_end_truncate(ui, row.caps[slot].text(), th.flat.mono, 11, budget, [th.flat.font])
            _ = _add(paint_requests, text.copy(), th.flat.mono, 11)
            caps.append(WhichKeyCapPaint(text^, item_width / ui.scale, location / ui.scale, row.caps[slot].yours))
            position += offset + item_width
        var repeat_pixels = ceil(ceil(measured.widths[repeats]) * ui.scale) if row.repeat else Float32(0)
        var label_pixels = max(native_snap(raw_column * ui.scale) - cap_pixels - native_snap(8 * ui.scale) - (repeat_pixels + native_snap(8 * ui.scale) if row.repeat else Float32(0)), Float32(0))
        var label_budget = label_pixels / ui.scale
        var label = row.label.copy() if measured.widths[label_indices[index]] <= label_budget else native_end_truncate(ui, row.label, th.flat.font, 12, label_budget, [th.flat.mono])
        _ = _add(paint_requests, label.copy(), th.flat.font, 12)
        rows.append(WhichKeyRowPaint(caps^, label^, label_pixels / ui.scale, row.repeat, repeat_pixels / ui.scale))
    var part_widths = List[Float32]()
    var prefix_width = _auto(ui, measured.widths[prefix_index])
    if sheet.prefix:
        prefix_width = length(ui, 2) * Float32(max(len(parts) - 1, 0))
        for index in range(len(parts)):
            var part_width = length(ui, 13) if parts[index].icon else _auto(ui, measured.widths[part_indices[index]])
            part_widths.append(part_width)
            prefix_width += part_width
            if not parts[index].icon:
                _ = _add(paint_requests, parts[index].value.copy(), th.flat.font, 11)
        var paint = th.paint("keycap")
        prefix_width = max(length(ui, 18), prefix_width + length(ui, 8) + 2 * length(ui, paint.border_width))
    else:
        _ = _add(paint_requests, sheet.prefix_raw.copy(), th.flat.mono, 11)
    _ = _add(paint_requests, sheet.table.copy(), th.flat.font, 12)
    _ = _add(paint_requests, "↻", th.flat.font, 11)
    _ = _add(paint_requests, more_text.copy(), th.flat.mono, 11)
    _ = _add(paint_requests, "all keys", th.flat.font, 12)
    for group in range(1, 7):
        _ = _add(paint_requests, which_key_group_title(UInt8(group)), th.flat.font, 11)
    var receipt = native_text_prepare(ui, paint_requests, context)
    receipt.validate(ui)
    var result = WhichKeyPlan(geometry^, context, receipt^, rows^, parts^, part_widths^, prefix_width, sheet.table.copy(), _auto(ui, measured.widths[table_index]), sheet.prefix_raw.copy(), Bool(sheet.prefix), Bool(sheet.more), more_text^, _auto(ui, measured.widths[more_index]), _auto(ui, measured.widths[more_label_index]), ui.width, ui.height, ui.scale, th.theme, th.flat.font, th.flat.mono, th.flat.penna_profile, th.flat.penna_reduced, surfaces)
    result.validate(ui, th)
    return result^
