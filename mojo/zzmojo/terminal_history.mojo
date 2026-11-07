from std.memory import ArcPointer
from std.sys import size_of
from .terminal_model import (
    TerminalCell,
    TerminalStyle,
    TerminalViewport,
    TerminalPatch,
    TerminalHistory,
    TerminalScrollbar,
    SCROLLBAR,
    GRAPHEME,
    valid_scalar,
)
from .local_scroll import ScrollContext, _add_u32, _sub_u32

comptime MAX_HISTORY_ROWS = 10_000
comptime MAX_BELOW_ROWS = 512
comptime MAX_HISTORY_CHUNK_ROWS = 512
comptime HISTORY_BYTE_BUDGET = 64 * 1024 * 1024
comptime MIN_DICTIONARY_COMPACTION_BYTES = 1024


def allocate_row_revision(mut counter: UInt64) -> UInt64:
    var result = counter
    counter += 1
    counter = max(counter, UInt64(1))
    return result


@fieldwise_init
struct HistoryStyles(Movable):
    var id: UInt64
    var values: List[TerminalStyle]


@fieldwise_init
struct HistoryGraphemes(Movable):
    var id: UInt64
    var offsets: List[UInt32]
    var bytes: List[UInt8]


@fieldwise_init
struct HistoryDictionary(Movable):
    var id: UInt64
    var styles: ArcPointer[HistoryStyles]
    var graphemes: ArcPointer[HistoryGraphemes]


@fieldwise_init
struct HistoryCells(Movable):
    var values: List[TerminalCell]


struct HistoryRow(Copyable, Movable):
    var cells: ArcPointer[HistoryCells]
    var dictionary: ArcPointer[HistoryDictionary]
    var revision: UInt64

    def __init__(
        out self,
        var cells: List[TerminalCell],
        var dictionary: ArcPointer[HistoryDictionary],
        revision: UInt64,
    ):
        self.cells = ArcPointer(HistoryCells(cells^))
        self.dictionary = dictionary^
        self.revision = revision


struct HistoryRing(Movable):
    var slots: List[Optional[HistoryRow]]
    var head: Int
    var count: Int
    var limit: Int

    def __init__(out self, limit: Int = MAX_HISTORY_ROWS):
        self.slots = List[Optional[HistoryRow]]()
        self.head = 0
        self.count = 0
        self.limit = min(max(limit, 0), MAX_HISTORY_ROWS)

    def row(ref self, index: Int) raises -> ref[self] HistoryRow:
        if index < 0 or index >= self.count:
            raise Error("terminal history row is outside the ring")
        return (
            Pointer(
                to=self.slots[(self.head + index) % len(self.slots)].value()
            )
            .unsafe_mut_cast[origin_of(self).mut]()
            .unsafe_origin_cast[origin_of(self)]()[]
        )

    def _grow(mut self):
        if self.count < len(self.slots):
            return
        var slots = List[Optional[HistoryRow]](
            length=max(4, len(self.slots) * 2), fill=None
        )
        for index in range(self.count):
            swap(
                self.slots[(self.head + index) % len(self.slots)], slots[index]
            )
        self.slots = slots^
        self.head = 0

    def push_back(mut self, var row: HistoryRow):
        self._grow()
        self.slots[(self.head + self.count) % len(self.slots)] = row^
        self.count += 1

    def push_front(mut self, var row: HistoryRow):
        self._grow()
        self.head = (self.head + len(self.slots) - 1) % len(self.slots)
        self.slots[self.head] = row^
        self.count += 1

    def pop_front(mut self) -> Optional[HistoryRow]:
        if self.count == 0:
            return None
        var result: Optional[HistoryRow] = None
        swap(result, self.slots[self.head])
        self.head = (self.head + 1) % len(self.slots)
        self.count -= 1
        return result^

    def pop_back(mut self) -> Optional[HistoryRow]:
        if self.count == 0:
            return None
        var result: Optional[HistoryRow] = None
        swap(result, self.slots[(self.head + self.count - 1) % len(self.slots)])
        self.count -= 1
        return result^

    def enforce_cap(mut self):
        while self.count > self.limit:
            _ = self.pop_front()

    def truncate(mut self, count: Int):
        while self.count > count:
            _ = self.pop_back()

    def shrink_storage(mut self):
        if len(self.slots) <= max(4, self.count * 2):
            return
        var slots = List[Optional[HistoryRow]](length=self.count, fill=None)
        for index in range(self.count):
            swap(
                self.slots[(self.head + index) % len(self.slots)], slots[index]
            )
        self.slots = slots^
        self.head = 0

    def clear(mut self):
        for index in range(self.count):
            self.slots[(self.head + index) % len(self.slots)] = None
        self.head = 0
        self.count = 0


