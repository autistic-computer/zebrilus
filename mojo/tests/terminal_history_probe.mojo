from std.sys import argv
from std.memory import ArcPointer
from std.testing import assert_equal
from zzmojo.protocol import Decoder, append_uint
from zzmojo.local_scroll import (
    ScrollContext,
    LocalScrollState,
    LocalScrollStep,
    pixel_scroll_position,
    daemon_follow_offset,
    scroll_fraction_offset,
    local_scroll_needs_prefetch,
)
from zzmojo.terminal_model import (
    TerminalScrollbar,
    TerminalCell,
    TerminalStyle,
    TerminalViewport,
)
from zzmojo.terminal_wire import decode_terminal_frame
from zzmojo.terminal_history import (
    TerminalHistoryState,
    HistoryRing,
    HistoryDictionary,
    HistoryStyles,
    HistoryGraphemes,
)
from zzmojo.terminal_history_protocol import (
    history_request,
    decode_history_budget,
)
from zzmojo.history_pacer import (
    HistoryPacer,
    HistoryRequestContext,
    HistoryRange,
    history_request_range,
)


def read_bytes(path: String) raises -> List[UInt8]:
    with open(path, "r") as source:
        return source.read_bytes()


def float_value(mut reader: Decoder) raises -> Float32:
    var bits = UInt32(reader.uint())
    return Pointer(to=bits).unsafe_bitcast[Float32]()[]


def put_float(mut output: List[UInt8], value: Float32):
    append_uint(output, UInt64(Pointer(to=value).unsafe_bitcast[UInt32]()[]))


def optional(mut output: List[UInt8], value: Optional[UInt32]):
    append_uint(output, UInt64(1 if value else 0))
    if value:
        append_uint(output, UInt64(value.value()))


def context(mut reader: Decoder) raises -> ScrollContext:
    var total = UInt32(reader.uint())
    var offset = UInt32(reader.uint())
    var length = UInt32(reader.uint())
    var mode = UInt8(reader.uint())
    var mouse = reader.uint() != 0
    var history = reader.uint()
    var below = reader.uint()
    var invalidations = reader.uint()
    var columns = UInt16(reader.uint())
    var rows = UInt16(reader.uint())
    return ScrollContext(
        TerminalScrollbar(total, offset, length),
        mode,
        mouse,
        history,
        below,
        invalidations,
        columns,
        rows,
    )


def signed(mut reader: Decoder) raises -> Int64:
    var value = reader.uint()
    return -Int64(value >> 1) - 1 if value & 1 else Int64(value >> 1)


def put_step(mut output: List[UInt8], value: Optional[LocalScrollStep]):
    append_uint(output, UInt64(1 if value else 0))
    if not value:
        return
    append_uint(output, UInt64(value.value().redraw))
    append_uint(output, UInt64(len(value.value().effects)))
    for effect in value.value().effects:
        append_uint(output, UInt64(effect.kind))
        append_uint(output, effect.value)


def local_trace(mut reader: Decoder, mut output: List[UInt8]) raises:
    var state = LocalScrollState(reader.uint())
    var count = Int(reader.uint())
    for _ in range(count):
        var kind = reader.uint()
        var retained = context(reader)
        var now = reader.uint()
        if kind == 0:
            var delta = float_value(reader)
            var height = float_value(reader)
            put_step(
                output, state.scroll_by_pixels(delta, height, retained, now)
            )
        elif kind == 1:
            put_step(
                output, state.scroll_by_rows(signed(reader), retained, now)
            )
        elif kind == 2:
            put_step(
                output, state.scroll_to(UInt32(reader.uint()), retained, now)
            )
        elif kind == 3:
            var result = state.sync(reader.uint(), retained)
            append_uint(
                output,
                UInt64(
                    0 if not result else 1 if result.value().kind == 0 else 2
                ),
            )
            if result and result.value().kind == 1:
                append_uint(output, UInt64(result.value().target))
        elif kind == 4:
            append_uint(output, UInt64(state.expire(reader.uint(), now)))
        elif kind == 5:
            var result = state.observe(retained, reader.uint() != 0, now)
            append_uint(output, UInt64(result.changed))
            optional(output, result.restore)
        elif kind == 6:
            append_uint(output, UInt64(state.cancel()))
        elif kind == 7:
            append_uint(output, UInt64(state.clear()))
        elif kind == 8:
            append_uint(output, UInt64(state.clear_sub_row()))
        elif kind == 9:
            optional(output, state.flush())
        elif kind == 10:
            optional(output, state.release())
        elif kind == 11:
            var room = state.overscroll_room(retained, float_value(reader))
            put_float(output, room[0])
            put_float(output, room[1])
        else:
            raise Error("unknown local scroll trace operation")
        optional(output, state.target())
        put_float(output, state.sub_row)


