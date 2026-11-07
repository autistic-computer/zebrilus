from std.testing import TestSuite, assert_equal, assert_true, assert_raises
from mjui.input import (
    KeyEvent,
    TextInput,
    TextEvent,
    TEXT_INSERT,
    TEXT_COMMAND,
    MOD_CTRL,
    MOD_ALT,
    MOD_SHIFT,
)
from mjui.keys import Key
from zzmojo.state import AppModel, Session, Window, Pane
from zzmojo.terminal_model import TerminalViewport
from zzmojo.keyboard_controller import KeyboardController


def model() -> AppModel:
    var value = AppModel()
    value.sessions = [Session(0, "first"), Session(1, "second")]
    value.windows = [Window(0, 0, "first", True), Window(1, 1, "second", True)]
    value.panes = [
        Pane(0, 0, "first", "shell", True),
        Pane(1, 0, "second", "shell", False),
        Pane(2, 1, "third", "shell", True),
    ]
    value.session = Optional[UInt64](0)
    value.window = Optional[UInt64](0)
    value.pane = Optional[UInt64](0)
    return value^


def key(
    portable: UInt16,
    ch: UInt32 = 0,
    plain: UInt32 = 0,
    mods: UInt32 = 0,
    lo: Int32 = 0,
    hi: Int32 = 0,
    repeat: Bool = False,
    release: Bool = False,
    consumed: Bool = False,
) -> KeyEvent:
    return KeyEvent(
        code=portable,
        key=portable,
        mods=mods,
        ch=ch,
        unshifted=plain,
        text_lo=lo,
        text_hi=hi,
        repeat=repeat,
        release=release,
        text=hi > lo,
        consumed=consumed,
    )


def inserted(input: TextInput, index: Int) raises -> String:
    var event = input.events[index]
    return String(
        from_utf8=Span(input.bytes)[Int(event.start) : Int(event.end)]
    )


def test_raw_text_subtraction_preserves_utf8_and_composition() raises:
    var value = model()
    var controller = KeyboardController()
    var input = TextInput()
    input.insert("preé🦀post")
    input.events[0].cmd = 1
    input.events.append(TextEvent(TEXT_COMMAND, 37, MOD_CTRL, 0, 0))
    input.marked = List[UInt8](String("未完").as_bytes())
    input.marked_sel_a = 0
    input.marked_sel_b = 3
    input.candidates = ["一", "二"]
    input.candidate_selected = 1
    input.candidates_horizontal = True
    var routed = controller.route(
        value, True, key(Key.A, 97, 97, MOD_CTRL, 3, 9), Span(input.bytes), 7
    )
    assert_true(Bool(routed))
    assert_equal(routed.value().input.text.value(), "é🦀")
    assert_equal(routed.value().epoch, UInt64(7))
    controller.filter_text(input)
    assert_equal(len(input.events), 3)
    assert_equal(inserted(input, 0), "pre")
    assert_equal(inserted(input, 1), "post")
    assert_equal(input.events[0].start, Int32(0))
    assert_equal(input.events[0].end, Int32(3))
    assert_equal(input.events[1].start, Int32(9))
    assert_equal(input.events[1].end, Int32(13))
    assert_equal(input.events[0].cmd, UInt8(1))
    assert_equal(input.events[1].cmd, UInt8(1))
    assert_equal(input.events[2].kind, TEXT_COMMAND)
    assert_equal(input.events[2].cmd, UInt8(37))
    assert_equal(input.events[2].mods, MOD_CTRL)
    assert_equal(String(from_utf8=Span(input.bytes)), "preé🦀post")
    assert_equal(String(from_utf8=Span(input.marked)), "未完")
    assert_equal(input.marked_sel_b, 3)
    assert_equal(input.candidates[1], "二")
    assert_equal(input.candidate_selected, 1)
    assert_true(input.candidates_horizontal)
    controller.filter_text(input)
    assert_equal(len(input.events), 3)


def test_overlapping_ranges_intersect_partial_insert_events() raises:
    var value = model()
    var controller = KeyboardController()
    var input = TextInput()
    input.bytes = List[UInt8](String("abcdefgh").as_bytes())
    input.events = [
        TextEvent(TEXT_INSERT, 0, MOD_ALT, 1, 4),
        TextEvent(TEXT_INSERT, 0, MOD_SHIFT, 4, 8),
    ]
    _ = controller.route(
        value, True, key(Key.A, 97, 97, MOD_CTRL, 2, 6), Span(input.bytes)
    )
    _ = controller.route(
        value, True, key(Key.B, 98, 98, MOD_ALT, 3, 5), Span(input.bytes)
    )
    controller.filter_text(input)
    assert_equal(len(input.events), 2)
    assert_equal(inserted(input, 0), "b")
    assert_equal(inserted(input, 1), "gh")
    assert_equal(input.events[0].mods, MOD_ALT)
    assert_equal(input.events[1].mods, MOD_SHIFT)
    var invalid = TextInput()
    invalid.insert("é")
    with assert_raises():
        _ = controller.route(
            value, True, key(Key.C, 99, 99, MOD_CTRL, 1, 2), Span(invalid.bytes)
        )
    controller.filter_text(invalid)
    assert_equal(inserted(invalid, 0), "é")


