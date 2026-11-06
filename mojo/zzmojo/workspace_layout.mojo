from std.math import fma, max, min
from std.utils.numerics import isfinite
from .tree_model import LayoutTree, WindowSnapshot, FloatingPane


@fieldwise_init
struct Bounds(TrivialRegisterPassable):
    var x: Float32
    var y: Float32
    var width: Float32
    var height: Float32

    def placed(self, axis: UInt8, start: Float32, length: Float32) -> Self:
        if axis == 0:
            return Self(
                fma(start, self.width, self.x),
                self.y,
                self.width * length,
                self.height,
            )
        return Self(
            self.x,
            fma(start, self.height, self.y),
            self.width,
            self.height * length,
        )

    def clipped(self, width: Float32, height: Float32) -> Optional[Self]:
        var left = max(self.x, Float32(0))
        var top = max(self.y, Float32(0))
        var right = min(self.x + self.width, width)
        var bottom = min(self.y + self.height, height)
        if right > left and bottom > top:
            return Self(left, top, right - left, bottom - top)
        return None


@fieldwise_init
struct PaneRect(TrivialRegisterPassable):
    var pane: UInt64
    var bounds: Bounds
    var inset_x: Float32
    var inset_y: Float32
    var floating: Bool
    var modal: Bool


@fieldwise_init
struct FloatPixels(Copyable, Movable):
    var frame: Bounds
    var content: Optional[Bounds]


@fieldwise_init
struct FloatPlacement(TrivialRegisterPassable):
    var frame: Bounds
    var content: Bounds


@fieldwise_init
struct _Pending(TrivialRegisterPassable):
    var index: Int
    var bounds: Bounds


def resolved_ratio(ratio: Float32) -> Float32:
    if not isfinite(ratio):
        return 0.5
    if ratio < 0:
        return 0
    if ratio > 1:
        return 1
    return ratio


def pane_rects(layout: LayoutTree) raises -> List[PaneRect]:
    var output = List[PaneRect]()
    if layout.root < 0:
        return output^
    var pending: List[_Pending] = [_Pending(layout.root, Bounds(0, 0, 1, 1))]
    var visited = 0
    while len(pending) > 0:
        var item = pending.pop()
        if item.index < 0 or item.index >= len(layout.nodes):
            raise Error("layout node index exceeds its arena")
        visited += 1
        if visited > len(layout.nodes):
            raise Error("layout arena contains a repeated node")
        var node = layout.nodes[item.index]
        if node.kind == 0:
            output.append(PaneRect(node.id, item.bounds, 0, 0, False, False))
        elif node.kind == 1:
            if node.axis > 1:
                raise Error("layout split has an unknown axis")
            var ratio = resolved_ratio(node.ratio)
            pending.append(
                _Pending(
                    node.second, item.bounds.placed(node.axis, ratio, 1 - ratio)
                )
            )
            pending.append(
                _Pending(node.first, item.bounds.placed(node.axis, 0, ratio))
            )
        elif node.kind != 2:
            raise Error("layout node has an unknown kind")
    return output^


def pane_box(slot: Bounds, divider: Float32) -> Bounds:
    var left = divider if slot.x > 0 else Float32(0)
    var top = divider if slot.y > 0 else Float32(0)
    return Bounds(
        slot.x + left, slot.y + top, slot.width - left, slot.height - top
    )


def float_pixels(
    float: FloatingPane,
    cell_width: Float32,
    cell_height: Float32,
    width: Float32,
    height: Float32,
) -> Optional[FloatPixels]:
    var pad = Float32(1) if float.border_lines != 6 else Float32(0)
    var content = Bounds(
        Float32(float.xoff) * cell_width,
        Float32(float.yoff) * cell_height,
        Float32(float.sx) * cell_width,
        Float32(float.sy) * cell_height,
    )
    var frame = Bounds(
        content.x - pad * cell_width,
        content.y - pad * cell_height,
        content.width + 2 * pad * cell_width,
        content.height + 2 * pad * cell_height,
    )
    var clipped = frame.clipped(width, height)
    if not clipped:
        return None
    return FloatPixels(clipped.value(), content.clipped(width, height))


