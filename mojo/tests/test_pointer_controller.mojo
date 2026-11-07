from std.testing import TestSuite, assert_true, assert_equal
from mjui.input import (
    MouseEvent,
    MOUSE_DOWN,
    MOUSE_UP,
    MOUSE_MOVED,
    MOUSE_DRAGGED,
    MOUSE_RIGHT_DOWN,
    MOUSE_RIGHT_UP,
    MOUSE_RIGHT_DRAGGED,
    MOUSE_MIDDLE_DOWN,
    MOUSE_MIDDLE_UP,
    MOUSE_MIDDLE_DRAGGED,
    MOUSE_EXITED,
    MOD_SHIFT,
    MOD_ALT,
    MOD_CTRL,
    MOD_CMD,
)
from mjui.scroll import ScrollEvent
from zzmojo.state import AppModel
from zzmojo.terminal_model import TerminalViewport
from zzmojo.terminal_view import PointerGrid
from zzmojo.pointer_controller import (
    PointerSurface,
    PointerController,
    RoutedPointer,
)
from zzmojo.input_protocol import decode_input_frame


def model() -> AppModel:
    var result = AppModel()
    result.session = Optional[UInt64](0)
    result.window = Optional[UInt64](0)
    result.pane = Optional[UInt64](0)
    result.terminals[0] = TerminalViewport()
    result.terminals[1] = TerminalViewport()
    return result^


def surfaces() -> List[PointerSurface]:
    return [
        PointerSurface(0, PointerGrid(10, 20, 80, 100, 8, 20, 5, 2, 10, 5)),
        PointerSurface(1, PointerGrid(100, 20, 80, 100, 8, 20, 0, 1, 10, 5)),
    ]


def event(
    kind: UInt8,
    x: Float32 = 25,
    y: Float32 = 45,
    mods: UInt32 = 0,
    clicks: Int32 = 1,
) -> MouseEvent:
    return MouseEvent(kind, x, y, clicks, 0, mods)


def route(
    mut controller: PointerController,
    value: AppModel,
    grids: Span[PointerSurface, _],
    pointer: MouseEvent,
    epoch: UInt64 = 0,
) raises -> Optional[RoutedPointer]:
    var result = controller.route(value, grids, pointer, epoch)
    assert_true(len(result) <= 2)
    if len(result) == 0:
        return None
    return result.pop()


def test_surface_exit_clears_old_hover_before_new_motion() raises:
    var value = model()
    var grids = surfaces()
    var controller = PointerController()
    var first = controller.route(value, Span(grids), event(MOUSE_MOVED))
    assert_equal(len(first), 1)
    var next = controller.route(value, Span(grids), event(MOUSE_MOVED, 125, 45))
    assert_equal(len(next), 2)
    assert_equal(next[0].pane, UInt64(0))
    assert_equal(
        decode_input_frame(next[0].frame.copy()).view_action, UInt8(10)
    )
    assert_true(next[0].passive)
    assert_equal(next[1].pane, UInt64(1))
    assert_equal(decode_input_frame(next[1].frame.copy()).view_action, UInt8(9))
    var chrome = controller.route(
        value, Span(grids), event(MOUSE_MOVED, 500, 500)
    )
    assert_equal(len(chrome), 1)
    assert_equal(chrome[0].pane, UInt64(1))
    assert_equal(
        decode_input_frame(chrome[0].frame.copy()).view_action, UInt8(10)
    )
    assert_true(controller.claimed)
    assert_equal(
        len(controller.route(value, Span(grids), event(MOUSE_MOVED, 500, 500))),
        0,
    )
    assert_true(not controller.claimed)