def fetch_context(mut reader: Decoder) raises -> HistoryRequestContext:
    return HistoryRequestContext(
        UInt32(reader.uint()),
        UInt32(reader.uint()),
        reader.uint(),
        reader.uint(),
        reader.uint(),
    )


def read_optional_u32(mut reader: Decoder) raises -> Optional[UInt32]:
    if reader.uint() != 0:
        return UInt32(reader.uint())
    return None


def read_optional_u64(mut reader: Decoder) raises -> Optional[UInt64]:
    if reader.uint() != 0:
        return reader.uint()
    return None


def put_optional_u64(mut output: List[UInt8], value: Optional[UInt64]):
    append_uint(output, UInt64(1 if value else 0))
    if value:
        append_uint(output, value.value())


def put_range(mut output: List[UInt8], value: Optional[HistoryRange]):
    append_uint(output, UInt64(1 if value else 0))
    if value:
        append_uint(output, UInt64(value.value().start))
        append_uint(output, UInt64(value.value().count))


def pacer_trace(mut reader: Decoder, mut output: List[UInt8]) raises:
    var pacer = HistoryPacer()
    var count = Int(reader.uint())
    var panes: List[UInt64] = [0, 42, UInt64.MAX]
    for _ in range(count):
        var kind = reader.uint()
        var pane = reader.uint()
        if kind == 0:
            var target = read_optional_u32(reader)
            var budget = reader.uint()
            var retained: Optional[HistoryRequestContext] = None
            if reader.uint() != 0:
                retained = fetch_context(reader)
            var now = reader.uint()
            put_range(
                output, pacer.request(pane, target, budget, retained, now)
            )
        elif kind == 1:
            var arrived = pacer.chunk_arrived(pane, read_optional_u64(reader))
            append_uint(output, UInt64(arrived.apply))
            append_uint(output, UInt64(arrived.follow_up.kind))
            append_uint(output, arrived.follow_up.value)
        elif kind == 2:
            append_uint(output, UInt64(pacer.defer(pane, reader.uint())))
        elif kind == 3:
            append_uint(
                output,
                UInt64(pacer.resume_deferred(pane, read_optional_u64(reader))),
            )
        elif kind == 4:
            pacer.forget(pane)
        elif kind == 5:
            pacer.clear_deferred()
        elif kind == 6:
            pacer.clear()
        else:
            raise Error("unknown history pacer operation")
        for query in panes:
            append_uint(
                output, UInt64(1 if query in pacer.pending_requests else 0)
            )
            if query in pacer.pending_requests:
                optional(output, pacer.pending_requests[query].prefetch)
            var deferred: Optional[UInt64] = None
            if query in pacer.deferred_requests:
                deferred = pacer.deferred_requests[query]
            put_optional_u64(output, deferred)


