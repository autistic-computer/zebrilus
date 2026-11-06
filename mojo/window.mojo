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


struct Workspace(App, Movable):
    var view: WorkspaceView
    var source: OwnedPointer[Readiness]
    var keyboard: KeyboardController
    var pointer_mods: UInt32

    def __init__(out self, path: String) raises:
        self.view = WorkspaceView(path)
        self.source = OwnedPointer(Readiness())
        self.view.event_driven = CompilationTarget.is_linux()
        self.keyboard = KeyboardController()
        self.pointer_mods = 0

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
        self.keyboard.filter_text(input)
        try:
            comptime if CompilationTarget.is_linux():
                self.source[].cancel()
            self.view.build(ui, input)
            var text_key = TERMINAL_KEY
            if self.view.model.pane:
                text_key = self.view.widget_key(4, self.view.model.pane.value())
            win.set_text_input(
                ui.focus == TERMINAL_KEY and Bool(self.view.model.pane),
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
                self.view.blur_pointer()
            except error:
                self.view.model.error = String(error)
            self.view.pointer.reset()
            self.view.active = False
            return True
        if event.kind == APP_ACTIVE:
            self.view.active = True
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
            var routed = self.keyboard.route(
                self.view.model,
                ui.focus == TERMINAL_KEY,
                key,
                Span(win.text.bytes),
                self.view.focus_epoch,
            )
            if routed:
                ref event = routed.value()
                self.view.forward_key(
                    event.pane, event.input, event.epoch, event.release
                )
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
    while offset < len(args):
        if args[offset] == "--socket" and offset + 1 < len(args):
            path = args[offset + 1]
            offset += 2
        elif args[offset] == "--frames" and offset + 1 < len(args):
            frames = Int(args[offset + 1])
            offset += 2
        else:
            raise Error("usage: zz-mojo-app --socket PATH [--frames N]")
    if path.byte_length() == 0:
        raise Error("set ZZ_SOCKET or pass --socket PATH")
    var app = Workspace(path)
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