def _style_equal(left: TerminalStyle, right: TerminalStyle) -> Bool:
    return (
        left.foreground == right.foreground
        and left.background == right.background
        and left.underline_color == right.underline_color
        and left.attributes == right.attributes
        and left.underline == right.underline
        and left.background_index == right.background_index
    )


def _glyph_equal(
    left: TerminalCell,
    dictionary: HistoryDictionary,
    right: TerminalCell,
    other: HistoryDictionary,
) -> Bool:
    if left.glyph & GRAPHEME == 0 or right.glyph & GRAPHEME == 0:
        return left.glyph == right.glyph
    var left_index = Int(left.glyph & ~GRAPHEME)
    var right_index = Int(right.glyph & ~GRAPHEME)
    var left_start = 0
    var left_end = 0
    var right_start = 0
    var right_end = 0
    ref left_plane = dictionary.graphemes[]
    ref right_plane = other.graphemes[]
    if left_index + 1 < len(left_plane.offsets):
        var start = Int(left_plane.offsets[left_index])
        var end = Int(left_plane.offsets[left_index + 1])
        if start <= end and end <= len(left_plane.bytes):
            left_start = start
            left_end = end
    if right_index + 1 < len(right_plane.offsets):
        var start = Int(right_plane.offsets[right_index])
        var end = Int(right_plane.offsets[right_index + 1])
        if start <= end and end <= len(right_plane.bytes):
            right_start = start
            right_end = end
    if left_end - left_start != right_end - right_start:
        return False
    for index in range(left_end - left_start):
        if (
            left_plane.bytes[left_start + index]
            != right_plane.bytes[right_start + index]
        ):
            return False
    return True


def rows_match(
    cells: Span[TerminalCell, _],
    dictionary: HistoryDictionary,
    other_cells: Span[TerminalCell, _],
    other: HistoryDictionary,
) -> Bool:
    if len(cells) != len(other_cells):
        return False
    for index in range(len(cells)):
        var left = cells[index]
        var right = other_cells[index]
        if left.flags != right.flags:
            return False
        var left_style = Int(left.style)
        var right_style = Int(right.style)
        var left_valid = left_style < len(dictionary.styles[].values)
        var right_valid = right_style < len(other.styles[].values)
        if left_valid != right_valid:
            return False
        if left_valid and not _style_equal(
            dictionary.styles[].values[left_style],
            other.styles[].values[right_style],
        ):
            return False
        if not _glyph_equal(left, dictionary, right, other):
            return False
    return True


def _capture_row(
    viewport: TerminalViewport,
    row: Int,
    dictionary: ArcPointer[HistoryDictionary],
    mut counter: UInt64,
) -> Optional[HistoryRow]:
    if row < 0 or row >= Int(viewport.rows):
        return None
    var start = row * Int(viewport.columns)
    var end = start + Int(viewport.columns)
    if end > len(viewport.cells):
        return None
    return HistoryRow(
        List[TerminalCell](Span(viewport.cells)[start:end]),
        dictionary.copy(),
        allocate_row_revision(counter),
    )


@fieldwise_init
struct HistorySnapshot(Movable):
    var generation: UInt64
    var view_generation: UInt64
    var columns: UInt16
    var rows: UInt16
    var scrollbar: TerminalScrollbar
    var history_mutations: UInt64
    var history_invalidations: UInt64
    var row_revision_epoch: UInt64
    var row_revisions: List[UInt64]
    var history: List[HistoryRow]
    var below: List[HistoryRow]


struct PreparedHistoryPatch(Movable):
    var previous: TerminalScrollbar
    var following: TerminalScrollbar
    var scroll: Int
    var invalidate: Bool
    var coordinates_kept: Bool
    var departing: HistoryRing
    var leaving_requested: Bool
    var leaving_valid: Bool
    var leaving: List[HistoryRow]
    var jumped: Bool
    var old_cells: List[TerminalCell]
    var outgoing: ArcPointer[HistoryDictionary]
    var changed_rows: List[UInt16]
    var state_revision: UInt64
    var generation: UInt64
    var view_generation: UInt64
    var columns: UInt16
    var rows: UInt16

    def __init__(
        out self,
        previous: TerminalScrollbar,
        following: TerminalScrollbar,
        scroll: Int,
        var outgoing: ArcPointer[HistoryDictionary],
    ):
        self.previous = previous
        self.following = following
        self.scroll = scroll
        self.invalidate = False
        self.coordinates_kept = False
        self.departing = HistoryRing(MAX_HISTORY_ROWS)
        self.leaving_requested = False
        self.leaving_valid = False
        self.leaving = List[HistoryRow]()
        self.jumped = False
        self.old_cells = List[TerminalCell]()
        self.outgoing = outgoing^
        self.changed_rows = List[UInt16]()
        self.state_revision = 0
        self.generation = 0
        self.view_generation = 0
        self.columns = 0
        self.rows = 0


