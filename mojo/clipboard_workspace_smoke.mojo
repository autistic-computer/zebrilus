from std.os import getenv
from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true
from mjui.ui import Ui
from mjui.input import (
    TextInput,
    KeyEvent,
    MOD_CTRL,
    MOD_SHIFT,
    MOD_CMD,
    CMD_COPY,
    CMD_PASTE,
    CMD_SELECT_ALL,
)
from mjui.keys import Key
from mjui.sdl.window import Window
from mjui.sdl.clipboard import ClipboardData
from mjui.resources import resource_path
from mjui.testing import frame_pixels
from mjui.png import encode_png
from mjui.sys.file import file_write
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.keyboard_controller import KeyboardController
from zzmojo.clipboard_controller import ClipboardController
from zzmojo.input_timeline import InputTimeline
from zzmojo.clipboard_protocol import ClipboardResponse, clipboard_reply
from zzmojo.transport import connect
from zzmojo.actions import MAX_PENDING_INPUT_BYTES


def pause():
    _ = external_call["usleep", Int32](UInt32(10000))


def frame(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut clipboard: ClipboardController,
    mut host: Window,
    mut keyboard: KeyboardController,
    mut timeline: InputTimeline,
    var input: TextInput = TextInput(),
    consumed_prefix: Int = 0,
) raises:
    ui.set_time(ui.time + 0.02)
    ui.begin_frame(1120, 720)
    _ = timeline.process(
        view, ui, host, keyboard, clipboard, input, consumed_prefix
    )
    view.build(ui, input)
    clipboard.deliver(view, ui, host)
    clipboard.tick(view, ui)
    ui.end_frame()
    view.flush()


def settle(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut clipboard: ClipboardController,
    mut host: Window,
    mut keyboard: KeyboardController,
    mut timeline: InputTimeline,
) raises:
    view.flush()
    view.next_poll = 0
    view.poll(ui.time)
    view.wait()
    frame(ui, view, clipboard, host, keyboard, timeline)
    pause()


def shortcut(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut clipboard: ClipboardController,
    mut host: Window,
    mut keyboard: KeyboardController,
    mut timeline: InputTimeline,
    code: UInt16,
    character: UInt32,
    modifiers: UInt32,
) raises:
    var input = TextInput()
    input.command(
        CMD_PASTE if code
        == Key.V else (CMD_COPY if code == Key.C else CMD_SELECT_ALL),
        modifiers,
    )
    input.insert(String(chr(Int(character))))
    var key = KeyEvent(
        code=code,
        key=code,
        mods=modifiers,
        ch=character,
        unshifted=UInt32(code),
        text_lo=0,
        text_hi=Int32(len(input.bytes)),
        repeat=False,
        release=False,
        text=True,
        consumed=False,
        text_event=0,
    )
    var plan = clipboard.plan(view, ui, keyboard, key, Span(input.bytes))
    assert_true(Bool(plan))
    timeline.clipboard(plan.value(), key)
    frame(ui, view, clipboard, host, keyboard, timeline, input^)


