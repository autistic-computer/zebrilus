from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true
from mjui.ui import Ui
from mjui.input import TextInput, MouseEvent, MOUSE_DOWN, MOUSE_UP
from mjui.testing import Driver, Scene, frame_pixels
from mjui.png import encode_png
from mjui.resources import resource_path
from mjui.sys.file import file_write
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from mjui.term.grid import cell_metrics
from zzmojo.geometry import grid_dimension


struct WorkspaceScene(Movable, Scene):
    var view: WorkspaceView

    def __init__(out self, path: String) raises:
        self.view = WorkspaceView(path)

    def build(mut self, mut ui: Ui, input: TextInput) raises:
        self.view.build(ui, input)


def main() raises:
    var args = argv()
    if (
        len(args) < 3
        or len(args) > 4
        or (len(args) == 4 and args[3] != "--exercise")
    ):
        raise Error("usage: zz-mojo-shot SOCKET OUTPUT.png [--exercise]")
    var ui = Ui(1)
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    var scene = WorkspaceScene(args[1])
    scene.view.wait()
    var driver = Driver(ui^, scene^, 1120, 720)
    driver.frame()
    driver.frame()
    if len(args) == 4:
        for attempt in range(100):
            driver.scene.view.flush()
            if (
                driver.scene.view.model.session
                and driver.scene.view.model.pane
                and driver.scene.view.model.pane.value()
                in driver.scene.view.model.terminals
                and driver.scene.view.backend.job[].connection
            ):
                if (
                    driver.scene.view.backend.job[]
                    .connection.value()
                    .model.view.session
                    == driver.scene.view.model.session
                ):
                    break
            _ = external_call["usleep", Int32](UInt32(10000))
            driver.scene.view.poll(Float64(attempt + 1))
            driver.scene.view.wait()
            driver.frame()
        driver.scene.view.flush()
        assert_true(Bool(driver.scene.view.model.session))
        assert_true(Bool(driver.scene.view.model.pane))
        assert_true(
            driver.scene.view.model.pane.value()
            in driver.scene.view.model.terminals
        )
        assert_true(Bool(driver.scene.view.backend.job[].connection))
        assert_equal(
            driver.scene.view.backend.job[]
            .connection.value()
            .model.view.session,
            driver.scene.view.model.session,
        )
        assert_equal(driver.scene.view.model.error, String())
        var before = len(driver.scene.view.model.panes)
        driver.click(2)
        driver.scene.view.flush()
        assert_equal(len(driver.scene.view.model.panes), before + 1)
        driver.frame()
        driver.ui.focus = TERMINAL_KEY
        driver.text("printf '\\x4d\\x4f\\x4a\\x4f_UI_OK\\n'")
        driver.frame()
        driver.scene.view.key("Enter")
        driver.scene.view.flush()
        for attempt in range(100):
            if "MOJO_UI_OK" in driver.scene.view.model.terminal.replace(
                "\n", ""
            ):
                break
            _ = external_call["usleep", Int32](UInt32(10000))
            driver.scene.view.poll(Float64(attempt + 3))
            driver.scene.view.wait()
        assert_true(
            "MOJO_UI_OK" in driver.scene.view.model.terminal.replace("\n", "")
        )
        for attempt in range(100):
            driver.frame()
            driver.scene.view.flush()
            var resized = 0
            var expected = 0
            var metrics = cell_metrics(
                driver.ui.shaper.fonts[2], 13, driver.ui.scale
            )
            for pane in driver.scene.view.model.panes:
                if (
                    not driver.scene.view.model.window
                    or pane.window != driver.scene.view.model.window.value()
                ):
                    continue
                expected += 1
                var hit = driver.ui.rect_of(
                    driver.scene.view.widget_key(5, pane.id)
                )
                assert_true(
                    hit.key != 0 and hit.x1 > hit.x0 and hit.y1 > hit.y0
                )
                if pane.id in driver.scene.view.model.terminals:
                    var viewport_rows = driver.scene.view.model.terminals[
                        pane.id
                    ].rows
                    var viewport_columns = driver.scene.view.model.terminals[
                        pane.id
                    ].columns
                    var columns = grid_dimension(
                        hit.x1 - hit.x0 - 16, metrics.width(), driver.ui.scale
                    )
                    var rows = grid_dimension(
                        hit.y1 - hit.y0 - 16, metrics.height(), driver.ui.scale
                    )
                    var grid = driver.ui.term.find(
                        driver.scene.view.widget_key(4, pane.id)
                    )
                    assert_true(grid >= 0)
                    assert_equal(driver.ui.term.views[grid].cols, Int(columns))
                    assert_equal(driver.ui.term.views[grid].rows, Int(rows))
                    assert_true(pane.id in driver.scene.view.reported)
                    var reported = driver.scene.view.reported[pane.id]
                    assert_equal(reported.columns, columns)
                    assert_equal(reported.rows, rows)
                    if (
                        viewport_rows == rows
                        and reported.generation
                        == driver.scene.view.model.layout_generation
                    ):
                        resized += 1
                    elif attempt == 99:
                        print(
                            "resize mismatch",
                            pane.id,
                            "actual",
                            viewport_columns,
                            viewport_rows,
                            "wanted",
                            columns,
                            rows,
                            "bounds",
                            hit.x0,
                            hit.y0,
                            hit.x1,
                            hit.y1,
                            "generation",
                            driver.scene.view.model.layout_generation,
                        )
            if resized == expected:
                assert_equal(expected, before + 1)
                break
            assert_true(attempt < 99)
            _ = external_call["usleep", Int32](UInt32(10000))
            driver.scene.view.poll(Float64(attempt + 200))
            driver.scene.view.wait()
        driver.frame()
        var surfaces = driver.scene.view.pointer_surfaces(driver.ui)
        assert_equal(len(surfaces), before + 1)
        var surface = surfaces[0]
        var x = surface.grid.left + surface.grid.cell_width * 0.5
        var y = surface.grid.top + surface.grid.line_height * 0.5
        driver.scene.view.mouse(
            driver.ui, MouseEvent(MOUSE_DOWN, x, y, 1, driver.ui.time, 0)
        )
        driver.scene.view.mouse(
            driver.ui, MouseEvent(MOUSE_UP, x, y, 1, driver.ui.time, 0)
        )
        driver.scene.view.flush()
        assert_equal(
            driver.scene.view.model.pane, Optional[UInt64](surface.pane)
        )
        assert_equal(driver.ui.focus, TERMINAL_KEY)
        assert_equal(len(driver.scene.view.pointer.captures), 0)
        assert_equal(driver.scene.view.model.error, String())
        driver.frame()
        var split_pixels = frame_pixels(
            driver.ui, driver.scene.view.theme.flat.bg
        )
        var split_png = encode_png(
            split_pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4
        )
        var split_output = file_write(args[2] + ".splits.png")
        split_output.write_bytes(Span(split_png))
        driver.click(1)
        driver.scene.view.flush()
        assert_equal(len(driver.scene.view.model.sessions), 2)
        driver.frame()
        print(
            "native workspace actions passed: visible splits, resized terminal"
            " grids, pointer focus, text input, Enter, create session"
        )
    var pixels = frame_pixels(driver.ui, driver.scene.view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[2])
    output.write_bytes(Span(png))
