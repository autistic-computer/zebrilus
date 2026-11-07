from std.testing import assert_equal, assert_true, assert_false, assert_raises
from zzmojo.terminal_model import (
    TerminalViewport,
    TerminalHistory,
    TerminalPatch,
    TerminalScrollbar,
    TerminalStyle,
    TerminalCell,
    TerminalSpan,
    ROWS,
    SCROLL,
    SCROLLBAR,
)
from zzmojo.terminal_history import TerminalHistoryState
from zzmojo.terminal_history_cache import TerminalHistoryCache
from zzmojo.history_pacer import (
    history_request_range,
    HistoryRequestContext,
    HistoryRange,
    bounded_history_range,
)
from zzmojo.local_scroll import LocalScrollState


def viewport() -> TerminalViewport:
    var result = TerminalViewport()
    result.columns = 1
    result.rows = 3
    result.generation = 1
    result.view_generation = 1
    result.dictionary_generation = 1
    result.styles.append(TerminalStyle(0x112233, 0, 0, 0, 0, 0))
    result.cells.extend(
        Span[TerminalCell](
            [
                TerminalCell(65, 0, 0),
                TerminalCell(66, 0, 0),
                TerminalCell(67, 0, 0),
            ]
        )
    )
    result.metadata.scrollbar = TerminalScrollbar(100, 10, 3)
    return result^


def history() -> TerminalHistory:
    var result = TerminalHistory()
    result.start = 4
    result.total = 100
    result.offset = 777
    result.columns = 1
    result.rows = 6
    result.styles.append(TerminalStyle(0x445566, 0, 0, 0, 0, 0))
    for glyph in range(40, 46):
        result.cells.append(TerminalCell(UInt32(glyph), 0, 0))
    return result^


def patch(view: TerminalViewport) -> TerminalPatch:
    var result = TerminalPatch()
    result.base_generation = view.generation
    result.base_view_generation = view.view_generation
    result.generation = view.generation + 1
    result.view_generation = view.view_generation + 1
    result.dictionary_generation = view.dictionary_generation
    result.columns = view.columns
    result.rows = view.rows
    result.style_base = UInt32(len(view.styles))
    result.grapheme_base = UInt32(len(view.grapheme_offsets) - 1)
    return result^


def budget_and_frozen_rows() raises:
    var view = viewport()
    var counter: UInt64 = 1
    var retained = TerminalHistoryState(view, counter, 10_000, 40)
    assert_true(retained.apply_history(view, history(), counter))
    assert_equal(retained.history.count, 2)
    assert_equal(retained.history.row(0).cells[].values[0].glyph, UInt32(44))
    assert_equal(retained.retained_bytes(), 36)
    assert_equal(retained.history.limit, 2)
    assert_equal(retained.history_invalidations, UInt64(1))
    assert_false(
        Bool(
            history_request_range(
                HistoryRequestContext.from_state(retained, view), 10_000, None
            )
        )
    )
    var frozen = retained.snapshot(view)
    var local = LocalScrollState()
    _ = local.scroll_to(9, retained.context(view), 0)
    retained.trim(0, counter)
    assert_equal(retained.history.count, 0)
    assert_equal(retained.history.limit, 0)
    var observed = local.observe(retained.context(view), False, 1)
    assert_equal(observed.restore.value(), UInt32(9))
    assert_true(observed.changed)
    assert_equal(frozen[].history[0].cells[].values[0].glyph, UInt32(44))
    assert_equal(
        frozen[].history[0].dictionary[].styles[].values[0].foreground,
        UInt32(0x445566),
    )


def stale_preparation() raises:
    var view = viewport()
    var counter: UInt64 = 1
    var retained = TerminalHistoryState(view, counter)
    var change = patch(view)
    change.scroll = -1
    change.fields = ROWS | SCROLL | SCROLLBAR
    change.metadata.scrollbar = TerminalScrollbar(100, 11, 3)
    change.spans.append(TerminalSpan(2, 0, 1, True))
    change.cells.append(TerminalCell(68, 0, 0))
    var prepared = retained.prepare_patch(view, change, counter)
    assert_equal(counter, UInt64(6))
    retained.replace(view, counter)
    view.apply(change)
    var before = counter
    with assert_raises():
        retained.commit_patch(view, change, prepared^, counter)
    assert_equal(counter, before)
    assert_equal(retained.history.count, 0)
    assert_equal(retained.history_scrollbar.offset, UInt32(10))
    retained.replace(view, counter)
    assert_equal(retained.history_scrollbar.offset, UInt32(11))


def frozen_compaction() raises:
    var view = viewport()
    for index in range(1, 81):
        view.styles.append(TerminalStyle(UInt32(index), 0, 0, 0, 0, 0))
    for index in range(len(view.cells)):
        view.cells[index].style = 79
    var counter: UInt64 = 1
    var retained = TerminalHistoryState(view, counter)
    var change = patch(view)
    change.scroll = -1
    change.fields = ROWS | SCROLL | SCROLLBAR
    change.metadata.scrollbar = TerminalScrollbar(100, 11, 3)
    change.spans.append(TerminalSpan(2, 0, 1, True))
    change.cells.append(TerminalCell(68, 79, 0))
    retained.apply_patch(view, change, counter)
    var old = retained.snapshot(view)
    change = patch(view)
    change.styles.append(TerminalStyle(81, 0, 0, 0, 0, 0))
    retained.apply_patch(view, change, counter)
    assert_equal(retained.history.row(0).cells[].values[0].style, UInt16(0))
    assert_equal(len(retained.history.row(0).dictionary[].styles[].values), 1)
    assert_equal(old[].history[0].cells[].values[0].style, UInt16(79))
    assert_equal(len(old[].history[0].dictionary[].styles[].values), 81)
    assert_equal(
        retained.history.row(0).dictionary[].styles[].values[0].foreground,
        old[].history[0].dictionary[].styles[].values[79].foreground,
    )


def across_panes() raises:
    var view = viewport()
    var cache = TerminalHistoryCache(6000)
    assert_true(cache.ensure(1, view))
    assert_true(
        cache.states[1].apply_history(view, history(), cache.next_row_revision)
    )
    var old = cache.states[1].snapshot(view)
    var local = LocalScrollState()
    _ = local.scroll_to(9, cache.context(1, view), 0)
    _ = cache.pacer.request(1, None, 2000, cache.request_context(1, view), 0)
    assert_true(cache.ensure(2, view))
    assert_true(
        cache.states[2].apply_history(view, history(), cache.next_row_revision)
    )
    cache.enforce()
    _ = cache.ensure(3, view)
    assert_true(cache.owned_bytes() <= cache.byte_budget)
    assert_false(1 in cache.pacer.pending_requests)
    assert_true(cache.context(1, view).invalidations > 0)
    var observed = local.observe(cache.context(1, view), False, 1)
    assert_equal(observed.restore.value(), UInt32(9))
    assert_equal(old[].history[0].cells[].values[0].glyph, UInt32(40))


def main() raises:
    var requested = bounded_history_range(HistoryRange(1000, 512), 65535)
    assert_equal(requested.start, UInt32(1384))
    assert_equal(requested.count, UInt32(128))
    budget_and_frozen_rows()
    stale_preparation()
    frozen_compaction()
    across_panes()
    print(
        "terminal history budget, immutable snapshots, stale plans and"
        " cross-pane eviction passed"
    )