def test_plain_ime_and_kitty_key_paths_do_not_duplicate_text() raises:
    var value = model()
    var controller = KeyboardController()
    var input = TextInput()
    input.insert("世界")
    assert_true(
        not controller.route(
            value, True, key(Key.A, 97, 97, 0, 0, 6), Span(input.bytes)
        )
    )
    assert_true(not controller.claimed)
    assert_true(
        not controller.route(
            value, True, key(Key.NONE, 0, 0, 0, 0, 6), Span(input.bytes)
        )
    )
    controller.filter_text(input)
    assert_equal(inserted(input, 0), "世界")
    var plane = TerminalViewport()
    plane.metadata.kitty_keyboard = True
    value.terminals[0] = plane^
    var empty = List[UInt8]()
    assert_true(
        not controller.route(value, True, key(Key.A, 97, 97), Span(empty))
    )
    var typed = TextInput()
    typed.insert("é")
    var routed = controller.route(
        value, True, key(Key.A, 97, 97, 0, 0, 2), Span(typed.bytes)
    )
    assert_true(Bool(routed))
    assert_equal(routed.value().input.text.value(), "é")
    controller.filter_text(typed)
    assert_equal(len(typed.events), 0)
    var ctrl = controller.route(
        value, True, key(Key.B, 98, 98, MOD_CTRL), Span(empty)
    )
    assert_true(Bool(ctrl))
    assert_equal(ctrl.value().input.modifiers, UInt8(2))
    var alt = controller.route(
        value, True, key(Key.A, 233, 97, MOD_ALT), Span(empty)
    )
    assert_true(Bool(alt))
    assert_equal(alt.value().input.modifiers, UInt8(4))
    assert_true(
        not controller.route(
            value, False, key(Key.C, 99, 99, MOD_CTRL), Span(empty)
        )
    )
    assert_true(
        not controller.route(
            value,
            True,
            key(Key.C, 99, 99, MOD_CTRL, consumed=True),
            Span(empty),
        )
    )
    value.terminals[0].metadata.kitty_keyboard = False
    value.routing.armed = True
    var percent = TextInput()
    percent.insert("%")
    var split = controller.route(
        value,
        True,
        key(Key.DIGIT_5, 37, 53, MOD_SHIFT, 0, 1),
        Span(percent.bytes),
    )
    assert_true(Bool(split))
    assert_equal(split.value().input.codepoint, UInt32(37))
    assert_equal(split.value().input.modifiers, UInt8(0))
    controller.filter_text(percent)
    assert_equal(len(percent.events), 0)
    value.terminals[0].metadata.kitty_keyboard = True
    var claimed_without_text = controller.route(
        value, True, key(Key.DIGIT_5, 37, 53, MOD_SHIFT), Span(empty)
    )
    assert_true(Bool(claimed_without_text))
    assert_true(controller.claimed)


def test_armed_prefix_repeats_are_consumed_without_losing_release_owner() raises:
    var value = model()
    var controller = KeyboardController()
    var text = TextInput()
    text.insert("b")
    var press = controller.route(
        value, True, key(Key.B, 98, 98, MOD_CTRL, 0, 1), Span(text.bytes), 7
    )
    assert_true(Bool(press))
    controller.filter_text(text)
    assert_equal(len(text.events), 0)
    value.pane = Optional[UInt64](1)
    value.routing.armed = True
    var repeated = TextInput()
    repeated.insert("b")
    var repeat = controller.route(
        value,
        True,
        key(Key.B, 98, 98, MOD_CTRL, 0, 1, repeat=True),
        Span(repeated.bytes),
        11,
    )
    assert_true(not repeat)
    assert_true(controller.claimed)
    controller.filter_text(repeated)
    assert_equal(len(repeated.events), 0)
    var empty = List[UInt8]()
    var release = controller.route(
        value, False, key(Key.B, 98, 98, release=True), Span(empty), 13
    )
    assert_true(Bool(release))
    assert_equal(release.value().pane, UInt64(0))
    assert_equal(release.value().epoch, UInt64(7))
    assert_true(controller.claimed)
    assert_true(
        not controller.route(
            value, False, key(Key.B, release=True), Span(empty)
        )
    )
    assert_true(not controller.claimed)


