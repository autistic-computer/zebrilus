from std.sys.info import CompilationTarget
from mjui.ui import Ui, Style, TextStyle, START, END
from mjui.input import (
    Clipboard,
    KeyEvent,
    TextInput,
    TEXT_INSERT,
    CMD_UP,
    CMD_DOWN,
    CMD_NEWLINE,
    CMD_NEWLINE_RAW,
    CMD_CANCEL,
    CMD_PASTE,
    CMD_EXTEND,
    CMD_LOGICAL,
    MOD_SHIFT,
    MOD_CTRL,
    MOD_ALT,
    MOD_CMD,
)
from mjui.keys import Key
from mjui.sys.target import USE_SWASH_RASTER
from mjui.text.paragraph import TextOpts
from mjui.widgets.editor import Editor
from mjui.widgets.highlight import (
    Highlighter,
    LANG_TEXT,
    LANG_RUST,
    LANG_JSON,
    LANG_TOML,
    LANG_MARKDOWN,
)
from mjui.widgets.key import sub
from mjui.widgets.markdown import CODE_STYLED, MdTheme
from mjui.widgets.penna import PennaTheme, button
from mjui.widgets.penna_native import root_color
from mjui.widgets.penna_icon_names import named_icon
from mjui.widgets.penna_palette import PaletteRow, palette_row
from mjui.widgets.penna_palette_input import PalettePill, palette_input, palette_empty
from mjui.widgets.penna_palette_shortcuts import (
    PaletteKeystroke,
    PaletteKeyPart,
    key_parts,
)
from mjui.widgets.penna_palette_surface import (
    PaletteHint,
    palette_surface_begin,
    palette_surface_end,
    palette_usage,
    palette_list_begin,
    palette_list_end,
    palette_footer,
)
from mjui.widgets.textedit import TextEdit
from .editor_files import read_editor_file, save_editor_file
from .editor_state import (
    EDITOR_OPEN_HINT,
    EDITOR_PICKER_PLACEHOLDER,
    EDITOR_RETRY_LABEL,
    EDITOR_SET_BUFFER,
    EDITOR_SET_LANGUAGE,
    EDITOR_LOAD,
    EDITOR_WRITE,
    EDITOR_PROMPT_NEW_PATH,
    EDITOR_PUBLISH_PATH,
    EDITOR_PUBLISH_TITLE,
    EditorEffect,
    EditorModel,
    descriptor_path_error,
)
from .file_picker import FilePicker, PICKER_SCANNING_NOTE, PICKER_LIMITED_NOTE
from .path_pattern import MAX_PATH_QUERY

comptime EDITOR_KEY_PASS: UInt8 = 0
comptime EDITOR_KEY_TEXT: UInt8 = 1
comptime EDITOR_KEY_TAKEN: UInt8 = 2

comptime EDITOR_OUT_COMMAND: UInt8 = 0
comptime EDITOR_OUT_SAVE_AS: UInt8 = 1

comptime EDITOR_CMD_OPEN: UInt8 = 0
comptime EDITOR_CMD_SAVE: UInt8 = 1
comptime EDITOR_CMD_SAVE_AS: UInt8 = 2
comptime EDITOR_CMD_RETRY: UInt8 = 3

comptime EDITOR_POINTER_DOWN: UInt8 = 0
comptime EDITOR_BUTTON_LEFT: UInt8 = 1

comptime EDITOR_FONT_SIZE: Float32 = 13
comptime EDITOR_PICKER_ROW: Float32 = 28
comptime EDITOR_PICKER_SLICE_NS: UInt64 = 4000000
comptime EDITOR_QUERY_REFUSED = "Search text cannot exceed 65536 bytes."


@fieldwise_init
struct EditorKeyRoute(TrivialRegisterPassable):
    var kind: UInt8


@fieldwise_init
struct EditorOutbound(Copyable, Movable):
    var pane: UInt64
    var kind: UInt8
    var name: String
    var arguments: List[String]


def editor_descriptor_refusal(path: Optional[String], cwd: String) -> Optional[String]:
    if path:
        return descriptor_path_error(path.value(), cwd)
    return descriptor_path_error(cwd, cwd)