def test_selection_press_drag_release_keep_owner_and_latest_geometry() raises:
    var value = model()
    var controller = PointerController()
    var grids = surfaces()
    var press = route(controller, value, Span(grids), event(MOUSE_DOWN), 7)
    assert_true(Bool(press))
    assert_true(press.value().activate)
    assert_true(not press.value().passive)
    var first = decode_input_frame(press.value().frame.copy())
    assert_equal(first.pane, UInt64(0))
    assert_equal(first.mouse.value().pointer.column, 1)
    assert_equal(first.mouse.value().pointer.row, 1)
    assert_true(not first.mouse.value().force_selection)
    value.pane = Optional[UInt64](1)
    grids[0].grid.left = 15
    var drag = route(
        controller,
        value,
        Span(grids),
        event(MOUSE_DRAGGED, 200, 180, MOD_ALT),
        11,
    )
    assert_true(Bool(drag))
    assert_equal(drag.value().pane, UInt64(0))
    assert_equal(drag.value().epoch, UInt64(7))
    assert_true(not drag.value().activate)
    var moved = decode_input_frame(drag.value().frame.copy())
    assert_equal(moved.mouse.value().pointer.column, 9)
    assert_equal(moved.mouse.value().pointer.row, 4)
    assert_equal(moved.mouse.value().pointer.x, 160)
    assert_true(moved.mouse.value().pointer.rectangle)
    assert_equal(controller.selection_lines, Int32(3))
    var auto = controller.autoscroll(value)
    assert_true(Bool(auto))
    var auto_frame = decode_input_frame(auto.value().frame.copy())
    assert_equal(auto_frame.view_action, UInt8(7))
    assert_equal(auto_frame.lines, Int32(3))
    var release = route(
        controller, value, Span(grids), event(MOUSE_UP, -100, -100), 13
    )
    assert_true(Bool(release))
    assert_equal(release.value().pane, UInt64(0))
    assert_equal(release.value().epoch, UInt64(7))
    var end = decode_input_frame(release.value().frame.copy())
    assert_equal(end.mouse.value().phase, UInt8(1))
    assert_equal(end.mouse.value().pointer.column, 0)
    assert_equal(end.mouse.value().pointer.row, 0)
    assert_true(not controller.autoscroll(value))
    assert_true(not route(controller, value, Span(grids), event(MOUSE_UP)))
    assert_equal(value.pane.value(), UInt64(1))


def test_tracking_forced_selection_and_passive_motion() raises:
    var value = model()
    value.terminals[0].metadata.mouse_tracking = True
    var controller = PointerController()
    var grids = surfaces()
    var hover = route(controller, value, Span(grids), event(MOUSE_MOVED))
    assert_true(Bool(hover))
    assert_true(hover.value().passive)
    assert_true(not hover.value().activate)
    assert_true(
        not decode_input_frame(hover.value().frame.copy()).mouse.value().button
    )
    _ = route(controller, value, Span(grids), event(MOUSE_DOWN))
    var outside = route(
        controller, value, Span(grids), event(MOUSE_DRAGGED, 200, 180)
    )
    assert_equal(
        decode_input_frame(outside.value().frame.copy()).view_action, UInt8(10)
    )
    assert_true(not controller.autoscroll(value))
    _ = route(controller, value, Span(grids), event(MOUSE_UP))
    var forced = route(
        controller,
        value,
        Span(grids),
        event(MOUSE_DOWN, mods=MOD_SHIFT | MOD_ALT),
    )
    var frame = decode_input_frame(forced.value().frame.copy())
    assert_true(frame.mouse.value().force_selection)
    assert_true(frame.mouse.value().pointer.rectangle)
    assert_equal(frame.mouse.value().modifiers, UInt8(5))
    var selecting = route(
        controller, value, Span(grids), event(MOUSE_DRAGGED, 200, 180)
    )
    assert_equal(
        decode_input_frame(selecting.value().frame.copy()).view_action, UInt8(9)
    )
    _ = route(controller, value, Span(grids), event(MOUSE_UP))
    for mods in [MOD_CTRL, MOD_CMD]:
        var triple = route(
            controller,
            value,
            Span(grids),
            event(MOUSE_DOWN, mods=mods, clicks=3),
        )
        assert_true(
            decode_input_frame(triple.value().frame.copy())
            .mouse.value()
            .force_selection
        )
        var refresh = controller.modifiers(
            value, Span(grids), 25, 45, MOD_ALT, 17
        )
        assert_true(Bool(refresh))
        var refreshed = decode_input_frame(refresh.value().frame.copy())
        assert_true(not refreshed.mouse.value().button)
        assert_true(refreshed.mouse.value().force_selection)
        assert_true(refreshed.mouse.value().pointer.rectangle)
        assert_true(refresh.value().passive)
        var up = route(controller, value, Span(grids), event(MOUSE_UP))
        assert_true(
            decode_input_frame(up.value().frame.copy())
            .mouse.value()
            .force_selection
        )
    var exit = route(controller, value, Span(grids), event(MOUSE_EXITED))
    assert_equal(
        decode_input_frame(exit.value().frame.copy()).view_action, UInt8(10)
    )
    assert_true(exit.value().passive)


