from mjui.ui import Ui, Style, TextStyle
from mjui.layers import MODAL, Placement
from mjui.input import (
    TextInput,
    TextEvent,
    TEXT_INSERT,
    TEXT_COMMAND,
    CMD_PASTE,
    CMD_UP,
    CMD_DOWN,
    CMD_NEWLINE,
    CMD_NEWLINE_RAW,
    CMD_CANCEL,
    CMD_TAB,
    CMD_BACKTAB,
    CMD_DELETE_BACK,
    CMD_EXTEND,
    CMD_LOGICAL,
    MOD_ALT,
    MOD_CMD,
)
from mjui.widgets.textedit import TextEdit, Edit
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_palette import (
    PaletteRow as KitRow,
    PaletteMatch,
    palette_row,
)
from mjui.widgets.penna_palette_input import (
    PalettePill as KitPill,
    palette_input,
    palette_section,
    palette_empty,
)
from mjui.widgets.penna_palette_shortcuts import (
    PaletteShortcut as KitShortcut,
    PaletteKeystroke as KitKeystroke,
    PaletteKeyPart,
    key_parts,
)
from mjui.widgets.penna_palette_surface import (
    PaletteHint as KitHint,
    palette_surface_begin,
    palette_usage,
    palette_list_begin,
    palette_list_end,
    palette_footer,
)
from .palette_controller import PaletteEffect, PALETTE_INPUT_BYTES
from .palette_controller_model import PaletteModel
from .palette_places_model import PaletteFleet
from .palette_presentation import (
    PaletteRow,
    PaletteDecorations,
    palette_place_pills,
)
from .palette_component_model import (
    PaletteBounds,
    palette_bounds,
    palette_component_hints,
    palette_component_usage,
)
from .search_clipboard import SearchClipboard
from .palette_text import palette_words
from .palette_assets import palette_asset_documents

comptime PALETTE_UNDO_BYTES: Int = 1024 * 1024


struct PaletteComponentResult(Movable):
    var consumed: Bool
    var limited: Bool
    var closed: Bool
    var effect: PaletteEffect

    def __init__(out self):
        self.consumed = False
        self.limited = False
        self.closed = False
        self.effect = PaletteEffect()


def palette_kit_row(row: PaletteRow, openai: Int32 = -1, claude: Int32 = -1) -> KitRow:
    var result = KitRow(row.label, row.detail, row.right)
    result.muted_prefix = row.muted_prefix
    result.status = row.status
    result.running = row.running
    result.indent = row.indent
    result.icon_name = "SquareTerminal" if row.icon.startswith(
        "Asset("
    ) else row.icon.copy()
    if row.icon == 'Asset("icons/openai.svg")' and openai >= 0:
        result.icon_doc = openai
        result.icon_name = String()
    elif row.icon == 'Asset("icons/claude.svg")' and claude >= 0:
        result.icon_doc = claude
        result.icon_name = String()
    result.expanded = row.expanded
    for hit in row.hits:
        result.matches.append(PaletteMatch(hit.start, hit.end))
    if row.shortcut:
        ref shortcut = row.shortcut.value()
        var keys = List[KitKeystroke](capacity=len(shortcut.keys))
        for key in shortcut.keys:
            keys.append(KitKeystroke(key.key, key.modifiers))
        result.shortcut = KitShortcut(shortcut.kind, shortcut.text, keys^)
    return result^


