from zzmojo.app_resume import ResumeStartup
from std.os import getenv
from std.sys import argv
from std.sys.info import CompilationTarget
from std.memory import OwnedPointer
from mjui.app import App, Runner, run_app, default_event
from mjui.windowing import AppOptions, WindowOptions
from mjui.host import Window
from mjui.ui import Ui
from mjui.input import KeyEvent, AppEvent, APP_ACTIVE, APP_INACTIVE
from mjui.resources import resource_path
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.readiness import Readiness, stream_ready
from zzmojo.keyboard_controller import KeyboardController
from zzmojo.clipboard_controller import ClipboardController
from zzmojo.input_timeline import InputTimeline


struct Workspace(App, Movable):
    var view: WorkspaceView
    var source: OwnedPointer[Readiness]
    var keyboard: KeyboardController
    var pointer_mods: UInt32
    var clipboard: ClipboardController
    var timeline: InputTimeline

    def __init__(
        out self, path: String, var startup: Optional[ResumeStartup] = None
    ) raises:
        self.view = WorkspaceView(path, startup^)
        self.source = OwnedPointer(Readiness())
        self.view.event_driven = CompilationTarget.is_linux()
        self.keyboard = KeyboardController()
        self.pointer_mods = 0
        self.clipboard = ClipboardController()
        self.timeline = InputTimeline()

    def wake(mut self, mut runner: Runner) -> Bool:
        if not self.source[].ready:
            return False
        self.source[].ready = False
        self.view.next_poll = 0
        return True

    def setup(mut self, mut ui: Ui, win: UInt32) raises:
        _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
        _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
        _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
        _ = ui.add_font(resource_path("fonts/lucide.ttf"))
        ui.focus = TERMINAL_KEY

    def build(mut self, mut ui: Ui, mut win: Window):
        var input = win.take_text()
        try:
            comptime if CompilationTarget.is_linux():
                self.source[].cancel()
            if self.timeline.process(
                self.view, ui, win, self.keyboard, self.clipboard, input
            ):
                win.text.unmark()
            self.view.build(ui, input)
            self.clipboard.deliver(self.view, ui, win)
            self.clipboard.tick(self.view, ui)
            var search = self.view.search.target_for_field(ui.focus)
            var search_input = Bool(search) and ui.text_input
            var text_key = ui.focus if search_input else self.view.input_key()
            win.set_text_input(
                search_input or (ui.focus == TERMINAL_KEY and Bool(self.view.input_pane())),
                text_key,
            )
            comptime if CompilationTarget.is_linux():
                var fd = self.view.backend.ready_fd()
                if fd >= 0:
                    ref queue = win.runner().sdl().host[].queue
                    self.source[].handle = queue.handle
                    self.source[].token = queue.watch_fd(
                        fd,
                        stream_ready,
                        self.source.ptr().unsafe_bitcast[NoneType](),
                    )
        except error:
            self.view.model.error = String(error)

    def on_event(mut self, mut runner: Runner, event: AppEvent) -> Bool:
        if event.kind == APP_INACTIVE:
            self.keyboard.reset()
            try:
                self.view.search.reset_keys()
                self.view.set_active(False)
            except error:
                self.view.model.error = String(error)
            self.view.pointer.reset()
            return True
        if event.kind == APP_ACTIVE:
            try:
                self.view.set_active(True)
            except error:
                self.view.model.error = String(error)
            return True
        return default_event(runner, event)

    def tick(mut self, mut ui: Ui, mut win: Window):
        try:
            for event in win.mouse:
                self.view.mouse(ui, event)
            for event in win.scrolls:
                self.view.scroll(ui, event, win.mouse_x, win.mouse_y)
            if self.pointer_mods != ui.mods:
                self.pointer_mods = ui.mods
                self.view.modifiers(ui)
        except error:
            self.view.model.error = String(error)

    def on_key(mut self, mut ui: Ui, mut win: Window, key: KeyEvent) -> Bool:
        try:
            var before_focus = ui.focus
            var search = self.view.plan_search(ui, self.keyboard, key, Span(win.text.bytes))
            if search:
                if search.value().kind != 2:
                    self.timeline.search(search.value(), key, before_focus, ui.focus)
                if search.value().kind == 0:
                    win.text.unmark()
                return True
            var clipboard = self.clipboard.plan(
                self.view,
                ui,
                self.keyboard,
                key,
                Span(win.text.bytes),
                composed=len(win.text.marked) > 0,
            )
            if clipboard:
                self.timeline.clipboard(clipboard.value(), key)
                ui.request_wake(ui.time)
                return True
            var routed = self.keyboard.route(
                self.view.model,
                ui.focus == TERMINAL_KEY and self.view.active,
                key,
                Span(win.text.bytes),
                self.view.focus_epoch,
                self.view.input_pane(),
            )
            if routed:
                ref event = routed.value()
                self.timeline.key(self.view, event, key)
                ui.request_wake(ui.time)
            if self.keyboard.claimed:
                return True
        except error:
            self.view.model.error = String(error)
            return True
        return ui.key(key)


def main() raises:
    var args = argv()
    var path = getenv("ZZ_SOCKET", "")
    var frames = 0
    var offset = 1
    var descriptors: Optional[Tuple[Int32, Int32, Int32]] = None
    while offset < len(args):
        if args[offset] == "--socket" and offset + 1 < len(args):
            path = args[offset + 1]
            offset += 2
        elif args[offset] == "--resume-fds" and offset + 3 < len(args) and not descriptors:
            var socket = Int(args[offset + 1])
            var payload = Int(args[offset + 2])
            var reply = Int(args[offset + 3])
            if socket < 3 or socket > Int(Int32.MAX) or payload < 3 or payload > Int(Int32.MAX) or reply < 3 or reply > Int(Int32.MAX):
                raise Error("invalid application resume descriptors")
            descriptors = (Int32(socket), Int32(payload), Int32(reply))
            offset += 4
        elif args[offset] == "--frames" and offset + 1 < len(args):
            frames = Int(args[offset + 1])
            offset += 2
        else:
            raise Error("usage: zz-mojo-app --socket PATH [--frames N]")
    if path.byte_length() == 0:
        raise Error("set ZZ_SOCKET or pass --socket PATH")
    var startup: Optional[ResumeStartup] = None
    if descriptors:
        var socket, payload, reply = descriptors.value()
        startup = ResumeStartup(socket, payload, reply)
    var app = Workspace(path, startup^)
    run_app(
        app,
        AppOptions(frames=frames, quit_keys=False),
        WindowOptions("zz", 1120, 720, focusable=frames == 0),
    )
    if frames > 0:
        app.view.flush()
        if app.view.model.error.byte_length() > 0:
            raise Error(app.view.model.error)
        if len(app.view.model.sessions) == 0:
            raise Error("native workspace received no session snapshot")
