from std.testing import assert_equal, assert_raises
from zzmojo.terminal_model import (
    TerminalCell,
    TerminalStyle,
    TerminalSpan,
    TerminalViewport,
    TerminalPatch,
    PRESENTATION,
    CURSOR_AT,
)
from terminal_codec_support import viewport_fingerprint


def initial() -> TerminalViewport:
    var view = TerminalViewport()
    view.generation = 8
    view.view_generation = 9
    view.dictionary_generation = 7
    view.columns = 3
    view.rows = 2
    view.styles.append(TerminalStyle(0, 0, 0, 0, 0, 0))
    for index in range(6):
        view.cells.append(
            TerminalCell(UInt32(65 + index), 0, UInt16(index + 1))
        )
    return view^


def patch() -> TerminalPatch:
    var change = TerminalPatch()
    change.base_generation = 8
    change.base_view_generation = 9
    change.generation = 10
    change.view_generation = 11
    change.dictionary_generation = 7
    change.columns = 3
    change.rows = 2
    change.scroll = 1
    change.style_base = 1
    change.styles.append(TerminalStyle(1, 2, 3, 0, 0, 0))
    change.grapheme_base = 0
    change.grapheme_lengths.append(2)
    change.grapheme_bytes.extend(Span[UInt8]([0xC3, 0xA9]))
    change.spans.append(TerminalSpan(0, 0, 1, True))
    change.cells.append(TerminalCell(90, 1, 8))
    change.fields = PRESENTATION
    change.metadata.title = "next"
    return change^


def result_cells() -> List[TerminalCell]:
    var cells: List[TerminalCell] = [
        TerminalCell(90, 1, 8),
        TerminalCell(0, 0, 0),
        TerminalCell(0, 0, 0),
        TerminalCell(65, 0, 1),
        TerminalCell(66, 0, 2),
        TerminalCell(67, 0, 3),
    ]
    return cells^


def main() raises:
    var view = initial()
    var change = patch()
    var before = viewport_fingerprint(view)
    var metadata = view.validate_patch(change)
    assert_equal(metadata.title, "next")
    assert_equal(viewport_fingerprint(view), before)
    var legacy = view.copy()
    legacy.apply(change)
    view.apply_verified_cells(change, result_cells())
    assert_equal(viewport_fingerprint(view), viewport_fingerprint(legacy))
    assert_equal(
        view.grapheme_offsets[len(view.grapheme_offsets) - 1], UInt32(2)
    )
    assert_equal(view.generation, UInt64(10))
    assert_equal(view.metadata.title, "next")
    view = initial()
    var short = result_cells()
    short.shrink(5)
    with assert_raises():
        view.apply_verified_cells(change, short^)
    assert_equal(viewport_fingerprint(view), before)
    change.fields |= CURSOR_AT
    with assert_raises():
        view.apply(change)
    assert_equal(viewport_fingerprint(view), before)
    with assert_raises():
        view.apply_verified_cells(change, result_cells())
    assert_equal(viewport_fingerprint(view), before)
    change = patch()
    change.grapheme_bytes[0] = 255
    with assert_raises():
        view.apply_verified_cells(change, result_cells())
    assert_equal(viewport_fingerprint(view), before)
    print("terminal validation and verified commit exact/atomic checks passed")
