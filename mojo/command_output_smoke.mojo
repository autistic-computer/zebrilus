from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true
from mjui.ui import Ui
from mjui.input import (
    TextInput,
    MouseEvent,
    KeyEvent,
    MOUSE_DOWN,
    MOUSE_DRAGGED,
    MOUSE_UP,
    MOD_CTRL,
)
from mjui.keys import Key
from mjui.testing import Driver, Scene, frame_pixels
from mjui.resources import resource_path
from mjui.png import encode_png
from mjui.sys.file import file_write
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.keyboard_controller import KeyboardController
from zzmojo.input_protocol import input_view


struct OutputScene(Movable, Scene):
    var view: WorkspaceView

    def __init__(out self, path: String) raises:
        self.view = WorkspaceView(path)

    def build(mut self, mut ui: Ui, input: TextInput) raises:
        self.view.build(ui, input)


def pause():
    _ = external_call["usleep", Int32](UInt32(10000))


def settle(mut driver: Driver[OutputScene], time: Float64) raises:
    driver.scene.view.flush()
    driver.scene.view.poll(time)
    driver.scene.view.wait()
    driver.frame()
    driver.scene.view.flush()


def open_output(
    mut driver: Driver[OutputScene], text: String, pane: Optional[UInt64] = None
) raises:
    driver.scene.view.flush()
    var args: List[String] = ["-p", text]
    var name = String("display-message")
    if pane:
        name = "run-shell"
        args = ["-t", "%" + String(pane.value()), "printf '%s' '" + text + "'"]
    var response = (
        driver.scene.view.backend.job[]
        .connection.value()
        .command(name, Span(args))
    )
    assert_equal(response.exit_code, UInt8(0))


def output_ready(mut driver: Driver[OutputScene], time: Float64) raises:
    for attempt in range(300):
        settle(driver, time + Float64(attempt))
        if (
            driver.scene.view.output_input
            and driver.scene.view.output.reported
            and driver.scene.view.model.command_output.viewport
        ):
            var reported = driver.scene.view.output.reported.value()
            ref viewport = (
                driver.scene.view.model.command_output.viewport.value()
            )
            if (
                viewport.columns == reported.columns
                and viewport.rows == reported.rows
            ):
                return
        pause()
    print(
        "output wait",
        time,
        "model",
        driver.scene.view.model.command_output.id.or_else(0),
        "presentation",
        driver.scene.view.output.id.or_else(0),
        "owner",
        driver.scene.view.output.pane.or_else(999),
        "input",
        driver.scene.view.output_input,
        "focus",
        driver.scene.view.output.focused,
        "selected",
        driver.scene.view.model.pane.or_else(999),
        "error",
        driver.scene.view.model.error,
    )
    if driver.scene.view.output.reported:
        var size = driver.scene.view.output.reported.value()
        print(
            "reported",
            size.columns,
            size.rows,
            size.cell_width,
            size.cell_height,
        )
    if driver.scene.view.model.command_output.viewport:
        ref viewport = driver.scene.view.model.command_output.viewport.value()
        print("viewport", viewport.columns, viewport.rows)
    raise Error("command-output presentation or native resize did not settle")


def output_closed(mut driver: Driver[OutputScene], time: Float64) raises:
    for attempt in range(300):
        settle(driver, time + Float64(attempt))
        if (
            not driver.scene.view.model.command_output.id
            and not driver.scene.view.output.id
        ):
            return
        pause()
    raise Error("native command-output copy-mode key did not dismiss")


def click_pane(mut driver: Driver[OutputScene], pane: UInt64) raises:
    var surfaces = driver.scene.view.pointer_surfaces(driver.ui)
    for surface in surfaces:
        if surface.pane == pane:
            var x = surface.grid.left + surface.grid.cell_width * 2.5
            var y = surface.grid.top + surface.grid.line_height * 3.5
            driver.scene.view.mouse(
                driver.ui, MouseEvent(MOUSE_DOWN, x, y, 1, driver.ui.time, 0)
            )
            driver.scene.view.mouse(
                driver.ui, MouseEvent(MOUSE_UP, x, y, 1, driver.ui.time, 0)
            )
            driver.scene.view.flush()
            driver.frame()
            return
    raise Error("private output fixture pane has no rendered surface")