struct Planes(Movable):
    var style_keys: Dict[UInt64, Int]
    var styles: List[ArcPointer[HistoryStyles]]
    var grapheme_keys: Dict[UInt64, Int]
    var graphemes: List[ArcPointer[HistoryGraphemes]]

    def __init__(out self):
        self.style_keys = Dict[UInt64, Int]()
        self.styles = List[ArcPointer[HistoryStyles]]()
        self.grapheme_keys = Dict[UInt64, Int]()
        self.graphemes = List[ArcPointer[HistoryGraphemes]]()

    def dictionary(
        mut self, dictionary: HistoryDictionary
    ) raises -> Tuple[Int, Int]:
        var style = dictionary.styles[].id
        if style not in self.style_keys:
            self.style_keys[style] = len(self.styles)
            self.styles.append(dictionary.styles.copy())
        var grapheme = dictionary.graphemes[].id
        if grapheme not in self.grapheme_keys:
            self.grapheme_keys[grapheme] = len(self.graphemes)
            self.graphemes.append(dictionary.graphemes.copy())
        return (self.style_keys[style], self.grapheme_keys[grapheme])

    def write(self, mut output: List[UInt8]):
        append_uint(output, UInt64(len(self.styles)))
        for plane in self.styles:
            append_uint(output, UInt64(len(plane[].values)))
            for style in plane[].values:
                append_uint(output, UInt64(style.foreground))
                append_uint(output, UInt64(style.background))
                append_uint(output, UInt64(style.underline_color))
                append_uint(output, UInt64(style.attributes))
                append_uint(output, UInt64(style.underline))
                append_uint(output, UInt64(style.background_index))
        append_uint(output, UInt64(len(self.graphemes)))
        for plane in self.graphemes:
            append_uint(output, UInt64(len(plane[].offsets)))
            for offset in plane[].offsets:
                append_uint(output, UInt64(offset))
            append_uint(output, UInt64(len(plane[].bytes)))
            output.extend(Span(plane[].bytes))


def packed(mut output: List[UInt8], cells: List[TerminalCell]):
    append_uint(output, UInt64(len(cells)))
    for cell in cells:
        append_uint(output, UInt64(cell.glyph))
        append_uint(output, UInt64(cell.style))
        append_uint(output, UInt64(cell.flags))


def put_ring(
    mut output: List[UInt8], ring: HistoryRing, mut planes: Planes
) raises:
    append_uint(output, UInt64(ring.count))
    for index in range(ring.count):
        ref row = ring.row(index)
        var dictionary = planes.dictionary(row.dictionary[])
        append_uint(output, UInt64(dictionary[0]))
        append_uint(output, UInt64(dictionary[1]))
        append_uint(output, row.revision)
        packed(output, row.cells[].values)


def fingerprint(
    mut output: List[UInt8],
    view: TerminalViewport,
    state: TerminalHistoryState,
    counter: UInt64,
) raises:
    var values: List[UInt64] = [
        view.generation,
        view.view_generation,
        UInt64(view.dictionary_generation),
        UInt64(view.columns),
        UInt64(view.rows),
        UInt64(view.metadata.scrollbar.total),
        UInt64(view.metadata.scrollbar.offset),
        UInt64(view.metadata.scrollbar.length),
        UInt64(state.history_scrollbar.total),
        UInt64(state.history_scrollbar.offset),
        UInt64(state.history_scrollbar.length),
        state.history_mutations,
        state.history_invalidations,
        state.row_revision_epoch,
        state.copy_generation,
        counter,
    ]
    for value in values:
        append_uint(output, value)
    append_uint(output, UInt64(len(state.row_revisions)))
    for revision in state.row_revisions:
        append_uint(output, revision)
    append_uint(output, UInt64(state.history.limit))
    append_uint(output, UInt64(state.below.limit))
    packed(output, view.cells)
    var planes = Planes()
    var baseline = planes.dictionary(state.dictionary[])
    assert_equal(baseline[0], 0)
    assert_equal(baseline[1], 0)
    put_ring(output, state.history, planes)
    put_ring(output, state.below, planes)
    planes.write(output)


