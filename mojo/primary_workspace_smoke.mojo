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
    MOD_ALT,
    MOD_SHIFT,
    CMD_PASTE,
    CMD_CANCEL,
    Clipboard,
    TEXT_PRIMARY,
    TEXT_POINTER,
    MOUSE_MIDDLE_UP,
    MOUSE_MIDDLE_DRAGGED,
    MOUSE_EXITED,
    MOUSE_RIGHT_DOWN,
)
from mjui.keys import Key
from mjui.resources import resource_path
from mjui.testing import frame_pixels
from mjui.png import encode_png
from mjui.sys.file import file_write
from zzmojo.view import WorkspaceView, TERMINAL_KEY
from zzmojo.state import _selected_text
from zzmojo.actions import MAX_PENDING_INPUT_BYTES
from zzmojo.backend import RECONNECT, PASTE
from zzmojo.input_protocol import mjui_key_input
from zzmojo.keyboard_controller import KeyboardController
from zzmojo.clipboard_controller import ClipboardController
from zzmojo.input_timeline import InputTimeline
from zzmojo.exec_protocol import Invocation


struct PrimaryHost(Clipboard, Movable):
    var text: String
    var primary_text: String
    var log: String
    var allow_reads: Bool

    def __init__(out self, log: String):
        self.text = String("clipboard")
        self.primary_text = String("P")
        self.log = log
        self.allow_reads = True

    def record(self, symbol: String) raises:
        var bytes: List[UInt8]
        with open(self.log, "r") as file:
            bytes = file.read_bytes()
        bytes.extend(symbol.as_bytes())
        with open(self.log, "w") as file:
            file.write_bytes(Span(bytes))

    def get_clipboard(self) -> String:
        return self.text.copy()

    def get_clipboard_bounded(self, maximum: Int) raises -> String:
        self.record("c")
        if not self.allow_reads or self.text.byte_length() > maximum:
            raise Error("isolated clipboard read refused")
        return self.text.copy()

    def set_clipboard(mut self, text: String):
        self.text = text

    def supports_primary(self) -> Bool:
        return True

    def get_primary_selection(self) -> String:
        return self.primary_text.copy()

    def get_primary_selection_bounded(self, maximum: Int) raises -> String:
        self.record("p")
        if not self.allow_reads or self.primary_text.byte_length() > maximum:
            raise Error("isolated PRIMARY read refused")
        return self.primary_text.copy()

    def set_primary_selection(mut self, text: String):
        self.primary_text = text

    def clear_log(self) raises:
        with open(self.log, "w") as file:
            file.write("")

    def reads(self) raises -> String:
        with open(self.log, "r") as file:
            return file.read()


def feed(mut ui: Ui, input: TextInput):
    for event in input.events:
        if event.kind == TEXT_PRIMARY:
            var pointer = input.primaries[Int(event.start)]
            if pointer.kind == MOUSE_DOWN:
                ui.pointer_middle(pointer.x, pointer.y)
            elif pointer.kind == MOUSE_MIDDLE_UP:
                ui.pointer_middle_up(pointer.x, pointer.y)
            else:
                ui.pointer_move(pointer.x, pointer.y)
        elif event.kind == TEXT_POINTER:
            var pointer = input.pointers[Int(event.start)]
            if pointer.kind == MOUSE_DOWN:
                ui.pointer_down(pointer.x, pointer.y, pointer.clicks)
            elif pointer.kind == MOUSE_UP:
                ui.pointer_up(pointer.x, pointer.y)
            elif pointer.kind == MOUSE_EXITED:
                ui.pointer_leave()
            elif pointer.kind == MOUSE_RIGHT_DOWN:
                ui.pointer_secondary(pointer.x, pointer.y)
            else:
                ui.pointer_move(pointer.x, pointer.y)


def pause():
    _ = external_call["usleep", Int32](UInt32(10_000))


def frame(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: PrimaryHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    var input: TextInput = TextInput(),
) raises:
    ui.set_time(ui.time + 0.02)
    feed(ui, input)
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
    mut host: PrimaryHost,
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
    mut host: PrimaryHost,
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


