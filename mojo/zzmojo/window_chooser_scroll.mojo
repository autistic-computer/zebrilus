from std.math import isfinite
from .chooser_protocol import CHOOSER_MAX_ITEMS


def window_chooser_scroll(
    scroll: Float32,
    row_height: Float32,
    viewport: Float32,
    count: UInt32,
    selected: UInt32,
    nearest: Bool,
) raises -> Float32:
    if (
        not isfinite(scroll)
        or not isfinite(row_height)
        or not isfinite(viewport)
        or row_height <= 0
        or viewport < 0
        or count > CHOOSER_MAX_ITEMS
    ):
        raise Error("invalid window chooser scroll geometry")
    var maximum = max(Float32(count) * row_height - viewport, Float32(0))
    var position = scroll.clamp(Float32(0), maximum)
    var top = Float32(selected) * row_height
    var bottom = top + row_height
    var above = top < position
    var below = bottom > position + viewport
    if above or below:
        var target = bottom - viewport if nearest and not above else top
        position = target.clamp(Float32(0), maximum)
    return Float32(0) if position == 0 else position