struct TerminalHistoryState(Movable):
    var history: HistoryRing
    var below: HistoryRing
    var history_scrollbar: TerminalScrollbar
    var history_mutations: UInt64
    var history_invalidations: UInt64
    var row_revisions: List[UInt64]
    var row_revision_epoch: UInt64
    var copy_generation: UInt64
    var dictionary: ArcPointer[HistoryDictionary]
    var next_plane: UInt64
    var next_dictionary: UInt64
    var byte_budget: Int
    var configured_history_limit: Int
    var budget_evictions: UInt64
    var state_revision: UInt64
    var frozen: Optional[ArcPointer[HistorySnapshot]]

    def __init__(
        out self,
        viewport: TerminalViewport,
        mut counter: UInt64,
        history_limit: Int = MAX_HISTORY_ROWS,
        byte_budget: Int = HISTORY_BYTE_BUDGET,
    ):
        self.history = HistoryRing(history_limit)
        self.below = HistoryRing(MAX_BELOW_ROWS)
        self.history_scrollbar = viewport.metadata.scrollbar
        self.history_mutations = 0
        self.history_invalidations = 0
        self.row_revisions = List[UInt64](capacity=Int(viewport.rows))
        for _ in range(Int(viewport.rows)):
            self.row_revisions.append(allocate_row_revision(counter))
        self.row_revision_epoch = allocate_row_revision(counter)
        self.copy_generation = 0
        self.dictionary = ArcPointer(
            HistoryDictionary(
                0,
                ArcPointer(HistoryStyles(0, viewport.styles.copy())),
                ArcPointer(
                    HistoryGraphemes(
                        1,
                        viewport.grapheme_offsets.copy(),
                        viewport.grapheme_bytes.copy(),
                    )
                ),
            )
        )
        self.next_plane = 2
        self.next_dictionary = 1
        self.byte_budget = min(max(byte_budget, 0), HISTORY_BYTE_BUDGET)
        self.configured_history_limit = self.history.limit
        self.budget_evictions = 0
        self.state_revision = 0
        self.frozen = None

    def context(self, viewport: TerminalViewport) -> ScrollContext:
        return ScrollContext.from_viewport(
            viewport,
            UInt64(self.history.count),
            UInt64(self.below.count),
            self.history_invalidations,
        )

    def _new_dictionary(
        mut self,
        var styles: ArcPointer[HistoryStyles],
        var graphemes: ArcPointer[HistoryGraphemes],
    ) -> ArcPointer[HistoryDictionary]:
        var result = ArcPointer(
            HistoryDictionary(self.next_dictionary, styles^, graphemes^)
        )
        self.next_dictionary += 1
        return result^

    def _replace_dictionary(mut self, viewport: TerminalViewport):
        var styles = ArcPointer(
            HistoryStyles(self.next_plane, viewport.styles.copy())
        )
        self.next_plane += 1
        var graphemes = ArcPointer(
            HistoryGraphemes(
                self.next_plane,
                viewport.grapheme_offsets.copy(),
                viewport.grapheme_bytes.copy(),
            )
        )
        self.next_plane += 1
        self.dictionary = self._new_dictionary(styles^, graphemes^)

    def _append_dictionary(
        mut self, viewport: TerminalViewport, patch: TerminalPatch
    ):
        if (
            len(patch.styles) == 0
            and len(patch.grapheme_lengths) == 0
            and len(patch.grapheme_bytes) == 0
        ):
            return
        var styles = self.dictionary[].styles.copy()
        var graphemes = self.dictionary[].graphemes.copy()
        if len(patch.styles) > 0:
            styles = ArcPointer(
                HistoryStyles(self.next_plane, viewport.styles.copy())
            )
            self.next_plane += 1
        if len(patch.grapheme_lengths) > 0 or len(patch.grapheme_bytes) > 0:
            graphemes = ArcPointer(
                HistoryGraphemes(
                    self.next_plane,
                    viewport.grapheme_offsets.copy(),
                    viewport.grapheme_bytes.copy(),
                )
            )
            self.next_plane += 1
        self.dictionary = self._new_dictionary(styles^, graphemes^)

    def _drop_history(mut self):
        self.history.clear()
        self.history_mutations += 1
        self.history_invalidations += 1

    def _rows_leaving_below(
        self, viewport: TerminalViewport, delta: UInt32, mut counter: UInt64
    ) raises -> Optional[List[HistoryRow]]:
        var rows = Int(viewport.rows)
        var gap = max(Int(delta) - rows, 0)
        if gap > MAX_BELOW_ROWS or gap > self.history.count:
            return None
        var leaving = List[HistoryRow]()
        for index in range(self.history.count - gap, self.history.count):
            leaving.append(self.history.row(index).copy())
        for row in range(max(rows - Int(delta), 0), rows):
            var captured = _capture_row(viewport, row, self.dictionary, counter)
            if not captured:
                return None
            leaving.append(captured.take())
        return leaving^

    def prepare_patch(
        self,
        viewport: TerminalViewport,
        patch: TerminalPatch,
        mut counter: UInt64,
    ) raises -> PreparedHistoryPatch:
        var previous = self.history_scrollbar
        var following = (
            patch.metadata.scrollbar if patch.fields & SCROLLBAR
            != 0 else viewport.metadata.scrollbar
        )
        var result = PreparedHistoryPatch(
            previous, following, patch.scroll, self.dictionary.copy()
        )
        result.state_revision = self.state_revision
        result.generation = patch.generation
        result.view_generation = patch.view_generation
        result.columns = patch.columns
        result.rows = patch.rows
        var rows = Int(patch.rows)
        var shift = abs(patch.scroll)
        var total_valid = following.total >= previous.total
        var total_delta = _sub_u32(following.total, previous.total)
        var forward_valid = following.offset >= previous.offset
        var forward = _sub_u32(following.offset, previous.offset)
        var reverse_valid = previous.offset >= following.offset
        var reverse = _sub_u32(previous.offset, following.offset)
        var full = (
            patch.scroll == 0
            and rows != 0
            and len(patch.spans) == rows
            and patch.generation != patch.base_generation
        )
        var resized = patch.columns != viewport.columns
        result.invalidate = resized or not total_valid or full
        var distance = abs(Int64(following.offset) - Int64(previous.offset))
        result.coordinates_kept = (
            total_valid and total_delta == 0 and distance == Int64(shift)
        )
        result.jumped = (
            patch.scroll == 0
            and not resized
            and total_valid
            and total_delta == 0
            and previous.offset != following.offset
            and len(patch.spans) > 0
        )
        if result.jumped:
            result.old_cells = viewport.cells.copy()
        if not result.invalidate:
            if patch.scroll < 0:
                result.invalidate = (
                    not total_valid
                    or UInt64(total_delta) > UInt64(shift)
                    or not forward_valid
                    or UInt64(forward) > UInt64(shift)
                )
                if not result.invalidate:
                    for row in range(max(shift - self.history.limit, 0), shift):
                        var captured = _capture_row(
                            viewport, row, self.dictionary, counter
                        )
                        if not captured:
                            result.invalidate = True
                            result.departing.clear()
                            break
                        result.departing.push_back(captured.take())
            elif patch.scroll > 0:
                result.invalidate = (
                    not total_valid
                    or (
                        total_delta != 0
                        and (
                            not reverse_valid
                            or UInt64(reverse) != UInt64(shift)
                        )
                    )
                    or not reverse_valid
                    or UInt64(reverse) > UInt64(shift)
                    or self.history.count < shift
                )
            else:
                result.invalidate = previous.offset != following.offset
        result.leaving_requested = (
            reverse_valid
            and reverse > 0
            and not resized
            and ((total_valid and total_delta == 0) or not result.invalidate)
        )
        if result.leaving_requested:
            var leaving = self._rows_leaving_below(viewport, reverse, counter)
            if leaving:
                result.leaving_valid = True
                result.leaving = leaving.take()
        for span in patch.spans:
            result.changed_rows.append(span.row)
        return result^

    def _reanchor(
        mut self,
        viewport: TerminalViewport,
        prepared: PreparedHistoryPatch,
        mut counter: UInt64,
    ) raises -> Bool:
        var history = self.history.count
        var rows = Int(viewport.rows)
        var combined = UInt64(prepared.following.offset) + UInt64(history)
        if combined > UInt64(UInt32.MAX) or combined < UInt64(
            prepared.previous.offset
        ):
            return False
        var start = Int(combined - UInt64(prepared.previous.offset))
        if rows == 0:
            return False
        var columns = Int(viewport.columns)
        for row in range(rows):
            var current_start = row * columns
            var current_end = current_start + columns
            if current_end > len(viewport.cells):
                return False
            var index = start + row
            var matched = False
            if index < history:
                ref cached = self.history.row(index)
                matched = rows_match(
                    Span(viewport.cells)[current_start:current_end],
                    self.dictionary[],
                    Span(cached.cells[].values),
                    cached.dictionary[],
                )
            elif index - history < rows:
                var old_start = (index - history) * columns
                var old_end = old_start + columns
                if old_end <= len(prepared.old_cells):
                    matched = rows_match(
                        Span(viewport.cells)[current_start:current_end],
                        self.dictionary[],
                        Span(prepared.old_cells)[old_start:old_end],
                        prepared.outgoing[],
                    )
            elif index - history - rows < self.below.count:
                ref cached = self.below.row(index - history - rows)
                matched = rows_match(
                    Span(viewport.cells)[current_start:current_end],
                    self.dictionary[],
                    Span(cached.cells[].values),
                    cached.dictionary[],
                )
            if not matched:
                return False
        var next_history = HistoryRing(self.history.limit)
        var next_below = HistoryRing(self.below.limit)
        var held_index = 0
        while self.history.count > 0:
            var item = self.history.pop_front()
            if held_index < start:
                next_history.push_back(item.take())
            elif (
                held_index >= start + rows
                and next_below.count < next_below.limit
            ):
                next_below.push_back(item.take())
            held_index += 1
        for row in range(rows):
            var revision = allocate_row_revision(counter)
            if held_index < start or (
                held_index >= start + rows
                and next_below.count < next_below.limit
            ):
                var cells = List[TerminalCell](
                    Span(prepared.old_cells)[
                        row * columns : (row + 1) * columns
                    ]
                )
                var item = HistoryRow(
                    cells^, prepared.outgoing.copy(), revision
                )
                if held_index < start:
                    next_history.push_back(item^)
                else:
                    next_below.push_back(item^)
            held_index += 1
        while self.below.count > 0:
            var item = self.below.pop_front()
            if held_index < start:
                next_history.push_back(item.take())
            elif (
                held_index >= start + rows
                and next_below.count < next_below.limit
            ):
                next_below.push_back(item.take())
            held_index += 1
        next_history.enforce_cap()
        self.history = next_history^
        self.below = next_below^
        return True

    def commit_patch(
        mut self,
        viewport: TerminalViewport,
        patch: TerminalPatch,
        var prepared: PreparedHistoryPatch,
        mut counter: UInt64,
    ) raises:
        if (
            prepared.state_revision != self.state_revision
            or viewport.generation != prepared.generation
            or viewport.view_generation != prepared.view_generation
            or viewport.columns != prepared.columns
            or viewport.rows != prepared.rows
            or patch.generation != prepared.generation
            or patch.view_generation != prepared.view_generation
        ):
            raise Error(
                "terminal history patch preparation is stale or not committed"
            )
        self.frozen = None
        self._append_dictionary(viewport, patch)
        var reanchored = prepared.jumped and self._reanchor(
            viewport, prepared, counter
        )
        prepared.invalidate = prepared.invalidate and not reanchored
        if not reanchored:
            if prepared.leaving_requested:
                if prepared.leaving_valid:
                    while len(prepared.leaving) > 0:
                        self.below.push_front(prepared.leaving.pop())
                    self.below.truncate(self.below.limit)
                else:
                    self.below.clear()
            elif prepared.invalidate:
                self.below.clear()
            else:
                var forward = _sub_u32(
                    prepared.following.offset, prepared.previous.offset
                )
                if UInt64(forward) >= UInt64(self.below.count):
                    self.below.clear()
                else:
                    for _ in range(Int(forward)):
                        _ = self.below.pop_front()
        if (
            _add_u32(prepared.following.offset, prepared.following.length)
            >= prepared.following.total
        ):
            self.below.clear()
        if prepared.invalidate:
            self._drop_history()
        elif prepared.scroll < 0:
            while prepared.departing.count > 0:
                var departing = prepared.departing.pop_front()
                self.history.push_back(departing.take())
            var advanced = _sub_u32(
                prepared.following.offset, self.history_scrollbar.offset
            )
            var evicted = max(abs(prepared.scroll) - Int(advanced), 0)
            for _ in range(evicted):
                _ = self.history.pop_front()
            self.history.enforce_cap()
        elif prepared.scroll > 0:
            for _ in range(prepared.scroll):
                _ = self.history.pop_back()
        if (
            not prepared.invalidate
            and prepared.scroll != 0
            and not prepared.coordinates_kept
        ):
            self.history_mutations += 1
        if (
            len(patch.styles) > 0
            or len(patch.grapheme_lengths) > 0
            or len(patch.grapheme_bytes) > 0
        ):
            _compact_ring(
                self.history,
                prepared.outgoing,
                self.dictionary,
                self.next_plane,
                self.next_dictionary,
            )
            _compact_ring(
                self.below,
                prepared.outgoing,
                self.dictionary,
                self.next_plane,
                self.next_dictionary,
            )
        self.history_scrollbar = prepared.following
        if prepared.scroll != 0 or len(prepared.changed_rows) > 0:
            var shift = abs(prepared.scroll)
            if prepared.scroll > 0:
                for index in range(len(self.row_revisions) - 1, shift - 1, -1):
                    self.row_revisions[index] = self.row_revisions[
                        index - shift
                    ]
            elif prepared.scroll < 0:
                for index in range(len(self.row_revisions) - shift):
                    self.row_revisions[index] = self.row_revisions[
                        index + shift
                    ]
            for row in prepared.changed_rows:
                self.row_revisions[Int(row)] = allocate_row_revision(counter)
            self.row_revision_epoch = allocate_row_revision(counter)
        self._enforce_byte_budget(counter)
        self.state_revision += 1

    def apply_patch(
        mut self,
        mut viewport: TerminalViewport,
        patch: TerminalPatch,
        mut counter: UInt64,
    ) raises:
        var prepared = self.prepare_patch(viewport, patch, counter)
        viewport.apply(patch)
        self.commit_patch(viewport, patch, prepared^, counter)

    def replace(mut self, viewport: TerminalViewport, mut counter: UInt64):
        self.frozen = None
        self.row_revisions.clear()
        for _ in range(Int(viewport.rows)):
            self.row_revisions.append(allocate_row_revision(counter))
        self.row_revision_epoch = allocate_row_revision(counter)
        self._drop_history()
        self.below.clear()
        self.history_scrollbar = viewport.metadata.scrollbar
        self.history.limit = self.configured_history_limit
        self.below.limit = MAX_BELOW_ROWS
        self._replace_dictionary(viewport)
        self.state_revision += 1

    def apply_history(
        mut self,
        viewport: TerminalViewport,
        var chunk: TerminalHistory,
        mut counter: UInt64,
    ) raises -> Bool:
        var count = Int(chunk.rows)
        if self.history.count > Int(self.history_scrollbar.offset):
            return False
        var front = self.history_scrollbar.offset - UInt32(self.history.count)
        var end = UInt64(chunk.start) + UInt64(count)
        if (
            chunk.columns != viewport.columns
            or chunk.total != self.history_scrollbar.total
            or end > UInt64(UInt32.MAX)
            or UInt32(end) != front
            or not _history_valid(chunk)
        ):
            return False
        self.frozen = None
        var styles = ArcPointer(HistoryStyles(self.next_plane, chunk.styles^))
        chunk.styles = List[TerminalStyle]()
        self.next_plane += 1
        var graphemes = ArcPointer(
            HistoryGraphemes(
                self.next_plane, chunk.grapheme_offsets^, chunk.grapheme_bytes^
            )
        )
        chunk.grapheme_offsets = List[UInt32]()
        chunk.grapheme_bytes = List[UInt8]()
        self.next_plane += 1
        var dictionary = self._new_dictionary(styles^, graphemes^)
        var columns = Int(chunk.columns)
        for row in range(count - 1, -1, -1):
            var cells = List[TerminalCell](
                Span(chunk.cells)[row * columns : (row + 1) * columns]
            )
            self.history.push_front(
                HistoryRow(
                    cells^, dictionary.copy(), allocate_row_revision(counter)
                )
            )
        self.history.enforce_cap()
        self.row_revision_epoch = allocate_row_revision(counter)
        self._enforce_byte_budget(counter)
        self.state_revision += 1
        return True

    def _enforce_byte_budget(mut self, mut counter: UInt64) raises:
        var counts = Dict[UInt64, Int]()
        var sizes = Dict[UInt64, Int]()
        counts[self.dictionary[].styles[].id] = 1
        counts[self.dictionary[].graphemes[].id] = 1
        sizes[self.dictionary[].styles[].id] = 0
        sizes[self.dictionary[].graphemes[].id] = 0
        var total = 0
        _budget_collect(self.history, counts, sizes, total)
        _budget_collect(self.below, counts, sizes, total)
        var history_evicted = False
        var below_evicted = False
        while total > self.byte_budget and self.history.count > 0:
            var row = self.history.pop_front()
            _budget_remove(row.value(), counts, sizes, total)
            history_evicted = True
        while total > self.byte_budget and self.below.count > 0:
            var row = self.below.pop_back()
            _budget_remove(row.value(), counts, sizes, total)
            below_evicted = True
        if history_evicted:
            self.history.limit = min(self.history.limit, self.history.count)
        if below_evicted:
            self.below.limit = min(self.below.limit, self.below.count)
        if history_evicted or below_evicted:
            self.frozen = None
            self.history_mutations += 1
            self.history_invalidations += 1
            self.budget_evictions += 1
            self.row_revision_epoch = allocate_row_revision(counter)

    def retained_bytes(self) raises -> Int:
        var counts = Dict[UInt64, Int]()
        var sizes = Dict[UInt64, Int]()
        counts[self.dictionary[].styles[].id] = 1
        counts[self.dictionary[].graphemes[].id] = 1
        sizes[self.dictionary[].styles[].id] = 0
        sizes[self.dictionary[].graphemes[].id] = 0
        var total = 0
        _budget_collect(self.history, counts, sizes, total)
        _budget_collect(self.below, counts, sizes, total)
        return total

    def trim(mut self, bytes: Int, mut counter: UInt64) raises:
        self.byte_budget = min(max(bytes, 0), HISTORY_BYTE_BUDGET)
        var before = self.budget_evictions
        self._enforce_byte_budget(counter)
        if self.budget_evictions != before:
            self.history.shrink_storage()
            self.below.shrink_storage()
            self.state_revision += 1

    def owned_bytes(self) raises -> Int:
        var payload = self.retained_bytes()
        var dictionary = (
            self.dictionary[].styles[].values.capacity() * 16
            + self.dictionary[].graphemes[].offsets.capacity() * 4
            + self.dictionary[].graphemes[].bytes.capacity()
        )
        var row_count = self.history.count + self.below.count
        var slots = (
            self.history.slots.capacity() + self.below.slots.capacity()
        ) * size_of[Optional[HistoryRow]]()
        var revisions = self.row_revisions.capacity() * 16
        var snapshots = row_count * size_of[HistoryRow]() + 256
        var dictionaries = Dict[UInt64, Bool]()
        var styles = Dict[UInt64, Bool]()
        var graphemes = Dict[UInt64, Bool]()
        dictionaries[self.dictionary[].id] = True
        styles[self.dictionary[].styles[].id] = True
        graphemes[self.dictionary[].graphemes[].id] = True
        _budget_metadata(self.history, dictionaries, styles, graphemes)
        _budget_metadata(self.below, dictionaries, styles, graphemes)
        var allocations = (
            len(dictionaries) * 128
            + len(styles) * 192
            + len(graphemes) * 256
            + row_count * 192
        )
        return (
            payload
            + dictionary
            + slots
            + revisions
            + snapshots
            + allocations
            + 256
        )

    def snapshot(
        mut self, viewport: TerminalViewport
    ) raises -> ArcPointer[HistorySnapshot]:
        if not self.frozen:
            var history = List[HistoryRow](capacity=self.history.count)
            var below = List[HistoryRow](capacity=self.below.count)
            for index in range(self.history.count):
                history.append(self.history.row(index).copy())
            for index in range(self.below.count):
                below.append(self.below.row(index).copy())
            self.frozen = ArcPointer(
                HistorySnapshot(
                    viewport.generation,
                    viewport.view_generation,
                    viewport.columns,
                    viewport.rows,
                    viewport.metadata.scrollbar,
                    self.history_mutations,
                    self.history_invalidations,
                    self.row_revision_epoch,
                    self.row_revisions.copy(),
                    history^,
                    below^,
                )
            )
        return self.frozen.value().copy()