def editor_highlight(language: String) -> Int:
    if language == "rust":
        return LANG_RUST
    if language == "markdown":
        return LANG_MARKDOWN
    if language == "json":
        return LANG_JSON
    if language == "toml":
        return LANG_TOML
    return LANG_TEXT


def editor_hint(mut ui: Ui, th: PennaTheme, width: Float32, height: Float32):
    var hint = String(EDITOR_OPEN_HINT)
    ui.open(Style.col().at(0, 0).width(width).height(height).center())
    ui.text_opts(
        hint.unsafe_ptr(),
        hint.byte_length(),
        TextStyle(th.flat.mono, 14, th.flat.dim, False, root_color(th.theme, 2)),
        TextOpts().line_height(20),
    )
    ui.close()


def editor_error_panel(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    title: String,
    message: String,
    width: Float32,
    height: Float32,
) raises -> Bool:
    var inner = min(Float32(360), max(width - 24, Float32(0)))
    var body = max(inner - 24, Float32(0))
    var retry = False
    ui.open(Style.col().at(0, 0).width(width).height(height).center().fill(th.flat.bg))
    var paint = th.paint("inline-panel")
    ui.open(paint.style(Style.col().width(inner).pad(12, 12).spacing(8)))
    paint.decorations(ui)
    ui.open(Style.row().width(body).spacing(8).align(START, START))
    var words = body
    comptime if USE_SWASH_RASTER:
        if th.flat.penna_index >= 0:
            ui.open(Style.col().width(16).height(18).center())
            named_icon(ui, th.flat, "triangle-alert", 16, th.flat.danger)
            ui.close()
            words = max(body - 24, Float32(0))
    ui.open(Style.col().width(words).spacing(4))
    ui.text_opts(
        title.unsafe_ptr(),
        title.byte_length(),
        TextStyle(th.flat.mono, 13, th.flat.fg, False, root_color(th.theme, 1)),
        TextOpts().line_height(18),
    )
    ui.text_opts(
        message.unsafe_ptr(),
        message.byte_length(),
        TextStyle(th.flat.mono, 12, th.flat.dim, True, root_color(th.theme, 2)),
        TextOpts().line_height(16),
    )
    ui.close()
    ui.close()
    ui.open(Style.row().width(body).spacing(8).align(END, START))
    if button(ui, th, key, EDITOR_RETRY_LABEL, tone="primary", prominent=True):
        retry = True
    ui.close()
    ui.close()
    ui.close()
    return retry


