from std.sys import size_of
from mjui.ui import Ui, TextStyle, CURSOR_TEXT
from mjui.input import (
    TextInput,
    TextEvent,
    TEXT_INSERT,
    TEXT_COMMAND,
    CMD_PASTE,
    CMD_NEWLINE,
    CMD_NEWLINE_RAW,
    CMD_CANCEL,
    MOD_SHIFT,
)
from mjui.widgets.textedit import TextEdit, Edit
from .search_clipboard import SearchClipboard
from .terminal_search import SearchSession, SearchEffect
from .editor_pointer import editor_composition

comptime SEARCH_TEXT_BYTES: Int = 1_048_576
comptime SEARCH_UNDO_BYTES: Int = 4_194_304
comptime SEARCH_FRAME_BYTES: Int = 4_194_304


struct SearchInputResult(Movable):
    var effects: List[SearchEffect]
    var consumed: Int
    var limited: Bool
    var bytes: Int
    var height: Float32

    def __init__(out self):
        self.effects = List[SearchEffect]()
        self.consumed = 0
        self.limited = False
        self.bytes = 0
        self.height = 0

    def append(mut self, var effect: SearchEffect):
        self.bytes += len(effect.frame)
        self.effects.append(effect^)


struct SearchEditor(Movable):
    var session: SearchSession
    var field: TextEdit
    var pending_begin: Bool
    var enter_released: Bool
    var escape_released: Bool

    def __init__(out self, key: UInt64, style: TextStyle):
        self.session = SearchSession()
        self.field = TextEdit(key, style, placeholder="Search")
        self.field.label = "Terminal search"
        self.pending_begin = False
        self.enter_released = False
        self.escape_released = False

    def key_pressed(mut self, key: UInt8):
        if key == 2:
            self.enter_released = False
        elif key == 4:
            self.escape_released = False

    def key_released(mut self, key: UInt8) -> Bool:
        if key == 2:
            self.enter_released = True
        elif key == 4:
            self.escape_released = True
        return self.session.swallow_key(key, True)

    def settle_released_key(mut self):
        if self.session.swallowed:
            var key = self.session.swallowed.value()
            if key == 2 and self.enter_released or key == 4 and self.escape_released:
                _ = self.session.swallow_key(key, True)

    def reset_text(mut self):
        self.field.set_text("")
        self.field.marked = List[UInt8]()
        self.field.disp = List[UInt8]()
        self.clear_flags()

    def clear_flags(mut self):
        self.field.changed = False
        self.field.submitted = False
        self.field.cancelled = False
        self.field.tab = 0

    def trim_undo(mut self):
        var bytes = 0
        for edit in self.field.undo:
            bytes += edit.removed.capacity() + edit.inserted.capacity()
        if bytes > SEARCH_UNDO_BYTES or len(self.field.undo) > 64:
            self.discard_undo()

    def discard_undo(mut self):
        self.field.undo = List[Edit]()
        self.field.undo_top = 0

    def owned_bytes(self) -> Int:
        var bytes = (
            self.field.buf.capacity()
            + self.field.marked.capacity()
            + self.field.disp.capacity()
            + self.field.mask.capacity()
            + self.field.mask_stops.capacity() * size_of[Int]()
            + self.field.undo.capacity() * size_of[Edit]()
        )
        for edit in self.field.undo:
            bytes += edit.removed.capacity() + edit.inserted.capacity()
        if self.session.query:
            bytes += self.session.query.value().text.byte_length() * 2 + 32
        return bytes

    def changed(
        mut self, pane: UInt64, command_output: Bool, mut result: SearchInputResult
    ) raises:
        if self.field.changed:
            result.append(self.session.edit(pane, self.field.text(), command_output))
            self.field.changed = False
            self.trim_undo()

    def prepare(
        mut self,
        mut ui: Ui,
        input: TextInput,
        mut clip: SearchClipboard,
        pane: UInt64,
        command_output: Bool = False,
        container: UInt64 = 0,
        pointer_enabled: Bool = True,
    ) raises -> SearchInputResult:
        var result = SearchInputResult()
        if not self.session.query:
            return result^
        self.clear_flags()
        if container != 0 and pointer_enabled:
            var response = ui.interact(container)
            if response.hovered or response.held:
                ui.cursor = CURSOR_TEXT
            if response.pressed:
                ui.focus = self.field.key
                var position = ui.text_hit(self.field.key, ui.press_x, ui.press_y)
                if position.byte >= 0:
                    self.field.select(position.byte, position.byte)
                self.field.blink_t0 = ui.time
        if len(input.marked) > SEARCH_TEXT_BYTES:
            result.limited = True
        var pointer = TextInput()
        if len(input.marked) <= SEARCH_TEXT_BYTES:
            pointer.marked.extend(Span(input.marked))
            pointer.marked_sel_a = input.marked_sel_a
            pointer.marked_sel_b = input.marked_sel_b
        if len(self.field.buf) + clip.primary_text.byte_length() <= SEARCH_TEXT_BYTES:
            pointer.primary = input.primary
        elif input.primary:
            result.limited = True
        if pointer_enabled:
            self.field.update(ui, pointer, clip)
        else:
            editor_composition(self.field, ui, pointer)
        self.changed(pane, command_output, result)
        return result^

    def event(
        mut self,
        mut ui: Ui,
        input: TextInput,
        event: TextEvent,
        mut clip: SearchClipboard,
        pane: UInt64,
        mut result: SearchInputResult,
        command_output: Bool = False,
    ) raises -> Bool:
        if not self.session.query or ui.focus != self.field.key:
            return False
        if event.kind != TEXT_INSERT and event.kind != TEXT_COMMAND:
            return False
        result.consumed += 1
        if result.consumed > 4096 or result.bytes + SEARCH_TEXT_BYTES + 128 > SEARCH_FRAME_BYTES:
            result.limited = True
            return True
        if event.kind == TEXT_INSERT:
            var start = Int(event.start)
            var end = Int(event.end)
            if start < 0 or end < start or end > len(input.bytes):
                raise Error("invalid search text event bounds")
            var selection = self.field.selection()
            if len(self.field.buf) - (selection[1] - selection[0]) + end - start > SEARCH_TEXT_BYTES:
                result.limited = True
                return True
            self.field.insert(input.bytes.unsafe_ptr().unsafe_offset(start), end - start)
        elif event.cmd == CMD_NEWLINE or event.cmd == CMD_NEWLINE_RAW:
            result.append(self.session.enter(pane, event.mods & MOD_SHIFT != 0, command_output))
        elif event.cmd == CMD_CANCEL:
            result.append(self.session.escape(pane, command_output))
        else:
            var selection = self.field.selection()
            if event.cmd == CMD_PASTE and len(self.field.buf) - (selection[1] - selection[0]) + clip.text.byte_length() > SEARCH_TEXT_BYTES:
                result.limited = True
                return True
            self.field.command(ui, event.cmd, event.mods, clip)
        self.field.blink_t0 = ui.time
        self.changed(pane, command_output, result)
        if self.field.tab != 0:
            _ = ui.focus_next(self.field.tab < 0)
            self.field.tab = 0
            self.field.focused = ui.focus == self.field.key
            ui.text_input = self.field.focused
        if not self.session.query:
            self.field.marked = List[UInt8]()
            self.field.disp = List[UInt8]()
            self.field.focused = False
            ui.text_input = False
            self.settle_released_key()
        self.clear_flags()
        return True

    def update(
        mut self,
        mut ui: Ui,
        input: TextInput,
        mut clip: SearchClipboard,
        pane: UInt64,
        command_output: Bool = False,
        container: UInt64 = 0,
    ) raises -> SearchInputResult:
        var result = self.prepare(ui, input, clip, pane, command_output, container)
        if not self.session.query or not self.field.focused:
            return result^
        for event in input.events:
            if not self.event(ui, input, event, clip, pane, result, command_output):
                break
        return result^