def _history_valid(chunk: TerminalHistory) -> Bool:
    if (
        chunk.rows == 0
        or chunk.rows > MAX_HISTORY_CHUNK_ROWS
        or len(chunk.cells) != Int(chunk.rows) * Int(chunk.columns)
        or len(chunk.grapheme_offsets) == 0
        or chunk.grapheme_offsets[0] != 0
    ):
        return False
    var previous: UInt32 = 0
    for offset in chunk.grapheme_offsets:
        if offset < previous or UInt64(offset) > UInt64(
            len(chunk.grapheme_bytes)
        ):
            return False
        previous = offset
    if UInt64(previous) != UInt64(len(chunk.grapheme_bytes)):
        return False
    for index in range(len(chunk.grapheme_offsets) - 1):
        try:
            _ = String(
                from_utf8=Span(chunk.grapheme_bytes)[
                    Int(chunk.grapheme_offsets[index]) : Int(
                        chunk.grapheme_offsets[index + 1]
                    )
                ]
            )
        except:
            return False
    for cell in chunk.cells:
        if Int(cell.style) >= len(chunk.styles):
            return False
        if cell.glyph == 0:
            continue
        if cell.glyph & GRAPHEME == 0:
            if not valid_scalar(cell.glyph):
                return False
        elif UInt64(cell.glyph & ~GRAPHEME) + 1 >= UInt64(
            len(chunk.grapheme_offsets)
        ):
            return False
    return True


