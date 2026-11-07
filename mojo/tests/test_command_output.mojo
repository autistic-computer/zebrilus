from std.testing import TestSuite, assert_equal, assert_true
from mjui.ui import Ui, Style
from mjui.widgets.penna import PennaTheme, DEFAULT_DARK
from mjui.widgets.icons import ICON_FILE, ICON_COPY
from mjui.input import TextInput, TEXT_INSERT, INSERT_COMPOSED
from zzmojo.command_output import CommandOutputPresentation, pasted_commit
from zzmojo.client_events import CommandOutputState
from zzmojo.geometry import GridSize
from zzmojo.terminal_model import (
    TerminalViewport,
    TerminalCell,
    TerminalStyle,
    TerminalCursor,
)
from zzmojo.input_protocol import decode_input_frame
from zzmojo.command_output_chrome import mode_indicator, render_mode_indicator
from zzmojo.terminal_model import TerminalMode


def output(id: UInt64 = 7, pane: UInt64 = 3) -> CommandOutputState:
    var viewport = TerminalViewport()
    viewport.columns = 4
    viewport.rows = 3
    viewport.metadata.foreground = 0xFFFFFF
    viewport.metadata.background = 0x112233
    viewport.metadata.cursor = TerminalCursor(0xFFFFFF, 0, 0, 0, 1, 1, 0)
    viewport.styles.append(
        TerminalStyle(0xFFFFFF, 0x112233, 0xFFFFFFFF, 0, 0, 0)
    )
    for glyph in [UInt32(65), 66, 90]:
        for _ in range(4):
            viewport.cells.append(TerminalCell(glyph, 0, 0))
    var result = CommandOutputState()
    result.apply(pane, id, Optional[TerminalViewport](viewport^))
    return result^


def test_output_identity_recreates_focus_and_resize_state() raises:
    var component = CommandOutputPresentation()
    var first = output()
    var opened = component.observe(first)
    assert_equal(opened.kind, UInt8(1))
    assert_equal(opened.pane.value(), UInt64(3))
    assert_true(component.owns(3))
    var focused = decode_input_frame(component.focus(True).value().copy())
    assert_equal(focused.kind, UInt8(7))
    assert_equal(focused.view_action, UInt8(23))
    assert_equal(focused.value, UInt32(1))
    assert_true(not component.focus(True))
    var size = GridSize(80, 24, 16, 36, 99)
    assert_true(not component.resize(size, False, True))
    assert_true(not component.resize(size, True, False))
    var resized = decode_input_frame(
        component.resize(size, True, True).value().copy()
    )
    assert_equal(resized.kind, UInt8(6))
    assert_equal(resized.columns, UInt16(80))
    assert_equal(resized.rows, UInt16(24))
    assert_equal(resized.cell_width, UInt32(16))
    assert_equal(resized.cell_height, UInt32(36))
    assert_equal(component.reported.value().generation, UInt64(0))
    size.generation = 100
    assert_true(not component.resize(size, True, True))
    assert_equal(component.observe(first).kind, UInt8(0))
    var replaced = output(8)
    assert_equal(component.observe(replaced).kind, UInt8(1))
    assert_true(not component.focused)
    assert_true(not component.reported)
    assert_true(Bool(component.resize(size, True, True)))
    _ = component.focus(True)
    assert_equal(component.observe(replaced, 9, 1).kind, UInt8(1))
    assert_true(not component.focused)
    assert_true(not component.reported)
    _ = component.focus(True)
    assert_equal(component.observe(replaced, 9, 2).kind, UInt8(1))
    assert_true(not component.focused)
    replaced.clear()
    var closed = component.observe(replaced, 9, 2)
    assert_equal(closed.kind, UInt8(2))
    assert_equal(closed.previous_pane.value(), UInt64(3))
    assert_true(not component.pane)
    assert_true(not component.commit("q"))
    assert_true(not component.resize(size, True, True))


