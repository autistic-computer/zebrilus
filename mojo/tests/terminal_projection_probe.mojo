from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_bytes
from zzmojo.terminal_model import (
    TerminalCell,
    TerminalStyle,
    TerminalMode,
    TerminalScrollbar,
    TerminalViewport,
    KittyPlacement,
)
from zzmojo.terminal_projection import (
    RowProjection,
    LocalRowSource,
    LocalColors,
    projection_plan,
    local_live_projection,
    local_row_source,
    peek_row_source,
    kitty_display_row,
    last_visible_row_has_content,
)


def read_bytes(path: String) raises -> List[UInt8]:
    with open(path, "r") as source:
        return source.read_bytes()


def read_row(mut reader: Decoder) raises -> RowProjection:
    return RowProjection(
        UInt16(reader.uint()), UInt16(reader.uint()), UInt16(reader.uint())
    )


def put_row(mut output: List[UInt8], value: RowProjection):
    append_uint(output, UInt64(value.source_start))
    append_uint(output, UInt64(value.destination_start))
    append_uint(output, UInt64(value.count))


def put_source(mut output: List[UInt8], value: LocalRowSource):
    append_uint(output, UInt64(value.kind))
    append_uint(output, value.index)


def signed(mut reader: Decoder) raises -> Int64:
    var value = reader.uint()
    return -Int64(value >> 1) - 1 if value & 1 else Int64(value >> 1)


def put_signed(mut output: List[UInt8], value: Int64):
    append_uint(
        output, UInt64(-(value + 1)) * 2 + 1 if value < 0 else UInt64(value) * 2
    )


def colors(mut reader: Decoder) raises -> LocalColors:
    var host = Array[UInt32, 18](fill=0)
    var local = Array[UInt32, 18](fill=0)
    for index in range(18):
        host[index] = UInt32(reader.uint())
    for index in range(18):
        local[index] = UInt32(reader.uint())
    return LocalColors(host^, local^)


def style(mut reader: Decoder) raises -> TerminalStyle:
    return TerminalStyle(
        UInt32(reader.uint()),
        UInt32(reader.uint()),
        UInt32(reader.uint()),
        UInt16(reader.uint()),
        UInt8(reader.uint()),
        UInt8(reader.uint()),
    )


def put_style(mut output: List[UInt8], value: TerminalStyle):
    append_uint(output, UInt64(value.foreground))
    append_uint(output, UInt64(value.background))
    append_uint(output, UInt64(value.underline_color))
    append_uint(output, UInt64(value.attributes))
    append_uint(output, UInt64(value.underline))
    append_uint(output, UInt64(value.background_index))


def viewport(mut reader: Decoder) raises -> TerminalViewport:
    var value = TerminalViewport()
    value.columns = UInt16(reader.uint())
    value.rows = UInt16(reader.uint())
    value.metadata.background = UInt32(reader.uint())
    value.metadata.mode = TerminalMode(UInt8(reader.uint()), 0, 0, False)
    value.metadata.scrollbar = TerminalScrollbar(
        UInt32(reader.uint()), UInt32(reader.uint()), UInt32(reader.uint())
    )
    var count = reader.uint()
    for _ in range(Int(count)):
        value.cells.append(
            TerminalCell(
                UInt32(reader.uint()),
                UInt16(reader.uint()),
                UInt16(reader.uint()),
            )
        )
    count = reader.uint()
    for _ in range(Int(count)):
        value.styles.append(style(reader))
    count = reader.uint()
    value.grapheme_offsets.clear()
    for _ in range(Int(count)):
        value.grapheme_offsets.append(UInt32(reader.uint()))
    value.grapheme_bytes = reader.bytes()
    return value^


def execute(mode: String, mut reader: Decoder) raises -> List[UInt8]:
    var output = List[UInt8]()
    if mode == "projections":
        var value = RowProjection.new(
            UInt16(reader.uint()), UInt16(reader.uint()), reader.uint() != 0
        )
        put_row(output, value)
        var count = reader.uint()
        for _ in range(Int(count)):
            var row = value.source_row(UInt16(reader.uint()))
            append_uint(output, UInt64(1 if row else 0))
            if row:
                append_uint(output, UInt64(row.value()))
        count = reader.uint()
        for _ in range(Int(count)):
            var row = value.display_row(UInt16(reader.uint()))
            append_uint(output, UInt64(1 if row else 0))
            if row:
                append_uint(output, UInt64(row.value()))
    elif mode == "local":
        put_row(
            output,
            local_live_projection(
                UInt32(reader.uint()),
                UInt32(reader.uint()),
                UInt16(reader.uint()),
                UInt16(reader.uint()),
            ),
        )
    elif mode == "sources":
        var absolute = UInt32(reader.uint())
        var server = UInt32(reader.uint())
        var live = UInt16(reader.uint())
        var history = reader.uint()
        var below = reader.uint()
        put_source(
            output, local_row_source(absolute, server, live, history, below)
        )
        var count = reader.uint()
        for _ in range(Int(count)):
            var source = peek_row_source(
                absolute, UInt32(reader.uint()), server, live, history
            )
            append_uint(output, UInt64(1 if source else 0))
            if source:
                put_source(output, source.value())
    elif mode == "kitty":
        var projection = read_row(reader)
        var viewport_row = Int32(signed(reader))
        var absolute = reader.uint()
        var rows = UInt32(reader.uint())
        var target: Optional[UInt32] = None
        if reader.uint():
            target = UInt32(reader.uint())
        var placement = KittyPlacement(
            0, 0, 0, 0, viewport_row, absolute, 0, 0, 0, rows, 0, 0, None
        )
        var result = kitty_display_row(placement, target, projection)
        append_uint(output, UInt64(1 if result else 0))
        if result:
            put_signed(output, result.value())
    elif mode == "recolors":
        var local = colors(reader)
        put_style(output, local.recolor(style(reader)))
    elif mode == "placements":
        var value = viewport(reader)
        var local: Optional[LocalColors] = None
        if reader.uint():
            local = colors(reader)
        var rows = UInt16(reader.uint())
        var command = reader.uint() != 0
        var scrolled = reader.uint() != 0
        var target: Optional[UInt32] = None
        if reader.uint():
            target = UInt32(reader.uint())
        for visible in [UInt16(0), UInt16(1), value.rows, UInt16.MAX]:
            append_uint(
                output,
                UInt64(
                    1 if last_visible_row_has_content(
                        value, visible, local
                    ) else 0
                ),
            )
        var plan = projection_plan(
            value, rows, command, scrolled, target, local
        )
        append_uint(output, UInt64(1 if plan.placement.bottom_anchored else 0))
        append_uint(
            output, UInt64(1 if plan.placement.project_from_bottom else 0)
        )
        put_row(output, plan.rows)
        put_row(output, plan.live)
    else:
        raise Error("unknown terminal projection category")
    reader.finish()
    return output^


def main() raises:
    var args = argv()
    var reader = Decoder(read_bytes(args[2]))
    reader.offset = 0
    var count = reader.uint()
    var output = List[UInt8]()
    for _ in range(Int(count)):
        var request = Decoder(reader.bytes())
        request.offset = 0
        var result = execute(args[1], request)
        append_bytes(output, Span(result))
    reader.finish()
    with open(args[3], "w") as destination:
        destination.write_all(Span(output))