def _compaction_start(
    ring: HistoryRing, dictionary_bytes: Int, plane: UInt64, styles: Bool
) raises -> Int:
    if dictionary_bytes <= MIN_DICTIONARY_COMPACTION_BYTES:
        return ring.count
    var start = ring.count
    var cell_bytes = 0
    for index in range(ring.count - 1, -1, -1):
        ref row = ring.row(index)
        var actual = (
            row.dictionary[]
            .styles[]
            .id if styles else row.dictionary[]
            .graphemes[]
            .id
        )
        if actual != plane:
            break
        cell_bytes += len(row.cells[].values) * 8
        if cell_bytes >= dictionary_bytes:
            return ring.count
        start -= 1
    return start


def _compact_ring(
    mut ring: HistoryRing,
    source: ArcPointer[HistoryDictionary],
    current: ArcPointer[HistoryDictionary],
    mut next_plane: UInt64,
    mut next_dictionary: UInt64,
) raises:
    var style_bytes = (
        0 if source[].styles[].id
        == current[].styles[].id else len(source[].styles[].values) * 16
    )
    var grapheme_bytes = (
        0 if source[].graphemes[].id
        == current[].graphemes[].id else len(source[].graphemes[].offsets) * 4
        + len(source[].graphemes[].bytes)
    )
    var styles_start = _compaction_start(
        ring, style_bytes, source[].styles[].id, True
    )
    var graphemes_start = _compaction_start(
        ring, grapheme_bytes, source[].graphemes[].id, False
    )
    var start = min(styles_start, graphemes_start)
    if start == ring.count:
        return
    var style_ids = Dict[UInt16, UInt16]()
    var styles = List[TerminalStyle]()
    var grapheme_ids = Dict[UInt32, UInt32]()
    var offsets: List[UInt32] = [UInt32(0)]
    var bytes = List[UInt8]()
    for index in range(start, ring.count):
        for cell in ring.row(index).cells[].values:
            if index >= styles_start and cell.style not in style_ids:
                if Int(cell.style) >= len(source[].styles[].values):
                    return
                style_ids[cell.style] = UInt16(len(styles))
                styles.append(source[].styles[].values[Int(cell.style)])
            if (
                index >= graphemes_start
                and cell.glyph & GRAPHEME != 0
                and cell.glyph not in grapheme_ids
            ):
                var glyph_index = Int(cell.glyph & ~GRAPHEME)
                ref old = source[].graphemes[]
                if glyph_index + 1 >= len(old.offsets):
                    return
                var begin = Int(old.offsets[glyph_index])
                var end = Int(old.offsets[glyph_index + 1])
                if begin > end or end > len(old.bytes):
                    return
                grapheme_ids[cell.glyph] = GRAPHEME | UInt32(len(offsets) - 1)
                bytes.extend(Span(old.bytes)[begin:end])
                offsets.append(UInt32(len(bytes)))
    var compact_styles = ArcPointer(HistoryStyles(next_plane, styles^))
    next_plane += 1
    var compact_graphemes = ArcPointer(
        HistoryGraphemes(next_plane, offsets^, bytes^)
    )
    next_plane += 1
    var previous: Optional[UInt64] = None
    var replacement: Optional[ArcPointer[HistoryDictionary]] = None
    for index in range(start, ring.count):
        ref row = ring.row(index)
        var original_id = row.dictionary[].id
        if not previous or previous.value() != original_id:
            previous = original_id
            var selected_styles = (
                compact_styles.copy() if index
                >= styles_start else row.dictionary[].styles.copy()
            )
            var selected_graphemes = (
                compact_graphemes.copy() if index
                >= graphemes_start else row.dictionary[].graphemes.copy()
            )
            replacement = ArcPointer(
                HistoryDictionary(
                    next_dictionary, selected_styles^, selected_graphemes^
                )
            )
            next_dictionary += 1
        var cells = row.cells[].values.copy()
        for cell_index in range(len(cells)):
            var cell = cells[cell_index]
            if index >= graphemes_start and cell.glyph in grapheme_ids:
                cell.glyph = grapheme_ids[cell.glyph]
            if index >= styles_start:
                cell.style = style_ids[cell.style]
            cells[cell_index] = cell
        row.cells = ArcPointer(HistoryCells(cells^))
        row.dictionary = replacement.value().copy()


