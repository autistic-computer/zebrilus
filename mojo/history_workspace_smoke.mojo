from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.input import TextInput
from mjui.scroll import ScrollEvent
from mjui.testing import Driver, Scene, frame_pixels
from mjui.png import encode_png
from mjui.sys.file import file_write
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from mjui.resources import resource_path
from zzmojo.local_scroll import pixel_scroll_position


struct HistoryScene(Movable, Scene):
    var view: WorkspaceView

    def __init__(out self, path: String) raises:
        self.view = WorkspaceView(path)

    def build(mut self, mut ui: Ui, input: TextInput) raises:
        self.view.build(ui, input)


def pause():
    _ = external_call["usleep", Int32](UInt32(10_000))


def settle(mut driver: Driver[HistoryScene]) raises:
    driver.scene.view.flush()
    driver.scene.view.next_poll = 0
    driver.scene.view.poll(driver.ui.time)
    driver.scene.view.wait()
    driver.frame(0.01)
    driver.scene.view.flush()


def main() raises:
    var args = argv()
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    ui.focus = TERMINAL_KEY
    var scene = HistoryScene(args[1])
    scene.view.wait()
    var driver = Driver(ui^, scene^, 1120, 720)
    driver.frame()
    for _ in range(200):
        settle(driver)
        if (
            UInt64(0) in driver.scene.view.model.histories
            and len(driver.scene.view.model.histories[0][].history) >= 64
        ):
            break
        pause()
    assert_equal(driver.scene.view.model.error, String())
    assert_true(UInt64(0) in driver.scene.view.model.histories)
    var surfaces = driver.scene.view.pointer_surfaces(driver.ui)
    assert_equal(len(surfaces), 1)
    var surface = surfaces[0]
    var x = surface.grid.left + surface.grid.cell_width * 3
    var y = surface.grid.top + surface.grid.line_height * 3
    var server = driver.scene.view.model.terminals[0].metadata.scrollbar.offset
    var oldest = server - UInt32(
        len(driver.scene.view.model.histories[0][].history)
    )
    var expected = pixel_scroll_position(
        server, 0, 45, surface.grid.line_height, oldest, server
    )
    driver.scene.view.scroll(
        driver.ui, ScrollEvent(0, 45, True, 0, 0, driver.ui.time), x, y
    )
    assert_equal(
        driver.scene.view.local_scroll.target(0).value(), expected.target
    )
    assert_equal(driver.scene.view.local_scroll.sub_row(0), expected.sub_row)
    driver.frame()
    var grid_key = driver.scene.view.widget_key(4, 0)
    var grid = driver.ui.term.find(grid_key)
    assert_true(driver.ui.term.views[grid].offset > 0)
    var shown = String(from_utf8=Span(driver.ui.term.views[grid].text))
    assert_true("HISTORY-" in shown)
    var rgba = frame_pixels(driver.ui, driver.scene.view.theme.flat.bg)
    var png = encode_png(rgba.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[2])
    output.write_bytes(Span(png))
    driver.frame(0.121)
    driver.scene.view.flush()
    for _ in range(100):
        settle(driver)
        if (
            driver.scene.view.model.terminals[0].metadata.scrollbar.offset
            == expected.target
        ):
            break
        pause()
    driver.frame()
    assert_equal(
        driver.scene.view.model.terminals[0].metadata.scrollbar.offset,
        expected.target,
    )
    assert_false(Bool(driver.scene.view.local_scroll.target(0)))
    assert_equal(driver.scene.view.local_scroll.sub_row(0), expected.sub_row)
    driver.scene.view.scroll(
        driver.ui, ScrollEvent(0, 45, True, 0, 0, driver.ui.time), x, y
    )
    assert_true(Bool(driver.scene.view.local_scroll.target(0)))
    driver.scene.view.flush_local_scroll(0)
    assert_false(Bool(driver.scene.view.local_scroll.flush(0)))
    driver.scene.view.flush()
    for _ in range(10):
        settle(driver)
    driver.scene.view.scroll(
        driver.ui, ScrollEvent(0, 45, True, 0, 0, driver.ui.time), x, y
    )
    assert_true(Bool(driver.scene.view.local_scroll.target(0)))
    driver.text("x")
    driver.frame()
    assert_false(Bool(driver.scene.view.local_scroll.target(0)))
    assert_false(Bool(driver.scene.view.local_scroll.next_wake()))
    driver.scene.view.flush()
    driver.ui.focus = 9999
    driver.frame()
    assert_true(driver.ui.focus != TERMINAL_KEY)
    var client = driver.scene.view.model.source_client
    driver.click(4)
    assert_equal(driver.ui.focus, TERMINAL_KEY)
    driver.scene.view.flush()
    settle(driver)
    assert_true(driver.scene.view.model.source_client != client)
    assert_equal(driver.scene.view.model.error, String())
    print(
        "native history workspace passed: source rows, fractional pixels,"
        " debounce convergence, selection flush, typed cancellation and"
        " reconnect focus"
    )
