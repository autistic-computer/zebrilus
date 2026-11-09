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
    MOUSE_MIDDLE_UP,
    TEXT_POINTER,
    TEXT_PRIMARY,
    CMD_NEWLINE,
)
from mjui.keys import Key
from mjui.resources import resource_path
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.keyboard_controller import KeyboardController
from zzmojo.clipboard_controller import ClipboardController
from zzmojo.input_timeline import InputTimeline
from zzmojo.desktop_nav import nav_command


struct PrimaryHost(Clipboard, Movable):
    var primary: String

    def __init__(out self):
        self.primary = String("ond")

    def get_clipboard(self) -> String:
        return String()

    def get_clipboard_bounded(self, maximum: Int) raises -> String:
        _ = maximum
        return String()

    def set_clipboard(mut self, text: String):
        _ = text

    def supports_primary(self) -> Bool:
        return True

    def get_primary_selection(self) -> String:
        return self.primary

    def get_primary_selection_bounded(self, maximum: Int) raises -> String:
        _ = maximum
        return self.primary

    def set_primary_selection(mut self, text: String):
        _ = text


struct Driver(Movable):
    var ui: Ui
    var view: WorkspaceView
    var host: PrimaryHost
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
        self.view = WorkspaceView(socket, native_chooser=True)
        self.view.event_driven = True
        self.host = PrimaryHost()
        self.keyboard = KeyboardController()
        self.clipboard = ClipboardController()
        self.timeline = InputTimeline()
        self.native = TextInput()
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
            elif event.kind == TEXT_PRIMARY:
                var pointer = input.primaries[Int(event.start)]
                if pointer.kind == MOUSE_DOWN:
                    self.ui.pointer_middle(pointer.x, pointer.y)
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
        assert_true(self.ui.native_scene_complete)
        self.ui.atlas.gpu_done(self.ui.atlas.frame)
        self.view.flush()

    @inline(.never)
    def settle(mut self) raises:
        self.view.flush()
        self.view.next_poll = 0
        self.view.poll(self.ui.time)
        self.view.wait()
        self.frame()
        _ = external_call["usleep", Int32](UInt32(10_000))

    def rows(self) -> List[String]:
        var labels = List[String]()
        for row in self.view.windows.session.controller.data.rows[].rows:
            labels.append(row.label.copy())
        return labels^

    def focused(self) -> Optional[UInt64]:
        for session in self.view.model.snapshot.sessions:
            if Optional[UInt64](session.id) == self.view.model.attached_session:
                return self.view.model.snapshot.focused_window_for(session)
        return None

    def window(self, name: String) raises -> UInt64:
        for session in self.view.model.snapshot.sessions:
            for window in session.windows:
                if window.name == name:
                    return window.id
        raise Error(String("no window named ", name))


def main() raises:
    var args = argv()
    var driver = Driver(String(args[1]))
    for _ in range(300):
        driver.settle()
        if driver.view.model.attached_session and driver.view.model.pane:
            break
    assert_true(Bool(driver.view.model.attached_session))
    var second = driver.window("second")
    driver.view.backend.palette_command(
        nav_command("choose-tree", [String("-w")]),
        driver.view.focus_epoch,
        (driver.view.model.source_server, driver.view.model.source_client),
    )
    for _ in range(300):
        driver.settle()
        if driver.view.windows.opened() and len(driver.rows()) > 0:
            break
    assert_true(driver.view.windows.opened())
    assert_false(driver.view.chooser.opened())
    assert_equal(driver.ui.focus, driver.view.windows.component.field.key)
    var collapsed = len(driver.rows())
    var third = -1
    var labels = driver.rows()
    for index in range(len(labels)):
        if labels[index] == "third":
            third = index
    assert_true(third >= 0)
    assert_equal(
        driver.view.windows.session.controller.data.rows[]
        .rows[third]
        .expanded,
        Optional[Bool](False),
    )
    driver.settle()
    var disclosure = driver.ui.rect_of(
        driver.view.windows.component.disclosure_key + UInt64(third)
    )
    assert_true(disclosure.x1 > disclosure.x0)
    var x = (disclosure.x0 + disclosure.x1) / 2
    var y = (disclosure.y0 + disclosure.y1) / 2
    _ = driver.native.record_pointer(
        MouseEvent(MOUSE_DOWN, x, y, 1, driver.ui.time, 0)
    )
    driver.frame()
    _ = driver.native.record_pointer(
        MouseEvent(MOUSE_UP, x, y, 1, driver.ui.time, 0)
    )
    driver.frame()
    for _ in range(300):
        driver.settle()
        if len(driver.rows()) == collapsed + 2:
            break
    assert_equal(len(driver.rows()), collapsed + 2)
    assert_equal(
        driver.view.windows.session.controller.data.rows[]
        .rows[third]
        .expanded,
        Optional[Bool](True),
    )
    assert_equal(
        driver.view.windows.session.controller.data.selected,
        Optional[Int](third),
    )

    driver.native.insert("sec")
    driver.frame()
    for _ in range(300):
        driver.settle()
        if driver.view.windows.session.controller.data.query == "sec":
            break
    assert_equal(driver.view.windows.session.controller.data.query, "sec")
    assert_equal(driver.view.windows.component.field.text(), "sec")
    driver.settle()
    var field = driver.ui.rect_of(driver.view.windows.component.field.key)
    assert_true(field.x1 > field.x0)
    var end_x = field.x1 - 2
    var end_y = (field.y0 + field.y1) / 2
    _ = driver.native.record_primary(
        MouseEvent(MOUSE_DOWN, end_x, end_y, 1, driver.ui.time, 0)
    )
    _ = driver.native.record_middle(
        MouseEvent(MOUSE_MIDDLE_UP, end_x, end_y, 1, driver.ui.time, 0)
    )
    driver.frame()
    for _ in range(300):
        driver.settle()
        if driver.view.windows.session.controller.data.query == "second":
            break
    assert_equal(driver.view.windows.session.controller.data.query, "second")
    assert_equal(driver.view.windows.component.field.text(), "second")

    var enter = KeyEvent(
        Key.ENTER, Key.ENTER, 0, 13, 13, 0, 0, False, False, True, False, 0
    )
    driver.timeline.raw_key(driver.view, enter, driver.native)
    driver.native.command(CMD_NEWLINE)
    driver.frame()
    for _ in range(300):
        driver.settle()
        if (
            not driver.view.windows.opened()
            and not driver.view.model.dialogs.chooser
            and driver.focused() == Optional[UInt64](second)
        ):
            break
    assert_false(driver.view.windows.opened())
    assert_false(Bool(driver.view.model.dialogs.chooser))
    assert_equal(driver.focused(), Optional[UInt64](second))
    assert_equal(driver.ui.focus, TERMINAL_KEY)
    assert_equal(driver.view.model.error, String())
    print("Windows chooser workspace checks passed")