def test_wheel_fraction_direction_modifiers_and_per_pane_remainders() raises:
    var value = model()
    var grids = surfaces()
    var controller = PointerController()
    var partial = ScrollEvent(0, 9, True, 0, 0, 0)
    assert_true(not controller.scroll(value, Span(grids), partial, 25, 45, 0))
    assert_true(controller.claimed)
    assert_true(not controller.scroll(value, Span(grids), partial, 125, 45, 0))
    var final = controller.scroll(
        value,
        Span(grids),
        ScrollEvent(0, 12, True, 0, 0, 0),
        25,
        45,
        MOD_SHIFT | MOD_CTRL,
        11,
    )
    var frame = decode_input_frame(final.value().frame.copy())
    assert_equal(frame.lines, Int32(-1))
    assert_equal(frame.mouse.value().button.value(), UInt8(3))
    assert_equal(frame.mouse.value().modifiers, UInt8(3))
    assert_true(frame.mouse.value().force_selection)
    assert_true(final.value().passive)
    assert_true(not final.value().activate)
    assert_equal(final.value().epoch, UInt64(11))
    var down = controller.scroll(
        value, Span(grids), ScrollEvent(0, -3, False, 0, 0, 0), 25, 45, MOD_ALT
    )
    var opposite = decode_input_frame(down.value().frame.copy())
    assert_equal(opposite.lines, Int32(2))
    assert_equal(opposite.mouse.value().button.value(), UInt8(4))
    assert_true(opposite.mouse.value().pointer.rectangle)
    assert_true(not controller.scroll(value, Span(grids), partial, 500, 500, 0))
    assert_true(not controller.claimed)


def test_command_output_uses_global_view_and_cancels_replaced_capture() raises:
    var value = model()
    var output = TerminalViewport()
    value.command_output.apply(0, 7, Optional[TerminalViewport](output^))
    var grids = surfaces()
    grids.append(PointerSurface(0, grids[0].grid, True))
    var controller = PointerController()
    var press = route(controller, value, Span(grids), event(MOUSE_DOWN), 7)
    assert_true(Bool(press))
    assert_true(not press.value().activate)
    assert_true(press.value().passive)
    assert_equal(decode_input_frame(press.value().frame.copy()).kind, UInt8(7))
    var wheel = controller.scroll(
        value, Span(grids), ScrollEvent(0, 2, False, 0, 0, 0), 25, 45, 0
    )
    assert_equal(decode_input_frame(wheel.value().frame.copy()).kind, UInt8(7))
    var next = TerminalViewport()
    value.command_output.apply(0, 8, Optional[TerminalViewport](next^))
    assert_true(not route(controller, value, Span(grids), event(MOUSE_UP)))

    _ = route(controller, value, Span(grids), event(MOUSE_DOWN))
    controller.reset()
    assert_true(not route(controller, value, Span(grids), event(MOUSE_UP)))
    _ = route(controller, value, Span(grids), event(MOUSE_RIGHT_DOWN))
    controller.remove_pane(0)
    assert_true(
        not route(controller, value, Span(grids), event(MOUSE_RIGHT_UP))
    )
    _ = route(controller, value, Span(grids), event(MOUSE_DOWN))
    value.source_client = 5
    assert_true(not route(controller, value, Span(grids), event(MOUSE_UP)))


