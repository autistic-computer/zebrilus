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
    TEXT_PRIMARY,
    CMD_PASTE,
    MOD_CTRL,
)
from mjui.keys import Key
from mjui.widgets.key import sub
from mjui.testing import frame_pixels
from mjui.png import encode_png
from mjui.resources import resource_path
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.keyboard_controller import KeyboardController
from zzmojo.clipboard_controller import ClipboardController
from zzmojo.input_timeline import InputTimeline
from zzmojo.exec_protocol import Invocation


struct ChooserHost(Clipboard, Movable):
    var log: String

    def __init__(out self, log: String):
        self.log = log

    def read_attempt(self) raises:
        var bytes: List[UInt8]
        with open(self.log, "r") as file:
            bytes = file.read_bytes()
        bytes.extend(String("read").as_bytes())
        with open(self.log, "w") as file:
            file.write_bytes(Span(bytes))

    def get_clipboard(self) -> String:
        try:
            self.read_attempt()
        except:
            pass
        return String()

    def get_clipboard_bounded(self, maximum: Int) raises -> String:
        _ = maximum
        self.read_attempt()
        return String()

    def set_clipboard(mut self, text: String):
        _ = text

    def supports_primary(self) -> Bool:
        return True

    def get_primary_selection(self) -> String:
        try:
            self.read_attempt()
        except:
            pass
        return String()

    def get_primary_selection_bounded(self, maximum: Int) raises -> String:
        _ = maximum
        self.read_attempt()
        return String()

    def set_primary_selection(mut self, text: String):
        _ = text


def pause():
    _ = external_call["usleep", Int32](UInt32(10_000))


@inline(.never)
def frame(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: ChooserHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    mut native: TextInput,
) raises:
    var input = native.take()
    for event in input.events:
        if event.kind == TEXT_POINTER:
            var pointer = input.pointers[Int(event.start)]
            if pointer.kind == MOUSE_DOWN:
                ui.pointer_down(pointer.x, pointer.y, pointer.clicks)
            elif pointer.kind == MOUSE_UP:
                ui.pointer_up(pointer.x, pointer.y)
        elif event.kind == TEXT_PRIMARY:
            var pointer = input.primaries[Int(event.start)]
            ui.pointer_middle(pointer.x, pointer.y)
    ui.set_time(ui.time + 0.02)
    ui.begin_frame(1120, 720)
    _ = timeline.process(view, ui, host, keyboard, clipboard, input)
    view.build(ui, input)
    clipboard.deliver(view, ui, host)
    clipboard.tick(view, ui)
    ui.end_frame()
    assert_true(ui.native_scene_complete)
    ui.atlas.gpu_done(ui.atlas.frame)
    view.flush()


@inline(.never)
def settle(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: ChooserHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    mut native: TextInput,
) raises:
    view.flush()
    view.next_poll = 0
    view.poll(ui.time)
    view.wait()
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    pause()


def key(
    code: UInt16,
    character: UInt32 = 0,
    position: Int32 = 0,
    lo: Int32 = 0,
    hi: Int32 = 0,
    release: Bool = False,
) -> KeyEvent:
    return KeyEvent(
        code,
        code,
        0,
        character,
        character,
        lo,
        hi,
        False,
        release,
        hi > lo,
        False,
        position,
    )


def command(mut view: WorkspaceView, name: String) raises:
    var invocation = Invocation()
    invocation.name = name
    view.backend.palette_command(
        invocation,
        view.focus_epoch,
        (view.model.source_server, view.model.source_client),
    )
    view.next_poll = 0


@inline(.never)
def wait_open(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: ChooserHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    mut native: TextInput,
    kind: UInt8,
) raises:
    for _ in range(150):
        settle(ui, view, host, keyboard, clipboard, timeline, native)
        if (
            view.chooser.opened()
            and view.chooser.session.snapshot.value().kind == kind
            and view.model.dialogs.chooser_plane
        ):
            return
    raise Error("native chooser did not open with retained presentation")


@inline(.never)
def wait_closed(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: ChooserHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    mut native: TextInput,
) raises:
    for _ in range(150):
        settle(ui, view, host, keyboard, clipboard, timeline, native)
        if not view.model.dialogs.chooser and not view.chooser.opened():
            assert_equal(ui.focus, TERMINAL_KEY)
            return
    raise Error("native chooser did not close")


