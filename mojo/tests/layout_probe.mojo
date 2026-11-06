from std.sys import argv
from zzmojo.protocol import append_uint
from zzmojo.stream_protocol import decode_stream_frame
from zzmojo.workspace_layout import (
    Bounds,
    PaneRect,
    pane_rects,
    window_rects,
    float_pixels,
    float_placement,
    pip_frame,
)


def read_bytes(path: String) raises -> List[UInt8]:
    with open(path, "r") as source:
        return source.read_bytes()


def float_value(bytes: List[UInt8], offset: Int) raises -> Float32:
    if offset + 4 > len(bytes):
        raise Error("geometry argument is truncated")
    var bits: UInt32 = 0
    for index in range(4):
        bits |= UInt32(bytes[offset + index]) << UInt32(index * 8)
    return Pointer(to=bits).unsafe_bitcast[Float32]()[]


def append_float(mut output: List[UInt8], var value: Float32):
    append_uint(output, UInt64(Pointer(to=value).unsafe_bitcast[UInt32]()[]))


def append_bounds(mut output: List[UInt8], value: Bounds):
    append_float(output, value.x)
    append_float(output, value.y)
    append_float(output, value.width)
    append_float(output, value.height)


def append_optional_bounds(mut output: List[UInt8], value: Optional[Bounds]):
    append_uint(output, UInt64(1 if value else 0))
    if value:
        append_bounds(output, value.value())


def append_rects(mut output: List[UInt8], rects: List[PaneRect]):
    append_uint(output, UInt64(len(rects)))
    for rect in rects:
        append_uint(output, rect.pane)
        append_bounds(output, rect.bounds)
        append_float(output, rect.inset_x)
        append_float(output, rect.inset_y)
        append_uint(output, UInt64(1 if rect.floating else 0))
        append_uint(output, UInt64(1 if rect.modal else 0))


def main() raises:
    var args = argv()
    var frame = decode_stream_frame(read_bytes(args[1]))
    if not frame.snapshot:
        raise Error("geometry fixture has no snapshot")
    var window = frame.snapshot.value().sessions[0].windows[0].copy()
    var parameters = read_bytes(args[2])
    if len(parameters) != 24:
        raise Error("geometry fixture must contain six float arguments")
    var width = float_value(parameters, 0)
    var height = float_value(parameters, 4)
    var cell_width = float_value(parameters, 8)
    var cell_height = float_value(parameters, 12)
    var divider = float_value(parameters, 16)
    var header = float_value(parameters, 20)
    var output = List[UInt8]()
    append_rects(output, pane_rects(window.layout))
    append_rects(
        output,
        window_rects(
            window, width, height, cell_width, cell_height, divider, header
        ),
    )
    append_uint(output, UInt64(len(window.floating)))
    for float in window.floating:
        var clipped = float_pixels(
            float, cell_width, cell_height, width, height
        )
        append_uint(output, UInt64(1 if clipped else 0))
        if clipped:
            append_bounds(output, clipped.value().frame)
            append_optional_bounds(output, clipped.value().content)
        var placed = float_placement(
            float, cell_width, cell_height, width, height
        )
        append_uint(output, UInt64(1 if placed else 0))
        if placed:
            append_bounds(output, placed.value().frame)
            append_bounds(output, placed.value().content)
        append_bounds(
            output,
            pip_frame(
                float,
                window.sx,
                window.sy,
                cell_width,
                cell_height,
                width,
                height,
                header,
            ),
        )
    with open(args[3], "w") as destination:
        destination.write_all(Span(output))