def retained_trace(mut reader: Decoder, mut output: List[UInt8]) raises:
    var counter = reader.uint()
    var limit = Int(reader.uint())
    var initial = decode_terminal_frame(reader.bytes())
    var viewport = initial.viewport.take()
    var state = TerminalHistoryState(viewport, counter, limit)
    fingerprint(output, viewport, state, counter)
    var count = Int(reader.uint())
    for _ in range(count):
        var kind = reader.uint()
        var update = decode_terminal_frame(reader.bytes())
        var accepted = True
        if kind == 0:
            var patch = update.patch.take()
            var mutation = reader.uint()
            if mutation == 1:
                patch.base_generation ^= UInt64(1) << 40
            elif mutation == 2:
                patch.base_view_generation ^= UInt64(1) << 40
            elif mutation == 3:
                patch.dictionary_generation += 1
            elif mutation == 4:
                patch.scroll = Int(patch.rows)
            try:
                state.apply_patch(viewport, patch, counter)
            except:
                accepted = False
        elif kind == 1:
            var next = update.viewport.take()
            state.replace(next, counter)
            viewport = next^
        elif kind == 2:
            accepted = state.apply_history(
                viewport, update.history.take(), counter
            )
        else:
            raise Error("unknown retained history operation")
        append_uint(output, UInt64(accepted))
        fingerprint(output, viewport, state, counter)


def run(kind: UInt8, var bytes: List[UInt8]) raises -> List[UInt8]:
    if kind == 9:
        var budget = decode_history_budget(bytes^)
        var result = List[UInt8]()
        append_uint(result, budget.or_else(UInt64(2000)))
        return result^
    var reader = Decoder(bytes^)
    reader.offset = 0
    var output = List[UInt8]()
    if kind == 0:
        var target = UInt32(reader.uint())
        var sub_row = float_value(reader)
        var delta = float_value(reader)
        var height = float_value(reader)
        var oldest = UInt32(reader.uint())
        var newest = UInt32(reader.uint())
        var result = pixel_scroll_position(
            target, sub_row, delta, height, oldest, newest
        )
        append_uint(output, UInt64(result.target))
        put_float(output, result.sub_row)
    elif kind == 1:
        var target = UInt32(reader.uint())
        var reference = UInt32(reader.uint())
        var rows = UInt16(reader.uint())
        var maximum = UInt32(reader.uint())
        var toward_newer = reader.uint() != 0
        optional(
            output,
            daemon_follow_offset(
                target, reference, rows, maximum, toward_newer
            ),
        )
    elif kind == 2:
        var fraction = UInt32(reader.uint())
        var maximum = UInt32(reader.uint())
        append_uint(output, UInt64(scroll_fraction_offset(fraction, maximum)))
    elif kind == 3:
        var target = UInt32(reader.uint())
        append_uint(
            output, UInt64(local_scroll_needs_prefetch(target, context(reader)))
        )
    elif kind == 4:
        local_trace(reader, output)
    elif kind == 5:
        pacer_trace(reader, output)
    elif kind == 6:
        retained_trace(reader, output)
    elif kind == 7:
        var pane = reader.uint()
        var start = UInt32(reader.uint())
        var count = UInt32(reader.uint())
        output = history_request(pane, start, count)
    elif kind == 8:
        var retained = fetch_context(reader)
        var budget = reader.uint()
        put_range(
            output,
            history_request_range(retained, budget, read_optional_u32(reader)),
        )
    else:
        raise Error("unknown terminal history vector kind")
    reader.finish()
    return output^


def main() raises:
    var args = argv()
    var directory = args[1]
    var kinds = read_bytes(directory + "/kinds.bin")
    for index in range(len(kinds)):
        var suffix = String(index) + ".bin"
        var result = run(
            kinds[index], read_bytes(directory + "/request-" + suffix)
        )
        with open(directory + "/actual-" + suffix, "w") as target:
            target.write_all(Span(result))
