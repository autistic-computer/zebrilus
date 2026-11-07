from std.math import floor
from mjui.term.grid import TermOpts
from mjui.term.appearance import adjust_line_height
from .appearance_protocol import TerminalAppearance


def _round_positive(value: Float32) -> Float32:
    var lower = floor(value)
    return lower + Float32(1) if value - lower >= Float32(0.5) else lower


def _snap_length(value: Float32, scale: Float32) -> Float32:
    var pixel = Float32(1) / scale
    return max(_round_positive(max(value, pixel) * scale) / scale, pixel)


@fieldwise_init
struct TerminalMetrics(TrivialRegisterPassable):
    var width: Float32
    var height: Float32
    var baseline: Float32


def terminal_metrics(
    advance: Float32,
    ascent: Float32,
    descent: Float32,
    line_gap: Float32,
    scale: Float32,
    height_kind: UInt8,
    height_adjustment: Float32,
) -> TerminalMetrics:
    var width = _snap_length(advance if advance > 1 else Float32(8), scale)
    var natural = max(ascent + descent + line_gap, Float32(1))
    var height = _snap_length(max(adjust_line_height(natural, height_kind, height_adjustment), Float32(1)), scale)
    var baseline = (height - ascent - descent) / Float32(2) + ascent
    return TerminalMetrics(width, height, baseline)


def terminal_options(
    appearance: TerminalAppearance,
    fonts: Tuple[Int32, Int32, Int32, Int32],
    size: Float32,
    focused: Bool,
    metrics: Optional[TerminalMetrics] = None,
    blink_epoch: UInt64 = 0,
) -> TermOpts:
    var opts = TermOpts(
        fonts[0],
        size,
        bold=fonts[1],
        italic=fonts[2],
        bold_italic=fonts[3],
        focused=focused,
        selection=appearance.selection_background,
        match_bg=appearance.search_match_color,
        match_current=appearance.search_current_color,
        link=(appearance.link_color << 8) | 255,
        native_colors=True,
        minimum_contrast=appearance.minimum_contrast,
        native_appearance=True,
        selection_foreground=appearance.selection_foreground,
        rounded_selection=appearance.rounded_selection,
        copy_cursor=appearance.copy_cursor_color,
        cursor_blink_policy=appearance.cursor_blink_policy,
        cursor_blink_interval_ms=appearance.cursor_blink_interval_ms,
        cursor_blink_epoch=blink_epoch,
        font_thicken=appearance.font_thicken,
        font_thicken_strength=appearance.font_thicken_strength,
        font_synthetic_style=appearance.font_synthetic_style,
    )
    if metrics:
        opts.cell_width = metrics.value().width
        opts.line_height = metrics.value().height
        opts.baseline = metrics.value().baseline
    return opts
