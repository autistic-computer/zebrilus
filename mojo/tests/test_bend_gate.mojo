from std.os import getenv
from std.testing import assert_equal, assert_raises, TestSuite
from zzmojo.bend_gate import BendGate
from zzmojo.terminal_model import (
    TerminalViewport,
    TerminalPatch,
    TerminalSpan,
    TerminalCell,
)


def test_compiled_bend_persistent_verdicts() raises:
    var path = getenv("ZZ_BEND", "../target/port/zz-bend")
    var gate = BendGate(path)
    var frame: List[UInt8] = [4, 0, 0, 0, 0, 0, 120, 0]
    assert_equal(gate.validate(Span(frame)), UInt8(0))
    frame[4] = 2
    frame[5] = 1
    frame[6] = 117
    assert_equal(gate.validate(Span(frame)), UInt8(4))
    frame[4] = 0
    assert_equal(gate.validate(Span(frame)), UInt8(5))
    frame[6] = 120
    assert_equal(gate.validate(Span(frame)), UInt8(6))
    frame[3] = 255
    assert_equal(gate.validate(Span(frame)), UInt8(2))
    var short = List[UInt8]()
    assert_equal(gate.validate(Span(short)), UInt8(1))


def test_dead_bend_worker_fails_closed() raises:
    var gate = BendGate("/usr/bin/false")
    var frame: List[UInt8] = [4, 0, 0, 0, 0, 0, 120, 0]
    with assert_raises():
        _ = gate.validate(Span(frame))


def test_compiled_bend_rejects_stale_and_partial_terminal_rows() raises:
    var gate = BendGate(getenv("ZZ_BEND", "../target/port/zz-bend"))
    var viewport = TerminalViewport()
    viewport.generation = UInt64.MAX
    viewport.view_generation = 0x100000000
    viewport.dictionary_generation = 7
    viewport.columns = 2
    viewport.rows = 3
    var patch = TerminalPatch()
    patch.base_generation = viewport.generation
    patch.base_view_generation = viewport.view_generation
    patch.dictionary_generation = 7
    patch.columns = 2
    patch.rows = 3
    patch.scroll = 1
    patch.spans.append(TerminalSpan(0, 0, 1, False))
    patch.cells.append(TerminalCell(65, 0, 0))
    assert_equal(gate.terminal(viewport, patch), UInt8(4))
    patch.spans[0].clear = True
    assert_equal(gate.terminal(viewport, patch), UInt8(0))
    patch.base_generation = 0xFFFFFFFF
    assert_equal(gate.terminal(viewport, patch), UInt8(1))
    patch.base_generation = viewport.generation
    patch.dictionary_generation = 8
    assert_equal(gate.terminal(viewport, patch), UInt8(2))
    patch.dictionary_generation = 7
    patch.rows = 4
    assert_equal(gate.terminal(viewport, patch), UInt8(3))
    patch.rows = 3
    patch.grapheme_base = 1
    assert_equal(gate.terminal(viewport, patch), UInt8(0))
    patch.grapheme_lengths.append(1)
    patch.grapheme_bytes.append(65)
    assert_equal(gate.terminal(viewport, patch), UInt8(2))


def test_bend_mutates_anonymous_cell_buffers_without_touching_retained_input() raises:
    var gate = BendGate(getenv("ZZ_BEND", "../target/port/zz-bend"))
    var viewport = TerminalViewport()
    viewport.columns = 4
    viewport.rows = 3
    for index in range(12):
        viewport.cells.append(
            TerminalCell(
                UInt32(index + 1), UInt16(index * 3), UInt16(index * 5)
            )
        )
    var patch = TerminalPatch()
    patch.columns = 4
    patch.rows = 3
    patch.scroll = 1
    patch.spans.append(TerminalSpan(0, 0, 1, True))
    patch.cells.append(TerminalCell(0xFFFFFFFF, 0xABCD, 0xFEDC))
    var result = gate.cells(viewport, patch)
    assert_equal(len(result), 12)
    assert_equal(result[0].glyph, UInt32.MAX)
    assert_equal(result[0].style, UInt16(0xABCD))
    assert_equal(result[0].flags, UInt16(0xFEDC))
    for index in range(1, 4):
        assert_equal(result[index].glyph, UInt32(0))
        assert_equal(result[index].style, UInt16(0))
        assert_equal(result[index].flags, UInt16(0))
    for index in range(4, 12):
        assert_equal(result[index].glyph, UInt32(index - 3))
        assert_equal(result[index].style, UInt16((index - 4) * 3))
        assert_equal(result[index].flags, UInt16((index - 4) * 5))
    assert_equal(viewport.cells[0].glyph, UInt32(1))
    assert_equal(viewport.cells[11].glyph, UInt32(12))
    viewport.cells.shrink(4)
    viewport.rows = 1
    patch.rows = 1
    patch.scroll = 0
    patch.spans[0].clear = False
    result = gate.cells(viewport, patch)
    assert_equal(len(result), 4)
    assert_equal(result[0].glyph, UInt32.MAX)
    assert_equal(result[3].glyph, UInt32(4))


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
