from std.ffi import external_call
from std.sys import argv
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui
from mjui.input import TextInput, KeyEvent, CMD_SELECT_ALL, CMD_COPY, CMD_PASTE, CMD_CANCEL, CMD_NEWLINE, MOD_CTRL, MOD_SHIFT, MOD_ALT, MouseEvent, MOUSE_DOWN, MOUSE_UP
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
from zzmojo.terminal_search_controller import SearchTarget
from zzmojo.terminal_search_protocol import copy_mode_request, copy_request, CopyAction, search_request, SearchQuery
from zzmojo.protocol import Decoder
from zzmojo.terminal_model import TerminalViewport


def pause():
    _ = external_call["usleep", Int32](UInt32(10_000))


def frame(mut ui: Ui, mut view: WorkspaceView, mut keyboard: KeyboardController, mut clipboard: ClipboardController, mut timeline: InputTimeline, mut host: SearchClipboard, var input: TextInput = TextInput(), defer: Bool = False) raises:
    ui.set_time(ui.time + 0.02)
    ui.begin_frame(1120, 720)
    _ = timeline.process(view, ui, host, keyboard, clipboard, input)
    view.build(ui, input)
    clipboard.deliver(view, ui, host)
    clipboard.tick(view, ui)
    ui.end_frame()
    ui.atlas.gpu_done(ui.atlas.frame)
    if not defer:
        view.flush()


def settle(mut ui: Ui, mut view: WorkspaceView, mut keyboard: KeyboardController, mut clipboard: ClipboardController, mut timeline: InputTimeline, mut host: SearchClipboard) raises:
    view.flush()
    view.next_poll = 0
    view.poll(ui.time)
    view.wait()
    frame(ui, view, keyboard, clipboard, timeline, host)
    pause()


def key(code: UInt16, character: UInt32, mods: UInt32, position: Int32, start: Int32 = -1, end: Int32 = -1, release: Bool = False, text: Bool = False) -> KeyEvent:
    return KeyEvent(code=code, key=code, mods=mods, ch=character, unshifted=UInt32(code), text_lo=start, text_hi=end, repeat=False, release=release, text=text, consumed=False, text_event=position)


def search_key(mut ui: Ui, mut view: WorkspaceView, mut keyboard: KeyboardController, mut timeline: InputTimeline, input: TextInput, event: KeyEvent) raises:
    var focus = ui.focus
    var plan = view.plan_search(ui, keyboard, event, Span(input.bytes))
    assert_true(Bool(plan))
    if plan.value().kind != 2:
        timeline.search(plan.value(), event, focus, ui.focus)


def contains(viewport: TerminalViewport, text: String) -> Bool:
    var bytes = text.as_bytes()
    for start in range(max(0, len(viewport.cells) - len(bytes) + 1)):
        var equal = True
        for offset in range(len(bytes)):
            if viewport.cells[start + offset].glyph != UInt32(bytes[offset]):
                equal = False
                break
        if equal:
            return True
    return False


def open_output(mut view: WorkspaceView, text: String) raises:
    view.flush()
    var args: List[String] = ["-t", "%0", "printf '%s' '" + text + "'"]
    var response = view.backend.job[].connection.value().command("run-shell", Span(args))
    assert_equal(response.exit_code, UInt8(0))
    view.next_poll = 0