def test_surface_blur_cancels_only_its_gesture_and_preserves_fraction() raises:
    var value = model()
    var grids = surfaces()
    var controller = PointerController()
    _ = controller.route(value, Span(grids), event(MOUSE_DOWN), 7)
    _ = controller.route(
        value, Span(grids), event(MOUSE_RIGHT_DOWN, 125, 45), 11
    )
    assert_true(
        not controller.scroll(
            value, Span(grids), ScrollEvent(0, 9, True, 0, 0, 0), 25, 45, 0
        )
    )
    var clear = controller.blur(value, 0, epoch=13)
    assert_true(Bool(clear))
    assert_true(clear.value().passive)
    assert_true(not clear.value().activate)
    assert_equal(clear.value().pane, UInt64(0))
    assert_equal(clear.value().epoch, UInt64(13))
    assert_equal(
        decode_input_frame(clear.value().frame.copy()).view_action, UInt8(10)
    )
    var cancelled = controller.route(value, Span(grids), event(MOUSE_UP))
    for frame in cancelled:
        assert_equal(
            decode_input_frame(frame.frame.copy()).view_action, UInt8(10)
        )
    var right = route(
        controller, value, Span(grids), event(MOUSE_RIGHT_UP, -100, -100), 17
    )
    assert_true(Bool(right))
    assert_equal(right.value().pane, UInt64(1))
    assert_equal(right.value().epoch, UInt64(11))
    var wheel = controller.scroll(
        value, Span(grids), ScrollEvent(0, 12, True, 0, 0, 0), 25, 45, 0
    )
    assert_true(Bool(wheel))
    assert_equal(
        decode_input_frame(wheel.value().frame.copy()).lines, Int32(-1)
    )



def test_middle_tracking_release_retains_pressed_owner_after_focus_and_blur() raises:
    var value = model()
    value.terminals[0].metadata.mouse_tracking = True
    var grids = surfaces()
    var controller = PointerController()
    var press = route(controller, value, Span(grids), event(MOUSE_MIDDLE_DOWN), 7)
    assert_true(Bool(press))
    assert_equal(decode_input_frame(press.value().frame.copy()).mouse.value().button, Optional[UInt8](1))
    assert_true(press.value().activate)
    _ = controller.blur(value, 0, epoch=8)
    value.pane = UInt64(1)
    _ = route(controller, value, Span(grids), event(MOUSE_MOVED, 125, 45), 9)
    var moved = route(controller, value, Span(grids), event(MOUSE_MIDDLE_DRAGGED, 35, 65), 10)
    assert_true(Bool(moved))
    assert_equal(moved.value().pane, UInt64(0))
    assert_equal(moved.value().epoch, UInt64(7))
    var release = route(controller, value, Span(grids), event(MOUSE_MIDDLE_UP, 500, 500), 11)
    assert_true(Bool(release))
    assert_equal(release.value().pane, UInt64(0))
    assert_equal(release.value().epoch, UInt64(7))
    assert_true(release.value().passive)
    assert_true(not release.value().activate)
    var wire = decode_input_frame(release.value().frame.copy())
    assert_equal(wire.mouse.value().button, Optional[UInt8](1))
    assert_equal(wire.mouse.value().phase, UInt8(1))
    assert_equal(wire.mouse.value().pointer.column, 9)
    assert_equal(wire.mouse.value().pointer.row, 4)
    assert_true(not route(controller, value, Span(grids), event(MOUSE_MIDDLE_UP)))

def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
