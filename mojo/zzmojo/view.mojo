from mjui.ui import Ui, Style, TextStyle
from mjui.input import (
    TextInput,
    TextEvent,
    TEXT_INSERT,
    MouseEvent,
    MOUSE_DOWN,
    MOUSE_RIGHT_DOWN,
    MOUSE_UP,
    MOUSE_RIGHT_UP,
    INSERT_COMPOSED,
    Clipboard,
    KeyEvent,
    CMD_PASTE,
)
from mjui.scroll import ScrollEvent
from mjui.keys import Key
from mjui.widgets.penna import (
    PennaTheme,
    DEFAULT_DARK,
    panel_begin,
    panel_end,
    row,
    button,
    label,
)
from .backend import (
    AsyncBackend,
    KEY,
    CREATE_SESSION,
    SPLIT_RIGHT,
    SPLIT_DOWN,
    RECONNECT,
)
from .state import AppModel
from .terminal_painter import render_viewport
from .terminal_history_painter import render_retained_viewport
from .local_scroll_controller import (
    TerminalScrollController,
    PaneScrollEffect,
    scroll_context,
)
from .input_protocol import input_view
from .terminal_view import accumulate_scroll
from .workspace_layout import window_rects, PaneRect
from .geometry import GridSize, grid_dimension
from mjui.term.grid import cell_metrics
from mjui.term.rows import TermView
from .input_protocol import KeyInput, input_key, mjui_key_input
from .terminal_projection import grid_placement
from .pointer_controller import PointerController, PointerSurface, RoutedPointer
from .terminal_view import PointerGrid
from .command_output import CommandOutputPresentation
from .terminal_model import TerminalMode, TerminalSearch
from .terminal_search import SearchEffect, SEARCH_FOCUS, TERMINAL_FOCUS, SEARCH_ACCEPT_AND_CLOSE, search_shortcut, search_key_action
from .terminal_search_protocol import SearchQuery
from .terminal_search_controller import TerminalSearchController, SearchTarget, SearchPlan
from .terminal_search_editor import SearchInputResult, SEARCH_TEXT_BYTES
from .terminal_search_chrome import SearchKeys, build_search_chrome, build_mode_indicator
from .search_clipboard import SearchClipboard
from .keyboard_controller import KeyboardController

comptime TERMINAL_KEY: UInt64 = 0x7000000000000000