struct PaletteComponent(Movable):
    var model: PaletteModel
    var field: TextEdit
    var key: UInt64
    var field_key: UInt64
    var list_key: UInt64
    var row_key: UInt64
    var scroll_top: Bool
    var scroll_selected: Bool
    var bounds: PaletteBounds
    var panel_height: Float32
    var previous_hot: UInt64
    var clicked_handled: Bool
    var asset_openai: Int32
    var asset_claude: Int32

    def __init__(out self, key: UInt64, style: TextStyle) raises:
        self.model = PaletteModel(browser=False, editor=False, insert_path=True)
        self.key = key
        self.field_key = key + 1
        self.list_key = key + 2
        self.row_key = key + 65536
        self.field = TextEdit(self.field_key, style)
        self.field.label = "Command palette"
        self.scroll_top = True
        self.scroll_selected = False
        self.bounds = PaletteBounds(0, 0, 0, 0)
        self.panel_height = 0
        self.previous_hot = 0
        self.clicked_handled = False
        self.asset_openai = -1
        self.asset_claude = -1

    def opened(self) -> Bool:
        return self.model.controller.opened

    def clear_flags(mut self):
        self.field.changed = False
        self.field.submitted = False
        self.field.cancelled = False
        self.field.tab = 0

    def discard_undo(mut self):
        self.field.undo = List[Edit]()
        self.field.undo_top = 0

    def trim_undo(mut self):
        var bytes = 0
        for edit in self.field.undo:
            bytes += edit.removed.capacity() + edit.inserted.capacity()
        if bytes > PALETTE_UNDO_BYTES or len(self.field.undo) > 64:
            self.discard_undo()

    def valid_display(self) -> Bool:
        var selection = self.field.selection()
        return (
            len(self.field.buf) <= PALETTE_INPUT_BYTES
            and len(self.field.marked)
            <= PALETTE_INPUT_BYTES
            - len(self.field.buf)
            + selection[1]
            - selection[0]
        )

    def sync_field(mut self):
        if self.field.text() != self.model.controller.query:
            self.field.set_text(self.model.controller.query)
            self.field.marked.clear()
            self.field.disp.clear()
        self.field.placeholder = self.model.controller.placeholder()
        self.clear_flags()

    def open(
        mut self,
        mut ui: Ui,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
        command: Bool = False,
    ) raises:
        self.model.controller.start(fleet, command)
        self.model.rebuild(fleet, decorations)
        ui.open_layer(
            self.key + 3,
            MODAL,
            Placement.at(0, 0, shift=False),
            scrim=0,
            light_dismiss=False,
            dismiss_escape=False,
        )
        self.sync_field()
        self.field.select(len(self.field.buf), len(self.field.buf))
        self.field.focused = True
        self.field.blink_t0 = ui.time
        ui.focus = self.field_key
        ui.text_input = True
        self.scroll_top = True
        self.scroll_selected = False
        self.previous_hot = 0
        self.clicked_handled = False
        self.panel_height = 0

    def close(mut self, mut ui: Ui):
        self.model.controller.close()
        self.field.marked.clear()
        self.field.disp.clear()
        self.field.focused = False
        if ui.focus == self.field_key:
            ui.focus = 0
        ui.close_layer(self.key + 3)
        ui.text_input = False

    def refresh(
        mut self, fleet: PaletteFleet, decorations: PaletteDecorations
    ) raises:
        if self.opened():
            self.model.rebuild(fleet, decorations)
            self.field.placeholder = self.model.controller.placeholder()

    def changed(
        mut self, fleet: PaletteFleet, decorations: PaletteDecorations
    ) raises:
        var value = self.field.text()
        if value.byte_length() > PALETTE_INPUT_BYTES:
            raise Error("palette editor exceeds its input limit")
        if (
            value != self.model.controller.query
            or self.field.caret != self.model.controller.cursor
        ):
            if not self.model.controller.commit_typed(value, fleet):
                self.model.controller.set_query(value)
                self.model.controller.cursor = self.field.caret
            self.model.rebuild(fleet, decorations)
            self.sync_field()
            self.scroll_top = True
            self.scroll_selected = False
        self.trim_undo()
        self.clear_flags()

    def prepare(
        mut self,
        mut ui: Ui,
        input: TextInput,
        mut clip: SearchClipboard,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
        allow_primary: Bool = True,
    ) raises -> PaletteComponentResult:
        var result = PaletteComponentResult()
        if not self.opened():
            return result^
        self.clicked_handled = False
        self.clear_flags()
        var pointer = TextInput()
        var selection = self.field.selection()
        var remaining = (
            PALETTE_INPUT_BYTES
            - len(self.field.buf)
            + selection[1]
            - selection[0]
        )
        if len(input.marked) <= remaining:
            pointer.marked.extend(Span(input.marked))
            pointer.marked_sel_a = input.marked_sel_a
            pointer.marked_sel_b = input.marked_sel_b
        else:
            pointer.marked.extend(Span(self.field.marked))
            pointer.marked_sel_a = self.field.marked_a
            pointer.marked_sel_b = self.field.marked_b
            result.limited = True
        if input.primary and allow_primary:
            if clip.primary_text.byte_length() <= PALETTE_INPUT_BYTES - len(
                self.field.buf
            ):
                pointer.primary = input.primary
            else:
                result.limited = True
        elif input.primary:
            result.limited = True
        if ui.retained_input_begin(self.key + 3, self.field_key):
            self.field.update(ui, pointer, clip)
            ui.retained_input_end()
        else:
            self.field.focused = False
            return result^
        self.changed(fleet, decorations)
        var row_key = ui.active if ui.active >= self.row_key else ui.press_hot
        if row_key >= self.row_key and row_key - self.row_key < UInt64(len(self.model.rows)):
            var index = Int(row_key - self.row_key)
            if self.model.rows[index].action != 0 and ui.retained_input_begin(self.key + 3, row_key):
                var response = ui.interact(row_key, focusable=True, focus_on_press=False)
                ui.retained_input_end()
                if response.clicked:
                    self.model.controller.selected = index
                    self.model.controller.navigation_engaged = True
                    result.effect = self.model.activate(False, fleet)
                    self.model.rebuild(fleet, decorations)
                    self.sync_field()
                    self.scroll_top = True
                    self.clicked_handled = True
        if ui.pressed and self.panel_height > 0:
            var x = ui.press_x
            var y = ui.press_y
            if (
                x < self.bounds.x
                or x >= self.bounds.x + self.bounds.width
                or y < self.bounds.y
                or y >= self.bounds.y + self.panel_height
            ):
                self.close(ui)
                result.closed = True
                result.consumed = True
        return result^

    def insert(mut self, bytes: Span[UInt8, _]) -> Bool:
        var count = 0
        for value in bytes:
            count += Int(value != 13)
        var selection = self.field.selection()
        if (
            count
            > PALETTE_INPUT_BYTES
            - len(self.field.buf)
            + selection[1]
            - selection[0]
        ):
            return False
        var clean = List[UInt8](capacity=count)
        for value in bytes:
            if value != 13:
                clean.append(UInt8(32) if value == 10 else value)
        self.field.insert(clean.unsafe_ptr(), len(clean))
        return True

    def event(
        mut self,
        mut ui: Ui,
        input: TextInput,
        event: TextEvent,
        mut clip: SearchClipboard,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
    ) raises -> PaletteComponentResult:
        var result = PaletteComponentResult()
        if not self.opened():
            return result^
        if event.kind != TEXT_INSERT and event.kind != TEXT_COMMAND:
            return result^
        result.consumed = True
        if self.model.controller.finishing:
            return result^
        var before = self.field.text()
        var anchor = self.field.anchor
        var caret = self.field.caret
        if event.kind == TEXT_INSERT:
            var start = Int(event.start)
            var end = Int(event.end)
            if start < 0 or end < start or end > len(input.bytes):
                raise Error("invalid palette text event bounds")
            result.limited = not self.insert(Span(input.bytes)[start:end])
        else:
            var command = event.cmd & ~(CMD_EXTEND | CMD_LOGICAL)
            if event.cmd & CMD_EXTEND == 0 and (command == CMD_UP or command == CMD_DOWN) and event.mods & (
                MOD_ALT | MOD_CMD
            ) == 0:
                self.model.navigate(-1 if command == CMD_UP else 1)
                self.scroll_selected = True
            elif (
                command == CMD_NEWLINE
                or command == CMD_NEWLINE_RAW
                or command == CMD_TAB
            ):
                result.effect = self.model.activate(command == CMD_TAB, fleet)
                self.model.rebuild(fleet, decorations)
                self.sync_field()
                self.scroll_top = True
            elif command == CMD_BACKTAB:
                _ = self.model.leave(False, fleet, decorations)
                self.sync_field()
                self.scroll_selected = True
            elif command == CMD_CANCEL:
                self.close(ui)
                result.closed = True
            elif command == CMD_DELETE_BACK and len(self.field.buf) == 0:
                _ = self.model.leave(True, fleet, decorations)
                self.sync_field()
                self.scroll_top = True
            elif command == CMD_PASTE:
                if clip.text.byte_length() > 0:
                    result.limited = not self.insert(clip.text.as_bytes())
            else:
                self.field.command(ui, event.cmd, event.mods, clip)
        if self.opened() and not self.valid_display():
            self.field.set_text(before)
            self.field.select(anchor, caret)
            result.limited = True
        self.field.blink_t0 = ui.time
        if self.opened():
            self.changed(fleet, decorations)
        return result^

    def build(
        mut self,
        mut ui: Ui,
        theme: PennaTheme,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
        width: Float32,
        height: Float32,
    ) raises -> PaletteComponentResult:
        var result = PaletteComponentResult()
        if not self.opened():
            return result^
        if not self.valid_display():
            raise Error("palette editor exceeds its input limit")
        if self.asset_openai < 0:
            self.asset_openai, self.asset_claude = palette_asset_documents(ui)
        if not ui.layer_begin(
            self.key + 3, Style.col().width(width).height(height)
        ):
            return result^
        _ = ui.interact(self.field_key, focusable=True, focus_on_press=False)
        ui.text_field(self.field_key)
        self.field.focused = ui.focus == self.field_key
        self.bounds = palette_bounds(width, height, len(self.model.rows))
        var content_width = palette_surface_begin(
            ui,
            theme,
            self.key,
            self.bounds.x,
            self.bounds.y,
            self.bounds.width,
            overlay=False,
        )
        var pills = palette_place_pills(self.model.controller, decorations)
        var kit_pills = List[KitPill](capacity=len(pills))
        for pill in pills:
            kit_pills.append(
                KitPill(
                    pill.kind,
                    pill.label,
                    pill.prefix,
                    pill.status.value() if pill.status else UInt8(0),
                )
            )
        palette_input(ui, theme, self.field, Span(kit_pills), content_width)
        var usage = palette_component_usage(self.model, fleet)
        palette_usage(
            ui, theme, usage.value() if usage else String(), content_width, True
        )
        var rows_width = palette_list_begin(ui, theme, content_width)
        if len(self.model.rows) == 0:
            palette_empty(ui, theme, "No matches", rows_width)
        else:
            if self.scroll_top:
                ui.scroll_to(self.list_key, 0)
                self.scroll_top = False
            if self.scroll_selected and self.model.controller.selected:
                var selected_y = (
                    Float32(self.model.controller.selected.value()) * 28
                )
                if self.list_key in ui.scroll_areas:
                    var current = ui.scroll_areas[self.list_key].y.offset
                    if selected_y < current:
                        ui.scroll_to(self.list_key, selected_y)
                    elif selected_y + 28 > current + self.bounds.list_height:
                        ui.scroll_to(
                            self.list_key,
                            selected_y + 28 - self.bounds.list_height,
                        )
                self.scroll_selected = False
            var first, last = ui.scroll_rows(
                self.list_key,
                Style.col().width(rows_width).height(self.bounds.list_height),
                len(self.model.rows),
                28,
            )
            var clicked: Optional[Int] = None
            for index in range(first, last):
                ref row = self.model.rows[index]
                ui.open(
                    Style.col()
                    .width(rows_width)
                    .height(28)
                    .at(0, Float32(index) * 28)
                )
                if row.action == 0:
                    palette_section(ui, theme, row.label, row.hint, rows_width)
                else:
                    var key = self.row_key + UInt64(index)
                    if ui.hot == key and ui.hot != self.previous_hot:
                        self.model.controller.selected = index
                        self.model.controller.navigation_engaged = True
                    var kit_row = palette_kit_row(row, self.asset_openai, self.asset_claude)
                    if palette_row(
                        ui,
                        theme,
                        key,
                        kit_row,
                        self.model.controller.selected == Optional[Int](index),
                        rows_width,
                    ) and not self.clicked_handled:
                        clicked = index
                ui.close()
            self.previous_hot = ui.hot
            ui.scroll_end()
            if clicked:
                self.model.controller.selected = clicked
                self.model.controller.navigation_engaged = True
                result.effect = self.model.activate(False, fleet)
                self.model.rebuild(fleet, decorations)
                self.sync_field()
                self.scroll_top = True
        palette_list_end(ui)
        var hints = palette_component_hints(self.model)
        var kit_hints = List[KitHint](capacity=len(hints))
        for hint in hints:
            var groups = List[List[PaletteKeyPart]]()
            for name in palette_words(hint.key):
                groups.append(key_parts(KitKeystroke(name)))
            kit_hints.append(KitHint(hint.label, groups^))
        palette_footer(ui, theme, Span(kit_hints), content_width)
        self.panel_height = ui.close_and_measure(self.bounds.width)
        ui.layer_end()
        return result^
