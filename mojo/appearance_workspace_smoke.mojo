from std.ffi import external_call
from std.sys import argv
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.input import TextInput, MouseEvent, MOUSE_DOWN, MOUSE_DRAGGED, MOUSE_UP
from mjui.resources import resource_path
from mjui.testing import frame_pixels
from mjui.png import encode_png
from mjui.sys.file import file_write
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.appearance_protocol import TerminalAppearance, AppearanceUpdate, encode_appearance
from zzmojo.geometry import grid_dimension


def frame(mut ui: Ui, mut view: WorkspaceView, seconds: Float64 = 0.01) raises:
    ui.set_time(ui.time + seconds)
    ui.dt = Float32(seconds)
    ui.begin_frame(1120, 720)
    view.build(ui, TextInput())
    ui.end_frame()
    ui.atlas.gpu_done(ui.atlas.frame)


def settle(mut ui: Ui, mut view: WorkspaceView) raises:
    view.flush()
    view.next_poll = 0
    view.poll(ui.time)
    view.wait()
    frame(ui, view)
    view.flush()


def appearance_event(mut view: WorkspaceView, appearance: TerminalAppearance) raises:
    view.flush()
    assert_false(view.backend.busy)
    assert_true(Bool(view.backend.job[].connection))
    var provenance = Dict[UInt8, UInt8]()
    for key in range(30):
        provenance[UInt8(key)] = 0
    var notice = AppearanceUpdate(0, appearance.copy(), provenance^)
    var bytes = encode_appearance(notice)
    ref connection = view.backend.job[].connection.value()
    connection.consume(bytes^)
    view.backend.job[].model.refresh(connection)
    view.model.refresh(connection)
    view.event_driven = True
    view.next_poll = Float64.MAX


def main() raises:
    var args = argv()
    var ui = Ui(1)
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    ui.focus = TERMINAL_KEY
    var view = WorkspaceView(args[1])
    view.wait()
    for _ in range(300):
        settle(ui, view)
        if view.appearance.fonts_ready and view.model.pane and view.model.pane.value() in view.model.terminals:
            break
        _ = external_call["usleep", Int32](UInt32(10000))
    assert_equal(view.model.error, String())
    assert_true(view.appearance.fonts_ready)
    assert_true(Bool(view.model.pane))
    var pane = view.model.pane.value()
    var initial_generation = ui.font_generation
    var requested = view.appearance.active.copy()
    requested.font_size_points = 20 if requested.font_size_points == 17 else 17
    requested.cell_height_kind = 1
    requested.cell_height_adjustment = 5
    requested.padding_left = 12
    requested.padding_right = 24
    requested.padding_top = 18
    requested.padding_bottom = 6
    requested.selection_foreground = 0x010203
    requested.selection_background = 0xE53F99A7
    requested.cursor_blink_policy = 1
    appearance_event(view, requested)
    frame(ui, view)
    assert_equal(view.model.error, String())
    assert_equal(ui.font_generation, initial_generation + 1)
    var key = view.widget_key(4, pane)
    var index = ui.term.find(key)
    assert_true(index >= 0)
    var opts = view.appearance.options(ui, True)
    var bounds = ui.rect_of(view.widget_key(5, pane))
    var width = bounds.x1 - bounds.x0 - 36
    var height = bounds.y1 - bounds.y0 - 24
    assert_equal(ui.term.views[index].x, bounds.x0 + 12)
    assert_equal(ui.term.views[index].y, bounds.y0 + 18)
    assert_equal(ui.term.views[index].cols, Int(grid_dimension(width, opts.cell_width, ui.scale)))
    assert_equal(ui.term.views[index].rows, Int(grid_dimension(height, opts.line_height, ui.scale)))
    assert_true(ui.term.views[index].opts.native_appearance)
    assert_true(ui.term.views[index].cursor_blink.active)
    for _ in range(200):
        settle(ui, view)
        if pane in view.reported and view.model.terminals[pane].columns == view.reported[pane].columns and view.model.terminals[pane].rows == view.reported[pane].rows:
            break
        _ = external_call["usleep", Int32](UInt32(10000))
    assert_equal(view.model.terminals[pane].columns, view.reported[pane].columns)
    assert_equal(view.model.terminals[pane].rows, view.reported[pane].rows)
    var surfaces = view.pointer_surfaces(ui)
    assert_equal(len(surfaces), 1)
    var surface = surfaces[0]
    var x = surface.grid.left + surface.grid.cell_width * 0.25
    var y = surface.grid.top + surface.grid.line_height * 0.5
    view.mouse(ui, MouseEvent(MOUSE_DOWN, x, y, 1, ui.time, 0))
    view.mouse(ui, MouseEvent(MOUSE_DRAGGED, x + surface.grid.cell_width * 5, y, 0, ui.time, 0))
    view.mouse(ui, MouseEvent(MOUSE_UP, x + surface.grid.cell_width * 5, y, 1, ui.time, 0))
    for _ in range(100):
        settle(ui, view)
        index = ui.term.find(key)
        if len(ui.term.views[index].selected) > 0:
            break
        _ = external_call["usleep", Int32](UInt32(10000))
    assert_true(len(ui.term.views[index].selected) > 0)
    var selection = ui.term.views[index].selected[0]
    assert_equal(ui.term.views[index].glyphs[Int(selection.first)].color, UInt32(0x010203FF))
    var generation = ui.font_generation
    requested.selection_foreground = 0x33CC99
    requested.selection_background = 0x88004472
    appearance_event(view, requested)
    frame(ui, view)
    assert_equal(ui.font_generation, generation)
    index = ui.term.find(key)
    selection = ui.term.views[index].selected[0]
    assert_equal(ui.term.views[index].glyphs[Int(selection.first)].color, UInt32(0x33CC99FF))
    assert_equal(view.model.error, String())
    var pixels = frame_pixels(ui, view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[2])
    output.write_bytes(Span(png))
    print("Appearance workspace: encoded updates, font replacement, asymmetric padding, cell-height resize, native cursor and live selection recoloring passed")