struct WorkspaceView(Movable):
    var path: String
    var backend: AsyncBackend
    var focus_epoch: UInt64
    var model: AppModel
    var theme: PennaTheme
    var next_poll: Float64
    var event_driven: Bool
    var active: Bool
    var pointer: PointerController
    var visible_panes: List[UInt64]
    var next_autoscroll: Float64
    var pointer_focus_pane: Optional[UInt64]
    var pointer_focus_output: Bool
    var output: CommandOutputPresentation
    var output_input: Bool
    var local_scroll: TerminalScrollController
    var output_seen: Optional[UInt64]
    var marked: Optional[String]
    var keys: Dict[String, UInt64]
    var next_key: UInt64
    var reported: Dict[UInt64, GridSize]
    var reported_window: Optional[GridSize]
    var reported_session: Optional[UInt64]
    var reported_client: UInt64
    var reported_server: UInt64
    var search: TerminalSearchController
    var search_clipboard: SearchClipboard
    var search_frame: SearchInputResult
    var search_prepared: Bool
    var search_fields: Dict[String, Bool]

    def __init__(out self, path: String) raises:
        self.path = path
        self.backend = AsyncBackend(path)
        self.focus_epoch = 0
        self.model = AppModel()
        self.theme = PennaTheme(DEFAULT_DARK, 0, 1, 2, 3)
        self.next_poll = 0
        self.event_driven = False
        self.active = True
        self.pointer = PointerController()
        self.visible_panes = List[UInt64]()
        self.next_autoscroll = 0
        self.pointer_focus_pane = None
        self.pointer_focus_output = False
        self.output = CommandOutputPresentation()
        self.output_input = False
        self.local_scroll = TerminalScrollController()
        self.output_seen = None
        self.marked = None
        self.keys = Dict[String, UInt64]()
        self.next_key = 100
        self.reported = Dict[UInt64, GridSize]()
        self.reported_window = None
        self.reported_session = None
        self.reported_client = 0
        self.reported_server = 0
        self.search = TerminalSearchController()
        self.search_clipboard = SearchClipboard()
        self.search_frame = SearchInputResult()
        self.search_prepared = False
        self.search_fields = Dict[String, Bool]()
        self.backend.submit(self.model, self.focus_epoch)

    def widget_key(mut self, kind: Int, id: UInt64) raises -> UInt64:
        var name = String(kind, ":", id)
        if name not in self.keys:
            if self.next_key == TERMINAL_KEY:
                raise Error("widget identifiers exhausted")
            self.keys[name] = self.next_key
            self.next_key += 1
        return self.keys[name]

    def search_target(self) -> Optional[SearchTarget]:
        var pane = self.input_pane()
        if not pane:
            return None
        if self.output_input and self.output.owns(pane.value()):
            return SearchTarget(pane.value(), True, self.output.id.value())
        return SearchTarget(pane.value(), False, pane.value())

    def ensure_search(mut self, target: SearchTarget) raises:
        var first = 17 if target.command_output else 8
        var keys = SearchKeys(
            self.widget_key(first, target.owner),
            self.widget_key(first + 1, target.owner),
            self.widget_key(first + 2, target.owner),
            self.widget_key(first + 3, target.owner),
            self.widget_key(first + 4, target.owner),
            self.widget_key(first + 5, target.owner),
            self.widget_key(first + 6, target.owner),
            self.widget_key(first + 7, target.owner),
        )
        self.search.ensure(target, keys, TextStyle(self.theme.flat.font, 13, self.theme.flat.fg, False))

    def search_valid(self, target: SearchTarget) -> Bool:
        if target.command_output:
            return self.output.id == Optional[UInt64](target.owner) and self.output.owns(target.pane)
        return target.pane in self.model.terminals

    def observe_search(mut self, mut ui: Ui) raises:
        var previous = self.search.target_for_field(ui.focus)
        var valid = List[SearchTarget]()
        for pane in self.model.terminals.keys():
            valid.append(SearchTarget(pane, False, pane))
        if self.output.id and self.output.pane:
            valid.append(SearchTarget(self.output.pane.value(), True, self.output.id.value()))
        self.search.observe(self.model.source_server, self.model.source_client, Span(valid))
        if previous and not self.search.target_for_field(ui.focus):
            ui.focus = TERMINAL_KEY

    def send_search(
        mut self,
        target: SearchTarget,
        frame: List[UInt8],
        epoch: UInt64,
        passive: Bool = False,
        source: Optional[Tuple[UInt64, UInt64]] = None,
    ) raises:
        if len(frame) == 0 or not self.search_valid(target):
            return
        self.backend.input(target.pane, frame, epoch, passive or target.command_output, source, Optional[UInt64](target.owner) if target.command_output else Optional[UInt64](None))
        self.next_poll = 0

    def apply_search_effect(
        mut self,
        mut ui: Ui,
        target: SearchTarget,
        effect: SearchEffect,
        passive: Bool = False,
        epoch: Optional[UInt64] = None,
        source: Optional[Tuple[UInt64, UInt64]] = None,
    ) raises:
        if not self.search_valid(target):
            return
        if effect.cancel_scroll and not target.command_output:
            _ = self.cancel_local_scroll(target.pane)
        if effect.clear_marked:
            self.marked = None
        if effect.focus != 0 and not passive:
            if target.command_output:
                self.output_input = True
            else:
                if self.output_input or self.model.pane != Optional[UInt64](target.pane):
                    self.blur_pointer()
                    self.model.select_pane(target.pane)
                    self.focus_epoch += 1
            if effect.focus == SEARCH_FOCUS:
                self.blur_pointer(target.command_output)
                ui.focus = self.search.keys[target.name()].field
                self.search.editors[target.name()].field.focused = True
                ui.text_input = True
            elif effect.focus == TERMINAL_FOCUS:
                if target.command_output:
                    self.focus_output(ui)
                else:
                    ui.focus = TERMINAL_KEY
                ui.text_input = False
        self.send_search(target, effect.frame, epoch.value() if epoch else self.focus_epoch, passive, source)
        if effect.redraw or len(effect.frame) > 0:
            ui.request_wake(ui.time)

    def search_results(mut self, mut ui: Ui, target: SearchTarget) raises:
        var effects = self.search_frame.effects^
        self.search_frame.effects = List[SearchEffect]()
        for effect in effects:
            self.apply_search_effect(ui, target, effect)
        if self.search_frame.limited:
            self.model.error = "Terminal search input exceeds its byte limit"

    def prepare_search_editor(mut self, mut ui: Ui, input: TextInput, target: SearchTarget, pointer: Bool = True) raises:
        var name = target.name()
        if name in self.search_fields or not self.search.editors[name].session.query:
            return
        var prepared = self.search.editors[name].prepare(ui, input, self.search_clipboard, target.pane, target.command_output, self.search.keys[name].container if pointer else UInt64(0))
        for effect in prepared.effects:
            self.search_frame.append(effect.copy())
        self.search_frame.limited = self.search_frame.limited or prepared.limited
        self.search_fields[name] = True
        self.search_results(ui, target)

    def prepare_search_input[Host: Clipboard](mut self, mut ui: Ui, input: TextInput, mut host: Host) raises:
        self.poll(ui.time)
        self.observe_output(ui)
        self.observe_search(ui)
        self.search_frame = SearchInputResult()
        self.search_clipboard = SearchClipboard()
        self.search_clipboard.primary_supported = host.supports_primary()
        self.search_fields.clear()
        var commands = self.backend.search_commands.take()
        for command in commands:
            if command.server != self.model.source_server or command.client != self.model.source_client:
                continue
            var target = SearchTarget(command.pane, False, command.pane)
            if command.output_owner:
                target = SearchTarget(command.pane, True, command.output_owner.value())
            elif self.output.owns(command.pane):
                continue
            if not self.search_valid(target):
                continue
            self.ensure_search(target)
            var effect = self.search.begin(target, command.direction, SEARCH_ACCEPT_AND_CLOSE)
            self.apply_search_effect(ui, target, effect)
        var target = self.search.target_for_field(ui.focus)
        if target and self.search_valid(target.value()):
            var name = target.value().name()
            if input.primary and self.search_clipboard.primary_supported:
                var event = input.primary.value()
                var hit = ui.hit_test(event.x, event.y)
                if hit == self.search.keys[name].field or hit == self.search.keys[name].container:
                    self.search_clipboard.primary_text = host.get_primary_selection_bounded(SEARCH_TEXT_BYTES)
            self.prepare_search_editor(ui, input, target.value())
        self.search_clipboard.deliver(host)
        self.search_prepared = True

    def search_event[Host: Clipboard](mut self, mut ui: Ui, input: TextInput, event: TextEvent, mut host: Host) raises -> Bool:
        var target = self.search.target_for_field(ui.focus)
        if not target or not self.search_valid(target.value()):
            return False
        var name = target.value().name()
        if not self.search.editors[name].session.query:
            return False
        if event.kind == 1 and event.cmd == CMD_PASTE:
            self.search_clipboard.load(host, SEARCH_TEXT_BYTES)
        var consumed = self.search.editors[name].event(ui, input, event, self.search_clipboard, target.value().pane, self.search_frame, target.value().command_output)
        if consumed:
            self.search.touch(name)
        self.search_results(ui, target.value())
        self.search_clipboard.deliver(host)
        return consumed

    def plan_search(mut self, mut ui: Ui, mut keyboard: KeyboardController, key: KeyEvent, bytes: Span[UInt8, _]) raises -> Optional[SearchPlan]:
        self.observe_search(ui)
        var input = mjui_key_input(key.key, key.ch, key.unshifted, key.mods, key.action())
        var closing_key = UInt8(2) if key.key == Key.ENTER or key.key == Key.KP_ENTER else UInt8(4) if key.key == Key.ESCAPE else UInt8(0)
        if key.release and key.key in self.search.closing_owners:
            var owner = self.search.closing_owners.pop(key.key)
            if owner.name() in self.search.editors:
                _ = self.search.editors[owner.name()].key_released(closing_key)
            keyboard.claim_text(key, bytes)
            return SearchPlan(owner, 2, input^, self.focus_epoch, self.model.source_server, self.model.source_client)
        if key.consumed or not self.active:
            return None
        var field_target = self.search.target_for_field(ui.focus)
        var chrome_target = self.search.target_for_chrome(ui.focus)
        var target = field_target
        if not target:
            target = chrome_target
        if not target:
            target = self.search_target()
        if not target or not self.search_valid(target.value()):
            return None
        if key.text_lo >= 0 and key.text_hi > key.text_lo and Int(key.text_hi) <= len(bytes):
            input.text = String(from_utf8=bytes[Int(key.text_lo):Int(key.text_hi)])
        var name = target.value().name()
        if (ui.focus == TERMINAL_KEY or field_target or chrome_target) and name in self.search.editors and self.search.editors[name].session.swallow_key(closing_key if closing_key != 0 else input.key, key.release):
            keyboard.claim_text(key, bytes)
            keyboard.claim_text_command(key)
            return SearchPlan(target.value(), 2, input^, self.focus_epoch, self.model.source_server, self.model.source_client)
        if key.release:
            return None
        if field_target and closing_key != 0 and (self.search.editors[name].session.query or self.search.editors[name].pending_begin):
            self.search.editors[name].key_pressed(closing_key)
            self.search.closing_owners[key.key] = target.value()
        var modal = False
        for session in self.model.snapshot.sessions:
            if self.model.session == Optional[UInt64](session.id):
                for window in session.windows:
                    if self.model.window == Optional[UInt64](window.id):
                        modal = Bool(window.modal)
        var prefix = not target.value().command_output and self.model.routing.claims_prefix_input(input)
        var kind: UInt8 = 255
        if (ui.focus == TERMINAL_KEY or field_target or chrome_target) and search_shortcut(input, prefix, modal):
            self.ensure_search(target.value())
            self.search.editors[name].pending_begin = True
            self.marked = None
            _ = self.cancel_local_scroll(target.value().pane) if not target.value().command_output else False
            ui.focus = self.search.keys[name].field
            kind = 0
        elif name in self.search.editors and (field_target or chrome_target) and (self.search.editors[name].session.query or self.search.editors[name].pending_begin):
            var probe = SearchQuery()
            if search_key_action(probe, input):
                kind = 1
            elif chrome_target and not field_target and (input.key == 0 or input.key == 1 or input.key == 4 or input.key == 5):
                ui.focus = self.search.keys[name].field
                keyboard.claim_text_command(key)
                kind = 4
        if kind == 255:
            return None
        keyboard.claim_text(key, bytes)
        ui.request_wake(ui.time)
        return SearchPlan(target.value(), kind, input^, self.focus_epoch, self.model.source_server, self.model.source_client)

    def dispatch_search(mut self, mut ui: Ui, input: TextInput, plan: SearchPlan) raises:
        if plan.server != self.model.source_server or plan.client != self.model.source_client or not self.search_valid(plan.target):
            return
        var name = plan.target.name()
        if name not in self.search.editors:
            return
        var effect = SearchEffect()
        if plan.kind == 0:
            effect = self.search.begin(plan.target)
            self.search.editors[name].pending_begin = False
        elif plan.kind == 1:
            effect = self.search.editors[name].session.key_action(plan.target.pane, plan.input, plan.target.command_output)
        elif plan.kind == 4:
            effect.focus = SEARCH_FOCUS
            effect.redraw = True
        self.apply_search_effect(ui, plan.target, effect, epoch=plan.epoch, source=(plan.server, plan.client))
        self.prepare_search_editor(ui, input, plan.target, False)

    def finish_search(mut self, mut ui: Ui) raises:
        var target = self.search.target_for_field(ui.focus)
        var protected: Optional[String] = None
        if target:
            protected = target.value().name()
        var effects = self.search.enforce(protected)
        for effect in effects:
            self.apply_search_effect(ui, effect.target, effect.effect, True)

    def paint_search_chrome(mut self, mut ui: Ui, target: SearchTarget, mode: TerminalMode, status: Optional[TerminalSearch], unseen: UInt32, width: Float32) raises:
        var top: Float32 = 8
        var name = target.name()
        if name in self.search.editors and self.search.editors[name].session.query:
            var bar_width = min(Float32(460), max(width - 16, Float32(0)))
            self.search.editors[name].field.focused = ui.focus == self.search.keys[name].field
            var result = build_search_chrome(self.search.editors[name], ui, self.theme, self.search.keys[name], target.pane, max(width - bar_width - 8, Float32(0)), top, bar_width, status, target.command_output, True)
            top += result.height + 4
            for effect in result.effects:
                self.apply_search_effect(ui, target, effect)
        var mode_key = self.widget_key(7 if target.command_output else 16, target.owner)
        build_mode_indicator(ui, self.theme, mode_key, mode, unseen, max(width - 8, Float32(0)), top, True)

    def terminal_chrome_hit(self, ui: Ui, x: Float32, y: Float32) raises -> Bool:
        var hit = ui.hit_test(x, y)
        if self.search.owns_key(hit):
            return True
        for entry in self.keys.items():
            if entry.value == hit and (entry.key.startswith("7:") or entry.key.startswith("16:")):
                return True
        return False

    def search_preserves_output(self, ui: Ui) raises -> Bool:
        var target = self.search.target_for_field(ui.focus)
        return Bool(target) and target.value().command_output and self.search_valid(target.value())

    def key(mut self, key: String):
        try:
            var portable: UInt16
            if key == "Enter":
                portable = 0x100
            elif key == "BSpace":
                portable = 0x102
            elif key == "Tab":
                portable = 0x101
            elif key == "Escape":
                portable = 0x103
            else:
                raise Error("unknown workspace key")
            var pane = self.input_pane()
            if pane:
                self.forward_key(
                    pane.value(),
                    mjui_key_input(portable, 0, 0, 0, 0),
                )
            self.next_poll = 0
        except error:
            self.model.error = String(error)

    def forward_key(
        mut self,
        pane: UInt64,
        key: KeyInput,
        epoch: Optional[UInt64] = None,
        release: Bool = False,
        command_output: Optional[Bool] = None,
        source: Optional[Tuple[UInt64, UInt64]] = None,
    ) raises:
        var output = command_output.value() if command_output else (
            self.output_input and self.output.owns(pane)
        )
        if not release and not output:
            _ = self.cancel_local_scroll(pane)
        self.backend.input(
            pane,
            input_key(pane, key),
            epoch.value() if epoch else self.focus_epoch,
            release or output,
            source,
        )
        self.next_poll = 0

    def commit_text_event(mut self, input: TextInput, event: TextEvent) raises:
        if event.kind != TEXT_INSERT or event.end <= event.start:
            return
        var text = String(
            from_utf8=Span(input.bytes)[Int(event.start) : Int(event.end)]
        )
        if self.output_input:
            if self.output.focused and self.output.pane:
                self.backend.commit(
                    self.output.pane.value(),
                    text,
                    self.focus_epoch,
                    composed=event.cmd & INSERT_COMPOSED != 0,
                    command_output=True,
                )
        elif self.model.pane:
            _ = self.cancel_local_scroll(self.model.pane.value())
            self.backend.commit(
                self.model.pane.value(),
                text,
                self.focus_epoch,
                composed=event.cmd & INSERT_COMPOSED != 0,
            )
        self.next_poll = 0

    def cancel_local_scroll(mut self, pane: UInt64) raises -> Bool:
        return self.local_scroll.cancel(pane)

    def flush_local_scroll(mut self, pane: UInt64) raises:
        var target = self.local_scroll.flush(pane)
        if target:
            self.backend.input(
                pane,
                input_view(pane, 25, target.value()),
                self.focus_epoch,
                True,
            )
            self.next_poll = 0

    def apply_local_scroll(mut self, effects: List[PaneScrollEffect]) raises:
        for effect in effects:
            if effect.kind == 0:
                self.backend.input(
                    effect.pane,
                    input_view(effect.pane, 25, effect.value),
                    self.focus_epoch,
                    True,
                )
            else:
                self.backend.history_prefetch(
                    effect.pane, effect.value, self.focus_epoch
                )
            self.next_poll = 0

    def input_pane(self) -> Optional[UInt64]:
        return self.output.pane if self.output_input else self.model.pane

    def input_key(mut self) raises -> UInt64:
        if self.output_input and self.output.id:
            return self.widget_key(6, self.output.id.value())
        if self.model.pane:
            return self.widget_key(4, self.model.pane.value())
        return TERMINAL_KEY

    def observe_output(mut self, mut ui: Ui) raises:
        var changed = (
            self.output.id != self.model.command_output.id
            or self.output.pane != self.model.command_output.pane
            or self.output.server != self.model.source_server
            or self.output.client != self.model.source_client
        )
        if not changed:
            return
        var focused = self.output_input
        if focused or self.pointer_focus_output:
            self.blur_pointer()
        if self.output.id:
            var name = String(6, ":", self.output.id.value())
            if name in self.keys:
                var key = self.keys.pop(name)
                var index = ui.term.find(key)
                if index >= 0:
                    _ = ui.term.keys.pop(key)
                    ui.term.views[index] = TermView(key)
                    ui.term.views[index].gen = ui.gen - 301
            var mode_name = String(7, ":", self.output.id.value())
            if mode_name in self.keys:
                _ = self.keys.pop(mode_name)
        var transition = self.output.observe(
            self.model.command_output,
            self.model.source_server,
            self.model.source_client,
        )
        self.output_seen = None
        if focused:
            self.marked = None
        if transition.kind == 2 and focused:
            ui.focus = TERMINAL_KEY
            self.focus_epoch += 1

    def focus_output(mut self, mut ui: Ui) raises:
        if not self.output.id or not self.output.pane:
            return
        if not self.output_input:
            self.blur_pointer()
            self.marked = None
            self.focus_epoch += 1
        self.output_input = True
        var focused = self.output.focus(self.active)
        if focused:
            self.backend.input(
                self.output.pane.value(),
                focused.value(),
                self.focus_epoch,
                True,
            )
        ui.focus = TERMINAL_KEY
        self.pointer_focus_pane = self.output.pane
        self.pointer_focus_output = True

    def pointer_surfaces(self, ui: Ui) raises -> List[PointerSurface]:
        var surfaces = List[PointerSurface](capacity=len(self.visible_panes))
        for pane in self.visible_panes:
            var output = self.output.owns(pane)
            var name = String(
                6, ":", self.output.id.value()
            ) if output else String(4, ":", pane)
            if name not in self.keys:
                continue
            var key = self.keys[name]
            var index = ui.term.find(key)
            if index < 0 or ui.term.views[index].gen < 0:
                continue
            ref grid = ui.term.views[index]
            surfaces.append(
                PointerSurface(
                    pane,
                    PointerGrid(
                        Float32(grid.x) / ui.scale,
                        Float32(grid.y) / ui.scale,
                        Float32(grid.cols) * grid.m.width(),
                        Float32(grid.rows) * grid.m.height(),
                        grid.m.width(),
                        grid.m.height(),
                        self.local_scroll.sub_row(
                            pane
                        ) if not output else Float32(0),
                        ui.scale,
                        grid.cols,
                        grid.rows,
                    ),
                    output,
                )
            )
        return surfaces^

    def blur_pointer(mut self, preserve_output: Bool = False) raises:
        if self.pointer_focus_pane:
            var clear = self.pointer.blur(
                self.model,
                self.pointer_focus_pane.value(),
                self.pointer_focus_output,
                epoch=self.focus_epoch,
            )
            if clear:
                ref event = clear.value()
                self.backend.input(event.pane, event.frame, event.epoch, True)
        if self.output.focused:
            if self.output.id == self.model.command_output.id:
                var focused = self.output.focus(False)
                if focused and self.output.pane:
                    self.backend.input(
                        self.output.pane.value(),
                        focused.value(),
                        self.focus_epoch,
                        True,
                    )
            else:
                self.output.focused = False
        self.pointer_focus_pane = None
        self.pointer_focus_output = False
        if not preserve_output:
            self.output_input = False

    def set_active(mut self, active: Bool) raises:
        if not active:
            self.blur_pointer(True)
            self.active = False
            return
        self.active = True
        if self.output_input and self.output.pane:
            var focused = self.output.focus(True)
            if focused:
                self.backend.input(
                    self.output.pane.value(),
                    focused.value(),
                    self.focus_epoch,
                    True,
                )
            self.pointer_focus_pane = self.output.pane
            self.pointer_focus_output = True

    def forward_pointer(mut self, mut ui: Ui, event: RoutedPointer) raises:
        if event.activate:
            if self.model.pane != Optional[UInt64](event.pane):
                self.blur_pointer()
                self.model.select_pane(event.pane)
                self.focus_epoch += 1
            ui.focus = TERMINAL_KEY
            self.pointer_focus_pane = event.pane
            self.pointer_focus_output = False
        self.backend.input(event.pane, event.frame, event.epoch, event.passive)
        self.next_poll = 0
        ui.request_wake(ui.time)

    def mouse(mut self, mut ui: Ui, event: MouseEvent) raises:
        if self.terminal_chrome_hit(ui, event.x, event.y):
            return
        if self.output.id and (
            event.kind == MOUSE_DOWN
            or event.kind == MOUSE_RIGHT_DOWN
            or event.kind == MOUSE_UP
            or event.kind == MOUSE_RIGHT_UP
        ):
            var mode_name = String(7, ":", self.output.id.value())
            if (
                mode_name in self.keys
                and ui.hit_test(event.x, event.y) == self.keys[mode_name]
            ):
                return
        var surfaces = self.pointer_surfaces(ui)
        if event.kind == MOUSE_DOWN or event.kind == MOUSE_RIGHT_DOWN:
            for index in range(len(surfaces) - 1, -1, -1):
                var surface = surfaces[index]
                if surface.grid.contains(event.x, event.y):
                    if surface.command_output:
                        self.focus_output(ui)
                    else:
                        self.flush_local_scroll(surface.pane)
                        if self.output_input:
                            self.blur_pointer()
                    break
        var routed = self.pointer.route(
            self.model, Span(surfaces), event, self.focus_epoch
        )
        for input in routed:
            self.forward_pointer(ui, input)

    def scroll(
        mut self, mut ui: Ui, event: ScrollEvent, x: Float32, y: Float32
    ) raises:
        if self.terminal_chrome_hit(ui, x, y):
            return
        var surfaces = self.pointer_surfaces(ui)
        for index in range(len(surfaces) - 1, -1, -1):
            var surface = surfaces[index]
            if surface.grid.contains(x, y):
                if not surface.command_output and event.precise:
                    var step = self.local_scroll.pixels(
                        self.model,
                        surface.pane,
                        event.dy,
                        surface.grid.line_height,
                        UInt64(max(ui.time, Float64(0)) * 1_000_000_000),
                    )
                    if step:
                        self.apply_local_scroll(step.value())
                        self.pointer.scroll_rows[surface.pane] = 0
                        self.pointer.claimed = event.dy != 0
                        ui.request_wake(ui.time)
                        return
                if not surface.command_output:
                    _ = self.local_scroll.clear_fraction(surface.pane)
                    var context = scroll_context(self.model, surface.pane)
                    var remainder = self.pointer.scroll_rows.get(
                        surface.pane, Float32(0)
                    )
                    var lines = accumulate_scroll(
                        remainder,
                        -event.dy
                        / surface.grid.line_height if event.precise else -event.dy,
                    )
                    if lines > 0 and context.available():
                        self.pointer.scroll_rows[surface.pane] = remainder
                        var step = self.local_scroll.rows(
                            self.model,
                            surface.pane,
                            Int64(lines),
                            UInt64(max(ui.time, Float64(0)) * 1_000_000_000),
                        )
                        self.apply_local_scroll(step.value())
                        self.pointer.claimed = True
                        ui.request_wake(ui.time)
                        return
                    if lines < 0:
                        var target = self.local_scroll.release(surface.pane)
                        if target:
                            self.backend.input(
                                surface.pane,
                                input_view(surface.pane, 25, target.value()),
                                self.focus_epoch,
                                True,
                            )
                break
        var routed = self.pointer.scroll(
            self.model, Span(surfaces), event, x, y, ui.mods, self.focus_epoch
        )
        if routed:
            self.forward_pointer(ui, routed.value())

    def modifiers(mut self, mut ui: Ui) raises:
        if not ui.pointer_in:
            return
        if self.terminal_chrome_hit(ui, ui.mx, ui.my):
            return
        var surfaces = self.pointer_surfaces(ui)
        var routed = self.pointer.modifiers(
            self.model, Span(surfaces), ui.mx, ui.my, ui.mods, self.focus_epoch
        )
        if routed:
            self.forward_pointer(ui, routed.value())

    def poll(mut self, time: Float64):
        var collected = self.backend.collect(self.model, self.focus_epoch)
        if self.backend.busy:
            return
        if collected and self.event_driven and self.next_poll != 0:
            self.next_poll = (
                time + 1 if self.backend.ready_fd() < 0 else Float64.MAX
            )
        self.next_poll = min(
            self.next_poll, self.backend.next_history_wake(time)
        )
        if time >= self.next_poll or self.backend.pending():
            self.backend.submit(self.model, self.focus_epoch)
            self.next_poll = Float64.MAX if self.event_driven else time + 0.25

    def wait(mut self):
        _ = self.backend.collect(self.model, self.focus_epoch, wait=True)

    def flush(mut self):
        while self.backend.busy or self.backend.pending():
            if self.backend.busy:
                self.wait()
            if self.backend.pending():
                self.backend.submit(self.model, self.focus_epoch)

    def report_geometry(
        mut self, pane: Optional[UInt64], size: GridSize
    ) raises:
        if (
            self.reported_session != self.model.session
            or self.reported_client != self.model.source_client
            or self.reported_server != self.model.source_server
        ):
            self.reported.clear()
            self.reported_window = None
            self.reported_session = self.model.session
            self.reported_client = self.model.source_client
            self.reported_server = self.model.source_server
        if pane:
            if pane.value() in self.reported and self.reported[
                pane.value()
            ].same(size):
                return
        elif self.reported_window and self.reported_window.value().same(size):
            return
        self.backend.resize(pane, size, self.focus_epoch)
        if pane:
            self.reported[pane.value()] = size
        else:
            self.reported_window = size

    @inline(.never)
    def terminals(mut self, mut ui: Ui) raises:
        self.visible_panes.clear()
        var flat = self.theme.flat
        var width = max(ui.width - 220, Float32(0))
        var height = max(ui.height - 146, Float32(0))
        var metrics = cell_metrics(
            ui.shaper.fonts[Int(flat.mono)], 13, ui.scale
        )
        var rects = List[PaneRect]()
        for session in self.model.snapshot.sessions:
            for window in session.windows:
                if self.model.window and self.model.window.value() == window.id:
                    rects = window_rects(
                        window, width, height, metrics.width(), metrics.height()
                    )
        ui.open(
            Style.col()
            .grow()
            .grow_h()
            .fill(flat.bg)
            .clipped()
            .id(TERMINAL_KEY + 1)
        )
        if len(self.model.sessions) == 0:
            label(
                ui, self.theme, "Create a session to open a terminal", flat.dim
            )
        for item in rects:
            var bounds = item.bounds
            if bounds.width <= 0 or bounds.height <= 0:
                continue
            var output = self.output.owns(item.pane)
            var key = self.widget_key(
                6, self.output.id.value()
            ) if output else self.widget_key(4, item.pane)
            var interaction = ui.interact(key, focusable=True)
            if interaction.pressed:
                if output:
                    self.focus_output(ui)
                elif self.output_input or self.model.pane != Optional[UInt64](
                    item.pane
                ):
                    self.blur_pointer()
                    self.model.select_pane(item.pane)
                    self.focus_epoch += 1
                self.next_poll = 0
                ui.focus = TERMINAL_KEY
            var selected = self.input_pane() == Optional[UInt64](item.pane)
            var background = flat.bg
            if output:
                background = (
                    (
                        self.model.command_output.viewport.value().metadata.background
                        & 0xFFFFFF
                    )
                    << 8
                ) | 255
            elif item.pane in self.model.terminals:
                background = (
                    (
                        self.model.terminals[item.pane].metadata.background
                        & 0xFFFFFF
                    )
                    << 8
                ) | 255
            var pad_x = max(item.inset_x, Float32(8))
            var pad_y = max(item.inset_y, Float32(8))
            var pane_style = (
                Style.col()
                .at(bounds.x, bounds.y)
                .width(bounds.width)
                .height(bounds.height)
                .pad(pad_x, pad_y)
                .fill(background)
                .clipped()
                .id(self.widget_key(5, item.pane))
            )
            var columns = grid_dimension(
                bounds.width - pad_x * 2, metrics.width(), ui.scale
            )
            var rows = grid_dimension(
                bounds.height - pad_y * 2, metrics.height(), ui.scale
            )
            if (
                not output
                and item.pane in self.model.terminals
                and grid_placement(
                    self.model.terminals[item.pane]
                ).bottom_anchored
            ):
                pane_style.pad_t += max(
                    bounds.height
                    - pad_y * 2
                    - Float32(rows) * metrics.height(),
                    Float32(0),
                )
            ui.open(pane_style)
            if output:
                if self.output_seen != self.output.id:
                    var search_focus = self.search.target_for_field(ui.focus)
                    self.focus_output(ui)
                    self.output_seen = self.output.id
                    if search_focus and search_focus.value().command_output and search_focus.value().owner == self.output.id.value():
                        ui.focus = self.search.keys[search_focus.value().name()].field
                        self.blur_pointer(True)
                _ = self.output.render(
                    ui,
                    key,
                    self.model.command_output,
                    flat.mono,
                    13,
                    self.output.focused
                    and ui.focus == TERMINAL_KEY
                    and self.active,
                    Int(columns),
                    Int(rows),
                    self.marked if self.output_input else Optional[String](
                        None
                    ),
                )
                self.visible_panes.append(item.pane)
                var resized = self.output.resize(
                    GridSize(
                        columns, rows, UInt32(metrics.w), UInt32(metrics.h), 0
                    ),
                    Bool(self.model.session),
                    bounds.width - pad_x * 2 >= metrics.width()
                    and bounds.height - pad_y * 2 >= metrics.height(),
                )
                if resized:
                    self.backend.input(
                        item.pane, resized.value(), self.focus_epoch, True
                    )
                var mode = self.model.command_output.viewport.value().metadata.mode
                var status = self.model.command_output.viewport.value().metadata.search
                var unseen = self.model.command_output.viewport.value().metadata.unseen_output
                self.paint_search_chrome(ui, SearchTarget(item.pane, True, self.output.id.value()), mode, status, unseen, bounds.width - pad_x * 2)
            elif item.pane in self.model.terminals:
                ref viewport = self.model.terminals[item.pane]
                if item.pane in self.model.histories:
                    _ = render_retained_viewport(
                        ui,
                        key,
                        viewport,
                        self.model.histories[item.pane],
                        self.local_scroll.target(item.pane),
                        self.local_scroll.sub_row(item.pane),
                        flat.mono,
                        13,
                        selected and ui.focus == TERMINAL_KEY and self.active,
                        Int(columns),
                        Int(rows),
                        self.marked if selected else Optional[String](None),
                    )
                else:
                    _ = render_viewport(
                        ui,
                        key,
                        viewport,
                        flat.mono,
                        13,
                        selected and ui.focus == TERMINAL_KEY and self.active,
                        Int(columns),
                        Int(rows),
                        marked=self.marked if selected else Optional[String](
                            None
                        ),
                    )
                self.visible_panes.append(item.pane)
                if not item.floating:
                    self.report_geometry(
                        item.pane,
                        GridSize(
                            columns,
                            rows,
                            UInt32(metrics.w),
                            UInt32(metrics.h),
                            self.model.layout_generation,
                        ),
                    )
                var mode = self.model.terminals[item.pane].metadata.mode
                var status = self.model.terminals[item.pane].metadata.search
                var unseen = self.model.terminals[item.pane].metadata.unseen_output
                self.paint_search_chrome(ui, SearchTarget(item.pane, False, item.pane), mode, status, unseen, bounds.width - pad_x * 2)
            else:
                label(ui, self.theme, "Connecting to terminal…", flat.dim)
            ui.close()
        ui.close()

    def build(mut self, mut ui: Ui, input: TextInput) raises:
        var search_skip = 0
        if not self.search_prepared:
            var host = SearchClipboard()
            self.prepare_search_input(ui, input, host)
            for event in input.events:
                if not self.search_event(ui, input, event, host):
                    break
                search_skip += 1
            self.finish_search(ui)
        self.search_prepared = False
        self.poll(ui.time)
        if (
            self.local_scroll.server != self.model.source_server
            or self.local_scroll.client != self.model.source_client
        ):
            for index in range(len(ui.term.views)):
                ui.term.views[index].reset()
        var now = UInt64(max(ui.time, Float64(0)) * 1_000_000_000)
        self.apply_local_scroll(self.local_scroll.observe(self.model, now))
        var scroll_wake = self.local_scroll.next_wake()
        if scroll_wake:
            ui.request_wake(Float64(scroll_wake.value()) / 1_000_000_000)
        self.marked = String(from_utf8=Span(input.marked)) if ui.focus == TERMINAL_KEY and len(
            input.marked
        ) > 0 else Optional[String](None)
        self.observe_output(ui)
        if ui.focus != TERMINAL_KEY or (
            self.active and self.pointer_focus_pane != self.input_pane()
        ):
            self.blur_pointer(self.search_preserves_output(ui))
        if self.pointer.selection_lines != 0:
            if ui.time >= self.next_autoscroll:
                var routed = self.pointer.autoscroll(self.model)
                if routed:
                    self.forward_pointer(ui, routed.value())
                self.next_autoscroll = ui.time + 0.033
            if self.pointer.selection_lines != 0:
                ui.request_wake(self.next_autoscroll)
        if self.backend.busy:
            ui.request_wake(ui.time + 0.016)
        elif self.next_poll != Float64.MAX:
            ui.request_wake(self.next_poll)
        if ui.focus == TERMINAL_KEY:
            for index in range(search_skip, len(input.events)):
                var event = input.events[index]
                if event.kind == TEXT_INSERT and event.end > event.start:
                    try:
                        self.commit_text_event(input, event)
                    except error:
                        self.model.error = String(error)
        var flat = self.theme.flat
        ui.open(Style.col().grow().grow_h().fill(flat.bg))
        ui.open(
            Style.row()
            .grow()
            .height(42)
            .pad(12, 7)
            .spacing(8)
            .center()
            .fill(flat.panel)
        )
        label(ui, self.theme, "zz", flat.fg, strong=True)
        ui.open(Style.col().grow())
        ui.close()
        if button(ui, self.theme, 1, "New session"):
            try:
                self.backend.enqueue(
                    CREATE_SESSION, selection_epoch=self.focus_epoch
                )
                ui.focus = TERMINAL_KEY
            except error:
                self.model.error = String(error)
        if button(
            ui, self.theme, 2, "Split right", disabled=not self.model.pane
        ):
            try:
                self.backend.enqueue(
                    SPLIT_RIGHT,
                    self.model.pane,
                    selection_epoch=self.focus_epoch,
                )
                ui.focus = TERMINAL_KEY
            except error:
                self.model.error = String(error)
        if button(
            ui, self.theme, 3, "Split down", disabled=not self.model.pane
        ):
            try:
                self.backend.enqueue(
                    SPLIT_DOWN,
                    self.model.pane,
                    selection_epoch=self.focus_epoch,
                )
                ui.focus = TERMINAL_KEY
            except error:
                self.model.error = String(error)
        if button(ui, self.theme, 4, "Reconnect"):
            try:
                self.backend.enqueue(
                    RECONNECT, selection_epoch=self.focus_epoch
                )
                ui.focus = TERMINAL_KEY
            except error:
                self.model.error = String(error)
        ui.close()
        ui.open(Style.row().grow().grow_h())
        panel_begin(ui, self.theme, width=220)
        ui.open(Style.col().grow().pad(8, 8).spacing(4))
        ui.open(Style.row().height(28).pad(8, 0).center())
        label(ui, self.theme, "Sessions", flat.dim, strong=True)
        ui.close()
        var selected_session: Optional[UInt64] = None
        for session in self.model.sessions:
            var selected = (
                self.model.session and session.id == self.model.session.value()
            )
            if row(
                ui,
                self.theme,
                self.widget_key(1, session.id),
                session.name,
                selected=selected,
            ):
                selected_session = session.id
        if selected_session:
            self.blur_pointer()
            self.model.select_session(selected_session.value())
            self.focus_epoch += 1
            self.next_poll = 0
            ui.focus = TERMINAL_KEY
        ui.close()
        panel_end(ui)
        ui.open(Style.col().grow().grow_h())
        ui.open(
            Style.row().grow().height(40).pad(8, 6).spacing(4).fill(flat.panel)
        )
        var selected_window: Optional[UInt64] = None
        for window in self.model.windows:
            if (
                self.model.session
                and window.session == self.model.session.value()
            ):
                if button(
                    ui, self.theme, self.widget_key(2, window.id), window.name
                ):
                    selected_window = window.id
        if selected_window:
            self.blur_pointer()
            self.model.select_window(selected_window.value())
            self.focus_epoch += 1
            self.next_poll = 0
            ui.focus = TERMINAL_KEY
        ui.close()
        ui.open(Style.row().grow().height(36).pad(8, 4).spacing(4))
        var selected_pane: Optional[UInt64] = None
        for pane in self.model.panes:
            if self.model.window and pane.window == self.model.window.value():
                var name = "%" + String(pane.id) + " " + pane.command
                if button(ui, self.theme, self.widget_key(3, pane.id), name):
                    selected_pane = pane.id
        if selected_pane:
            self.blur_pointer()
            self.model.select_pane(selected_pane.value())
            self.focus_epoch += 1
            self.next_poll = 0
            ui.focus = TERMINAL_KEY
        ui.close()
        self.terminals(ui)
        ui.close()
        if self.backend.pending():
            ui.request_wake(ui.time)
        ui.close()
        ui.open(
            Style.row().grow().height(28).pad(12, 0).center().fill(flat.panel)
        )
        if self.model.error.byte_length() > 0:
            label(ui, self.theme, self.model.error, flat.danger)
        else:
            var status = String(
                len(self.model.sessions),
                " sessions · ",
                len(self.model.windows),
                " windows · ",
                len(self.model.panes),
                " panes",
            )
            label(ui, self.theme, status, flat.dim)
        ui.close()
        ui.close()
        if ui.focus != TERMINAL_KEY or (
            self.active and self.pointer_focus_pane != self.input_pane()
        ):
            self.blur_pointer(self.search_preserves_output(ui))
        self.pointer_focus_pane = (
            self.input_pane() if ui.focus == TERMINAL_KEY
            and self.active else Optional[UInt64](None)
        )
        self.pointer_focus_output = self.output_input and Bool(
            self.pointer_focus_pane
        )
        if ui.focus != TERMINAL_KEY and not self.search.target_for_chrome(ui.focus):
            self.search.reset_keys()