def float_placement(
    float: FloatingPane,
    cell_width: Float32,
    cell_height: Float32,
    width: Float32,
    height: Float32,
) -> Optional[FloatPlacement]:
    if (
        width > 0
        and height > 0
        and not float_pixels(float, cell_width, cell_height, width, height)
    ):
        return None
    var pad = Int(1) if float.border_lines != 6 else Int(0)
    var frame = Bounds(
        Float32(Int(float.xoff) - pad) * cell_width,
        Float32(Int(float.yoff) - pad) * cell_height,
        Float32(Int(float.sx) + 2 * pad) * cell_width,
        Float32(Int(float.sy) + 2 * pad) * cell_height,
    )
    var content = Bounds(
        Float32(float.xoff) * cell_width,
        Float32(float.yoff) * cell_height,
        Float32(float.sx) * cell_width,
        Float32(float.sy) * cell_height,
    )
    return FloatPlacement(frame, content)


def _pip_axis(
    offset: Int32,
    extent: UInt16,
    window: UInt16,
    frame: Float32,
    canvas: Float32,
    cell: Float32,
    pad: Int,
) -> Float32:
    if window == 0:
        return cell * Float32(Int(offset) - pad)
    var free = canvas - frame
    if free <= 0:
        return 0
    var room = Float32(window) - Float32(extent) - 2 * Float32(pad)
    var along = min(
        max(Float32(Int(offset) - pad) / room, Float32(0)), Float32(1)
    ) if room > 0 else Float32(0.5)
    var margin = min(Float32(12), free / 2)
    return margin + (free - margin * 2) * along


def pip_frame(
    float: FloatingPane,
    columns: UInt16,
    rows: UInt16,
    cell_width: Float32,
    cell_height: Float32,
    width: Float32,
    height: Float32,
    header: Float32,
) -> Bounds:
    var bordered = float.border_lines != 6
    var frame_width = cell_width * Float32(float.sx)
    var frame_height = cell_height * Float32(float.sy)
    if bordered:
        frame_width += cell_width * 2
        frame_height += max(header, cell_height) + cell_height
    var area_width = (
        width if width > 0 and height > 0 else cell_width * Float32(columns)
    )
    var area_height = (
        height if width > 0 and height > 0 else cell_height * Float32(rows)
    )
    var pad = Int(1) if bordered else Int(0)
    return Bounds(
        _pip_axis(
            float.xoff,
            float.sx,
            columns,
            frame_width,
            area_width,
            cell_width,
            pad,
        ),
        _pip_axis(
            float.yoff,
            float.sy,
            rows,
            frame_height,
            area_height,
            cell_height,
            pad,
        ),
        frame_width,
        frame_height,
    )


def window_rects(
    window: WindowSnapshot,
    width: Float32,
    height: Float32,
    cell_width: Float32,
    cell_height: Float32,
    divider: Float32 = 0,
    header: Float32 = 0,
) raises -> List[PaneRect]:
    var output = List[PaneRect]()
    if window.zoomed_pane:
        if window.zoomed_pane.value() in window.panes:
            output.append(
                PaneRect(
                    window.zoomed_pane.value(),
                    Bounds(0, 0, width, height),
                    0,
                    0,
                    False,
                    False,
                )
            )
    else:
        for item in pane_rects(window.layout):
            if item.pane in window.panes:
                var slot = Bounds(
                    item.bounds.x * width,
                    item.bounds.y * height,
                    item.bounds.width * width,
                    item.bounds.height * height,
                )
                output.append(
                    PaneRect(
                        item.pane, pane_box(slot, divider), 0, 0, False, False
                    )
                )
    for index in range(len(window.floating) - 1, -1, -1):
        var float = window.floating[index]
        if not float.visible or float.pane not in window.panes:
            continue
        var placed = float_placement(
            float, cell_width, cell_height, width, height
        )
        if not placed:
            continue
        var modal = window.modal and window.modal.value().pane == float.pane
        var frame = placed.value().frame if modal else pip_frame(
            float,
            window.sx,
            window.sy,
            cell_width,
            cell_height,
            width,
            height,
            header,
        )
        var inset_x = cell_width if float.border_lines != 6 else Float32(0)
        var inset_y = cell_height if float.border_lines != 6 else Float32(0)
        output.append(
            PaneRect(float.pane, frame, inset_x, inset_y, True, modal)
        )
    return output^