def test_releases_keep_first_press_owner_across_navigation() raises:
    var value = model()
    var controller = KeyboardController()
    var empty = List[UInt8]()
    var first = controller.route(value, True, key(Key.LEFT), Span(empty), 7)
    assert_true(Bool(first))
    value.pane = Optional[UInt64](1)
    var repeat = controller.route(
        value, True, key(Key.LEFT, repeat=True), Span(empty), 9
    )
    assert_true(Bool(repeat))
    assert_equal(repeat.value().pane, UInt64(0))
    assert_equal(repeat.value().epoch, UInt64(7))
    var release = controller.route(
        value, False, key(Key.LEFT, release=True), Span(empty), 11
    )
    assert_true(Bool(release))
    assert_true(release.value().release)
    assert_equal(release.value().pane, UInt64(0))
    assert_equal(release.value().epoch, UInt64(7))
    assert_equal(release.value().input.action, UInt8(2))
    assert_equal(value.pane.value(), UInt64(1))
    assert_true(
        not controller.route(
            value, False, key(Key.LEFT, release=True), Span(empty)
        )
    )
    value.pane = Optional[UInt64](0)
    _ = controller.route(value, True, key(Key.LEFT), Span(empty), 13)
    value.session = Optional[UInt64](1)
    value.window = Optional[UInt64](1)
    value.pane = Optional[UInt64](2)
    var cross_session = controller.route(
        value, False, key(Key.LEFT, release=True), Span(empty), 17
    )
    assert_true(Bool(cross_session))
    assert_equal(cross_session.value().pane, UInt64(0))
    assert_equal(cross_session.value().epoch, UInt64(13))
    assert_equal(value.session.value(), UInt64(1))
    assert_equal(value.pane.value(), UInt64(2))
    _ = controller.route(value, True, key(Key.LEFT), Span(empty), 19)
    value.session = Optional[UInt64](0)
    value.window = Optional[UInt64](0)
    value.pane = Optional[UInt64](1)
    _ = controller.route(value, True, key(Key.LEFT), Span(empty), 23)
    var fresh = controller.route(
        value, False, key(Key.LEFT, release=True), Span(empty)
    )
    assert_equal(fresh.value().pane, UInt64(1))
    assert_equal(fresh.value().epoch, UInt64(23))


def test_detach_deactivation_removed_panes_and_output_surface() raises:
    var value = model()
    var controller = KeyboardController()
    var empty = List[UInt8]()
    _ = controller.route(value, True, key(Key.ENTER), Span(empty))
    controller.reset()
    assert_true(
        not controller.route(
            value, False, key(Key.ENTER, release=True), Span(empty)
        )
    )
    _ = controller.route(value, True, key(Key.ENTER), Span(empty))
    value.pane = None
    assert_true(
        not controller.route(
            value, False, key(Key.ENTER, release=True), Span(empty)
        )
    )
    value.pane = Optional[UInt64](0)
    assert_true(
        not controller.route(
            value, False, key(Key.ENTER, release=True), Span(empty)
        )
    )
    _ = controller.route(value, True, key(Key.ENTER), Span(empty))
    controller.remove_pane(0)
    assert_true(
        not controller.route(
            value, False, key(Key.ENTER, release=True), Span(empty)
        )
    )
    _ = controller.route(value, True, key(Key.ENTER), Span(empty))
    value.source_client = 1
    assert_true(
        not controller.route(
            value, False, key(Key.ENTER, release=True), Span(empty)
        )
    )
    var normal = TerminalViewport()
    normal.metadata.kitty_keyboard = True
    value.terminals[0] = normal^
    var output = TerminalViewport()
    value.command_output.apply(0, 7, Optional[TerminalViewport](output^))
    value.routing.table = String("copy-mode")
    var input = TextInput()
    input.insert("a")
    assert_true(
        not controller.route(
            value, True, key(Key.A, 97, 97, 0, 0, 1), Span(input.bytes)
        )
    )
    value.command_output.viewport.value().metadata.kitty_keyboard = True
    var routed = controller.route(
        value, True, key(Key.A, 97, 97, 0, 0, 1), Span(input.bytes)
    )
    assert_true(Bool(routed))
    controller.filter_text(input)
    assert_equal(len(input.events), 0)


def test_local_output_focus_keeps_daemon_selection_and_release_owner() raises:
    var value = model()
    value.pane = Optional[UInt64](1)
    var output = TerminalViewport()
    value.command_output.apply(0, 7, Optional[TerminalViewport](output^))
    value.routing.table = String("copy-mode")
    var controller = KeyboardController()
    var input = TextInput()
    input.insert("q")
    assert_true(
        not controller.route(
            value,
            True,
            key(Key.Q, 113, 113, 0, 0, 1),
            Span(input.bytes),
            7,
            UInt64(1),
        )
    )
    assert_true(
        not controller.route(
            value,
            True,
            key(Key.Q, 113, 113, 0, 0, 1),
            Span(input.bytes),
            7,
            UInt64(0),
        )
    )
    var empty = List[UInt8]()
    var press = controller.route(
        value, True, key(Key.LEFT), Span(empty), 11, UInt64(0)
    )
    assert_true(Bool(press))
    assert_equal(press.value().pane, UInt64(0))
    assert_equal(value.pane.value(), UInt64(1))
    var release = controller.route(
        value, False, key(Key.LEFT, release=True), Span(empty), 13, UInt64(1)
    )
    assert_equal(release.value().pane, UInt64(0))
    assert_equal(release.value().epoch, UInt64(11))


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