def main() raises:
    var args = argv()
    var ui = Ui(1)
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    var scene = OutputScene(args[1])
    scene.view.wait()
    var driver = Driver(ui^, scene^, 1120, 720)
    driver.ui.focus = TERMINAL_KEY
    driver.frame()
    driver.frame()
    driver.scene.view.flush()
    assert_equal(len(driver.scene.view.model.panes), 2)
    var live_panes = len(driver.scene.view.model.panes)
    var other = driver.scene.view.model.pane.value()
    assert_equal(other, UInt64(1))
    var owner = UInt64(0)
    var text = String()
    for index in range(200):
        text += String("OUTPUT_MARKER_", index, " selection target\n")
    open_output(driver, text, owner)
    output_ready(driver, 1)
    assert_equal(driver.scene.view.model.pane.value(), other)
    assert_equal(driver.scene.view.output.pane.value(), owner)
    assert_equal(driver.scene.view.input_pane().value(), owner)
    assert_equal(driver.ui.focus, TERMINAL_KEY)
    assert_equal(
        driver.scene.view.output.reported.value().generation, UInt64(0)
    )
    var first_id = driver.scene.view.output.id.value()
    var mode_key = driver.scene.view.widget_key(7, first_id)
    var mode_bounds = driver.ui.rect_of(mode_key)
    assert_equal(mode_bounds.y1 - mode_bounds.y0, Float32(28))
    driver.scene.view.mouse(
        driver.ui,
        MouseEvent(
            MOUSE_DOWN,
            mode_bounds.x0 + 1,
            mode_bounds.y0 + 1,
            1,
            driver.ui.time,
            0,
        ),
    )
    driver.scene.view.mouse(
        driver.ui,
        MouseEvent(
            MOUSE_UP,
            mode_bounds.x0 + 1,
            mode_bounds.y0 + 1,
            1,
            driver.ui.time,
            0,
        ),
    )
    assert_equal(len(driver.scene.view.pointer.captures), 0)
    var first_size = driver.scene.view.output.reported.value()
    var output_key = driver.scene.view.input_key()
    var grid = driver.ui.term.find(output_key)
    assert_true(grid >= 0)
    assert_true(
        "OUTPUT_MARKER_"
        in String(from_utf8=Span(driver.ui.term.views[grid].text))
    )
    var surfaces = driver.scene.view.pointer_surfaces(driver.ui)
    var remapped = False
    for surface in surfaces:
        if surface.pane == owner:
            remapped = surface.command_output
    assert_true(remapped)
    driver.width = 1000
    driver.height = 650
    output_ready(driver, 400)
    assert_true(not driver.scene.view.output.reported.value().same(first_size))
    var pixels = frame_pixels(driver.ui, driver.scene.view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1000, 650, 1000 * 4, 4)
    var destination = file_write(args[2])
    destination.write_bytes(Span(png))
    driver.scene.view.set_active(False)
    assert_true(driver.scene.view.output_input)
    assert_true(not driver.scene.view.output.focused)
    driver.scene.view.set_active(True)
    assert_true(driver.scene.view.output.focused)
    assert_equal(driver.scene.view.input_pane().value(), owner)
    click_pane(driver, other)
    assert_true(not driver.scene.view.output_input)
    driver.text("q")
    driver.frame()
    settle(driver, 800)
    assert_equal(driver.scene.view.output.id.value(), first_id)
    assert_equal(driver.scene.view.model.pane.value(), other)
    click_pane(driver, owner)
    assert_true(driver.scene.view.output_input)
    assert_equal(driver.scene.view.model.pane.value(), other)
    driver.text("qq")
    driver.frame()
    settle(driver, 900)
    assert_equal(driver.scene.view.output.id.value(), first_id)
    driver.input.insert("qq", composed=True)
    driver.frame()
    output_closed(driver, 1000)
    assert_equal(driver.scene.view.input_pane().value(), other)
    assert_equal(driver.scene.view.model.pane.value(), other)
    assert_true(driver.ui.term.find(output_key) < 0)
    assert_true(String(6, ":", first_id) not in driver.scene.view.keys)
    assert_true(String(7, ":", first_id) not in driver.scene.view.keys)
    open_output(driver, "SECOND_OUTPUT_MARKER")
    output_ready(driver, 1400)
    assert_true(driver.scene.view.output.id.value() > first_id)
    var second_owner = driver.scene.view.output.pane.value()
    var second_key = driver.scene.view.input_key()
    var keyboard = KeyboardController()
    var bytes = List[UInt8]()
    var press = KeyEvent(
        code=Key.C,
        key=Key.C,
        mods=MOD_CTRL,
        ch=99,
        unshifted=99,
        text_lo=0,
        text_hi=0,
        repeat=False,
        release=False,
        text=False,
        consumed=False,
    )
    var routed = keyboard.route(
        driver.scene.view.model,
        True,
        press,
        Span(bytes),
        driver.scene.view.focus_epoch,
        driver.scene.view.input_pane(),
    )
    assert_true(Bool(routed))
    driver.scene.view.forward_key(
        routed.value().pane,
        routed.value().input,
        routed.value().epoch,
        routed.value().release,
    )
    output_closed(driver, 1800)
    press.release = True
    var release = keyboard.route(
        driver.scene.view.model,
        True,
        press,
        Span(bytes),
        driver.scene.view.focus_epoch,
        driver.scene.view.input_pane(),
    )
    assert_true(Bool(release))
    assert_equal(release.value().pane, second_owner)
    driver.scene.view.forward_key(
        release.value().pane,
        release.value().input,
        release.value().epoch,
        release.value().release,
    )
    settle(driver, 2200)
    assert_true(not driver.scene.view.output.id)
    assert_true(driver.ui.term.find(second_key) < 0)
    assert_true(len(driver.ui.term.views) <= live_panes + 1)
    assert_equal(driver.scene.view.model.error, String())
    assert_equal(len(driver.scene.view.model.panes), 2)
    print(
        "command output: exact pane replacement, native resize, local focus,"
        " pointer parking, Paste/IME split, copy-mode dismissal and release"
        " passed"
    )
