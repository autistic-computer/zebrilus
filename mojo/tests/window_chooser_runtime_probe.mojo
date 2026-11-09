from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui, Style, MOTION_REDUCED
from mjui.input import (
    Clipboard,
    TextInput,
    TextEvent,
    MouseEvent,
    TEXT_COMMAND,
    CMD_PASTE,
    MOUSE_DOWN,
    MOUSE_MIDDLE_UP,
    MOUSE_MIDDLE_DRAGGED,
)
from mjui.widgets.penna import PennaTheme
from zzmojo.backend import AsyncBackend
from zzmojo.actions import MAX_PENDING_ACTIONS
from zzmojo.path_service import PathSource
from zzmojo.chooser_session import ChooserSource
from zzmojo.chooser_protocol import ChooseState, ChooseItem, ChooseEvent
from zzmojo.palette_dialog_state import PaletteDialogs
from zzmojo.window_chooser_runtime import WindowChooserRuntime


struct WindowHost(Clipboard, Movable):
    var text: String
    var primary: String

    def __init__(out self):
        self.text = String()
        self.primary = String()

    def get_clipboard(self) -> String:
        return self.text

    def get_clipboard_bounded(self, maximum: Int) raises -> String:
        if self.text.byte_length() > maximum:
            raise Error("isolated clipboard is over budget")
        return self.text

    def set_clipboard(mut self, text: String):
        self.text = text.copy()

    def supports_primary(self) -> Bool:
        return True

    def get_primary_selection(self) -> String:
        return self.primary


def main() raises:
    var ui = Ui(1)
    ui.motion = MOTION_REDUCED
    ui.native_scene = True
    ui.native_paths = True
    var regular = Int32(ui.add_font("bench/assets/fonts/Inter-Regular.ttf"))
    var bold = Int32(ui.add_font("bench/assets/fonts/Inter-SemiBold.ttf"))
    var mono = Int32(ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf"))
    var icons = Int32(ui.add_font("bench/assets/fonts/lucide.ttf"))
    var theme = PennaTheme(4, regular, bold, mono, icons)
    var dialogs = PaletteDialogs()
    var state = ChooseState()
    var item = ChooseItem()
    item.label = "dev"
    item.flags = 2
    state.items.append(item^)
    var event = ChooseEvent()
    event.state = state^
    dialogs.apply_chooser(event^)
    var backend = AsyncBackend("/tmp/zz-unused-window-runtime", native_chooser=True)
    backend.online = True
    backend.path_source = PathSource(11, 12, 13)
    backend.source = (UInt64(11), UInt64(12))
    var source = ChooserSource(11, 12, 13)
    var runtime = WindowChooserRuntime()
    var input = TextInput()
    ui.set_time(1)
    ui.begin_frame(1000, 700)
    ui.open(Style.col().width(1000).height(700))
    ui.close()
    var observed = runtime.observe(ui, backend, dialogs, source, False, 0, input)
    assert_true(observed.owner_changed)
    assert_true(runtime.owns_input(ui))
    runtime.paint(ui, theme)
    ui.end_frame()
    assert_true(ui.native_scene_complete)
    var host = WindowHost()
    input.insert("dev")
    for _ in range(MAX_PENDING_ACTIONS - 1):
        backend.queue.append(10, "", None)
    var refused = False
    try:
        _ = runtime.event(ui, backend, input, input.events[0], host, 0)
    except:
        refused = True
    assert_true(refused)
    assert_equal(runtime.component.field.text(), "")
    assert_equal(len(runtime.component.field.undo), 0)
    assert_equal(runtime.session.controller.data.query, "")
    assert_false(Bool(runtime.session.controller.data.search.pending))
    _ = backend.queue.take()
    var result = runtime.event(ui, backend, input, input.events[0], host, 0)
    assert_true(result.consumed)
    assert_equal(runtime.component.field.text(), "dev")
    assert_equal(runtime.session.controller.data.query, "dev")
    assert_equal(len(backend.queue), 2)
    host.text = String("x") * 65536
    refused = False
    try:
        _ = runtime.event(ui, backend, input, TextEvent(TEXT_COMMAND, CMD_PASTE, 0, 0, 0), host, 0)
    except:
        refused = True
    assert_true(refused)
    assert_equal(runtime.component.field.text(), "dev")
    var composing = TextInput()
    var kana = String("か")
    composing.set_marked(kana.unsafe_ptr(), kana.byte_length(), 3, 3)
    assert_true(runtime.compose(ui, backend, composing, 0))
    assert_equal(runtime.session.controller.data.query, "devか")
    assert_equal(runtime.session.controller.data.cursor, 6)
    assert_equal(runtime.component.field.text(), "dev")
    var wide = String("x") * 65534
    composing.set_marked(wide.unsafe_ptr(), wide.byte_length(), 0, 0)
    assert_false(runtime.compose(ui, backend, composing, 0))
    assert_equal(runtime.session.controller.data.query, "devか")
    composing.unmark()
    composing.insert(kana.unsafe_ptr(), kana.byte_length(), composed=True)
    assert_true(
        runtime.ordered(ui, backend, composing, composing.events[0], host, 0)
    )
    assert_true(runtime.compose(ui, backend, composing, 0))
    assert_equal(runtime.component.field.text(), "devか")
    assert_equal(runtime.session.controller.data.query, "devか")
    assert_equal(runtime.session.controller.data.cursor, 6)
    ui.begin_frame(1000, 700)
    ui.open(Style.col().width(1000).height(700))
    ui.close()
    runtime.paint(ui, theme)
    ui.end_frame()
    host.primary = String("ab")
    var field = ui.rect_of(runtime.component.field.key)
    var middle = MouseEvent(
        MOUSE_DOWN, field.x1 - 1, (field.y0 + field.y1) / 2, 1, 2, 0
    )
    assert_true(runtime.primary(ui, backend, middle, host, 0))
    middle.kind = MOUSE_MIDDLE_DRAGGED
    assert_false(runtime.primary(ui, backend, middle, host, 0))
    middle.kind = MOUSE_MIDDLE_UP
    assert_false(runtime.primary(ui, backend, middle, host, 0))
    assert_equal(runtime.component.field.text(), "devかab")
    assert_equal(runtime.session.controller.data.query, "devかab")
    assert_equal(runtime.session.controller.data.cursor, 8)
    var count = len(runtime.component.field.undo)
    backend.path_source = PathSource(11, 12, 14)
    _ = runtime.event(ui, backend, input, input.events[0], host, 0)
    assert_equal(runtime.component.field.text(), "devかab")
    assert_equal(len(runtime.component.field.undo), count)
    runtime.reset(ui)
    assert_false(runtime.opened())
    print("Windows runtime source ownership, queue refusal, editor rollback, composition, ordered commit and one PRIMARY paste per middle click passed")
