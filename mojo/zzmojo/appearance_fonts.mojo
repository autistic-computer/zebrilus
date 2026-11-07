from mjui.ui import Ui
from mjui.text.font_group import FontSpec
from mjui.text.fonts import SLANT_UPRIGHT, SLANT_ITALIC
from mjui.text.hb import HbFeature
from .appearance_protocol import TerminalAppearance
from .terminal_settings import font_request
from .terminal_appearance import TerminalMetrics, terminal_metrics
from .protocol import append_text, append_uint


def _hb_tag(tag: UInt32) -> UInt32:
    return (tag & 255) << 24 | (tag & 65280) << 8 | (tag >> 8) & 65280 | (tag >> 24)


def terminal_font_specs(appearance: TerminalAppearance, fallback: String, size: Float32) -> List[FontSpec]:
    var result = List[FontSpec]()
    for style in range(4):
        var request = font_request(appearance, fallback, style & 1 != 0, style & 2 != 0)
        var features = List[HbFeature]()
        for feature in request.features:
            features.append(HbFeature(_hb_tag(feature.tag), feature.value, 0, 0xFFFFFFFF))
        result.append(FontSpec(request.family, request.fallbacks, features, Int(request.weight), SLANT_ITALIC if request.italic else SLANT_UPRIGHT, 100, size))
    return result^


def terminal_font_key(specs: List[FontSpec]) -> List[UInt8]:
    var key = List[UInt8]()
    append_uint(key, UInt64(len(specs)))
    for spec in specs:
        append_text(key, spec.family)
        append_uint(key, UInt64(len(spec.fallbacks)))
        for family in spec.fallbacks:
            append_text(key, family)
        append_uint(key, UInt64(len(spec.features)))
        for feature in spec.features:
            append_uint(key, UInt64(feature.tag))
            append_uint(key, UInt64(feature.value))
            append_uint(key, UInt64(feature.start))
            append_uint(key, UInt64(feature.end))
        append_uint(key, UInt64(spec.weight))
        append_uint(key, UInt64(spec.slant))
        append_uint(key, UInt64(spec.stretch))
        append_uint(key, UInt64(spec.size.to_bits[DType.uint32]()))
        append_text(key, spec.path)
        append_uint(key, UInt64(spec.index))
        append_uint(key, UInt64(len(spec.axes)))
        for axis in spec.axes:
            append_uint(key, UInt64(axis.tag))
            append_uint(key, UInt64(axis.value.to_bits[DType.uint32]()))
    return key^


def terminal_grid_metrics(mut ui: Ui, font: Int32, size: Float32, appearance: TerminalAppearance) -> TerminalMetrics:
    var advance = ui.font_advance(font, size, 109)
    var metrics = ui.native_font_metrics(font, size)
    return terminal_metrics(advance, metrics.ascender, -metrics.descender, metrics.line_gap, ui.scale, appearance.cell_height_kind, appearance.cell_height_adjustment)
