from std.sys import argv
from std.os import getenv
from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.input import (
    TextInput,
    KeyEvent,
    MouseEvent,
    MOUSE_DOWN,
    MOUSE_UP,
    MOD_CTRL,
    MOD_ALT,
    MOD_SHIFT,
    CMD_PASTE,
    CMD_SELECT_ALL,
    CMD_NEWLINE,
    CMD_CANCEL,
    CMD_TAB,
    CMD_DOWN,
    CMD_BACKTAB,
    CMD_EXTEND,
)
from mjui.keys import Key
from mjui.resources import resource_path
from mjui.testing import frame_pixels
from mjui.png import encode_png
from mjui.sys.file import file_write
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.keyboard_controller import KeyboardController
from zzmojo.clipboard_controller import ClipboardController
from zzmojo.input_timeline import InputTimeline
from zzmojo.search_clipboard import SearchClipboard
from zzmojo.input_protocol import KeyInput
from zzmojo.exec_protocol import Invocation
from zzmojo.path_mailbox import PathDelivery, PATH_CLOSE
from zzmojo.path_protocol import PathMessage


def pause():
    _ = external_call["usleep", Int32](UInt32(10_000))


def frame(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: SearchClipboard,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    var input: TextInput = TextInput(),
) raises:
    ui.set_time(ui.time + 0.02)
    ui.begin_frame(1120, 720)
    _ = timeline.process(view, ui, host, keyboard, clipboard, input)
    view.build(ui, input)
    clipboard.deliver(view, ui, host)
    clipboard.tick(view, ui)
    ui.end_frame()
    ui.atlas.gpu_done(ui.atlas.frame)
    view.flush()


def settle(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: SearchClipboard,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
) raises:
    view.flush()
    view.next_poll = 0
    view.poll(ui.time)
    view.wait()
    frame(ui, view, host, keyboard, clipboard, timeline)
    pause()


def key(
    code: UInt16,
    modifiers: UInt32 = 0,
    release: Bool = False,
    position: Int32 = 0,
    start: Int32 = 0,
    end: Int32 = 0,
) -> KeyEvent:
    return KeyEvent(
        code=code,
        key=code,
        mods=modifiers,
        ch=UInt32(80 if code == Key.P else 0),
        unshifted=UInt32(112 if code == Key.P else 0),
        text_lo=start,
        text_hi=end,
        repeat=False,
        release=release,
        text=end > start,
        consumed=False,
        text_event=position,
    )


def physical(
    mut view: WorkspaceView,
    mut ui: Ui,
    mut keyboard: KeyboardController,
    mut timeline: InputTimeline,
    event: KeyEvent,
    input: TextInput,
) raises -> Bool:
    var before = ui.focus
    var cancel = (
        event.text_event >= 0
        and Int(event.text_event) < len(input.events)
        and input.events[Int(event.text_event)].kind == 1
        and input.events[Int(event.text_event)].cmd == CMD_CANCEL
    )
    var plan = view.plan_palette(ui, keyboard, event, Span(input.bytes), cancel)
    if plan:
        if plan.value().kind != 1:
            timeline.palette(plan.value(), event, before, ui.focus)
        return True
    if event.release:
        var released = keyboard.route(
            view.model,
            False,
            event,
            Span(input.bytes),
            view.focus_epoch,
            view.input_pane(),
        )
        if released:
            timeline.key(view, released.value(), event)
            return True
    return False


def open_palette(
    mut view: WorkspaceView,
    mut ui: Ui,
    mut host: SearchClipboard,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
) raises:
    var input = TextInput()
    input.insert("P")
    assert_true(
        physical(
            view,
            ui,
            keyboard,
            timeline,
            key(Key.P, MOD_CTRL | MOD_SHIFT, end=1),
            input,
        )
    )
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_true(view.palette.component.opened())
    assert_equal(ui.focus, view.palette.component.field_key)
    assert_equal(view.palette.component.field.text(), "")
    assert_true(
        physical(
            view,
            ui,
            keyboard,
            timeline,
            key(Key.P, MOD_CTRL | MOD_SHIFT, True),
            TextInput(),
        )
    )


def query(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: SearchClipboard,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    text: String,
) raises:
    var input = TextInput()
    input.command(CMD_SELECT_ALL)
    input.insert(text)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)


def ranked(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: SearchClipboard,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
) raises:
    for _ in range(300):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if (
            view.palette.files.files.service.listing
            and view.palette.files.files.service.listing.value().done
            and not view.palette.files.busy()
            and not view.palette.files.files.ranking.dirty
        ):
            break
    assert_true(Bool(view.palette.files.files.service.listing))
    assert_false(Bool(view.palette.files.files.service.listing.value().error))
    assert_false(view.palette.files.busy())
    assert_false(view.palette.files.files.ranking.dirty)
    assert_true(ui.native_scene_complete)


