from std.math import floor
from std.sys.info import CompilationTarget
from mjui.ui import Ui, Style, TextStyle
from mjui.layers import MODAL, Placement
from mjui.input import (
    TextInput,
    TextEvent,
    MouseEvent,
    TEXT_INSERT,
    TEXT_COMMAND,
    INSERT_COMPOSED,
    MOUSE_DOWN,
    MOUSE_UP,
    MOUSE_MOVED,
    CMD_UP,
    CMD_DOWN,
    CMD_LEFT,
    CMD_RIGHT,
    CMD_NEWLINE,
    CMD_NEWLINE_RAW,
    CMD_TAB,
    CMD_BACKTAB,
    CMD_CANCEL,
    CMD_DELETE_BACK,
    CMD_PASTE,
    CMD_EXTEND,
    CMD_LOGICAL,
    MOD_ALT,
    MOD_CMD,
)
from mjui.widgets.textedit import TextEdit, Edit
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_palette import palette_tree_row
from mjui.widgets.penna_palette_input import (
    PalettePill,
    palette_input,
    palette_empty,
)
from mjui.widgets.penna_palette_surface import (
    PaletteHint,
    palette_surface_begin,
    palette_list_begin,
    palette_list_end,
    palette_footer,
)
from mjui.widgets.penna_palette_shortcuts import (
    PaletteKeyPart,
    PaletteKeystroke,
    key_parts,
)
from .window_chooser_controller import (
    WindowChooserData,
    WindowChooserEffects,
    WindowChooserOperation,
)
from .window_chooser_scroll import window_chooser_scroll
from .chooser_runtime import chooser_wheel_scroll
from .palette_component import palette_kit_row
from .palette_component_model import PaletteBounds
from .palette_navigation import parent_index
from .palette_ui_keys import palette_ui_key
from .palette_text import palette_words
from .chooser_presentation import CHOOSER_DISPLAY_BYTES
from .search_clipboard import SearchClipboard
from .window_chooser_text import (
    window_chooser_text,
    window_chooser_marked_offset,
)


struct WindowChooserComponentResult(Movable):
    var consumed: Bool
    var limited: Bool
    var operations: List[WindowChooserOperation]

    def __init__(out self):
        self.consumed = False
        self.limited = False
        self.operations = List[WindowChooserOperation]()


def window_chooser_hints(
    data: WindowChooserData,
) raises -> List[Tuple[String, String]]:
    var result = List[Tuple[String, String]]()
    result.append((String("up down"), String("navigate")))
    var expand = False
    var enter = String("choose")
    if data.selected:
        var index = data.selected.value()
        if index >= 0 and index < len(data.rows[].rows):
            ref row = data.rows[].rows[index]
            if data.query.byte_length() == 0:
                var navigation = data.rows[].navigation()
                expand = Bool(row.expanded) or Bool(
                    parent_index(Span(navigation), index)
                )
            if data.snapshot:
                ref items = data.snapshot.value().items[].values
                if row.index >= 0 and row.index < len(items):
                    var kind = items[row.index].target_kind
                    enter = String(
                        "switch session" if kind
                        == 0 else "switch window" if kind
                        == 1 else "focus pane" if kind
                        == 2 else "choose"
                    )
    if expand:
        result.append((String("left right"), String("expand")))
    result.append((String("enter"), enter^))
    result.append((String("backspace"), String("leave mode")))
    result.append((String("escape"), String("close")))
    return result^