def point(view: WorkspaceView, ui: Ui, pane: UInt64) raises -> MouseEvent:
    var surfaces = view.pointer_surfaces(ui)
    for surface in surfaces:
        if surface.pane == pane:
            return MouseEvent(
                MOUSE_DOWN,
                surface.grid.left + 2.5 * surface.grid.cell_width,
                surface.grid.top + 3.5 * surface.grid.line_height,
                1,
                ui.time,
                0,
            )
    raise Error("fixture pane has no painted terminal surface")


def up(event: MouseEvent) -> MouseEvent:
    var result = event
    result.kind = MOUSE_MIDDLE_UP
    return result


def raw_key(
    code: UInt16,
    character: UInt32,
    position: Int32,
    release: Bool = False,
    lo: Int32 = -1,
    hi: Int32 = -1,
) -> KeyEvent:
    return KeyEvent(
        code,
        code,
        MOD_CTRL,
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


def assert_pty(path: String, expected: String) raises:
    var bytes: List[UInt8]
    with open(path, "r") as file:
        bytes = file.read_bytes()
    assert_equal(bytes, List[UInt8](expected.as_bytes()))


@inline(.never)
def phase_ordered(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: PrimaryHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    directory: String,
    output_path: String,
    source: UInt64,
    zero: MouseEvent,
    one: MouseEvent,
    mut expected_zero: String,
    mut expected_one: String,
) raises:
    var input = TextInput()
    input.insert("a")
    assert_true(input.record_primary(zero))
    input.insert("b")
    assert_true(input.record_middle(up(zero)))
    input.insert("c")
    assert_true(input.record_primary(one))
    input.insert("d")
    assert_true(input.record_middle(up(one)))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    expected_zero = String("\x1b[200~P\x1b[201~bc")
    expected_one = String("a\x1b[200~P\x1b[201~d")
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_pty(String(directory, "/pane1"), expected_one)
    assert_equal(host.reads(), "pp")
    assert_equal(view.model.pane, Optional[UInt64](1))
    assert_true(not view.primary.pressed)
    host.clear_log()
    input = TextInput()
    input.insert("e")
    assert_true(input.record_primary(zero))
    input.insert("f")
    assert_true(input.record_middle(up(zero)))
    timeline.raw_key(view, raw_key(Key.X, 120, 1), input)
    timeline.raw_key(view, raw_key(Key.Y, 121, 2), input)
    timeline.raw_key(view, raw_key(Key.X, 120, 4, True), input)
    timeline.raw_key(view, raw_key(Key.Y, 121, 4, True), input)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    expected_zero += "\x1b[200~P\x1b[201~\x19f"
    expected_one += "e\x18"
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_pty(String(directory, "/pane1"), expected_one)
    assert_equal(host.reads(), "p")
    assert_equal(view.model.pane, Optional[UInt64](0))
    assert_equal(len(keyboard.forwarded), 0)
    host.clear_log()
    input = TextInput()
    assert_true(input.record_primary(zero))
    input.insert("x")
    assert_true(input.record_middle(up(zero)))
    timeline.raw_key(view, raw_key(Key.X, 120, 1, lo=0, hi=1), input)
    timeline.raw_key(view, raw_key(Key.X, 120, 3, True), input)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    expected_zero += "\x1b[200~P\x1b[201~\x18"
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_equal(host.reads(), "p")
    assert_equal(view.model.pane, Optional[UInt64](0))
    input = TextInput()
    var marked = String("draft")
    input.set_marked(marked.unsafe_ptr(), marked.byte_length(), 1, 3)
    assert_true(input.record_primary(zero))
    assert_true(input.record_middle(up(zero)))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(view.marked, Optional[String]("draft"))
    expected_zero += "\x1b[200~P\x1b[201~"
    input = TextInput()
    input.set_marked(marked.unsafe_ptr(), marked.byte_length(), 1, 3)
    assert_true(input.record_primary(one))
    assert_true(input.record_middle(up(one)))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_true(not view.marked)
    expected_one += "\x1b[200~P\x1b[201~"
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_pty(String(directory, "/pane1"), expected_one)


@inline(.never)
def phase_tracking_occlusion(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: PrimaryHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    directory: String,
    output_path: String,
    source: UInt64,
    zero: MouseEvent,
    one: MouseEvent,
    mut expected_zero: String,
    mut expected_one: String,
) raises:
    var input: TextInput
    host.clear_log()
    host.allow_reads = False
    view.backend.commit(0, "T", view.focus_epoch, True)
    view.flush()
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if view.model.terminals[0].metadata.mouse_tracking:
            break
    assert_true(view.model.terminals[0].metadata.mouse_tracking)
    var grids = view.pointer_surfaces(ui)
    var cell_width = Float32(0)
    for grid in grids:
        if grid.pane == 0:
            cell_width = grid.grid.cell_width
    assert_true(cell_width > 0)
    var motion = zero
    motion.kind = MOUSE_MIDDLE_DRAGGED
    motion.x += cell_width
    var outside = up(zero)
    outside.x -= 100
    input = TextInput()
    assert_true(input.record_primary(zero))
    assert_true(input.record_middle(motion))
    assert_true(input.record_middle(outside))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    expected_zero += "\x1b[<1;3;4M\x1b[<33;4;4M\x1b[<1;1;4m"
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_equal(host.reads(), "")
    assert_equal(view.model.pane, Optional[UInt64](0))
    assert_true(not view.primary.pressed)
    host.allow_reads = True
    host.primary_text = "S"
    var shifted = zero
    shifted.mods = MOD_SHIFT | MOD_ALT
    input = TextInput()
    assert_true(input.record_primary(shifted))
    assert_true(input.record_middle(up(shifted)))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    expected_zero += "\x1b[200~S\x1b[201~"
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_equal(host.reads(), "p")
    assert_true(not view.primary.pressed)
    view.backend.commit(0, "U", view.focus_epoch, True)
    view.flush()
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if not view.model.terminals[0].metadata.mouse_tracking:
            break
    assert_false(view.model.terminals[0].metadata.mouse_tracking)
    host.clear_log()
    host.allow_reads = False
    view.set_active(False)
    input = TextInput()
    _ = input.record_primary(zero)
    _ = input.record_middle(up(zero))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(host.reads(), "")
    view.set_active(True)
    view.palette.notify(ui, "retained hit ownership")
    view.palette.notify(ui, "PRIMARY stays occluded")
    for _ in range(16):
        frame(ui, view, host, keyboard, clipboard, timeline)
    var occluded: Optional[MouseEvent] = None
    for grid in view.pointer_surfaces(ui):
        if grid.pane == 0:
            for row in range(min(grid.grid.rows, 10)):
                var x = min(grid.grid.left + grid.grid.width - 8, Float32(560))
                var y = (
                    grid.grid.top + (Float32(row) + 0.5) * grid.grid.line_height
                )
                var hit = ui.hit_test(x, y)
                var grid_key = view.keys[String(4, ":", UInt64(0))]
                var pane_key = view.keys[String(5, ":", UInt64(0))]
                if hit != 0 and hit != grid_key and hit != pane_key:
                    occluded = MouseEvent(MOUSE_DOWN, x, y, 1, ui.time, 0)
                    break
    assert_true(Bool(occluded))
    input = TextInput()
    _ = input.record_primary(occluded.value())
    _ = input.record_middle(up(occluded.value()))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(host.reads(), "")
    view.palette.notifications.clear()
    frame(ui, view, host, keyboard, clipboard, timeline)


@inline(.never)
def phase_editors(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: PrimaryHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    directory: String,
    output_path: String,
    source: UInt64,
    zero: MouseEvent,
    one: MouseEvent,
    mut expected_zero: String,
    mut expected_one: String,
) raises:
    var input: TextInput
    var output_command = Invocation()
    output_command.name = "list-sessions"
    view.backend.palette_command(
        output_command,
        view.focus_epoch,
        (view.model.source_server, view.model.source_client),
    )
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if view.output.id and view.model.command_output.viewport:
            break
    assert_true(Bool(view.output.id))
    var output_owner = view.output.pane.value()
    var output_point = point(view, ui, output_owner)
    input = TextInput()
    _ = input.record_primary(output_point)
    _ = input.record_middle(up(output_point))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(host.reads(), "")
    assert_true(not view.primary.pressed)
    view.forward_key(
        output_owner,
        mjui_key_input(Key.C, 99, 99, MOD_CTRL, 0),
        command_output=True,
    )
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if not view.output.id:
            break
    assert_true(not view.output.id)
    host.allow_reads = True
    host.primary_text = "P"
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    input = TextInput()
    input.insert("X")
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    var field = ui.rect_of(view.palette.component.field_key)
    var field_point = MouseEvent(
        MOUSE_DOWN, field.x1 - 2, (field.y0 + field.y1) * 0.5, 1, ui.time, 0
    )
    assert_equal(
        ui.text_hit(
            view.palette.component.field_key, field_point.x, field_point.y
        ).byte,
        1,
    )
    host.clear_log()
    input = TextInput()
    input.insert("a")
    _ = input.record_primary(field_point)
    _ = input.record_middle(up(field_point))
    input.insert("b")
    _ = input.record_primary(field_point)
    _ = input.record_middle(up(field_point))
    input.insert("c")
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(view.palette.component.field.text(), "XPcPba")
    assert_equal(host.reads(), "pp")
    input = TextInput()
    input.command(CMD_CANCEL)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_false(view.palette.opened())
    input = TextInput()
    var find_key = raw_key(Key.F, 102, 0)
    find_key.mods |= MOD_SHIFT
    find_key.ch = 70
    timeline.raw_key(view, find_key, input)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    input = TextInput()
    find_key.release = True
    timeline.raw_key(view, find_key, input)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    var search_target = view.search.target_for_field(ui.focus)
    assert_true(Bool(search_target))
    var search_name = search_target.value().name()
    input = TextInput()
    input.insert("X")
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    field = ui.rect_of(view.search.keys[search_name].field)
    field_point = MouseEvent(
        MOUSE_DOWN, field.x1 - 2, (field.y0 + field.y1) * 0.5, 1, ui.time, 0
    )
    assert_equal(
        ui.text_hit(
            view.search.keys[search_name].field, field_point.x, field_point.y
        ).byte,
        1,
    )
    host.clear_log()
    input = TextInput()
    input.insert("a")
    _ = input.record_primary(field_point)
    _ = input.record_middle(up(field_point))
    input.insert("b")
    _ = input.record_primary(field_point)
    _ = input.record_middle(up(field_point))
    input.insert("c")
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(view.search.editors[search_name].field.text(), "XPcPba")
    assert_equal(host.reads(), "pp")
    host.clear_log()
    host.allow_reads = False
    input = TextInput()
    input.command(CMD_CANCEL)
    _ = input.record_primary(field_point)
    _ = input.record_middle(up(field_point))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(host.reads(), "")
    host.allow_reads = True
    assert_equal(ui.focus, TERMINAL_KEY)
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_pty(String(directory, "/pane1"), expected_one)


@inline(.never)
def phase_refusal(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: PrimaryHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    directory: String,
    output_path: String,
    source: UInt64,
    zero: MouseEvent,
    one: MouseEvent,
    mut expected_zero: String,
    mut expected_one: String,
) raises:
    var input: TextInput
    host.clear_log()
    host.primary_text = String()
    input = TextInput()
    assert_true(input.record_primary(one))
    assert_true(input.record_middle(up(one)))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(host.reads(), "p")
    assert_true(not view.primary.pressed)
    assert_true(not view.primary.pending)
    host.clear_log()
    host.primary_text = String("x") * (MAX_PENDING_INPUT_BYTES + 1)
    var next_notice = view.palette.notifications.next_id
    view.palette.notifications.next_id = UInt64.MAX
    input = TextInput()
    assert_true(input.record_primary(one))
    assert_true(input.record_middle(up(one)))
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(host.reads(), "p")
    assert_true(not view.primary.pressed)
    assert_true(not view.primary.pending)
    assert_equal(view.model.error, "notification identifier space exhausted")
    view.palette.notifications.next_id = next_notice
    view.model.error = ""
    host.primary_text = String("P")
    host.clear_log()
    host.allow_reads = False
    input = TextInput()
    _ = input.record_primary(one)
    input.pointer_overflow = True
    timeline.raw_key(view, raw_key(Key.X, 120, 0), input)
    ui.begin_frame(1120, 720)
    var refused = False
    try:
        _ = timeline.process(view, ui, host, keyboard, clipboard, input)
    except:
        refused = True
    ui.end_frame()
    ui.atlas.gpu_done(ui.atlas.frame)
    assert_true(refused)
    assert_equal(len(timeline.pending), 0)
    assert_equal(host.reads(), "")
    input = TextInput()
    _ = input.record_primary(one)
    input.events[0].start = -1
    input.events[0].end = -1
    timeline.raw_key(view, raw_key(Key.X, 120, 0), input)
    ui.begin_frame(1120, 720)
    refused = False
    try:
        _ = timeline.process(view, ui, host, keyboard, clipboard, input)
    except:
        refused = True
    ui.end_frame()
    ui.atlas.gpu_done(ui.atlas.frame)
    assert_true(refused)
    assert_equal(len(timeline.pending), 0)
    assert_equal(host.reads(), "")
    open_palette(view, ui, host, keyboard, clipboard, timeline)
    var field = ui.rect_of(view.palette.component.field_key)
    var field_point = MouseEvent(MOUSE_DOWN, field.x1 - 2, (field.y0 + field.y1) * 0.5, 1, ui.time, 0)
    input = TextInput()
    _ = input.record_primary(field_point)
    input.events[0].start = -1
    input.events[0].end = -1
    timeline.raw_key(view, raw_key(Key.X, 120, 0), input)
    ui.pointer_middle(field_point.x, field_point.y)
    ui.begin_frame(1120, 720)
    refused = False
    try:
        _ = timeline.process(view, ui, host, keyboard, clipboard, input)
    except:
        refused = True
    assert_true(refused)
    assert_false(Bool(input.primary))
    assert_equal(len(timeline.pending), 0)
    view.build(ui, input)
    ui.end_frame()
    ui.atlas.gpu_done(ui.atlas.frame)
    assert_equal(host.reads(), "")
    assert_equal(view.palette.component.field.text(), "")
    input = TextInput()
    input.command(CMD_CANCEL)
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_false(view.palette.opened())
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_pty(String(directory, "/pane1"), expected_one)


@inline(.never)
def phase_reconnect_budget(
    mut ui: Ui,
    mut view: WorkspaceView,
    mut host: PrimaryHost,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    mut timeline: InputTimeline,
    directory: String,
    output_path: String,
    source: UInt64,
    zero: MouseEvent,
    one: MouseEvent,
    mut expected_zero: String,
    mut expected_one: String,
) raises:
    var input = TextInput()
    assert_equal(view.model.source_client, source)
    host.allow_reads = False
    host.clear_log()
    input.insert("x")
    input.command(CMD_PASTE, MOD_CTRL)
    input.insert("j")
    _ = input.record_primary(zero)
    _ = input.record_middle(up(zero))
    timeline.raw_key(view, raw_key(Key.X, 120, 0, lo=0, hi=1), input)
    timeline.raw_key(view, raw_key(Key.V, 118, 1, lo=0, hi=1), input)
    view.backend.enqueue(RECONNECT, selection_epoch=view.focus_epoch)
    view.flush()
    view.next_poll = 0
    view.poll(ui.time)
    view.wait()
    assert_true(view.model.source_client != source)
    var suffix_pane = view.input_pane().value()
    frame(ui, view, host, keyboard, clipboard, timeline, input^)
    assert_equal(host.reads(), "")
    for _ in range(20):
        settle(ui, view, host, keyboard, clipboard, timeline)
    if suffix_pane == 0:
        expected_zero += "j"
    else:
        expected_one += "j"
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_pty(String(directory, "/pane1"), expected_one)
    host.allow_reads = True
    host.primary_text = "P"
    host.clear_log()
    var full_point = point(view, ui, view.model.pane.value())
    full_point.y += 200
    view.backend.commit(
        view.model.pane.value(),
        String("z") * MAX_PENDING_INPUT_BYTES,
        view.focus_epoch,
        True,
    )
    input = TextInput()
    _ = input.record_primary(full_point)
    _ = input.record_middle(up(full_point))
    feed(ui, input)
    ui.begin_frame(1120, 720)
    for event in input.events:
        _ = view.ordered_pointer(ui, input, event, host)
    assert_equal(host.reads(), "p")
    assert_equal(view.backend.queue.pending_bytes, MAX_PENDING_INPUT_BYTES)
    var refused_queue = view.backend.queue.take()
    assert_equal(len(refused_queue), 1)
    assert_equal(refused_queue[0].text.byte_length(), MAX_PENDING_INPUT_BYTES)
    assert_true(not view.primary.pressed)
    assert_true(not view.primary.pending)
    refused_queue.clear()
    ui.end_frame()
    ui.atlas.gpu_done(ui.atlas.frame)
    frame(ui, view, host, keyboard, clipboard, timeline)
    host.primary_text = String("q") * MAX_PENDING_INPUT_BYTES
    host.clear_log()
    var budget_point = point(view, ui, view.model.pane.value())
    budget_point.y += 200
    input = TextInput()
    _ = input.record_primary(budget_point)
    _ = input.record_middle(up(budget_point))
    feed(ui, input)
    ui.begin_frame(1120, 720)
    for event in input.events:
        _ = view.ordered_pointer(ui, input, event, host)
    assert_equal(host.reads(), "p")
    assert_equal(view.backend.queue.pending_bytes, MAX_PENDING_INPUT_BYTES)
    var dropped = view.backend.queue.take()
    assert_equal(len(dropped), 1)
    assert_equal(dropped[0].kind, PASTE)
    assert_equal(dropped[0].text.byte_length(), MAX_PENDING_INPUT_BYTES)
    assert_equal(len(dropped[0].frame), 0)
    assert_true(not view.primary.pressed)
    dropped.clear()
    ui.end_frame()
    ui.atlas.gpu_done(ui.atlas.frame)
    host.primary_text = "P"
    view.palette.notifications.clear()
    frame(ui, view, host, keyboard, clipboard, timeline)
    assert_pty(String(directory, "/pane0"), expected_zero)
    assert_pty(String(directory, "/pane1"), expected_one)
    assert_equal(view.model.error, "")
    assert_true(ui.native_scene_complete)
    var pixels = frame_pixels(ui, view.theme.flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(output_path)
    output.write_bytes(Span(png))


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
    var host = PrimaryHost(String(args[2], "/reads"))
    var keyboard = KeyboardController()
    var clipboard = ClipboardController()
    var timeline = InputTimeline()
    view.wait()
    for _ in range(200):
        settle(ui, view, host, keyboard, clipboard, timeline)
        if (
            UInt64(0) in view.model.terminals
            and UInt64(1) in view.model.terminals
            and "PRIMARY_READY"
            in _selected_text(view.model.terminals, UInt64(0))
            and "PRIMARY_READY"
            in _selected_text(view.model.terminals, UInt64(1))
        ):
            break
    assert_equal(view.model.error, "")
    assert_true(
        "PRIMARY_READY" in _selected_text(view.model.terminals, UInt64(0))
    )
    assert_true(
        "PRIMARY_READY" in _selected_text(view.model.terminals, UInt64(1))
    )
    assert_equal(view.model.pane, Optional[UInt64](1))
    var source = view.model.source_client
    var zero = point(view, ui, 0)
    var one = point(view, ui, 1)
    var expected_zero = String()
    var expected_one = String()
    phase_ordered(
        ui,
        view,
        host,
        keyboard,
        clipboard,
        timeline,
        String(args[2]),
        String(args[3]),
        source,
        zero,
        one,
        expected_zero,
        expected_one,
    )
    phase_tracking_occlusion(
        ui,
        view,
        host,
        keyboard,
        clipboard,
        timeline,
        String(args[2]),
        String(args[3]),
        source,
        zero,
        one,
        expected_zero,
        expected_one,
    )
    phase_editors(
        ui,
        view,
        host,
        keyboard,
        clipboard,
        timeline,
        String(args[2]),
        String(args[3]),
        source,
        zero,
        one,
        expected_zero,
        expected_one,
    )
    phase_refusal(
        ui,
        view,
        host,
        keyboard,
        clipboard,
        timeline,
        String(args[2]),
        String(args[3]),
        source,
        zero,
        one,
        expected_zero,
        expected_one,
    )
    phase_reconnect_budget(
        ui,
        view,
        host,
        keyboard,
        clipboard,
        timeline,
        String(args[2]),
        String(args[3]),
        source,
        zero,
        one,
        expected_zero,
        expected_one,
    )
    print(
        "PRIMARY workspace: ordered two-pane text/keys, same-frame releases,"
        " composition, empty/oversized refusal and pre-read payload guards"
        " passed"
    )