def main() raises:
    var args = argv()
    var ui = Ui(1)
    ui.sem_on = True
    ui.native_scene = True
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    ui.focus = TERMINAL_KEY
    var view = WorkspaceView(args[1])
    var host = SearchClipboard()
    var keyboard = KeyboardController()
    var clipboard = ClipboardController()
    var timeline = InputTimeline()
    view.wait()
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if view.model.pane and UInt64(0) in view.model.terminals:
            break
    assert_equal(view.model.pane, Optional[UInt64](1))
    var source = view.model.source_client
    var command = Invocation()
    command.name = "choose-path"
    for argument in [String("-t"), String("%0"), String("-c"), String(args[2])]:
        command.args.append(List[UInt8](argument.as_bytes()))
    view.backend.palette_command(
        command, view.focus_epoch, (view.model.source_server, source)
    )
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if view.palette.files.files.active:
            break
    assert_equal(view.model.error, "")
    assert_true(view.palette.files.files.active)
    assert_equal(
        view.palette.files.files.service.insert_pane, Optional[UInt64](0)
    )
    assert_equal(ui.focus, view.palette.component.field_key)
    ranked(ui, view, host, keyboard, clipboard, timeline)
    assert_true(len(view.palette.component.model.rows) >= 4)
    var retained_rows = len(view.palette.component.model.rows)
    view.backend.paths.append(PathDelivery(view.backend.path_source.value(), PathMessage(PATH_CLOSE)))
    frame(ui, view, host, keyboard, clipboard, timeline)
    assert_true(view.palette.opened())
    assert_equal(len(view.palette.component.model.rows), retained_rows)
    query(ui, view, host, keyboard, clipboard, timeline, "dir")
    ranked(ui, view, host, keyboard, clipboard, timeline)
    assert_equal(len(view.palette.component.model.rows), 2)
    assert_equal(view.palette.component.model.rows[0].label, "dir")
    assert_equal(view.palette.component.model.rows[1].label, "dir/nested")
    var input = TextInput()
    input.command(CMD_TAB)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(view.palette.component.field.text(), "dir/")
    ranked(ui, view, host, keyboard, clipboard, timeline)
    assert_equal(view.palette.component.model.rows[0].label, "nested")
    input = TextInput()
    input.command(CMD_BACKTAB)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    ranked(ui, view, host, keyboard, clipboard, timeline)
    assert_true(view.palette.component.field.text().endswith("/files/"))
    query(ui, view, host, keyboard, clipboard, timeline, "alp")
    ranked(ui, view, host, keyboard, clipboard, timeline)
    input = TextInput()
    query(ui, view, host, keyboard, clipboard, timeline, String("/", String("x") * 4096, "/"))
    assert_equal(view.palette.component.field.text(), "alp")
    assert_equal(view.palette.files.files.path.filter, "alp")
    input.command(CMD_NEWLINE, MOD_ALT)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_true(view.palette.opened())
    assert_equal(host.text, String(args[2], "/alpha"))
    assert_true(len(view.palette.notifications.entries) > 0)
    assert_true(ui.layer_is_open(view.palette.component.key + 4))
    assert_equal(view.model.pane, Optional[UInt64](1))
    input = TextInput()
    input.command(CMD_NEWLINE)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_false(view.palette.opened())
    assert_false(view.palette.files.files.active)
    assert_equal(view.model.pane, Optional[UInt64](1))
    assert_equal(view.model.source_client, source)
    assert_true(ui.layer_is_open(view.palette.component.key + 4))
    var request = view.palette.files.files.service.next_request
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    query(ui, view, host, keyboard, clipboard, timeline, "insert path")
    input = TextInput()
    input.command(CMD_NEWLINE)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    ranked(ui, view, host, keyboard, clipboard, timeline)
    assert_equal(
        view.palette.files.files.service.insert_pane, Optional[UInt64](1)
    )
    assert_true(view.palette.files.files.service.next_request > request)
    query(ui, view, host, keyboard, clipboard, timeline, "dir")
    ranked(ui, view, host, keyboard, clipboard, timeline)
    var bounds = ui.rect_of(view.palette.component.row_key)
    var x = bounds.x0 + 12
    var y = (bounds.y0 + bounds.y1) * 0.5
    ui.pointer_down(x, y, 1)
    frame(ui, view, host, keyboard, clipboard, timeline)
    ui.pointer_up(x, y)
    frame(ui, view, host, keyboard, clipboard, timeline)
    assert_equal(view.palette.component.field.text(), "dir/")
    ranked(ui, view, host, keyboard, clipboard, timeline)
    assert_equal(view.palette.component.model.rows[0].label, "nested")
    var pixels = frame_pixels(ui, view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[3])
    output.write_bytes(Span(png))
    view.close_palette(ui)
    assert_equal(view.model.error, "")
    print(
        "native files workspace: choose-path, native editor, directory"
        " keyboard/click, clipboard notice, captured paste and persistent"
        " requests passed"
    )