@inline(.never)
def search_phase(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: ChooserHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    mut native: TextInput,
) raises:
    command(view, "choose-tree")
    wait_open(ui, view, host, keyboard, clipboard, timeline, native, 1)
    assert_equal(ui.caret_rect.x0, view.chooser.panel.x)
    assert_equal(ui.caret_rect.y0, view.chooser.panel.y)
    assert_equal(ui.caret_rect.x1, view.chooser.panel.x)
    native.insert("/")
    timeline.raw_key(view, key(47, 47, 0, 0, 1), native)
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    for _ in range(100):
        settle(ui, view, host, keyboard, clipboard, timeline, native)
        if view.model.dialogs.chooser.value().search:
            break
    assert_true(Bool(view.model.dialogs.chooser.value().search))
    assert_equal(
        view.model.dialogs.chooser.value().search.value().query, String()
    )
    var marked = String("😀")
    native.set_marked(
        marked.unsafe_ptr(), marked.byte_length(), 0, marked.byte_length()
    )
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_equal(
        view.chooser.session.input.marked_utf16_length(), Optional[Int](2)
    )
    native.insert("λ\n🦀", composed=True)
    native.unmark()
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    for _ in range(100):
        settle(ui, view, host, keyboard, clipboard, timeline, native)
        if view.model.dialogs.chooser.value().search.value().query == "λ🦀":
            break
    assert_equal(
        view.model.dialogs.chooser.value().search.value().query, String("λ🦀")
    )
    marked = "pending"
    native.set_marked(
        marked.unsafe_ptr(), marked.byte_length(), 0, marked.byte_length()
    )
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_true(Bool(view.chooser.session.input.marked))
    var scope = view.chooser.session.scope
    var oversized = String("x") * 65536
    native.set_marked(
        oversized.unsafe_ptr(),
        oversized.byte_length(),
        0,
        oversized.byte_length(),
    )
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_true(view.chooser.opened())
    assert_equal(view.chooser.session.scope, scope)
    assert_equal(view.chooser.session.input.marked, Optional[String](marked))
    native.insert(oversized)
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_true(view.chooser.opened())
    assert_equal(view.chooser.session.scope, scope)
    assert_equal(view.chooser.session.input.marked, Optional[String](marked))
    assert_equal(
        view.model.dialogs.chooser.value().search.value().query, String("λ🦀")
    )
    view.palette.notifications.clear()
    timeline.raw_key(view, key(Key.ESCAPE), native)
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    for _ in range(100):
        settle(ui, view, host, keyboard, clipboard, timeline, native)
        if not view.model.dialogs.chooser.value().search:
            break
    assert_false(Bool(view.model.dialogs.chooser.value().search))
    assert_false(Bool(view.chooser.session.input.marked))
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_false(Bool(view.chooser.session.input.marked))
    native.set_marked(
        marked.unsafe_ptr(), marked.byte_length(), 0, marked.byte_length()
    )
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_equal(view.chooser.session.input.marked, Optional[String](marked))
    native.insert("ignored", composed=True)
    native.unmark()
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    _ = native.record_primary(
        MouseEvent(
            MOUSE_DOWN,
            view.chooser.panel.x + 20,
            view.chooser.panel.y + 20,
            1,
            ui.time,
            0,
        )
    )
    native.command(CMD_PASTE)
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    timeline.raw_key(view, key(Key.ESCAPE), native)
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    wait_closed(ui, view, host, keyboard, clipboard, timeline, native)


