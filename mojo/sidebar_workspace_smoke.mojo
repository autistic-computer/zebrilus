from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.input import TextInput, KeyEvent, Clipboard, MOD_CTRL
from mjui.keys import Key
from mjui.testing import frame_pixels
from mjui.png import encode_png
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
    def frame(mut self) raises:
        var input = self.native.take()
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

    def key(mut self, code: UInt16, character: UInt32, mods: UInt32) raises:
        for release in [False, True]:
            var event = KeyEvent(
                code,
                code,
                mods,
                character,
                character,
                0,
                0,
                False,
                release,
                False,
                False,
                -1,
            )
            self.timeline.raw_key(self.view, event, self.native)
            self.frame()

    def click(mut self, key: UInt64) raises:
        var rect = self.ui.rect_of(key)
        assert_true(rect.x1 > rect.x0 and rect.y1 > rect.y0)
        var x = (rect.x0 + rect.x1) / 2
        var y = (rect.y0 + rect.y1) / 2
        self.ui.pointer_move(x, y)
        self.ui.pointer_down(x, y, 1)
        self.frame()
        self.ui.pointer_up(x, y)
        self.frame()

    def session(self, name: String) raises -> UInt64:
        for session in self.view.model.snapshot.sessions:
            if session.name == name:
                return session.id
        raise Error(String("no session named ", name))

    def window(self, name: String) raises -> UInt64:
        for session in self.view.model.snapshot.sessions:
            for window in session.windows:
                if window.name == name:
                    return window.id
        raise Error(String("no window named ", name))

    def windows(self, session_name: String) raises -> Int:
        for session in self.view.model.snapshot.sessions:
            if session.name == session_name:
                return len(session.windows)
        return 0

    def focused(self) -> Optional[UInt64]:
        for session in self.view.model.snapshot.sessions:
            if Optional[UInt64](session.id) == self.view.model.attached_session:
                return self.view.model.snapshot.focused_window_for(session)
        return None

    def active_pane(self) -> Optional[Tuple[UInt64, UInt8, Int]]:
        for session in self.view.model.snapshot.sessions:
            if Optional[UInt64](session.id) == self.view.model.attached_session:
                var focused = self.view.model.snapshot.focused_window_for(
                    session
                )
                for window in session.windows:
                    if window.id == focused:
                        try:
                            return (
                                window.active_pane,
                                window.panes[window.active_pane].kind.kind,
                                len(window.panes),
                            )
                        except:
                            return None
        return None

    def row_key(mut self, label: String) raises -> UInt64:
        var rows = self.view.sidebar.rows(
            0, self.view.sidebar.row_count(), False
        )
        for row in rows:
            if row.label == label:
                return self.view.name_key(
                    String(
                        "sidebar:",
                        row.node.host,
                        ":",
                        row.node.kind,
                        ":",
                        row.node.id,
                    )
                )
        raise Error(String("no sidebar row labelled ", label))


