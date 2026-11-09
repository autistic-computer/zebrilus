from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.input import (
    TextInput,
    KeyEvent,
    MouseEvent,
    Clipboard,
    MOUSE_DOWN,
    MOUSE_UP,
    TEXT_POINTER,
    MOD_CTRL,
)
from mjui.keys import Key
from mjui.resources import resource_path
from mjui.sys.clock import monotonic_seconds
from mjui.web import web_subprocess_check
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
    var notices: String

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
        self.notices = String()
        self.ui.focus = TERMINAL_KEY

    @inline(.never)
    def frame(mut self) raises:
        var input = self.native.take()
        for event in input.events:
            if event.kind == TEXT_POINTER:
                var pointer = input.pointers[Int(event.start)]
                if pointer.kind == MOUSE_DOWN:
                    self.ui.pointer_down(pointer.x, pointer.y, pointer.clicks)
                elif pointer.kind == MOUSE_UP:
                    self.ui.pointer_up(pointer.x, pointer.y)
        self.ui.set_time(self.ui.time + 0.02)
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
        self.ui.atlas.gpu_done(self.ui.atlas.frame)
        self.view.flush()

    @inline(.never)
    def settle(mut self) raises:
        _ = self.view.browser.pump()
        self.view.flush()
        self.view.next_poll = 0
        self.view.poll(self.ui.time)
        self.view.wait()
        self.frame()
        for entry in self.view.palette.notifications.entries:
            if entry.notification.message not in self.notices:
                self.notices += String(entry.notification.message, "; ")
        _ = external_call["usleep", Int32](UInt32(5_000))

    def key(
        mut self, code: UInt16, character: UInt32, mods: UInt32, text: String
    ) raises:
        for release in [False, True]:
            var start = Int32(len(self.native.bytes))
            if not release and text.byte_length() > 0:
                self.native.insert(text)
            var end = Int32(len(self.native.bytes))
            self.timeline.raw_key(
                self.view,
                KeyEvent(
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
                ),
                self.native,
            )
            self.frame()

    def browser(self) -> Optional[UInt64]:
        for session in self.view.model.snapshot.sessions:
            for window in session.windows:
                for entry in window.panes.items():
                    if entry.value.kind.kind == 2:
                        return entry.key
        return None

    def tabs(self) -> Int:
        for session in self.view.model.snapshot.sessions:
            for window in session.windows:
                for entry in window.panes.items():
                    if entry.value.kind.kind == 2:
                        return len(entry.value.kind.tabs)
        return 0

    def title(self, pane: UInt64) -> String:
        var view = self.view.browser.presentation(pane)
        if not view:
            return String()
        return view.value().tabs[view.value().active].label.copy()

    def wait(mut self, pane: UInt64, title: String) raises:
        var deadline = monotonic_seconds() + 20
        while monotonic_seconds() < deadline:
            self.settle()
            if self.title(pane) == title:
                return
        raise Error(
            String(
                "browser title did not reach ",
                title,
                "; saw ",
                self.title(pane),
                " / ",
                self.view.model.error,
                " / ",
                self.notices,
            )
        )


def main() raises:
    web_subprocess_check()
    var args = argv()
    var driver = Driver(String(args[1]))
    var deadline = monotonic_seconds() + 20
    while monotonic_seconds() < deadline:
        driver.settle()
        if driver.browser() and driver.view.browser.owns(
            driver.browser().value()
        ):
            break
    var pane = driver.browser().value()
    if not driver.view.browser.owns(pane):
        var failure = driver.view.browser.failure()
        var notices = driver.notices.copy()
        raise Error(
            String(
                "browser pane was not started: ",
                failure.value() if failure else String("no failure"),
                " / ",
                driver.view.model.error,
                " / ",
                notices,
            )
        )
    driver.wait(pane, "one")
    assert_true(Bool(driver.view.model.pane))
    assert_true(driver.view.model.pane != Optional[UInt64](pane))
    assert_false(Bool(driver.view.browser_pane(driver.ui)))
    var content = driver.ui.rect_of(
        driver.view.name_key(driver.view.browser_key_name(pane))
    )
    assert_true(content.x1 - content.x0 > 100)
    var x = content.x0 + 40
    var y = content.y0 + 40
    var clicks = 0
    deadline = monotonic_seconds() + 20
    while monotonic_seconds() < deadline and driver.title(pane) != "clicked":
        clicks += 1
        _ = driver.native.record_pointer(
            MouseEvent(MOUSE_DOWN, x, y, 1, driver.ui.time, 0)
        )
        driver.frame()
        _ = driver.native.record_pointer(
            MouseEvent(MOUSE_UP, x, y, 1, driver.ui.time, 0)
        )
        driver.frame()
        for _ in range(40):
            driver.settle()
            if driver.title(pane) == "clicked":
                break
    assert_equal(driver.title(pane), "clicked")
    for _ in range(100):
        driver.settle()
        if driver.view.model.pane == Optional[UInt64](pane):
            break
    assert_equal(driver.view.model.pane, Optional[UInt64](pane))
    assert_equal(driver.ui.focus, TERMINAL_KEY)
    assert_equal(driver.view.browser_pane(driver.ui), Optional[UInt64](pane))
    driver.key(Key.Z, 122, 0, String("z"))
    driver.wait(pane, "typed:z")
    driver.key(Key.B, 98, MOD_CTRL, String())
    deadline = monotonic_seconds() + 20
    while monotonic_seconds() < deadline:
        driver.settle()
        if driver.view.model.routing.table:
            break
    assert_equal(driver.view.model.routing.table.value(), "prefix")
    driver.key(Key.ESCAPE, 27, 0, String())
    deadline = monotonic_seconds() + 20
    while monotonic_seconds() < deadline:
        driver.settle()
        if not driver.view.model.routing.table:
            break
    assert_false(Bool(driver.view.model.routing.table))
    driver.key(Key.Q, 113, 0, String("q"))
    driver.wait(pane, "typed:zq")
    driver.key(Key.T, 116, MOD_CTRL, String())
    deadline = monotonic_seconds() + 20
    while monotonic_seconds() < deadline:
        driver.settle()
        if driver.tabs() == 2:
            break
    assert_equal(driver.tabs(), 2)
    assert_equal(len(driver.view.browser.presentation(pane).value().tabs), 2)
    driver.key(Key.W, 119, MOD_CTRL, String())
    deadline = monotonic_seconds() + 20
    while monotonic_seconds() < deadline:
        driver.settle()
        if driver.tabs() == 1:
            break
    assert_equal(driver.tabs(), 1)
    driver.key(Key.W, 119, MOD_CTRL, String())
    deadline = monotonic_seconds() + 20
    while monotonic_seconds() < deadline:
        driver.settle()
        if not driver.browser() and driver.view.browser.count() == 0:
            break
    assert_false(Bool(driver.browser()))
    assert_equal(driver.view.browser.count(), 0)
    assert_equal(driver.view.model.error, String())
    print("Browser workspace checks passed; page clicks", clicks)