def _budget_add(
    plane: UInt64,
    bytes: Int,
    mut counts: Dict[UInt64, Int],
    mut sizes: Dict[UInt64, Int],
    mut total: Int,
) raises:
    if plane in counts:
        counts[plane] += 1
    else:
        counts[plane] = 1
        sizes[plane] = bytes
        total += bytes


def _budget_collect(
    ring: HistoryRing,
    mut counts: Dict[UInt64, Int],
    mut sizes: Dict[UInt64, Int],
    mut total: Int,
) raises:
    for index in range(ring.count):
        ref row = ring.row(index)
        total += row.cells[].values.capacity() * 8
        _budget_add(
            row.dictionary[].styles[].id,
            row.dictionary[].styles[].values.capacity() * 16,
            counts,
            sizes,
            total,
        )
        _budget_add(
            row.dictionary[].graphemes[].id,
            row.dictionary[].graphemes[].offsets.capacity() * 4
            + row.dictionary[].graphemes[].bytes.capacity(),
            counts,
            sizes,
            total,
        )


def _budget_remove(
    row: HistoryRow,
    mut counts: Dict[UInt64, Int],
    mut sizes: Dict[UInt64, Int],
    mut total: Int,
) raises:
    total -= row.cells[].values.capacity() * 8
    var planes: List[UInt64] = [
        row.dictionary[].styles[].id,
        row.dictionary[].graphemes[].id,
    ]
    for plane in planes:
        counts[plane] -= 1
        if counts[plane] == 0:
            total -= sizes[plane]
            _ = counts.pop(plane)
            _ = sizes.pop(plane)


def _budget_metadata(
    ring: HistoryRing,
    mut dictionaries: Dict[UInt64, Bool],
    mut styles: Dict[UInt64, Bool],
    mut graphemes: Dict[UInt64, Bool],
) raises:
    for index in range(ring.count):
        ref dictionary = ring.row(index).dictionary[]
        dictionaries[dictionary.id] = True
        styles[dictionary.styles[].id] = True
        graphemes[dictionary.graphemes[].id] = True
