from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.input import TextInput, KeyEvent, Clipboard, MOD_CTRL, MOD_SHIFT
from mjui.keys import Key
from mjui.resources import resource_path
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.keyboard_controller import KeyboardController
from zzmojo.clipboard_controller import ClipboardController
from zzmojo.input_timeline import InputTimeline


struct NullHost(Clipboard, Movable):
    def __init__(out self):
        pass

    def get_clipboard(self) -> String:
        return String()

    def get_clipboard_bounded(self, maximum: Int) raises -> String:
        _ = maximum
        return String()

    def set_clipboard(mut self, text: String):
        _ = text

    def supports_primary(self) -> Bool:
        return False

    def get_primary_selection(self) -> String:
        return String()

    def get_primary_selection_bounded(self, maximum: Int) raises -> String:
        _ = maximum
        return String()

    def set_primary_selection(mut self, text: String):
        _ = text


struct Driver(Movable):
    var ui: Ui
    var view: WorkspaceView
    var host: NullHost
    var keyboard: KeyboardController
    var clipboard: ClipboardController
    var timeline: InputTimeline
    var native: TextInput

    def __init__(out self, socket: String) raises:
        self.ui = Ui(1)
        self.ui.native_scene = True
        self.ui.native_paths = True
        _ = self.ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
        _ = self.ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
        _ = self.ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
        _ = self.ui.add_font(resource_path("fonts/lucide.ttf"))
        self.view = WorkspaceView(socket)
        self.view.event_driven = True
        self.host = NullHost()
        self.keyboard = KeyboardController()
        self.clipboard = ClipboardController()
        self.timeline = InputTimeline()
        self.native = TextInput()
        self.ui.focus = TERMINAL_KEY

    @inline(.never)
    def frame(mut self, step: Float64 = 0.02) raises:
        var input = self.native.take()
        self.ui.set_time(self.ui.time + step)
        self.ui.begin_frame(1120, 720)
        _ = self.timeline.process(
            self.view,
            self.ui,
            self.host,
            self.keyboard,
            self.clipboard,
            input,
        )
        self.view.build(self.ui, input)
        self.ui.end_frame()
        assert_true(self.ui.native_scene_complete)
        self.ui.atlas.gpu_done(self.ui.atlas.frame)
        self.view.flush()

    @inline(.never)
    def settle(mut self, step: Float64 = 0.02) raises:
        self.view.flush()
        self.view.next_poll = 0
        self.view.poll(self.ui.time)
        self.view.wait()
        self.frame(step)
        _ = external_call["usleep", Int32](UInt32(10_000))

    def key(
        mut self, code: UInt16, character: UInt32, mods: UInt32, text: String
    ) raises:
        for release in [False, True]:
            var start = Int32(len(self.native.bytes))
            if not release and text.byte_length() > 0:
                self.native.insert(text)
            var end = Int32(len(self.native.bytes))
            var event = KeyEvent(
                code,
                code,
                mods,
                character,
                character,
                start,
                end,
                False,
                release,
                end > start,
                False,
                -1,
            )
            self.timeline.raw_key(self.view, event, self.native)
            self.frame()

    def pane_text(self) -> String:
        if not self.view.model.pane:
            return String()
        return self.view.model.terminal.copy()


def main() raises:
    var args = argv()
    var driver = Driver(String(args[1]))
    for _ in range(300):
        driver.settle()
        if (
            driver.view.model.pane
            and driver.view.model.key_tables.snapshot
            and driver.view.model.attached_session
            and len(driver.view.model.routing.options) > 0
        ):
            break
    assert_equal(driver.view.model.error, String())
    assert_equal(driver.view.which_key_delay, UInt64(400))
    driver.key(Key.B, 98, MOD_CTRL, String())
    for _ in range(300):
        driver.settle(0.001)
        if driver.view.model.routing.table:
            break
    assert_equal(driver.view.model.routing.table.value(), "prefix")
    var armed = driver.ui.time
    while driver.ui.time < armed + 0.5:
        driver.frame(0.05)
    assert_true(driver.view.which_key.timing.visible)
    var short = len(driver.view.which_key.sheet.value().rows)
    assert_true(Bool(driver.view.which_key.sheet.value().more))
    driver.key(Key.SLASH, 63, MOD_SHIFT, String("?"))
    assert_true(driver.view.which_key.timing.visible)
    assert_false(Bool(driver.view.which_key.sheet.value().more))
    assert_true(len(driver.view.which_key.sheet.value().rows) > short)
    assert_false(Key.SLASH in driver.keyboard.forwarded)
    for _ in range(20):
        driver.settle()
    assert_true(driver.view.which_key.timing.visible)
    assert_equal(driver.view.model.routing.table.value(), "prefix")
    assert_true(driver.view.model.routing.armed)
    driver.key(Key.ESCAPE, 27, 0, String())
    assert_false(driver.view.which_key.timing.visible)
    for _ in range(300):
        driver.settle(0.001)
        if not driver.view.model.routing.table:
            break
    assert_false(Bool(driver.view.model.routing.table))
    driver.key(Key.SLASH, 63, MOD_SHIFT, String("?"))
    for _ in range(300):
        driver.settle()
        if "?" in driver.pane_text():
            break
    assert_true("?" in driver.pane_text())
    assert_false(driver.view.which_key.timing.visible)
    assert_equal(driver.view.model.error, String())
    print("Which-key workspace checks passed")