def test_commit_preserves_native_composed_and_paste_lanes() raises:
    var component = CommandOutputPresentation()
    _ = component.observe(output(7, 7))
    assert_true(not component.commit("q"))
    _ = component.focus(True)
    assert_true(not component.commit(""))
    for text in [String("q"), "é", "🦀"]:
        assert_true(not pasted_commit(text, False))
        var frame = decode_input_frame(component.commit(text).value().copy())
        assert_equal(frame.kind, UInt8(0))
        assert_equal(frame.pane, UInt64(7))
        assert_equal(frame.text, text)
    for text in [String("世界"), "e\u0301", "a\nb", "\r"]:
        assert_true(pasted_commit(text, False))
        var paste = decode_input_frame(component.commit(text).value().copy())
        assert_equal(paste.kind, UInt8(7))
        assert_equal(paste.view_action, UInt8(24))
        assert_equal(paste.text, text)
        var composed = decode_input_frame(
            component.commit(text, True).value().copy()
        )
        assert_equal(composed.kind, UInt8(0))
        assert_equal(composed.text, text)
    var copied = decode_input_frame(
        component.copy_selection(UInt64.MAX, 1).value().copy()
    )
    assert_equal(copied.kind, UInt8(7))
    assert_equal(copied.view_action, UInt8(22))
    assert_equal(copied.request_id, UInt64.MAX)
    assert_equal(copied.target, UInt8(1))


def test_same_frame_composed_provenance_survives_take_and_filtering() raises:
    var component = CommandOutputPresentation()
    _ = component.observe(output())
    _ = component.focus(True)
    var input = TextInput()
    var text = String("世界")
    input.set_marked(
        text.unsafe_ptr(), text.byte_length(), 0, text.byte_length()
    )
    input.insert(text, composed=True)
    input.unmark()
    input.insert("世界")
    input.insert("\n")
    var taken = input.take()
    assert_equal(len(taken.marked), 0)
    assert_equal(taken.events[0].cmd, INSERT_COMPOSED)
    assert_equal(taken.events[1].cmd, UInt8(0))
    assert_equal(taken.events[2].cmd, UInt8(0))
    var composed = decode_input_frame(
        component.text_event(taken, taken.events[0]).value().copy()
    )
    var ordinary = decode_input_frame(
        component.text_event(taken, taken.events[1]).value().copy()
    )
    var newline = decode_input_frame(
        component.text_event(taken, taken.events[2]).value().copy()
    )
    assert_equal(composed.kind, UInt8(0))
    assert_equal(ordinary.kind, UInt8(7))
    assert_equal(ordinary.view_action, UInt8(24))
    assert_equal(newline.kind, UInt8(7))
    assert_equal(newline.text, "\n")


def test_output_renderer_is_top_projected_and_retains_ime() raises:
    var component = CommandOutputPresentation()
    var state = output()
    _ = component.observe(state)
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    ui.set_time(1)
    ui.begin_frame(32, 36)
    var metrics = component.render(
        ui, 71, state, 0, 13, True, 4, 2, String("か")
    )
    ui.end_frame()
    assert_equal((metrics.w, metrics.h), (8, 18))
    ref view = ui.term.views[ui.term.find(71)]
    assert_equal(String(from_utf8=Span(view.text)), "AAAA\nBBBB")
    assert_equal(String(from_utf8=Span(view.ime)), "か")
    assert_equal(view.cursor.row, UInt16(0))
    assert_true(ui.text_input)


def test_mode_chrome_keeps_source_text_and_absolute_strip_bounds() raises:
    var view = mode_indicator(TerminalMode(2, 7, 29, False), 31)
    assert_equal(view.value().label.value(), "View mode")
    assert_equal(view.value().detail, "7 / 29")
    assert_equal(view.value().icon, ICON_FILE)
    var copy = mode_indicator(TerminalMode(1, 7, 29, True), 31)
    assert_equal(copy.value().detail, "+31 output")
    assert_equal(copy.value().icon, ICON_COPY)
    assert_true(not mode_indicator(TerminalMode(0, 0, 0, False), 0))
    var ui = Ui(1)
    _ = ui.add_font("bench/assets/fonts/Inter-Regular.ttf")
    _ = ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    _ = ui.add_font("bench/assets/fonts/lucide.ttf")
    var theme = PennaTheme(DEFAULT_DARK, 0, 0, 1, 2)
    ui.begin_frame(300, 100)
    ui.open(Style.col().width(300).height(100).pad(13, 21))
    assert_true(
        render_mode_indicator(
            ui, theme, TerminalMode(2, 7, 29, False), 0, 71, 300
        )
    )
    ui.close()
    ui.end_frame()
    var bounds = ui.rect_of(71)
    assert_equal(
        (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
        (Float32(8), Float32(8), Float32(292), Float32(36)),
    )
    assert_equal(ui.hit_test(10, 10), UInt64(71))


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