struct EditorPane(Movable):
    var pane: UInt64
    var base: UInt64
    var model: EditorModel
    var editor: Editor
    var hl: Highlighter
    var picker: Optional[FilePicker]
    var field: TextEdit
    var previous_hot: UInt64
    var to_top: Bool
    var reveal: Bool
    var left: Float32
    var top: Float32
    var width: Float32
    var height: Float32
    var surface_left: Float32
    var surface_top: Float32
    var surface_right: Float32
    var surface_bottom: Float32
    var focused: Bool
    var theme: Int
    var font_size: Float32
    var outbox: List[EditorOutbound]

    def __init__(
        out self, pane: UInt64, path: Optional[String], cwd: String, th: PennaTheme
    ) raises:
        var refused = editor_descriptor_refusal(path, cwd)
        if refused:
            raise Error(refused.value())
        self.pane = pane
        self.base = 0
        self.font_size = EDITOR_FONT_SIZE
        self.theme = th.theme
        self.editor = Editor(
            0,
            th.flat,
            MdTheme.of(th.flat, th.flat.font_italic, 14, EDITOR_FONT_SIZE, 10),
            LANG_TEXT,
            wrap=True,
        )
        self.editor.hosted = True
        self.hl = Highlighter(CODE_STYLED)
        self.picker = None
        self.field = TextEdit(
            0,
            TextStyle(th.flat.font, 12, th.flat.fg, False),
            placeholder=EDITOR_PICKER_PLACEHOLDER,
        )
        self.field.label = EDITOR_PICKER_PLACEHOLDER
        self.previous_hot = 0
        self.to_top = False
        self.reveal = False
        self.left = 0
        self.top = 0
        self.width = 0
        self.height = 0
        self.surface_left = 0
        self.surface_top = 0
        self.surface_right = 0
        self.surface_bottom = 0
        self.focused = False
        self.outbox = List[EditorOutbound]()
        var effects = List[EditorEffect]()
        self.model = EditorModel.open(path, cwd, effects)
        self._apply(effects^)

    def take_outbox(mut self) -> List[EditorOutbound]:
        var out = self.outbox^
        self.outbox = List[EditorOutbound]()
        return out^

    def title(self) -> String:
        return self.model.title

    def dirty(self) -> Bool:
        return self.model.dirty

    def picking(self) -> Bool:
        return Bool(self.picker)

    def set_focused(mut self, focused: Bool):
        self.focused = focused

    def _adopt(mut self, key: UInt64):
        if key == self.base:
            return
        self.base = key
        self.editor.key = sub(key, 2)
        self.field.key = sub(key, 4)

    def _target(self) -> String:
        return String("%", self.pane)

    def _apply(mut self, var effects: List[EditorEffect]) raises:
        var index = 0
        while index < len(effects):
            var effect = effects[index].copy()
            index += 1
            if effect.kind == EDITOR_SET_BUFFER:
                self.editor.set_language(editor_highlight(effect.detail))
                self.editor.set_text(effect.text)
                self.editor.changed = False
            elif effect.kind == EDITOR_SET_LANGUAGE:
                self.editor.set_language(editor_highlight(effect.text))
            elif effect.kind == EDITOR_LOAD:
                var read = read_editor_file(effect.text)
                if read.ok:
                    var more = self.model.finish_load(effect.text, read.contents)
                    for item in more:
                        effects.append(item.copy())
                else:
                    self.model.load_failed(effect.text, read.message)
            elif effect.kind == EDITOR_WRITE:
                var failure = save_editor_file(effect.text, effect.detail)
                if failure:
                    self.model.save_failed(effect.text, failure.value())
                else:
                    var more = self.model.finish_save(
                        effect.text, effect.detail, self.editor.text()
                    )
                    for item in more:
                        effects.append(item.copy())
            elif effect.kind == EDITOR_PROMPT_NEW_PATH:
                var arguments = List[String]()
                arguments.append(effect.detail)
                self.outbox.append(
                    EditorOutbound(self.pane, EDITOR_OUT_SAVE_AS, effect.text, arguments^)
                )
            elif effect.kind == EDITOR_PUBLISH_PATH:
                var arguments = List[String]()
                arguments.append("-t")
                arguments.append(self._target())
                arguments.append(effect.text)
                self.outbox.append(
                    EditorOutbound(
                        self.pane, EDITOR_OUT_COMMAND, "set-editor-path", arguments^
                    )
                )
            elif effect.kind == EDITOR_PUBLISH_TITLE:
                var arguments = List[String]()
                arguments.append("-t")
                arguments.append(self._target())
                arguments.append("-T")
                arguments.append(effect.text)
                self.outbox.append(
                    EditorOutbound(
                        self.pane, EDITOR_OUT_COMMAND, "select-pane", arguments^
                    )
                )
            else:
                raise Error("unknown editor effect")

    def descriptor(mut self, path: Optional[String], cwd: String) raises:
        var refused = editor_descriptor_refusal(path, cwd)
        if refused:
            raise Error(refused.value())
        var effects = self.model.synchronize(path, cwd)
        self._apply(effects^)

    def command(mut self, kind: UInt8, text: String = String()) raises:
        if kind == EDITOR_CMD_OPEN:
            if not self.model.open_picker():
                return
            self.field.clear()
            try:
                self.picker = FilePicker(self.model.cwd, False)
            except error:
                _ = self.model.picker_closed(None)
                raise Error(String(error))
            self.to_top = True
        elif kind == EDITOR_CMD_SAVE:
            var effects = self.model.save(self.editor.text())
            self._apply(effects^)
        elif kind == EDITOR_CMD_SAVE_AS:
            var effects = self.model.begin_save(text, self.editor.text())
            self._apply(effects^)
        elif kind == EDITOR_CMD_RETRY:
            if not self.model.error:
                return
            var failed = self.model.error.value().copy()
            var effects = self.model.retry(
                failed.retry, failed.path, self.editor.text()
            )
            self._apply(effects^)
        else:
            raise Error("unknown editor command")

    def _close(mut self, selected: Optional[String]) raises:
        self.picker = None
        self.field.clear()
        var effects = self.model.picker_closed(selected)
        self._apply(effects^)

    def _query(mut self) raises:
        var query = self.field.text()
        self.picker.value().set_query(query)
        self.to_top = True

    def _navigate(mut self, direction: Int):
        if self.picker.value().navigate(direction):
            self.reveal = True

    def _accept_selected(mut self) raises:
        var chosen = self.picker.value().accept_selected()
        if chosen:
            self._close(chosen)

    def _picker_insert(mut self, bytes: Span[UInt8, _]) raises -> Bool:
        var count = 0
        for value in bytes:
            count += Int(value != 13)
        var selection = self.field.selection()
        if count > MAX_PATH_QUERY - len(self.field.buf) + selection[1] - selection[0]:
            self.picker.value().flow.error = Optional[String](
                String(EDITOR_QUERY_REFUSED)
            )
            return False
        var clean = List[UInt8](capacity=count)
        for value in bytes:
            if value != 13:
                clean.append(UInt8(32) if value == 10 else value)
        self.field.insert(clean.unsafe_ptr(), len(clean))
        return True

    def _picker_event[C: Clipboard](
        mut self, mut ui: Ui, input: TextInput, index: Int, mut clip: C
    ) raises:
        ref event = input.events[index]
        if event.kind == TEXT_INSERT:
            var start = Int(event.start)
            var end = Int(event.end)
            if start < 0 or end < start or end > len(input.bytes):
                raise Error("editor picker text range is outside the input")
            if self._picker_insert(Span(input.bytes)[start:end]):
                self._query()
            self.field.blink_t0 = ui.time
            return
        var command = event.cmd & ~(CMD_EXTEND | CMD_LOGICAL)
        if (
            event.cmd & CMD_EXTEND == 0
            and (command == CMD_UP or command == CMD_DOWN)
            and event.mods & (MOD_ALT | MOD_CMD) == 0
        ):
            self._navigate(-1 if command == CMD_UP else 1)
        elif command == CMD_NEWLINE or command == CMD_NEWLINE_RAW:
            self._accept_selected()
        elif command == CMD_CANCEL:
            self._close(None)
        elif command == CMD_PASTE:
            var pasted = clip.get_clipboard_bounded(MAX_PATH_QUERY)
            if self._picker_insert(pasted.as_bytes()):
                self._query()
        else:
            var before = self.field.text()
            self.field.command(ui, event.cmd, event.mods, clip)
            if self.field.text() != before:
                self._query()
        self.field.blink_t0 = ui.time

    def text[C: Clipboard](
        mut self,
        mut ui: Ui,
        input: TextInput,
        mut clip: C,
        key: UInt64,
        skip: Int = 0,
    ) raises:
        self._adopt(key)
        if self.picker:
            self.editor.focused = False
            if self.focused:
                for index in range(skip, len(input.events)):
                    if not self.picker:
                        break
                    self._picker_event(ui, input, index, clip)
            return
        self.editor.hosted_focus = self.focused
        if skip > 0:
            var rest = TextInput()
            for value in input.bytes:
                rest.bytes.append(value)
            for index in range(skip, len(input.events)):
                rest.events.append(input.events[index])
            for value in input.marked:
                rest.marked.append(value)
            rest.marked_sel_a = input.marked_sel_a
            rest.marked_sel_b = input.marked_sel_b
            self.editor.update(ui, rest, clip)
        else:
            self.editor.update(ui, input, clip)
        if self.editor.changed:
            self.editor.changed = False
            var effects = self.model.buffer_changed(self.editor.text())
            self._apply(effects^)

    def key(mut self, event: KeyEvent) raises -> EditorKeyRoute:
        if event.release or event.consumed:
            return EditorKeyRoute(EDITOR_KEY_PASS)
        var shortcut = MOD_CMD if CompilationTarget.is_macos() else MOD_CTRL
        var held = event.mods & (MOD_SHIFT | MOD_CTRL | MOD_ALT | MOD_CMD)
        if held == shortcut and event.key == Key.O:
            self.command(EDITOR_CMD_OPEN)
            return EditorKeyRoute(EDITOR_KEY_TAKEN)
        if held == shortcut and event.key == Key.S:
            self.command(EDITOR_CMD_SAVE)
            return EditorKeyRoute(EDITOR_KEY_TAKEN)
        if event.text:
            return EditorKeyRoute(EDITOR_KEY_TEXT)
        if self.picker and held == 0:
            if event.key == Key.ESCAPE:
                self._close(None)
                return EditorKeyRoute(EDITOR_KEY_TAKEN)
            if event.key == Key.ENTER or event.key == Key.KP_ENTER:
                self._accept_selected()
                return EditorKeyRoute(EDITOR_KEY_TAKEN)
            if event.key == Key.UP or event.key == Key.DOWN:
                self._navigate(-1 if event.key == Key.UP else 1)
                return EditorKeyRoute(EDITOR_KEY_TAKEN)
        return EditorKeyRoute(EDITOR_KEY_PASS)

    def _inside(self, x: Float32, y: Float32) -> Bool:
        return (
            x >= self.left
            and y >= self.top
            and x < self.left + self.width
            and y < self.top + self.height
        )

    def pointer(
        mut self,
        phase: UInt8,
        button: UInt8,
        pressed: UInt8,
        x: Float32,
        y: Float32,
        mods: UInt32,
        clicks: Int32 = 1,
    ) raises -> Bool:
        if (
            self.picker
            and phase == EDITOR_POINTER_DOWN
            and button == EDITOR_BUTTON_LEFT
        ):
            var on_surface = (
                x >= self.surface_left
                and y >= self.surface_top
                and x < self.surface_right
                and y < self.surface_bottom
            )
            if not on_surface:
                self._close(None)
        return self._inside(x, y)

    def wheel(
        self, x: Float32, y: Float32, dx: Float32, dy: Float32, mods: UInt32
    ) -> Bool:
        return self._inside(x, y)

    def _error_panel(mut self, mut ui: Ui, th: PennaTheme) raises:
        var failed = self.model.error.value().copy()
        if editor_error_panel(
            ui,
            th,
            sub(self.base, 1),
            failed.title(),
            failed.message,
            self.width,
            self.height,
        ):
            self.command(EDITOR_CMD_RETRY)

    def _picker_panel(mut self, mut ui: Ui, th: PennaTheme) raises:
        if self.picker.value().advance(EDITOR_PICKER_SLICE_NS):
            ui.request_wake(ui.time)
        var surface_key = sub(self.base, 3)
        var list_key = sub(self.base, 5)
        var rows_key = sub(self.base, 6)
        var surface_width = min(Float32(560), max(self.width - 32, Float32(0)))
        var x = self.left + (self.width - surface_width) * 0.5
        var y = self.top + self.height * 0.12
        var limit = min(max(self.height * 0.88 - 175, Float32(40)), Float32(440))
        _ = ui.interact(self.field.key, focusable=True, focus_on_press=False)
        ui.text_field(self.field.key)
        self.field.focused = self.focused
        var count = len(self.picker.value().rows)
        var note = String()
        if self.picker.value().flow.error:
            note = self.picker.value().flow.error.value()
        elif self.picker.value().flow.limited:
            note = String(PICKER_LIMITED_NOTE)
        elif self.picker.value().flow.scanning_note(count > 0):
            note = String(PICKER_SCANNING_NOTE)
        var content_width = palette_surface_begin(
            ui, th, surface_key, x, y, surface_width, overlay=True
        )
        var pills = List[PalettePill]()
        palette_input(ui, th, self.field, Span(pills), content_width)
        palette_usage(ui, th, note, content_width, True)
        var rows_width = palette_list_begin(ui, th, content_width)
        var clicked = -1
        if count == 0:
            var message = self.picker.value().empty_message()
            palette_empty(ui, th, message, rows_width)
        else:
            var list_height = min(Float32(count) * EDITOR_PICKER_ROW, limit)
            var selected = self.picker.value().flow.selected
            if list_key in ui.scroll_areas:
                if self.to_top:
                    ui.scroll_to(list_key, 0)
                elif self.reveal and selected:
                    var selected_y = Float32(selected.value()) * EDITOR_PICKER_ROW
                    var current = Float32(ui.scroll_areas[list_key].y.offset)
                    if selected_y < current:
                        ui.scroll_to(list_key, selected_y)
                    elif selected_y + EDITOR_PICKER_ROW > current + list_height:
                        ui.scroll_to(
                            list_key, selected_y + EDITOR_PICKER_ROW - list_height
                        )
            self.to_top = False
            self.reveal = False
            var first, last = ui.scroll_rows(
                list_key,
                Style.col().width(rows_width).height(list_height),
                count,
                EDITOR_PICKER_ROW,
            )
            for index in range(first, last):
                var row_key = sub(rows_key, index)
                ui.open(
                    Style.col()
                    .width(rows_width)
                    .height(EDITOR_PICKER_ROW)
                    .at(0, Float32(index) * EDITOR_PICKER_ROW)
                )
                if ui.hot == row_key and ui.hot != self.previous_hot:
                    _ = self.picker.value().hover(index)
                var row = PaletteRow(self.picker.value().rows[index].relative)
                var chosen = self.picker.value().flow.selected == Optional[Int](index)
                if palette_row(ui, th, row_key, row, chosen, rows_width):
                    clicked = index
                ui.close()
            self.previous_hot = ui.hot
            ui.scroll_end()
        palette_list_end(ui)
        comptime if USE_SWASH_RASTER:
            if th.flat.penna_index >= 0:
                var hints = List[PaletteHint]()
                var select = List[List[PaletteKeyPart]]()
                select.append(key_parts(PaletteKeystroke("up")))
                select.append(key_parts(PaletteKeystroke("down")))
                hints.append(PaletteHint("select", select^))
                var open = List[List[PaletteKeyPart]]()
                open.append(key_parts(PaletteKeystroke("enter")))
                hints.append(PaletteHint("open", open^))
                var close = List[List[PaletteKeyPart]]()
                close.append(key_parts(PaletteKeystroke("escape")))
                hints.append(PaletteHint("close", close^))
                palette_footer(ui, th, Span(hints), content_width)
        palette_surface_end(ui)
        var surface = ui.rect_of(surface_key)
        if surface.key != 0:
            self.surface_left = surface.x0
            self.surface_top = surface.y0
            self.surface_right = surface.x1
            self.surface_bottom = surface.y1
        else:
            self.surface_left = x
            self.surface_top = y
            self.surface_right = x + surface_width
            self.surface_bottom = self.top + self.height
            ui.request_wake(ui.time)
        if clicked >= 0:
            var path = self.picker.value().accept(clicked)
            if path:
                self._close(path)

    def build(mut self, mut ui: Ui, th: PennaTheme, key: UInt64) raises:
        self._adopt(key)
        if th.theme != self.theme:
            self.theme = th.theme
            self.editor.use_theme(
                th.flat,
                MdTheme.of(th.flat, th.flat.font_italic, 14, self.font_size, 10),
            )
        var rect = ui.rect_of(key)
        if rect.key != 0:
            self.left = rect.x0
            self.top = rect.y0
            self.width = rect.x1 - rect.x0
            self.height = rect.y1 - rect.y0
        else:
            ui.request_wake(ui.time)
        ui.open(Style.col().grow().grow_h().clipped().id(key))
        self.editor.build(ui, self.hl, Style.col().grow().grow_h())
        if self.editor.needs_frame(ui.time):
            ui.request_wake(ui.time)
        var bare = (
            not self.model.path and not self.model.error and not self.model.picker_open
        )
        if bare and self.editor.store.byte_count() == 0:
            editor_hint(ui, th, self.width, self.height)
        if self.model.error:
            self._error_panel(ui, th)
        ui.close()
        if self.picker:
            self._picker_panel(ui, th)