@inline(.never)
def pointer_phase(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: ChooserHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    mut native: TextInput,
    directory: String,
) raises:
    var held = key(Key.X, 120)
    held.mods = MOD_CTRL
    var owner = view.model.pane.value()
    timeline.raw_key(view, held, native)
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    var recorded = String()
    var log = directory + "/pane" + String(owner)
    for _ in range(100):
        with open(log, "r") as file:
            recorded = file.read()
        if recorded.byte_length() > 0:
            break
        pause()
    assert_equal(recorded, String("\x18"))
    with open(log, "w") as file:
        file.write("")
    command(view, "choose-tree")
    wait_open(ui, view, host, keyboard, clipboard, timeline, native, 1)
    held.release = True
    timeline.raw_key(view, held, native)
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_equal(view.model.pane, Optional[UInt64](owner))
    assert_false(Key.X in keyboard.forwarded)
    var index = -1
    var items = view.chooser.session.snapshot.value().items
    for row in range(len(items[].values)):
        if (
            items[].values[row].target_kind == 2
            and items[].values[row].target == 0
        ):
            index = row
            break
    assert_true(index >= 0)
    var rect = ui.rect_of(sub(view.chooser.key, index + 64))
    assert_true(rect.key != 0)
    var x = (rect.x0 + rect.x1) / 2
    var y = (rect.y0 + rect.y1) / 2
    _ = native.record_pointer(MouseEvent(MOUSE_DOWN, x, y, 1, ui.time, 0))
    timeline.raw_key(view, key(Key.DOWN, position=1), native)
    _ = native.record_pointer(MouseEvent(MOUSE_UP, x, y, 1, ui.time, 0))
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    for _ in range(100):
        settle(ui, view, host, keyboard, clipboard, timeline, native)
        if view.model.dialogs.chooser.value().selected == UInt32(index + 1):
            break
    assert_equal(view.model.dialogs.chooser.value().selected, UInt32(index + 1))
    timeline.raw_key(view, key(Key.DOWN, release=True), native)
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_equal(view.model.dialogs.chooser.value().selected, UInt32(index + 1))
    _ = native.record_pointer(MouseEvent(MOUSE_DOWN, x, y, 2, ui.time, 0))
    _ = native.record_pointer(MouseEvent(MOUSE_UP, x, y, 2, ui.time, 0))
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    wait_closed(ui, view, host, keyboard, clipboard, timeline, native)
    assert_equal(view.model.pane, Optional[UInt64](0))


@inline(.never)
def clients_phase(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: ChooserHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    mut native: TextInput,
    image: String,
) raises:
    command(view, "choose-client")
    wait_open(ui, view, host, keyboard, clipboard, timeline, native, 2)
    assert_true(len(view.chooser.session.snapshot.value().items[].values) > 0)
    assert_equal(
        view.chooser.session.snapshot.value().items[].values[0].target_kind,
        UInt8(3),
    )
    var pixels = frame_pixels(ui, view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    with open(image, "w") as output:
        output.write_bytes(Span(png))
    var rect = ui.rect_of(sub(view.chooser.key, 2))
    var x = (rect.x0 + rect.x1) / 2
    var y = (rect.y0 + rect.y1) / 2
    _ = native.record_pointer(MouseEvent(MOUSE_DOWN, x, y, 1, ui.time, 0))
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    assert_true(view.chooser.opened())
    _ = native.record_pointer(MouseEvent(MOUSE_UP, x, y, 1, ui.time, 0))
    frame(ui, view, host, keyboard, clipboard, timeline, native)
    wait_closed(ui, view, host, keyboard, clipboard, timeline, native)


def main() raises:
    var args = argv()
    var ui = Ui(1)
    ui.native_scene = True
    ui.native_paths = True
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    var view = WorkspaceView(args[1], native_chooser=True)
    view.event_driven = True
    var host = ChooserHost(String(args[2]) + "/reads")
    var keyboard = KeyboardController()
    var clipboard = ClipboardController()
    var timeline = InputTimeline()
    var native = TextInput()
    ui.focus = TERMINAL_KEY
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline, native)
        if view.model.pane and len(view.model.terminals) >= 2:
            break
    assert_true(Bool(view.model.pane))
    assert_true(len(view.model.terminals) >= 2)
    search_phase(ui, view, host, keyboard, clipboard, timeline, native)
    pointer_phase(
        ui, view, host, keyboard, clipboard, timeline, native, String(args[2])
    )
    clients_phase(
        ui, view, host, keyboard, clipboard, timeline, native, String(args[3])
    )
    for name in ["pane0", "pane1", "reads"]:
        with open(String(args[2]) + "/" + name, "r") as file:
            assert_equal(file.read(), String(), name)
    print("Native Panes/Clients chooser workspace checks passed")