struct WindowChooserComponent(Movable):
    var field: TextEdit
    var key: UInt64
    var layer: UInt64
    var list_key: UInt64
    var row_key: UInt64
    var disclosure_key: UInt64
    var bounds: PaletteBounds
    var panel_height: Float32
    var viewport: PaletteBounds
    var scroll: Float32
    var pending_scroll: Optional[Tuple[Int, UInt8]]
    var row_height: Float32
    var scope: UInt64
    var painted_scope: UInt64
    var previous_hot: UInt64
    var pressed: Optional[Tuple[UInt64, UInt64]]
    var visible: List[Int]

    def __init__(out self, key: UInt64, style: TextStyle):
        self.key = key
        self.layer = key + 3
        self.list_key = key + 2
        self.row_key = key + 65536
        self.disclosure_key = key + 131072
        self.field = TextEdit(
            key + 1, style, placeholder="Search sessions, windows, panes"
        )
        self.field.label = "Command palette"
        self.bounds = PaletteBounds(0, 0, 0, 0)
        self.panel_height = 0
        self.viewport = PaletteBounds(0, 0, 0, 0)
        self.scroll = 0
        self.pending_scroll = None
        self.row_height = 28
        self.scope = 0
        self.painted_scope = 0
        self.previous_hot = 0
        self.pressed = None
        self.visible = List[Int]()

    def open(mut self, mut ui: Ui, scope: UInt64):
        self.scope = scope
        self.painted_scope = 0
        self.scroll = 0
        self.pending_scroll = (0, UInt8(0))
        self.previous_hot = 0
        self.pressed = None
        self.field.set_text("")
        self.field.marked.clear()
        self.field.disp.clear()
        self.field.undo = List[Edit]()
        self.field.undo_top = 0
        ui.open_layer(
            self.layer,
            MODAL,
            Placement.at(0, 0, shift=False),
            scrim=0,
            light_dismiss=False,
            dismiss_escape=False,
        )
        ui.focus = self.field.key
        ui.text_input = True
        self.field.focused = True

    def close(mut self, mut ui: Ui):
        ui.close_layer_only(self.layer)
        if ui.focus == self.field.key:
            ui.focus = 0
        self.field.focused = False
        self.field.marked.clear()
        self.field.disp.clear()
        self.painted_scope = 0
        self.pressed = None
        self.visible.clear()

    def input_scope(self, mut ui: Ui) -> Bool:
        if ui.focus != self.field.key or not ui.retained_input_begin(
            self.layer, self.field.key
        ):
            return False
        ui.retained_input_end()
        return True

    def synchronize(
        mut self, data: WindowChooserData, effects: WindowChooserEffects
    ):
        if self.state()[0] != data.query:
            self.field.set_text(data.query)
            self.field.marked.clear()
            self.field.disp.clear()
            self.field.select(data.cursor, data.cursor)
        if effects.scroll:
            self.pending_scroll = effects.scroll

    def state(self) -> Tuple[String, Int]:
        if len(self.field.marked) == 0:
            return (self.field.text(), self.field.caret)
        var selection = self.field.selection()
        var bytes = List[UInt8](
            capacity=len(self.field.buf)
            - selection[1]
            + selection[0]
            + len(self.field.marked)
        )
        bytes.extend(Span(self.field.buf)[: selection[0]])
        bytes.extend(Span(self.field.marked))
        bytes.extend(Span(self.field.buf)[selection[1] :])
        return (
            String(unsafe_from_utf8=Span(bytes)),
            selection[0] + self.field.marked_b,
        )

    def edited(
        self, data: WindowChooserData
    ) raises -> Optional[WindowChooserOperation]:
        var text, cursor = self.state()
        if text == data.query and cursor == data.cursor:
            return None
        var operation = WindowChooserOperation(1)
        operation.text = text^
        operation.cursor = cursor
        return operation^

    def compose(
        mut self, marked: Span[UInt8, _], start: Int, end: Int
    ) raises -> Bool:
        if len(marked) > CHOOSER_DISPLAY_BYTES:
            return False
        var original = String(from_utf8=marked)
        var text = window_chooser_text(original)
        var selection = self.field.selection()
        if (
            text.byte_length()
            > CHOOSER_DISPLAY_BYTES
            - len(self.field.buf)
            + selection[1]
            - selection[0]
        ):
            return False
        if len(self.field.marked) == 0:
            if len(marked) == 0:
                return True
            if selection[0] != selection[1]:
                var empty = List[UInt8]()
                self.field.insert(empty.unsafe_ptr(), 0)
        self.field.marked.clear()
        self.field.marked.extend(text.as_bytes())
        self.field.marked_a = 0
        self.field.marked_b = 0
        if text.byte_length() > 0:
            self.field.marked_a = window_chooser_marked_offset(
                original, start, text
            )
            self.field.marked_b = window_chooser_marked_offset(
                original, end, text
            )
        self.field.disp.clear()
        self.field.follow = True
        self.field.dirty = True
        return True

    def unmark(mut self):
        if len(self.field.marked) == 0:
            return
        var marked = List[UInt8]()
        swap(marked, self.field.marked)
        self.field.disp.clear()
        var start = self.field.selection()[0]
        self.field.insert(marked.unsafe_ptr(), len(marked))
        self.field.select(
            start + self.field.marked_a, start + self.field.marked_b
        )
        self.field.marked_a = 0
        self.field.marked_b = 0

    def trim_undo(mut self):
        var bytes = 0
        for edit in self.field.undo:
            bytes += edit.removed.capacity() + edit.inserted.capacity()
        if bytes > 1024 * 1024 or len(self.field.undo) > 64:
            self.field.undo = List[Edit]()
            self.field.undo_top = 0

    def insert(
        mut self, bytes: Span[UInt8, _], composed: Bool = False
    ) raises -> Bool:
        var text = String()
        if len(bytes) <= CHOOSER_DISPLAY_BYTES:
            text = window_chooser_text(String(from_utf8=bytes))
        var selection = self.field.selection()
        if (
            len(bytes) > CHOOSER_DISPLAY_BYTES
            or text.byte_length()
            > CHOOSER_DISPLAY_BYTES
            - len(self.field.buf)
            + selection[1]
            - selection[0]
        ):
            if composed:
                self.unmark()
            return False
        self.field.marked.clear()
        self.field.disp.clear()
        self.field.insert(text.unsafe_ptr(), text.byte_length())
        return True

    def prepare(mut self, mut ui: Ui, mut clip: SearchClipboard, pointer: Bool):
        if pointer and ui.retained_input_begin(self.layer, self.field.key):
            var pointer_input = TextInput()
            pointer_input.marked = self.field.marked.copy()
            pointer_input.marked_sel_a = self.field.marked_a
            pointer_input.marked_sel_b = self.field.marked_b
            self.field.update(ui, pointer_input, clip)
            ui.retained_input_end()
        self.trim_undo()

    def event(
        mut self,
        mut ui: Ui,
        input: TextInput,
        event: TextEvent,
        mut clip: SearchClipboard,
        data: WindowChooserData,
    ) raises -> WindowChooserComponentResult:
        var result = WindowChooserComponentResult()
        if event.kind != TEXT_INSERT and event.kind != TEXT_COMMAND:
            return result^
        result.consumed = True
        if event.kind == TEXT_INSERT:
            if (
                event.start < 0
                or event.end < event.start
                or Int(event.end) > len(input.bytes)
            ):
                raise Error("window chooser committed text range is invalid")
            result.limited = not self.insert(
                Span(input.bytes)[Int(event.start) : Int(event.end)],
                event.cmd == INSERT_COMPOSED,
            )
        else:
            var command = event.cmd & ~(CMD_EXTEND | CMD_LOGICAL)
            var operation = WindowChooserOperation(11)
            if (
                event.cmd & CMD_EXTEND == 0
                and (command == CMD_UP or command == CMD_DOWN)
                and event.mods & (MOD_ALT | MOD_CMD) == 0
            ):
                operation.kind = 2
                operation.direction = -1 if command == CMD_UP else 1
            elif command == CMD_NEWLINE or command == CMD_NEWLINE_RAW:
                if not data.finishing:
                    operation.kind = 4
            elif command == CMD_TAB:
                operation.kind = 4
            elif command == CMD_CANCEL:
                operation.kind = 7
            elif command == CMD_DELETE_BACK and len(self.field.buf) == 0:
                operation.kind = 9
            elif (
                event.cmd & CMD_EXTEND == 0
                and (command == CMD_LEFT or command == CMD_RIGHT)
                and data.query.byte_length() == 0
            ):
                operation.kind = 3
                operation.flag = command == CMD_RIGHT
            elif command == CMD_PASTE:
                result.limited = not self.insert(clip.text.as_bytes())
            elif command != CMD_BACKTAB:
                self.field.command(ui, event.cmd, event.mods, clip)
            if operation.kind != 11:
                result.operations.append(operation^)
        var edited = self.edited(data)
        if edited:
            result.operations.append(edited.take())
        self.trim_undo()
        self.field.blink_t0 = ui.time
        return result^

    def row_at(self, hit: UInt64) -> Optional[Tuple[Int, Bool]]:
        for index in self.visible:
            if hit == self.row_key + UInt64(index):
                return (index, False)
            if hit == self.disclosure_key + UInt64(index):
                return (index, True)
        return None

    def pointer(
        mut self, ui: Ui, event: MouseEvent, data: WindowChooserData
    ) -> List[WindowChooserOperation]:
        var operations = List[WindowChooserOperation]()
        if self.scope != self.painted_scope:
            self.pressed = None
            return operations^
        var list = ui.rect_of(self.list_key)
        self.viewport = PaletteBounds(
            list.x0, list.y0, list.x1 - list.x0, list.y1 - list.y0
        )
        var hit = ui.hit_test(event.x, event.y)
        var row = self.row_at(hit)
        if event.kind == MOUSE_DOWN:
            self.pressed = (hit, self.scope) if row else Optional[
                Tuple[UInt64, UInt64]
            ](None)
            if hit == self.key + 4:
                operations.append(WindowChooserOperation(7))
        elif event.kind == MOUSE_UP:
            var pressed = self.pressed
            self.pressed = None
            if (
                pressed == Optional[Tuple[UInt64, UInt64]]((hit, self.scope))
                and row
            ):
                var selected = WindowChooserOperation(5)
                selected.index = row.value()[0]
                operations.append(selected^)
                var operation = WindowChooserOperation(
                    UInt8(3 if row.value()[1] else 4)
                )
                operation.flag = (
                    not data.rows[].rows[row.value()[0]].expanded.or_else(False)
                )
                operations.append(operation^)
        elif event.kind == MOUSE_MOVED and row:
            var operation = WindowChooserOperation(5)
            operation.index = row.value()[0]
            operations.append(operation^)
        return operations^

    def wheel(
        mut self,
        ui: Ui,
        x: Float32,
        y: Float32,
        dx: Float32,
        dy: Float32,
        precise: Bool,
        count: Int,
    ) raises -> Bool:
        var list = ui.rect_of(self.list_key)
        self.viewport = PaletteBounds(
            list.x0, list.y0, list.x1 - list.x0, list.y1 - list.y0
        )
        if (
            self.scope != self.painted_scope
            or x < self.viewport.x
            or x >= self.viewport.x + self.viewport.width
            or y < self.viewport.y
            or y >= self.viewport.y + self.viewport.list_height
        ):
            return False
        var hit = ui.hit_test(x, y)
        if hit != self.list_key and not self.row_at(hit):
            return False
        self.scroll = chooser_wheel_scroll(
            self.scroll,
            max(
                Float32(count) * self.row_height - self.viewport.list_height,
                Float32(0),
            ),
            18,
            dx,
            dy,
            precise,
        )
        return True

    def paint(
        mut self, mut ui: Ui, theme: PennaTheme, data: WindowChooserData
    ) raises:
        if not ui.layer_begin(
            self.layer, Style.col().width(ui.width).height(ui.height)
        ):
            return
        ui.open(Style.col().width(ui.width).height(ui.height).id(self.key + 4))
        _ = ui.interact(self.key + 4, focusable=False)
        var width = min(Float32(560), max(ui.width - 32, Float32(0)))
        var list_height = min(
            Float32(len(data.rows[].rows)) * 28,
            (ui.height * 0.88 - 140).clamp(40, 440),
        )
        self.bounds = PaletteBounds(
            (ui.width - width) / 2, ui.height * 0.12, width, list_height
        )
        var content = palette_surface_begin(
            ui, theme, self.key, self.bounds.x, self.bounds.y, width, False
        )
        _ = ui.interact(self.key, focusable=False)
        _ = ui.interact(self.field.key, focusable=True, focus_on_press=False)
        ui.text_field(self.field.key)
        self.field.focused = ui.focus == self.field.key
        var pills = List[PalettePill]()
        pills.append(PalettePill(0, "Go to", "@"))
        palette_input(ui, theme, self.field, Span(pills), content)
        var rows_width = palette_list_begin(ui, theme, content)
        self.row_height = 28
        self.scroll = self.scroll.clamp(
            Float32(0),
            max(
                Float32(len(data.rows[].rows)) * self.row_height - list_height,
                Float32(0),
            ),
        )
        if self.pending_scroll:
            var selected, strategy = self.pending_scroll.take()
            self.scroll = window_chooser_scroll(
                self.scroll,
                self.row_height,
                list_height,
                UInt32(len(data.rows[].rows)),
                UInt32(max(selected, 0)),
                strategy == 1,
            )
        self.visible.clear()
        if len(data.rows[].rows) == 0:
            palette_empty(ui, theme, "No matches", rows_width)
        else:
            ui.open(
                Style.col()
                .width(rows_width)
                .height(list_height)
                .clipped()
                .id(self.list_key)
            )
            _ = ui.interact(self.list_key, focusable=False)
            var first = Int(floor(self.scroll / self.row_height))
            var last = min(
                len(data.rows[].rows),
                first + Int(list_height / self.row_height) + 2,
            )
            for index in range(first, last):
                self.visible.append(index)
                ui.open(
                    Style.col()
                    .width(rows_width)
                    .height(self.row_height)
                    .at(0, Float32(index) * self.row_height - self.scroll)
                )
                var row = palette_kit_row(data.rows[].rows[index])
                _ = palette_tree_row(
                    ui,
                    theme,
                    self.row_key + UInt64(index),
                    row,
                    data.selected == Optional[Int](index),
                    rows_width,
                    self.disclosure_key + UInt64(index),
                )
                ui.close()
            ui.close()
        palette_list_end(ui)
        var hints = List[PaletteHint]()
        for hint in window_chooser_hints(data):
            var groups = List[List[PaletteKeyPart]]()
            for key in palette_words(hint[0]):
                var parsed = palette_ui_key(key, CompilationTarget.is_macos())
                groups.append(
                    key_parts(
                        PaletteKeystroke(
                            parsed.value().key, parsed.value().modifiers
                        ) if parsed else PaletteKeystroke(key)
                    )
                )
            hints.append(PaletteHint(hint[1], groups^))
        palette_footer(ui, theme, Span(hints), content)
        self.panel_height = ui.close_and_measure(width)
        ui.close()
        ui.layer_end()
        self.painted_scope = self.scope
        var list = ui.rect_of(self.list_key)
        self.viewport = PaletteBounds(
            list.x0, list.y0, list.x1 - list.x0, list.y1 - list.y0
        )