def main() raises:
    var args = argv()
    var driver = Driver(String(args[1]))
    for _ in range(300):
        driver.settle()
        if (
            driver.view.model.attached_session
            and len(driver.view.model.snapshot.sessions) >= 2
            and driver.view.sidebar.row_count() > 0
        ):
            break
    var alpha = driver.session("alpha")
    var beta = driver.session("beta")
    var first = driver.window("first")
    var second = driver.window("second")
    var third = driver.window("third")
    var rows = driver.view.sidebar.rows(
        0, driver.view.sidebar.row_count(), False
    )
    assert_equal(rows[0].node.kind, UInt8(0))
    assert_equal(rows[0].label, driver.view.sidebar.local_hostname)
    assert_equal(rows[1].label, String("alpha"))
    assert_equal(rows[1].depth, UInt8(1))
    var labels = List[String]()
    for row in rows:
        labels.append(row.label.copy())
    assert_true(String("beta") in labels)

    driver.key(Key.DIGIT_1, 49, MOD_CTRL)
    assert_equal(len(driver.view.chrome_held), 0)
    assert_false(Key.DIGIT_1 in driver.keyboard.forwarded)
    for _ in range(300):
        driver.settle()
        if driver.view.model.attached_session == Optional[UInt64](
            alpha
        ) and driver.focused() == Optional[UInt64](first):
            break
    assert_equal(driver.view.model.attached_session, Optional[UInt64](alpha))
    assert_equal(driver.focused(), Optional[UInt64](first))
    driver.key(Key.DIGIT_2, 50, MOD_CTRL)
    for _ in range(300):
        driver.settle()
        if driver.focused() == Optional[UInt64](second):
            break
    assert_equal(driver.focused(), Optional[UInt64](second))
    driver.key(Key.DIGIT_3, 51, MOD_CTRL)
    for _ in range(300):
        driver.settle()
        if driver.view.model.attached_session == Optional[UInt64](beta):
            break
    assert_equal(driver.view.model.attached_session, Optional[UInt64](beta))
    assert_equal(driver.focused(), Optional[UInt64](third))

    driver.click(driver.row_key("alpha"))
    for _ in range(300):
        driver.settle()
        if driver.view.model.attached_session == Optional[UInt64](alpha):
            break
    assert_equal(driver.view.model.attached_session, Optional[UInt64](alpha))
    assert_equal(driver.ui.focus, TERMINAL_KEY)
    driver.settle()
    driver.click(driver.row_key("first"))
    for _ in range(300):
        driver.settle()
        if driver.focused() == Optional[UInt64](first):
            break
    assert_equal(driver.focused(), Optional[UInt64](first))

    var before = driver.windows("alpha")
    driver.click(5)
    for _ in range(300):
        driver.settle()
        if driver.windows("alpha") == before + 1:
            break
    assert_equal(driver.windows("alpha"), before + 1)

    var panes = driver.active_pane().value()[2]
    driver.click(2)
    for _ in range(300):
        driver.settle()
        var active = driver.active_pane()
        if active and active.value()[1] == 0 and active.value()[2] == panes + 1:
            break
    var picker = driver.active_pane().value()
    assert_equal(picker[1], UInt8(0))
    assert_equal(picker[2], panes + 1)
    driver.settle()
    var choice = driver.ui.rect_of(
        driver.view.name_key(String("picker:", picker[0], ":", 0))
    )
    assert_true(choice.x1 > choice.x0)
    var pixels = frame_pixels(driver.ui, driver.view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    with open(String(args[2]), "w") as output:
        output.write_bytes(Span(png))
    driver.key(Key.T, 116, 0)
    for _ in range(300):
        driver.settle()
        var active = driver.active_pane()
        if active and active.value()[0] == picker[0] and active.value()[1] == 1:
            break
    assert_equal(driver.active_pane().value()[1], UInt8(1))
    assert_equal(driver.active_pane().value()[0], picker[0])

    driver.click(3)
    for _ in range(300):
        driver.settle()
        var active = driver.active_pane()
        if active and active.value()[1] == 0:
            break
    assert_equal(driver.active_pane().value()[1], UInt8(0))
    assert_equal(driver.active_pane().value()[2], panes + 2)
    driver.key(Key.B, 98, MOD_CTRL)
    for _ in range(300):
        driver.settle()
        if driver.view.model.routing.table:
            break
    assert_equal(driver.view.model.routing.table.value(), "prefix")
    driver.key(Key.ESCAPE, 27, 0)
    for _ in range(300):
        driver.settle()
        if not driver.view.model.routing.table:
            break
    assert_false(Bool(driver.view.model.routing.table))
    for _ in range(10):
        driver.settle()
    assert_equal(driver.active_pane().value()[1], UInt8(0))
    assert_equal(driver.active_pane().value()[2], panes + 2)
    driver.key(Key.ESCAPE, 0, 0)
    for _ in range(300):
        driver.settle()
        var active = driver.active_pane()
        if active and active.value()[2] == panes + 1:
            break
    assert_equal(driver.active_pane().value()[2], panes + 1)

    var sessions = len(driver.view.model.snapshot.sessions)
    driver.click(1)
    for _ in range(300):
        driver.settle()
        if len(driver.view.model.snapshot.sessions) == sessions + 1:
            break
    assert_equal(len(driver.view.model.snapshot.sessions), sessions + 1)
    assert_equal(driver.view.model.error, String())
    print("Sidebar workspace checks passed")
