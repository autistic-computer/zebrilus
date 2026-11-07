from std.testing import assert_true
from std.time import perf_counter_ns
from zzmojo.terminal_model import (
    TerminalViewport,
    TerminalHistory,
    TerminalScrollbar,
    TerminalCell,
    TerminalStyle,
)
from zzmojo.terminal_history_cache import TerminalHistoryCache


def main() raises:
    var cache = TerminalHistoryCache()
    var started = perf_counter_ns()
    var dimensions: List[SIMD[.uint16, 2]] = [
        SIMD[.uint16, 2](80, 24),
        SIMD[.uint16, 2](200, 80),
        SIMD[.uint16, 2](400, 120),
        SIMD[.uint16, 2](400, 120),
    ]
    for pane in range(len(dimensions)):
        var columns = dimensions[pane][0]
        var rows = dimensions[pane][1]
        var viewport = TerminalViewport()
        viewport.columns = columns
        viewport.rows = rows
        viewport.generation = 1
        viewport.view_generation = 1
        viewport.metadata.scrollbar = TerminalScrollbar(
            20_000, 10_000, UInt32(rows)
        )
        viewport.styles.append(TerminalStyle(0, 0, 0, 0, 0, 0))
        viewport.cells = List[TerminalCell](
            length=Int(columns) * Int(rows), fill=TerminalCell(65, 0, 0)
        )
        assert_true(cache.ensure(UInt64(pane), viewport))
        var remaining = 10_000
        while remaining > 0 and UInt64(pane) in cache.states:
            var count = min(512, remaining)
            var chunk = TerminalHistory()
            chunk.columns = columns
            chunk.rows = UInt16(count)
            chunk.total = 20_000
            chunk.start = UInt32(remaining - count)
            chunk.styles.append(TerminalStyle(1, 2, 3, 0, 0, 0))
            chunk.cells = List[TerminalCell](
                length=count * Int(columns), fill=TerminalCell(66, 0, 0)
            )
            assert_true(
                cache.states[UInt64(pane)].apply_history(
                    viewport, chunk^, cache.next_row_revision
                )
            )
            remaining -= count
            cache.enforce()
        assert_true(cache.owned_bytes() <= cache.byte_budget)
        print(
            "pane",
            pane,
            columns,
            rows,
            "global owned bytes",
            cache.owned_bytes(),
            "cached panes",
            len(cache.states),
        )
    print("bounded cache profile ns", perf_counter_ns() - started)
