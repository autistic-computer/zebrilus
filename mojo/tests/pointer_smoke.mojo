from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true
from mjui.input import (
    MouseEvent,
    MOUSE_DOWN,
    MOUSE_UP,
    MOUSE_DRAGGED,
    MOUSE_MOVED,
    MOD_SHIFT,
    MOD_ALT,
)
from mjui.scroll import ScrollEvent
from zzmojo.state import AppModel, terminal_text
from zzmojo.stream_transport import InteractiveConnection, connect_interactive
from zzmojo.stream_protocol import input_text
from zzmojo.input_protocol import input_view
from zzmojo.terminal_view import PointerGrid
from zzmojo.pointer_controller import (
    PointerController,
    PointerSurface,
    RoutedPointer,
)


def pause():
    _ = external_call["usleep", Int32](UInt32(10000))


def send(
    mut connection: InteractiveConnection, var routed: Optional[RoutedPointer]
) raises:
    assert_true(Bool(routed))
    connection.transport.write(Span(routed.value().frame))


def send_many(
    mut connection: InteractiveConnection, var routed: List[RoutedPointer]
) raises:
    assert_true(len(routed) > 0 and len(routed) <= 2)
    for item in routed:
        connection.transport.write(Span(item.frame))


def mouse(kind: UInt8, x: Float32, y: Float32, mods: UInt32 = 0) -> MouseEvent:
    return MouseEvent(kind, x, y, 1, 0, mods)


def selection(
    model: AppModel, pane: UInt64, rectangle: Bool = False
) raises -> Bool:
    for overlay in model.terminals[pane].metadata.overlays:
        if overlay.kind & 255 == 0 and (
            not rectangle or overlay.kind & 256 != 0
        ):
            return True
    return False


def main() raises:
    var args = argv()
    var connection = connect_interactive(args[1], "")
    var model = AppModel()
    model.refresh(connection)
    model.sync_focus(connection)
    var pane = model.pane.value()
    for _ in range(300):
        model.refresh(connection)
        if pane in model.terminals and "POINTER_READY" in terminal_text(
            model.terminals[pane]
        ):
            break
        pause()
    assert_true(pane in model.terminals)
    assert_true("POINTER_READY" in terminal_text(model.terminals[pane]))
    var grid = PointerGrid(
        10,
        20,
        Float32(model.terminals[pane].columns) * 8,
        Float32(model.terminals[pane].rows) * 20,
        8,
        20,
        0,
        1,
        Int(model.terminals[pane].columns),
        Int(model.terminals[pane].rows),
    )
    var surfaces: List[PointerSurface] = [PointerSurface(pane, grid)]
    var controller = PointerController()
    var original = model.terminals[pane].metadata.scrollbar.offset
    var wheel = controller.scroll(
        model, Span(surfaces), ScrollEvent(0, 3, False, 0, 0, 0), 27, 81, 0, 7
    )
    assert_true(Bool(wheel))
    assert_true(wheel.value().passive)
    assert_true(not wheel.value().activate)
    send(connection, wheel^)
    for _ in range(300):
        model.refresh(connection)
        if model.terminals[pane].metadata.scrollbar.offset < original:
            break
        pause()
    assert_true(model.terminals[pane].metadata.scrollbar.offset < original)
    assert_equal(model.pane.value(), pane)
    send_many(
        connection,
        controller.route(model, Span(surfaces), mouse(MOUSE_DOWN, 27, 81), 7),
    )
    send_many(
        connection,
        controller.route(
            model,
            Span(surfaces),
            mouse(MOUSE_DRAGGED, grid.left + grid.width + 50, 121),
            11,
        ),
    )
    send_many(
        connection,
        controller.route(
            model,
            Span(surfaces),
            mouse(MOUSE_UP, grid.left + grid.width + 50, 121),
            11,
        ),
    )
    for _ in range(300):
        model.refresh(connection)
        if selection(model, pane):
            break
        pause()
    assert_true(selection(model, pane))
    var bottom = input_view(pane, 3)
    connection.transport.write(Span(bottom))
    var clear = input_view(pane, 14)
    connection.transport.write(Span(clear))
    var enable = input_text(pane, "T")
    connection.transport.write(Span(enable))
    for _ in range(300):
        model.refresh(connection)
        if model.terminals[
            pane
        ].metadata.mouse_tracking and "TRACKING_READY" in terminal_text(
            model.terminals[pane]
        ):
            break
        pause()
    assert_true(model.terminals[pane].metadata.mouse_tracking)
    send_many(
        connection,
        controller.route(model, Span(surfaces), mouse(MOUSE_DOWN, 27, 81), 13),
    )
    send_many(
        connection,
        controller.route(
            model, Span(surfaces), mouse(MOUSE_DRAGGED, 35, 81), 13
        ),
    )
    send_many(
        connection,
        controller.route(model, Span(surfaces), mouse(MOUSE_UP, 35, 81), 17),
    )
    send(
        connection,
        controller.scroll(
            model,
            Span(surfaces),
            ScrollEvent(0, 2, False, 0, 0, 0),
            27,
            81,
            0,
            17,
        ),
    )
    send_many(
        connection,
        controller.route(
            model,
            Span(surfaces),
            mouse(MOUSE_DOWN, 27, 81, MOD_SHIFT | MOD_ALT),
            19,
        ),
    )
    send_many(
        connection,
        controller.route(
            model,
            Span(surfaces),
            mouse(MOUSE_DRAGGED, 43, 121, MOD_SHIFT | MOD_ALT),
            19,
        ),
    )
    send_many(
        connection,
        controller.route(
            model,
            Span(surfaces),
            mouse(MOUSE_UP, 43, 121, MOD_SHIFT | MOD_ALT),
            19,
        ),
    )
    for _ in range(300):
        model.refresh(connection)
        if selection(model, pane, True):
            break
        pause()
    assert_true(selection(model, pane, True))
    for _ in range(20):
        pause()
    assert_equal(model.pane.value(), pane)
    print(
        "native pointer selection, outside release, scrollback, SGR tracking,"
        " Shift+Alt override passed"
    )
