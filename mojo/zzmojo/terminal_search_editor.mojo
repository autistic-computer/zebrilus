from std.sys import size_of
from mjui.ui import Ui, TextStyle
from mjui.input import (
    TextInput,
    TEXT_INSERT,
    CMD_PASTE,
    CMD_NEWLINE,
    CMD_NEWLINE_RAW,
    CMD_CANCEL,
    MOD_SHIFT,
)
from mjui.widgets.textedit import TextEdit, Edit
from .search_clipboard import SearchClipboard
from .terminal_search import SearchSession, SearchEffect

comptime SEARCH_TEXT_BYTES: Int = 1_048_576
comptime SEARCH_UNDO_BYTES: Int = 4_194_304
comptime SEARCH_FRAME_BYTES: Int = 4_194_304


struct SearchInputResult(Movable):
    var effects: List[SearchEffect]
    var consumed: Int
    var limited: Bool
    var bytes: Int

    def __init__(out self):
        self.effects = List[SearchEffect]()
        self.consumed = 0
        self.limited = False
        self.bytes = 0

    def append(mut self, var effect: SearchEffect):
        self.bytes += len(effect.frame)
        self.effects.append(effect^)


struct SearchEditor(Movable):
    var session: SearchSession
    var field: TextEdit

    def __init__(out self, key: UInt64, style: TextStyle):
        self.session = SearchSession()
        self.field = TextEdit(key, style, placeholder="Search")
        self.field.label = "Terminal search"

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

    def update(
        mut self,
        mut ui: Ui,
        input: TextInput,
        mut clip: SearchClipboard,
        pane: UInt64,
        command_output: Bool = False,
    ) raises -> SearchInputResult:
        var result = SearchInputResult()
        if not self.session.query:
            return result^
        self.clear_flags()
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
        self.field.update(ui, pointer, clip)
        self.changed(pane, command_output, result)
        if not self.field.focused:
            return result^
        for index in range(len(input.events)):
            if index >= 4096 or result.bytes + SEARCH_TEXT_BYTES + 128 > SEARCH_FRAME_BYTES:
                result.limited = True
                result.consumed = len(input.events)
                break
            var event = input.events[index]
            result.consumed = index + 1
            if event.kind == TEXT_INSERT:
                var start = Int(event.start)
                var end = Int(event.end)
                if start < 0 or end < start or end > len(input.bytes):
                    raise Error("invalid search text event bounds")
                var selection = self.field.selection()
                if len(self.field.buf) - (selection[1] - selection[0]) + end - start > SEARCH_TEXT_BYTES:
                    result.limited = True
                    continue
                self.field.insert(input.bytes.unsafe_ptr().unsafe_offset(start), end - start)
            elif event.cmd == CMD_NEWLINE or event.cmd == CMD_NEWLINE_RAW:
                result.append(self.session.enter(pane, event.mods & MOD_SHIFT != 0, command_output))
            elif event.cmd == CMD_CANCEL:
                result.append(self.session.escape(pane, command_output))
            else:
                var selection = self.field.selection()
                if event.cmd == CMD_PASTE and len(self.field.buf) - (selection[1] - selection[0]) + clip.text.byte_length() > SEARCH_TEXT_BYTES:
                    result.limited = True
                    continue
                self.field.command(ui, event.cmd, event.mods, clip)
            self.field.blink_t0 = ui.time
            self.changed(pane, command_output, result)
            if self.field.tab != 0:
                _ = ui.focus_next(self.field.tab < 0)
                self.field.tab = 0
                self.field.focused = ui.focus == self.field.key
                ui.text_input = self.field.focused
                if not self.field.focused:
                    break
            if not self.session.query:
                self.field.marked = List[UInt8]()
                self.field.disp = List[UInt8]()
                break
        self.clear_flags()
        return result^
