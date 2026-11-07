from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.input import TextInput
from mjui.scroll import ScrollEvent
from mjui.testing import frame_pixels
from mjui.png import encode_png
from mjui.sys.file import file_write
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from mjui.resources import resource_path
from zzmojo.local_scroll import pixel_scroll_position
from zzmojo.local_scroll_controller import scroll_context


def frame(
    mut ui: Ui,
    mut view: WorkspaceView,
    seconds: Float64 = 0,
    input: TextInput = TextInput(),
) raises:
    var time = ui.time + seconds
    ui.set_time(time)
    ui.dt = Float32(min(seconds, Float64(0.1)))
    ui.begin_frame(1120, 720)
    view.build(ui, input)
    ui.end_frame()


def main() raises:
    var args = argv()
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    ui.focus = TERMINAL_KEY
    var view = WorkspaceView(args[1])
    view.wait()
    frame(ui, view)
    for _ in range(200):
        view.flush()
        view.next_poll = 0
        view.poll(0)
        view.wait()
        frame(ui, view, 0.01)
        view.flush()
        if (
            UInt64(0) in view.model.histories
            and Int(scroll_context(view.model, 0).history_rows) >= 64
        ):
            break
        _ = external_call["usleep", Int32](UInt32(10_000))
    assert_equal(view.model.error, String())
    assert_true(UInt64(0) in view.model.histories)
    var surfaces = view.pointer_surfaces(ui)
    assert_equal(len(surfaces), 1)
    var surface = surfaces[0]
    var x = surface.grid.left + surface.grid.cell_width * 3
    var y = surface.grid.top + surface.grid.line_height * 3
    var server = view.model.terminals[0].metadata.scrollbar.offset
    var oldest = server - UInt32(scroll_context(view.model, 0).history_rows)
    var expected = pixel_scroll_position(
        server, 0, 45, surface.grid.line_height, oldest, server
    )
    var scroll_event = ScrollEvent(0, 45, True, 0, 0, ui.time)
    view.scroll(ui, scroll_event, x, y)
    assert_equal(view.local_scroll.target(0).value(), expected.target)
    assert_equal(view.local_scroll.sub_row(0), expected.sub_row)
    frame(ui, view)
    var grid_key = view.widget_key(4, 0)
    var grid = ui.term.find(grid_key)
    assert_true(ui.term.views[grid].offset > 0)
    var shown = String(from_utf8=Span(ui.term.views[grid].text))
    assert_true("HISTORY-" in shown)
    var rgba = frame_pixels(ui, view.theme.flat.bg)
    var png = encode_png(rgba.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[2])
    output.write_bytes(Span(png))
    frame(ui, view, 0.121)
    view.flush()
    for _ in range(100):
        view.flush()
        view.next_poll = 0
        view.poll(0)
        view.wait()
        frame(ui, view, 0.01)
        view.flush()
        if view.model.terminals[0].metadata.scrollbar.offset == expected.target:
            break
        _ = external_call["usleep", Int32](UInt32(10_000))
    frame(ui, view)
    assert_equal(
        view.model.terminals[0].metadata.scrollbar.offset,
        expected.target,
    )
    assert_false(Bool(view.local_scroll.target(0)))
    assert_equal(view.local_scroll.sub_row(0), expected.sub_row)
    scroll_event = ScrollEvent(0, 45, True, 0, 0, ui.time)
    view.scroll(ui, scroll_event, x, y)
    assert_true(Bool(view.local_scroll.target(0)))
    view.flush_local_scroll(0)
    assert_false(Bool(view.local_scroll.flush(0)))
    view.flush()
    for _ in range(10):
        view.flush()
        view.next_poll = 0
        view.poll(0)
        view.wait()
        frame(ui, view, 0.01)
        view.flush()
    scroll_event = ScrollEvent(0, 45, True, 0, 0, ui.time)
    view.scroll(ui, scroll_event, x, y)
    assert_true(Bool(view.local_scroll.target(0)))
    var input = TextInput()
    input.insert("x")
    frame(ui, view, input=input)
    assert_false(Bool(view.local_scroll.target(0)))
    assert_false(Bool(view.local_scroll.next_wake()))
    view.flush()
    ui.focus = 9999
    frame(ui, view)
    assert_true(ui.focus != TERMINAL_KEY)
    var client = view.model.source_client
    var reconnect = ui.rect_of(4)
    var reconnect_x = (reconnect.x0 + reconnect.x1) / 2
    var reconnect_y = (reconnect.y0 + reconnect.y1) / 2
    ui.pointer_down(reconnect_x, reconnect_y, 1)
    frame(ui, view)
    ui.pointer_up(reconnect_x, reconnect_y)
    frame(ui, view)
    assert_equal(ui.focus, TERMINAL_KEY)
    view.flush()
    view.flush()
    view.next_poll = 0
    view.poll(0)
    view.wait()
    frame(ui, view, 0.01)
    view.flush()
    assert_true(view.model.source_client != client)
    assert_equal(view.model.error, String())
    print(
        "native history workspace passed: source rows, fractional pixels,"
        " debounce convergence, selection flush, typed cancellation and"
        " reconnect focus"
    )
