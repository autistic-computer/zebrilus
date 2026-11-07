from std.sys import argv
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
    MOD_SHIFT,
    CMD_PASTE,
    CMD_SELECT_ALL,
    CMD_NEWLINE,
    CMD_CANCEL,
    CMD_TAB,
    CMD_DOWN,
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
    var host = SearchClipboard()
    var keyboard = KeyboardController()
    var clipboard = ClipboardController()
    var timeline = InputTimeline()
    view.wait()
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if (
            view.model.pane
            and view.model.pane.value() in view.model.terminals
            and "PALETTE_READY" in view.model.terminal
        ):
            break
    assert_equal(view.model.error, "")
    assert_true(view.backend.online)
    assert_true("PALETTE_READY" in view.model.terminal)
    var source_client = view.model.source_client
    var first_pane = view.model.pane.value()
    var input = TextInput()
    input.insert("a")
    input.insert("P")
    input.insert(":rename-window 'palette λ name'")
    assert_true(
        physical(
            view,
            ui,
            keyboard,
            timeline,
            key(Key.P, MOD_CTRL | MOD_SHIFT, position=1, start=1, end=2),
            input,
        )
    )
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(
        view.palette.component.field.text(), ":rename-window 'palette λ name'"
    )
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
    input = TextInput()
    input.command(CMD_NEWLINE)
    input.insert("b")
    host.text = "TAIL"
    input.command(CMD_PASTE)
    input.insert("c")
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_false(view.palette.opened())
    assert_equal(ui.focus, TERMINAL_KEY)
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if view.model.windows[0].name == "palette λ name":
            break
    assert_equal(view.model.windows[0].name, "palette λ name")
    assert_equal(view.model.source_client, source_client)
    assert_false(Bool(view.model.command_output.viewport))
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    var selected = view.palette.component.model.controller.selected
    input = TextInput()
    input.command(CMD_DOWN | CMD_EXTEND, MOD_SHIFT)
    var arrow = key(Key.DOWN, MOD_SHIFT)
    arrow.text = True
    assert_true(physical(view, ui, keyboard, timeline, arrow, input))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(selected, Optional[Int](1))
    assert_equal(view.palette.component.model.rows[2].action, UInt8(0))
    assert_equal(
        view.palette.component.model.controller.selected, Optional[Int](3)
    )
    assert_true(
        physical(
            view,
            ui,
            keyboard,
            timeline,
            key(Key.DOWN, MOD_SHIFT, True),
            TextInput(),
        )
    )
    input = TextInput()
    input.command(CMD_SELECT_ALL)
    input.insert(
        ":if-shell -F 1 \"rename-window 'daemon parser'\"", composed=True
    )
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_true(view.palette.opened())
    assert_equal(view.model.windows[0].name, "palette λ name")
    input = TextInput()
    input.command(CMD_NEWLINE)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if view.model.windows[0].name == "daemon parser":
            break
    assert_equal(view.model.windows[0].name, "daemon parser")
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    input = TextInput()
    input.insert("preserve selection")
    input.command(CMD_SELECT_ALL)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    host.primary_supported = True
    host.primary_text = String("x") * (64 * 1024 + 1)
    input = TextInput()
    input.primary = MouseEvent(MOUSE_DOWN, 0, 0, 1, ui.time, 0)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(view.palette.component.field.text(), "preserve selection")
    assert_equal(view.palette.component.field.selection(), (0, 18))
    host.text = String("x") * (6 * 1024 * 1024)
    input = TextInput()
    input.command(CMD_PASTE)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(view.palette.component.field.text(), "preserve selection")
    input = TextInput()
    input.command(CMD_CANCEL)
    input.insert("d")
    input.command(CMD_NEWLINE)
    var escape = key(Key.ESCAPE)
    escape.text = True
    assert_true(physical(view, ui, keyboard, timeline, escape, input))
    assert_equal(ui.focus, TERMINAL_KEY)
    var enter = key(Key.ENTER, position=2, start=1, end=1)
    enter.text = True
    var routed = keyboard.route(
        view.model,
        True,
        enter,
        Span(input.bytes),
        view.focus_epoch,
        view.input_pane(),
    )
    assert_true(Bool(routed))
    timeline.key(view, routed.value(), enter)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_false(view.palette.opened())
    assert_equal(ui.focus, TERMINAL_KEY)
    assert_true(
        physical(
            view,
            ui,
            keyboard,
            timeline,
            key(Key.ESCAPE, release=True),
            TextInput(),
        )
    )
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    var chosen = ui.rect_of(view.palette.component.row_key + 1)
    var click_x = (chosen.x0 + chosen.x1) * 0.5
    var click_y = (chosen.y0 + chosen.y1) * 0.5
    ui.pointer_down(click_x, click_y, 1)
    frame(ui, view, host, keyboard, clipboard, timeline)
    ui.pointer_up(click_x, click_y)
    input = TextInput()
    input.insert("e")
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_false(view.palette.opened())
    assert_equal(ui.focus, TERMINAL_KEY)
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    input = TextInput()
    input.insert(":split-window -h")
    input.command(CMD_NEWLINE)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if len(view.model.panes) == 2:
            break
    assert_equal(len(view.model.panes), 2)
    assert_false(Bool(view.model.command_output.viewport))
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    var pixels = frame_pixels(ui, view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[2])
    output.write_bytes(Span(png))
    view.close_palette(ui)
    var parked_pane = view.model.pane.value()
    assert_true(parked_pane != first_pane)
    var command = Invocation()
    command.name = "run-shell"
    var output_args: List[String] = [
        "-t",
        "%" + String(first_pane),
        "printf '%s' 'PALETTE_OUTPUT_READY'",
    ]
    for argument in output_args:
        command.args.append(List[UInt8](argument.as_bytes()))
    view.backend.palette_command(
        command,
        view.focus_epoch,
        (view.model.source_server, view.model.source_client),
    )
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if (
            view.output.id
            and view.output_input
            and view.model.command_output.viewport
        ):
            break
    assert_true(view.output_input)
    var output_owner = view.output.id
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    assert_true(view.output_input)
    input = TextInput()
    input.command(CMD_CANCEL)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_true(view.output_input)
    assert_equal(view.output.id, output_owner)
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    var place_row = -1
    for index in range(len(view.palette.component.model.rows)):
        ref row = view.palette.component.model.rows[index]
        if row.action == 1 and row.place.session and not row.place.window:
            place_row = index
            break
    assert_true(place_row >= 0)
    var place_bounds = ui.rect_of(
        view.palette.component.row_key + UInt64(place_row)
    )
    var place_x = (place_bounds.x0 + place_bounds.x1) * 0.5
    var place_y = (place_bounds.y0 + place_bounds.y1) * 0.5
    ui.pointer_down(place_x, place_y, 1)
    frame(ui, view, host, keyboard, clipboard, timeline)
    ui.pointer_up(place_x, place_y)
    frame(ui, view, host, keyboard, clipboard, timeline)
    assert_false(view.palette.opened())
    assert_false(view.output_input)
    assert_equal(view.input_pane(), Optional[UInt64](parked_pane))
    assert_equal(view.output.id, output_owner)
    var surfaces = view.pointer_surfaces(ui)
    var clicked = False
    for surface in surfaces:
        if surface.pane != parked_pane:
            continue
        var x = surface.grid.left + surface.grid.cell_width * 2
        var y = surface.grid.top + surface.grid.line_height * 2
        ui.pointer_down(x, y, 1)
        view.mouse(ui, MouseEvent(MOUSE_DOWN, x, y, 1, ui.time, 0))
        frame(ui, view, host, keyboard, clipboard, timeline)
        ui.pointer_up(x, y)
        view.mouse(ui, MouseEvent(MOUSE_UP, x, y, 1, ui.time, 0))
        frame(ui, view, host, keyboard, clipboard, timeline)
        clicked = True
        break
    assert_true(clicked)
    assert_false(view.output_input)
    assert_equal(view.model.pane, Optional[UInt64](parked_pane))
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    input = TextInput()
    input.command(CMD_CANCEL)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_false(view.output_input)
    assert_equal(view.output.id, output_owner)
    input = TextInput()
    var find = key(Key.F, MOD_CTRL | MOD_SHIFT)
    find.ch = 70
    find.unshifted = 102
    var before_search = ui.focus
    var search = view.plan_search(ui, keyboard, find, Span(input.bytes))
    assert_true(Bool(search))
    timeline.search(search.value(), find, before_search, ui.focus)
    input.insert("NO_PALETTE_SEARCH_EXPECTED")
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        var status = view.model.terminals[parked_pane].metadata.search
        if status and status.value().total == 0 and not status.value().pending:
            break
    assert_equal(
        view.model.terminals[parked_pane].metadata.search.value().total,
        UInt32(0),
    )
    input = TextInput()
    input.command(CMD_CANCEL)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(ui.focus, TERMINAL_KEY)
    assert_false(view.output_input)
    for _ in range(10):
        settle(ui, view, host, keyboard, clipboard, timeline)
    assert_equal(view.model.error, "")
    print(
        "palette workspace: native shortcut, ordered PTY suffix, query/IME,"
        " bounded PRIMARY/paste, navigation and real daemon mutations passed"
    )