def main() raises:
    assert_equal(getenv("SDL_VIDEODRIVER", ""), "dummy")
    var args = argv()
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    ui.focus = TERMINAL_KEY
    var view = WorkspaceView(args[1])
    view.wait()
    var host = Window.open("Private clipboard", 160, 100, gpu=False)
    var clipboard = ClipboardController()
    var keyboard = KeyboardController()
    var timeline = InputTimeline()
    for _ in range(200):
        settle(ui, view, clipboard, host[], keyboard, timeline)
        if "CLIPBOARD_MARKER_LINE" in view.model.terminal:
            break
    assert_true("CLIPBOARD_MARKER_LINE" in view.model.terminal)
    var pane = view.model.pane.value()
    view.backend.commit(
        pane, String("x") * MAX_PENDING_INPUT_BYTES, view.focus_epoch
    )
    var rejected = False
    try:
        view.backend.commit(pane, "x", view.focus_epoch)
    except:
        rejected = True
    assert_true(rejected)
    var discarded = view.backend.queue.take()
    assert_equal(len(discarded), 1)
    assert_equal(discarded[0].text.byte_length(), MAX_PENDING_INPUT_BYTES)
    host[].set_clipboard("PASTE_TWO")
    shortcut(
        ui,
        view,
        clipboard,
        host[],
        keyboard,
        timeline,
        Key.V,
        86,
        MOD_CTRL | MOD_SHIFT,
    )
    host[].set_clipboard("é")
    shortcut(
        ui, view, clipboard, host[], keyboard, timeline, Key.V, 118, MOD_CMD
    )
    var composed = TextInput()
    composed.insert("COMPOSED", composed=True)
    frame(ui, view, clipboard, host[], keyboard, timeline, composed^)
    var plain = TextInput()
    plain.insert("MULTI")
    frame(ui, view, clipboard, host[], keyboard, timeline, plain^)
    host[].set_clipboard("MENU_PASTE")
    var menu = TextInput()
    menu.command(CMD_PASTE)
    frame(ui, view, clipboard, host[], keyboard, timeline, menu^)
    host[].set_clipboard("IME_MENU")
    var marked = String("か")
    menu = TextInput()
    menu.set_marked(
        marked.unsafe_ptr(), marked.byte_length(), 0, marked.byte_length()
    )
    menu.command(CMD_PASTE)
    frame(ui, view, clipboard, host[], keyboard, timeline, menu^)
    host[].set_clipboard("ORDER_MENU")
    var interleaved = TextInput()
    interleaved.insert("a")
    interleaved.command(CMD_PASTE)
    interleaved.insert("b")
    frame(ui, view, clipboard, host[], keyboard, timeline, interleaved^)
    host[].set_clipboard("ORDER_KEY")
    interleaved = TextInput()
    interleaved.insert("c")
    interleaved.command(CMD_PASTE, MOD_CTRL | MOD_SHIFT)
    interleaved.insert("V")
    interleaved.insert("d")
    var paste_key = KeyEvent(
        code=Key.V,
        key=Key.V,
        mods=MOD_CTRL | MOD_SHIFT,
        ch=86,
        unshifted=118,
        text_lo=1,
        text_hi=2,
        repeat=False,
        release=False,
        text=True,
        consumed=False,
        text_event=1,
    )
    var paste_plan = clipboard.plan(
        view, ui, keyboard, paste_key, Span(interleaved.bytes)
    )
    assert_true(Bool(paste_plan))
    timeline.clipboard(paste_plan.value(), paste_key)
    frame(ui, view, clipboard, host[], keyboard, timeline, interleaved^)
    interleaved = TextInput()
    interleaved.insert("e")
    interleaved.insert("f")
    var enter = KeyEvent(
        code=Key.ENTER,
        key=Key.ENTER,
        mods=0,
        ch=0,
        unshifted=0,
        text_lo=1,
        text_hi=1,
        repeat=False,
        release=False,
        text=False,
        consumed=False,
        text_event=1,
    )
    var routed = keyboard.route(
        view.model,
        True,
        enter,
        Span(interleaved.bytes),
        view.focus_epoch,
        view.input_pane(),
    )
    assert_true(Bool(routed))
    timeline.key(view, routed.value(), enter)
    frame(ui, view, clipboard, host[], keyboard, timeline, interleaved^)
    host[].set_clipboard("ORDER_SUFFIX")
    interleaved = TextInput()
    interleaved.insert("DROP")
    interleaved.insert("g")
    interleaved.command(CMD_PASTE, MOD_CTRL | MOD_SHIFT)
    interleaved.insert("V")
    interleaved.insert("h")
    paste_key.text_event = 2
    paste_key.text_lo = 5
    paste_key.text_hi = 6
    paste_plan = clipboard.plan(
        view, ui, keyboard, paste_key, Span(interleaved.bytes)
    )
    assert_true(Bool(paste_plan))
    timeline.clipboard(paste_plan.value(), paste_key)
    _ = interleaved.events.pop(0)
    frame(
        ui,
        view,
        clipboard,
        host[],
        keyboard,
        timeline,
        interleaved^,
        consumed_prefix=1,
    )
    shortcut(
        ui,
        view,
        clipboard,
        host[],
        keyboard,
        timeline,
        Key.A,
        65,
        MOD_CTRL | MOD_SHIFT,
    )
    shortcut(
        ui,
        view,
        clipboard,
        host[],
        keyboard,
        timeline,
        Key.C,
        67,
        MOD_CTRL | MOD_SHIFT,
    )
    for _ in range(200):
        settle(ui, view, clipboard, host[], keyboard, timeline)
        if "CLIPBOARD_MARKER_LINE" in host[].get_clipboard_bounded(
            4 * 1024 * 1024
        ):
            break
    assert_true(
        "CLIPBOARD_MARKER_LINE" in host[].get_clipboard_bounded(4 * 1024 * 1024)
    )
    if view.model.terminals[pane].metadata.mode.kind != 0:
        view.backend.commit(pane, "q", view.focus_epoch)
        for _ in range(200):
            settle(ui, view, clipboard, host[], keyboard, timeline)
            if view.model.terminals[pane].metadata.mode.kind == 0:
                break
    assert_equal(view.model.terminals[pane].metadata.mode.kind, UInt8(0))
    host[].set_clipboard("QUERY_MARKER_世界")
    var arguments: List[String] = ["-l"]
    var response = (
        view.backend.job[]
        .connection.value()
        .command("refresh-client", Span(arguments))
    )
    assert_equal(response.exit_code, UInt8(0))
    var reference = connect(args[1])
    var empty = List[String]()
    var query_seen = False
    for _ in range(200):
        settle(ui, view, clipboard, host[], keyboard, timeline)
        var buffer = reference.command("show-buffer", Span(empty))
        if (
            buffer.exit_code == 0
            and String(from_utf8=Span(buffer.output)) == "QUERY_MARKER_世界"
        ):
            query_seen = True
            break
    assert_true(query_seen)
    response = (
        view.backend.job[]
        .connection.value()
        .command("refresh-client", Span(arguments))
    )
    assert_equal(response.exit_code, UInt8(0))
    var stale = String("STALE_QUERY_MUST_NOT_LAND")
    view.backend.reply_clipboard(
        Optional[ClipboardResponse](
            ClipboardResponse(
                view.model.source_server ^ 1,
                view.model.source_client,
                clipboard_reply(stale.as_bytes()),
            )
        )
    )
    view.flush()
    var buffer = reference.command("show-buffer", Span(empty))
    assert_equal(String(from_utf8=Span(buffer.output)), "QUERY_MARKER_世界")
    host[].set_clipboard("SECOND_QUERY_MARKER")
    for _ in range(200):
        settle(ui, view, clipboard, host[], keyboard, timeline)
        buffer = reference.command("show-buffer", Span(empty))
        if (
            buffer.exit_code == 0
            and String(from_utf8=Span(buffer.output)) == "SECOND_QUERY_MARKER"
        ):
            break
    assert_equal(String(from_utf8=Span(buffer.output)), "SECOND_QUERY_MARKER")
    arguments = ["-g", "set-clipboard", "on"]
    response = (
        view.backend.job[]
        .connection.value()
        .command("set-option", Span(arguments))
    )
    assert_equal(response.exit_code, UInt8(0))
    host[].set_clipboard("KEEP_CLIPBOARD")
    view.backend.commit(pane, "OSC52_TRIGGER", view.focus_epoch, composed=True)
    for _ in range(200):
        settle(ui, view, clipboard, host[], keyboard, timeline)
        if host[].get_primary_selection() == "APP_PRIMARY_MARKER":
            break
    assert_equal(host[].get_primary_selection(), "APP_PRIMARY_MARKER")
    assert_equal(host[].get_clipboard_bounded(100), "KEEP_CLIPBOARD")
    var image: List[UInt8]
    with open(args[2], "r") as source:
        image = source.read_bytes()
    var offer = ClipboardData(
        String("IMAGE_FALLBACK_MUST_NOT_TYPE"), String(), image^
    )
    assert_true(host[].set_clipboard_data(offer^))
    shortcut(
        ui,
        view,
        clipboard,
        host[],
        keyboard,
        timeline,
        Key.V,
        86,
        MOD_CTRL | MOD_SHIFT,
    )
    for _ in range(200):
        settle(ui, view, clipboard, host[], keyboard, timeline)
        if len(clipboard.uploads) == 0:
            break
    assert_equal(len(clipboard.uploads), 0)
    for _ in range(10):
        settle(ui, view, clipboard, host[], keyboard, timeline)
    assert_equal(view.model.error, String())
    var pixels = frame_pixels(ui, view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[3])
    output.write_bytes(Span(png))
    print(
        "native clipboard shortcuts, exact paste lanes, copy selection, query"
        " ownership, OSC52 producer and paced PNG upload passed"
    )