def main() raises:
    var args = argv()
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    ui.focus = TERMINAL_KEY
    var view = WorkspaceView(args[1])
    var keyboard = KeyboardController()
    var clipboard = ClipboardController()
    var timeline = InputTimeline()
    var host = SearchClipboard()
    view.wait()
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        if view.appearance.fonts_ready and UInt64(0) in view.model.terminals and contains(view.model.terminals[0], "UI-SEARCH-READY"):
            break
    assert_true(view.appearance.fonts_ready)
    assert_true(UInt64(0) in view.model.terminals)
    assert_true(contains(view.model.terminals[0], "UI-SEARCH-READY"))
    view.wait()
    var input = TextInput()
    input.insert("a")
    search_key(ui, view, keyboard, timeline, input, key(Key.F, 70, MOD_CTRL | MOD_SHIFT, 1))
    input.insert("nee")
    var start = Int32(len(input.bytes))
    input.insert("c", MOD_ALT)
    search_key(ui, view, keyboard, timeline, input, key(Key.C, 99, MOD_ALT, 2, start, Int32(len(input.bytes)), text=True))
    input.insert("dle")
    input.command(CMD_SELECT_ALL)
    input.command(CMD_COPY)
    input.command(CMD_PASTE)
    frame(ui, view, keyboard, clipboard, timeline, host, input^, True)
    var target = SearchTarget(0, False, 0)
    var name = target.name()
    var search_keys = view.search.keys[name]
    assert_equal(ui.focus, search_keys.field)
    assert_equal(view.search.editors[name].field.text(), String("needle"))
    assert_equal(view.search.editors[name].session.query.value().case_policy, UInt8(1))
    assert_equal(host.text, String("needle"))
    var frames = List[String]()
    var text_before = -1
    var begin_after = -1
    for index in range(len(view.backend.queue.items)):
        ref action = view.backend.queue.items[index]
        if action.text == "a":
            text_before = index
        if len(action.frame) < 10:
            continue
        var reader = Decoder(action.frame.copy())
        if reader.uint() != 9 or reader.uint() != 5 or reader.uint() != 0:
            continue
        var kind = reader.uint()
        if kind != 17 and kind != 18:
            continue
        if kind == 17:
            begin_after = index
        var text = reader.text()
        var mode = reader.uint()
        var case_policy = reader.uint()
        var direction = reader.uint()
        frames.append(String(kind, ":", text, ":", mode, ":", case_policy, ":", direction))
    var expected: List[String] = ["17::0:0:0", "18:nee:0:0:0", "18:nee:0:1:0", "18:needle:0:1:0", "18:needle:0:1:0"]
    assert_equal(frames, expected)
    assert_true(text_before >= 0 and begin_after > text_before)
    view.flush()
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        var status = view.model.terminals[0].metadata.search
        if status and status.value().total == 2 and not status.value().pending:
            break
    assert_equal(view.model.terminals[0].metadata.search.value().total, UInt32(2))
    assert_equal(view.model.error, String())
    assert_true(ui.rect_of(search_keys.panel).x1 > ui.rect_of(search_keys.panel).x0)
    var pixels = frame_pixels(ui, view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[2])
    output.write_bytes(Span(png))
    var button = ui.rect_of(search_keys.regex)
    var x = (button.x0 + button.x1) / 2
    var y = (button.y0 + button.y1) / 2
    ui.pointer_down(x, y, 1)
    view.mouse(ui, MouseEvent(MOUSE_DOWN, x, y, 1, ui.time, 0))
    frame(ui, view, keyboard, clipboard, timeline, host)
    ui.pointer_up(x, y)
    view.mouse(ui, MouseEvent(MOUSE_UP, x, y, 1, ui.time, 0))
    frame(ui, view, keyboard, clipboard, timeline, host)
    assert_equal(view.search.editors[name].session.query.value().mode, UInt8(1))
    assert_equal(ui.focus, search_keys.field)
    input = TextInput()
    input.command(CMD_SELECT_ALL)
    input.insert("[")
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        var status = view.model.terminals[0].metadata.search
        if status and status.value().invalid_pattern and not status.value().pending:
            break
    assert_true(view.model.terminals[0].metadata.search.value().invalid_pattern)
    input = TextInput()
    input.command(CMD_CANCEL)
    var closing = view.plan_search(ui, keyboard, key(Key.ESCAPE, 0, 0, 0, text=True), Span(input.bytes))
    assert_false(Bool(closing))
    input.insert("b")
    input.command(CMD_PASTE)
    input.insert("c")
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    assert_equal(ui.focus, TERMINAL_KEY)
    assert_false(Bool(view.search.editors[name].session.query))
    input = TextInput()
    input.command(CMD_CANCEL)
    search_key(ui, view, keyboard, timeline, input, key(Key.ESCAPE, 0, 0, 0, text=True))
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    search_key(ui, view, keyboard, timeline, TextInput(), key(Key.ESCAPE, 0, 0, 0, release=True))
    input = TextInput()
    input.command(CMD_NEWLINE)
    var enter = key(Key.ENTER, 0, 0, 0, text=True)
    var routed = keyboard.route(view.model, True, enter, Span(input.bytes), view.focus_epoch, view.input_pane())
    assert_true(Bool(routed))
    timeline.key(view, routed.value(), enter)
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        if contains(view.model.terminals[0], "INPUT:abneedlec"):
            break
    assert_true(contains(view.model.terminals[0], "INPUT:abneedlec"))
    assert_false(Bool(view.model.terminals[0].metadata.search))
    input = TextInput()
    search_key(ui, view, keyboard, timeline, input, key(Key.F, 70, MOD_CTRL | MOD_SHIFT, 0))
    input.command(CMD_CANCEL)
    closing = view.plan_search(ui, keyboard, key(Key.ESCAPE, 0, 0, 0, text=True), Span(input.bytes))
    assert_false(Bool(closing))
    search_key(ui, view, keyboard, timeline, input, key(Key.ESCAPE, 0, 0, 1, release=True))
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    assert_false(Bool(view.search.editors[name].session.swallowed))
    input = TextInput()
    input.command(CMD_CANCEL)
    var fresh_escape = key(Key.ESCAPE, 0, 0, 0, text=True)
    closing = view.plan_search(ui, keyboard, fresh_escape, Span(input.bytes))
    assert_false(Bool(closing))
    routed = keyboard.route(view.model, True, fresh_escape, Span(input.bytes), view.focus_epoch, view.input_pane())
    assert_true(Bool(routed))
    timeline.key(view, routed.value(), fresh_escape)
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    var released_escape = key(Key.ESCAPE, 0, 0, 0, release=True)
    input = TextInput()
    routed = keyboard.route(view.model, True, released_escape, Span(input.bytes), view.focus_epoch, view.input_pane())
    assert_true(Bool(routed))
    timeline.key(view, routed.value(), released_escape)
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    view.backend.input(0, copy_mode_request(0, False, False), view.focus_epoch)
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        if view.model.terminals[0].metadata.mode.kind == 1:
            break
    assert_equal(view.model.terminals[0].metadata.mode.kind, UInt8(1))
    input = TextInput()
    input.insert("/")
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        if view.search.editors[name].session.query:
            break
    assert_true(Bool(view.search.editors[name].session.query))
    assert_equal(view.search.editors[name].session.behavior, UInt8(1))
    assert_equal(ui.focus, search_keys.field)
    input = TextInput()
    input.insert("Gamma needle")
    input.command(CMD_NEWLINE)
    closing = view.plan_search(ui, keyboard, key(Key.KP_ENTER, 0, 0, 1, text=True), Span(input.bytes))
    assert_false(Bool(closing))
    search_key(ui, view, keyboard, timeline, input, key(Key.KP_ENTER, 0, 0, 2, release=True))
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    assert_false(Bool(view.search.editors[name].session.query))
    assert_false(Bool(view.search.editors[name].session.swallowed))
    assert_equal(ui.focus, TERMINAL_KEY)
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        var status = view.model.terminals[0].metadata.search
        if status and status.value().total == 1 and not status.value().pending:
            break
    assert_equal(view.model.terminals[0].metadata.search.value().total, UInt32(1))
    view.backend.input(0, search_request(0, 21), view.focus_epoch)
    view.backend.input(0, copy_request(0, CopyAction(38)), view.focus_epoch)
    for _ in range(20):
        settle(ui, view, keyboard, clipboard, timeline, host)
    open_output(view, "FIRST needle\nOUTPUT tail\n")
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        if view.output.id and view.model.command_output.viewport and view.output_input:
            break
    assert_true(Bool(view.output.id))
    var first_id = view.output.id.value()
    var output_target = SearchTarget(0, True, first_id)
    input = TextInput()
    search_key(ui, view, keyboard, timeline, input, key(Key.F, 70, MOD_CTRL | MOD_SHIFT, 0))
    input.insert("needle")
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        var status = view.model.command_output.viewport.value().metadata.search
        if status and status.value().total == 1 and not status.value().pending:
            break
    assert_equal(view.model.command_output.viewport.value().metadata.search.value().total, UInt32(1))
    assert_false(Bool(view.model.terminals[0].metadata.search))
    assert_true(view.output_input)
    assert_equal(ui.focus, view.search.keys[output_target.name()].field)
    input = TextInput()
    input.insert("c", MOD_ALT)
    search_key(ui, view, keyboard, timeline, input, key(Key.C, 99, MOD_ALT, 0, 0, 1, text=True))
    open_output(view, "SECOND needle\nOUTPUT replacement\n")
    frame(ui, view, keyboard, clipboard, timeline, host, input^)
    for _ in range(200):
        settle(ui, view, keyboard, clipboard, timeline, host)
        if view.output.id and view.output.id.value() != first_id:
            break
    for _ in range(10):
        settle(ui, view, keyboard, clipboard, timeline, host)
    assert_true(view.output.id.value() != first_id)
    assert_false(output_target.name() in view.search.editors)
    assert_false(Bool(view.model.command_output.viewport.value().metadata.search))
    assert_equal(ui.focus, TERMINAL_KEY)
    view.backend.input(0, search_request(0, 17, SearchQuery("stale")), view.focus_epoch, True, (view.model.source_server, view.model.source_client), UInt64.MAX)
    for _ in range(10):
        settle(ui, view, keyboard, clipboard, timeline, host)
    assert_false(Bool(view.model.terminals[0].metadata.search))
    assert_equal(view.model.error, String())
    ui.focus = 9999
    frame(ui, view, keyboard, clipboard, timeline, host)
    assert_equal(ui.focus, UInt64(9999))
    assert_false(Bool(view.model.command_output.viewport.value().metadata.search))
    print("live search workspace passed: ordered Find/text/Alt frames, exact daemon matches, query clipboard, toolbar click, invalid regex, post-close paste and raw Enter, swallowed Escape, daemon copy-mode prompt, accept-without-close, command-output search, replacement retirement, stale output owner and sidebar focus")
