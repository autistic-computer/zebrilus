from .app_resume import ResumeStartup
from mjui.ui import Ui, Style, TextStyle
from mjui.input import (
    TextInput,
    TextEvent,
    TEXT_INSERT,
    MouseEvent,
    MOUSE_DOWN,
    MOUSE_MOVED,
    MOUSE_DRAGGED,
    MOUSE_RIGHT_DRAGGED,
    MOUSE_EXITED,
    MOUSE_MIDDLE_DOWN,
    MOUSE_MIDDLE_UP,
    MOUSE_MIDDLE_DRAGGED,
    TEXT_PRIMARY,
    TEXT_POINTER,
    MOD_SHIFT,
    MOUSE_RIGHT_DOWN,
    MOUSE_UP,
    MOUSE_RIGHT_UP,
    INSERT_COMPOSED,
    Clipboard,
    KeyEvent,
    CMD_PASTE,
    MOD_ALT,
    MOD_CMD,
    MOD_CTRL,
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
from .backend import AsyncBackend, KEY, RECONNECT
from .state import AppModel
from .terminal_painter import render_viewport_opts
from .terminal_history_painter import render_retained_viewport_opts
from .local_scroll_controller import (
    TerminalScrollController,
    PaneScrollEffect,
    scroll_context,
)
from .input_protocol import input_view
from .terminal_view import accumulate_scroll
from .workspace_layout import window_rects, PaneRect
from .geometry import GridSize, grid_dimension
from mjui.term.grid import cell_metrics_options
from .appearance_runtime import AppearanceRuntime
from mjui.term.rows import TermView
from .input_protocol import KeyInput, input_key, mjui_key_input
from .terminal_projection import grid_placement
from .pointer_controller import PointerController, PointerSurface, RoutedPointer
from .terminal_view import PointerGrid
from .command_output import CommandOutputPresentation
from .terminal_model import TerminalMode, TerminalSearch
from .terminal_search import (
    SearchEffect,
    SEARCH_FOCUS,
    TERMINAL_FOCUS,
    SEARCH_ACCEPT_AND_CLOSE,
    search_shortcut,
    search_key_action,
)
from .terminal_search_protocol import SearchQuery
from .terminal_search_controller import (
    TerminalSearchController,
    SearchTarget,
    SearchPlan,
)
from .terminal_search_editor import SearchInputResult, SEARCH_TEXT_BYTES
from .terminal_search_chrome import (
    SearchKeys,
    build_search_chrome,
    build_mode_indicator,
)
from .search_clipboard import SearchClipboard
from .keyboard_controller import KeyboardController
from .palette_runtime import PaletteRuntime, PalettePlan
from .palette_keymap import palette_open_shortcut
from .palette_controller import (
    PaletteEffect,
    PALETTE_EXECUTE,
    PALETTE_ACTIVATE,
    PALETTE_CONNECT,
    PALETTE_NOTIFY,
    PALETTE_LEVEL,
)
from .palette_places_model import PalettePlace
from .palette_files import FILES_UP, FILES_DOWN
from .path_protocol import PATH_OPEN
from .path_text import start_dir
from .primary_paste import PrimaryPasteController, PrimaryTarget
from .editor_pointer import (
    EditorPointerScope,
    editor_primary,
    editor_composition,
    pointer_payload,
)
from .actions import MAX_PENDING_INPUT_BYTES, MAX_PENDING_ACTIONS
from .palette_controller import PALETTE_INPUT_BYTES
from .chooser_live import LiveChooser
from .window_chooser_runtime import WindowChooserRuntime
from .chooser_session import ChooserSource, ChooserPlan
from .which_key_live import LiveWhichKey, WhichKeyIntent, WhichKeyContext
from .prefix_bindings import PrefixTable
from .input_protocol import canonical_key
from .desktop_key_config import DesktopKeyConfig
from .desktop_key_config_io import (
    load_desktop_key_config,
    local_desktop_config_environment,
)
from std.sys.info import CompilationTarget
from std.ffi import external_call
from std.os.path import isdir
from mjui.widgets.penna_palette import PaletteRow, palette_tree_row
from .desktop_sidebar import (
    DesktopSidebar,
    DesktopNavIntent,
    DesktopNavSource,
    stage_navigation,
    commit_navigation,
    LOCAL_HOST,
)
from .desktop_fleet import FleetHostInput
from .desktop_host import (
    HostState,
    HOST_CONNECTED,
    HOST_CONNECTING,
    HOST_REASON_BYTES,
)
from .desktop_nav_tree import NavNode
from .desktop_nav import nav_command
from .desktop_input import desktop_router_map
from .desktop_chrome_dispatch import DesktopChromeDispatch
from .chrome_keymap import ChromeKeymap
from .palette_host import palette_local_hostname
from .tree_model import MuxSnapshot

comptime TERMINAL_KEY: UInt64 = 0x7000000000000000
comptime SIDEBAR_KEY: UInt64 = 0x7100000000000000
comptime SIDEBAR_WIDTH: Float32 = 220
comptime SIDEBAR_ROW: Float32 = 26
comptime TOOLBAR_HEIGHT: Float32 = 42
comptime STATUS_HEIGHT: Float32 = 28
comptime NEW_SESSION_ATTACH_BIT: UInt64 = 1 << 27


def current_directory() -> Optional[String]:
    var buffer = List[UInt8](length=4097, fill=0)
    var pointer = external_call["getcwd", Int](buffer.unsafe_ptr(), 4097)
    if pointer == 0:
        return None
    var length = 0
    while length < 4096 and buffer[length] != 0:
        length += 1
    var text = String(from_utf8_lossy=Span(buffer)[:length])
    if not isdir(text):
        return None
    return text^


struct WorkspaceView(Movable):
    var path: String
    var backend: AsyncBackend
    var focus_epoch: UInt64
    var model: AppModel
    var theme: PennaTheme
    var appearance: AppearanceRuntime
    var cursor_epoch: UInt64
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
    var palette: PaletteRuntime
    var palette_prepared: Bool
    var palette_closed_frame: Bool
    var primary: PrimaryPasteController
    var ordered_pointer_frame: Bool
    var painted_source: Tuple[UInt64, UInt64]
    var chooser: LiveChooser
    var chooser_prepared: Bool
    var chooser_closed_frame: Bool
    var chooser_unmark: Bool
    var chooser_restore_output: Optional[Tuple[UInt64, UInt64, UInt64]]
    var windows: WindowChooserRuntime
    var windows_prepared: Bool
    var windows_unmark: Bool
    var which_key: LiveWhichKey
    var which_key_delay: UInt64
    var which_key_changes: UInt64
    var config_notice: Optional[String]
    var sidebar: DesktopSidebar
    var sidebar_seen: String
    var sidebar_first: Int
    var sidebar_shown: Bool
    var chrome: ChromeKeymap
    var chrome_held: Dict[UInt16, Bool]
    var picker_selected: Dict[UInt64, Int]

    def __init__(
        out self,
        path: String,
        var startup: Optional[ResumeStartup] = None,
        native_chooser: Bool = False,
    ) raises:
        self.path = path
        self.backend = AsyncBackend(path, startup^, native_chooser)
        self.focus_epoch = 0
        self.model = AppModel()
        self.theme = PennaTheme(DEFAULT_DARK, 0, 1, 2, 3)
        self.appearance = AppearanceRuntime(self.theme.flat.mono)
        self.cursor_epoch = 0
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
        self.palette = PaletteRuntime()
        self.palette_prepared = False
        self.palette_closed_frame = False
        self.primary = PrimaryPasteController()
        self.ordered_pointer_frame = False
        self.painted_source = (0, 0)
        self.chooser = LiveChooser()
        self.chooser_prepared = False
        self.chooser_closed_frame = False
        self.chooser_unmark = False
        self.chooser_restore_output = None
        self.windows = WindowChooserRuntime()
        self.windows_prepared = False
        self.windows_unmark = False
        self.which_key = LiveWhichKey()
        var macos = CompilationTarget.is_macos()
        var config = DesktopKeyConfig(macos)
        self.which_key_changes = 0
        self.config_notice = None
        try:
            config = load_desktop_key_config(
                local_desktop_config_environment(), macos
            ).value.copy()
        except error:
            self.config_notice = String(error)
        self.which_key_delay = config.which_key_ms()
        try:
            self.chrome = desktop_router_map(config.map(macos, 1), 2)
        except error:
            self.config_notice = String(error)
            self.chrome = desktop_router_map(
                DesktopKeyConfig(macos).map(macos, 1), 2
            )
        self.sidebar = DesktopSidebar(palette_local_hostname())
        self.sidebar_seen = String()
        self.sidebar_first = 0
        self.sidebar_shown = True
        self.chrome_held = Dict[UInt16, Bool]()
        self.picker_selected = Dict[UInt64, Int]()
        self.backend.submit(self.model, self.focus_epoch)

    def widget_key(mut self, kind: Int, id: UInt64) raises -> UInt64:
        return self.name_key(String(kind, ":", id))

    def name_key(mut self, name: String) raises -> UInt64:
        if name not in self.keys:
            if self.next_key == TERMINAL_KEY:
                raise Error("widget identifiers exhausted")
            self.keys[name] = self.next_key
            self.next_key += 1
        return self.keys[name]

    def dispatch_chooser(mut self, mut ui: Ui, plan: ChooserPlan) raises:
        if not self.chooser.session.current(plan):
            return
        if self.backend.chooser(plan, self.focus_epoch):
            self.next_poll = 0
            ui.request_wake(ui.time)

    def chooser_source(self) -> ChooserSource:
        if self.backend.path_source:
            ref connected = self.backend.path_source.value()
            return ChooserSource(
                connected.server, connected.client, connected.epoch
            )
        return ChooserSource(0, 0, 0)

    def prepare_chooser_input(mut self, mut ui: Ui, input: TextInput) raises:
        self.chooser_prepared = True
        self.chooser_closed_frame = False
        self.chooser_unmark = False
        var host_owned = (
            Bool(self.chooser.host_scope) and not self.palette.opened()
        )
        var owned = self.chooser.host_scope == Optional[UInt64](
            self.chooser.session.scope
        )
        var old_scope = self.chooser.session.scope
        var source = self.chooser_source()
        owned = (
            owned
            and self.active
            and not self.palette.opened()
            and self.chooser.host_source.matches(source)
        )
        var was_open = self.chooser.session.opened()
        try:
            var result = self.chooser.prepare(
                ui,
                self.model.dialogs,
                source,
                self.backend.job[].native_chooser and self.backend.online,
                self.palette.opened(),
                input.marked_revision,
            )
            if result.owner_changed:
                self.chooser_restore_output = (
                    self.model.source_server,
                    self.model.source_client,
                    self.output.id.value(),
                ) if self.output_input and self.output.id else Optional[
                    Tuple[UInt64, UInt64, UInt64]
                ](
                    None
                )
                if self.chooser.opened():
                    self.marked = None
                    self.chooser_unmark = True
                    self.blur_pointer(True)
            if result.closed:
                self.chooser_closed_frame = host_owned
                self.chooser_unmark = host_owned
                if not self.palette.opened():
                    var restore = self.chooser_restore_output
                    ui.focus = TERMINAL_KEY
                    self.observe_output(ui)
                    if (
                        restore
                        and restore.value()[0] == self.model.source_server
                        and restore.value()[1] == self.model.source_client
                        and self.output.id
                        == Optional[UInt64](restore.value()[2])
                        and self.output.id == self.model.command_output.id
                        and self.model.command_output.viewport
                    ):
                        self.focus_output(ui)
                    else:
                        self.output_input = False
                self.chooser_restore_output = None
            if input.marked_exhausted:
                _ = self.chooser.session.mark(
                    "", input.marked_revision, True, owned
                )
            elif self.chooser.session.marked_seen != input.marked_revision:
                if owned and old_scope == self.chooser.session.scope:
                    if len(input.marked) > 65536:
                        self.chooser.session.marked_seen = input.marked_revision
                        self.primary_notice(
                            ui, "Chooser composition exceeds 64 KiB"
                        )
                    else:
                        try:
                            _ = self.chooser.session.mark(
                                String(from_utf8=Span(input.marked)),
                                input.marked_revision,
                                False,
                                True,
                            )
                        except error:
                            self.primary_notice(ui, String(error))
                else:
                    _ = self.chooser.session.mark(
                        "", input.marked_revision, False, False
                    )
        except error:
            self.chooser.reset(ui)
            self.chooser_closed_frame = not self.palette.opened()
            self.chooser_unmark = self.chooser_closed_frame
            if not self.palette.opened() and (
                was_open or ui.focus == self.chooser.field
            ):
                ui.focus = TERMINAL_KEY
            self.primary_notice(ui, String(error))

    def modal_open(self) -> Bool:
        return self.chooser.opened() or self.windows.opened()

    def prepare_windows_input(mut self, mut ui: Ui, input: TextInput):
        self.windows_prepared = True
        self.windows_unmark = False
        try:
            var observed = self.windows.observe(
                ui,
                self.backend,
                self.model.dialogs,
                self.chooser_source(),
                self.palette.opened(),
                self.focus_epoch,
                input,
            )
            if observed.owner_changed:
                self.marked = None
                self.blur_pointer(True)
            if observed.replaced_local and self.palette.opened():
                self.close_palette(ui)
                ui.focus = self.windows.component.field.key
                ui.text_input = True
            if observed.closed and not self.palette.opened():
                ui.focus = TERMINAL_KEY
                self.observe_output(ui)
            self.windows_unmark = self.windows.unmark
            self.windows.prepare(ui, input)
        except error:
            self.windows.reset(ui)
            self.windows_unmark = True
            if not self.palette.opened():
                ui.focus = TERMINAL_KEY
            self.primary_notice(ui, String(error))

    def windows_returned(mut self, mut ui: Ui) raises:
        if self.windows.unmark:
            self.windows_unmark = True
        if not self.windows.returned:
            return
        self.windows.returned = False
        self.open_local_palette(ui)

    def open_local_palette(mut self, mut ui: Ui) raises:
        self.blur_pointer(True)
        self.marked = None
        self.palette.open(ui, self.model, self.backend.online)
        if self.palette.component.opened():
            self.windows.local_opened(self.model.dialogs.chooser_revision)
            ui.focus = self.palette.component.field_key
            ui.text_input = True
        else:
            self.close_palette(ui)

    def windows_event[
        Host: Clipboard
    ](
        mut self, mut ui: Ui, input: TextInput, event: TextEvent, mut host: Host
    ) raises -> Bool:
        if not self.windows.opened() or self.palette.opened():
            return False
        var consumed = True
        try:
            consumed = self.windows.ordered(
                ui, self.backend, input, event, host, self.focus_epoch
            )
        except error:
            self.primary_notice(ui, String(error))
        self.windows_returned(ui)
        return consumed

    def compose_windows(mut self, mut ui: Ui, input: TextInput):
        try:
            if not self.windows.compose(
                ui, self.backend, input, self.focus_epoch
            ):
                self.primary_notice(ui, "Chooser composition exceeds 64 KiB")
            self.windows_returned(ui)
        except error:
            self.primary_notice(ui, String(error))

    def chooser_event(
        mut self, mut ui: Ui, input: TextInput, event: TextEvent
    ) raises -> Bool:
        if self.chooser_closed_frame or self.chooser_unmark:
            return True
        if self.chooser.session.opened() and not self.active:
            return True
        if not self.chooser.input_scope(ui):
            return False
        if event.kind != TEXT_INSERT:
            return True
        if (
            event.start < 0
            or event.end < event.start
            or Int(event.end) > len(input.bytes)
        ):
            raise Error("chooser committed text range is invalid")
        if Int(event.end - event.start) > 65536:
            self.primary_notice(ui, "Chooser input exceeds 64 KiB")
            return True
        try:
            var text = String(
                from_utf8=Span(input.bytes)[Int(event.start) : Int(event.end)]
            )
            var plan = self.chooser.session.commit(text)
            if plan:
                self.dispatch_chooser(ui, plan.value())
        except error:
            self.primary_notice(ui, String(error))
        return True

    def plan_palette(
        mut self,
        mut ui: Ui,
        mut keyboard: KeyboardController,
        key: KeyEvent,
        bytes: Span[UInt8, _],
        cancel: Bool = False,
    ) raises -> Optional[PalettePlan]:
        if key.release and key.key in self.palette.held:
            _ = self.palette.held.pop(key.key)
            keyboard.claim_text(key, bytes)
            return PalettePlan(
                1,
                self.model.source_server,
                self.model.source_client,
                self.focus_epoch,
            )
        if not key.release and key.key in self.palette.held:
            _ = self.palette.held.pop(key.key)
        if key.consumed or not self.active:
            return None
        if self.palette.opened():
            if (
                not key.release
                and key.key not in keyboard.forwarded
                and (
                    key.key == Key.ENTER
                    or key.key == Key.KP_ENTER
                    or key.key == Key.ESCAPE
                    or key.key == Key.UP
                    or key.key == Key.DOWN
                )
            ):
                self.palette.held[key.key] = True
            if (
                not key.release
                and key.key == Key.ESCAPE
                and (cancel or not key.text)
            ):
                self.palette.pending_close = True
                keyboard.claim_text(key, bytes)
                keyboard.claim_text_command(key)
                ui.focus = TERMINAL_KEY
                return PalettePlan(
                    4,
                    self.model.source_server,
                    self.model.source_client,
                    self.focus_epoch,
                )
            if (
                not key.release
                and (key.key == Key.UP or key.key == Key.DOWN)
                and key.mods & (MOD_ALT | MOD_CMD) == 0
            ):
                keyboard.claim_text(key, bytes)
                keyboard.claim_text_command(key)
                return PalettePlan(
                    UInt8(2 if key.key == Key.UP else 3),
                    self.model.source_server,
                    self.model.source_client,
                    self.focus_epoch,
                )
            return None
        var input = mjui_key_input(
            key.key, key.ch, key.unshifted, key.mods, key.action()
        )
        if (
            key.text_lo >= 0
            and key.text_hi > key.text_lo
            and Int(key.text_hi) <= len(bytes)
        ):
            input.text = String(
                from_utf8=bytes[Int(key.text_lo) : Int(key.text_hi)]
            )
        var capture = False
        for session in self.model.snapshot.sessions:
            if self.model.session == Optional[UInt64](session.id):
                for window in session.windows:
                    if self.model.window == Optional[UInt64](window.id):
                        capture = Bool(window.modal)
        var overlay = ui.layers.modal != 0 and not (
            self.palette.pending_close
            and ui.layers.modal == self.palette.component.key + 3
        )
        if not palette_open_shortcut(
            input, CompilationTarget.is_macos(), overlay, capture
        ):
            return None
        self.palette.restore_output = (
            self.model.source_server,
            self.model.source_client,
            self.output.id.value(),
        ) if self.output_input and self.output.id else Optional[
            Tuple[UInt64, UInt64, UInt64]
        ](
            None
        )
        self.palette.pending_open = True
        self.palette.held[key.key] = True
        if key.key in keyboard.forwarded:
            _ = keyboard.forwarded.pop(key.key)
        keyboard.claim_text(key, bytes)
        keyboard.claim_text_command(key)
        ui.focus = self.palette.component.field_key
        ui.request_wake(ui.time)
        return PalettePlan(
            0,
            self.model.source_server,
            self.model.source_client,
            self.focus_epoch,
        )

    def dispatch_palette(
        mut self, mut ui: Ui, input: TextInput, plan: PalettePlan
    ) raises:
        if (
            plan.server != self.model.source_server
            or plan.client != self.model.source_client
        ):
            self.palette.pending_open = False
            self.palette.pending_close = False
            self.palette.restore_output = None
            return
        if plan.kind == 4:
            self.close_palette(ui)
            return
        if plan.kind == 2 or plan.kind == 3:
            if self.palette.component.opened():
                if self.palette.files.files.active:
                    var effect = self.palette.files.files.key(
                        FILES_UP if plan.kind == 2 else FILES_DOWN,
                        self.palette.files.inserts(self.model),
                    )
                    self.palette.file_effect(
                        ui, self.backend, self.model, effect
                    )
                    self.palette.project_files(self.model)
                else:
                    self.palette.component.model.navigate(
                        -1 if plan.kind == 2 else 1
                    )
                self.palette.component.scroll_selected = True
            return
        if plan.kind != 0:
            return
        self.backend.palette_open(plan.epoch, (plan.server, plan.client))
        self.blur_pointer(True)
        self.marked = None
        self.palette.open(ui, self.model, self.backend.online)
        if self.palette.component.opened():
            self.windows.local_opened(self.model.dialogs.chooser_revision)
            var clip = SearchClipboard()
            _ = self.palette.component.prepare(
                ui,
                input,
                clip,
                self.palette.projection.fleet,
                self.palette.projection.decorations,
                False,
                False,
                len(input.primaries) == 0 and len(input.pointers) == 0,
            )
        else:
            self.close_palette(ui)

    def close_palette(mut self, mut ui: Ui) raises:
        var restore = self.palette.restore_output
        self.palette.files.close(self.backend)
        self.palette.close(ui)
        ui.focus = TERMINAL_KEY
        self.observe_output(ui)
        if (
            restore
            and restore.value()[0] == self.model.source_server
            and restore.value()[1] == self.model.source_client
            and self.output.id == Optional[UInt64](restore.value()[2])
            and self.output.id == self.model.command_output.id
            and self.model.command_output.viewport
        ):
            self.focus_output(ui)
        else:
            self.output_input = False
        self.marked = None
        self.palette_closed_frame = True

    def palette_effect(mut self, mut ui: Ui, effect: PaletteEffect) raises:
        if effect.kind == PALETTE_EXECUTE:
            if not effect.command:
                raise Error("palette execution has no invocation")
            self.backend.palette_command(
                effect.command.value(),
                self.focus_epoch,
                (self.model.source_server, self.model.source_client),
            )
            self.close_palette(ui)
            self.palette.notify(ui, effect.message)
        elif effect.kind == PALETTE_ACTIVATE:
            var target = effect.place.copy()
            var landing = self.palette.projection.fleet.landing(target)
            var success = False
            if landing:
                var host, session, window, pane = landing.value()
                ref source_host = self.palette.projection.fleet.hosts[host]
                if (
                    source_host.id == self.palette.projection.fleet.here
                    and source_host.online
                ):
                    ref source_session = source_host.sessions[session]
                    ref source_window = source_session.windows[window]
                    var session_id = source_session.id
                    var window_id = source_window.id
                    var pane_id = source_window.panes[pane].id
                    self.palette.restore_output = None
                    self.blur_pointer()
                    self.model.select_session(session_id)
                    if target.window or not target.session:
                        self.model.select_window(window_id)
                    if target.pane:
                        self.model.select_pane(pane_id)
                    self.focus_epoch += 1
                    self.next_poll = 0
                    success = True
            var result = self.palette.component.model.controller.activated(
                target, success, self.palette.projection.fleet
            )
            if success:
                self.close_palette(ui)
            else:
                self.palette.component.model.rebuild(
                    self.palette.projection.fleet,
                    self.palette.projection.decorations,
                )
                self.palette.component.sync_field()
            if result.kind == PALETTE_NOTIFY:
                self.palette.notify(ui, result.message)
        elif effect.kind == PALETTE_CONNECT:
            if effect.host == Optional[UInt32](
                self.palette.projection.fleet.here
            ):
                self.backend.enqueue(
                    RECONNECT, selection_epoch=self.focus_epoch
                )
                self.next_poll = 0
        elif effect.kind == PALETTE_NOTIFY:
            self.palette.notify(ui, effect.message)
        elif effect.kind == PALETTE_LEVEL:
            if effect.slot and effect.slot.value().kind == 3:
                self.palette.sync_files(
                    ui, self.backend, self.model, self.focus_epoch
                )
            else:
                self.palette.notify(
                    ui, "This palette level is not available yet"
                )

    def prepare_palette_input[
        Host: Clipboard
    ](mut self, mut ui: Ui, input: TextInput, mut host: Host) raises:
        self.palette_closed_frame = False
        self.palette_prepared = True
        self.pump_palette_files(ui)
        if not self.palette.component.opened():
            return
        self.palette.refresh(self.model, self.backend.online)
        var result = self.palette.prepare(
            ui, input, host, self.backend, self.model, self.focus_epoch
        )
        if result.closed or self.palette.files.close_ready:
            self.close_palette(ui)
        self.palette_effect(ui, result.effect)
        if result.limited:
            self.palette.notify(ui, "Palette input exceeds 64 KiB")

    def palette_event[
        Host: Clipboard
    ](
        mut self, mut ui: Ui, input: TextInput, event: TextEvent, mut host: Host
    ) raises -> Bool:
        var result = self.palette.event(
            ui, input, event, host, self.backend, self.model, self.focus_epoch
        )
        if not result.consumed:
            return False
        if result.closed or self.palette.files.close_ready:
            self.close_palette(ui)
        self.palette_effect(ui, result.effect)
        if result.limited:
            self.palette.notify(ui, "Palette input exceeds 64 KiB")
        return True

    def pump_palette_files(mut self, mut ui: Ui) raises:
        var inbox = self.backend.paths.take()
        inbox.events.reverse()
        while len(inbox.events) > 0:
            var delivery = inbox.events.pop()
            if delivery.message.kind == PATH_OPEN:
                if (
                    self.palette.opened()
                    or not delivery.message.pane
                    or not self.backend.path_source
                    or not delivery.source.matches(
                        self.backend.path_source.value()
                    )
                ):
                    continue
                var pane = delivery.message.pane.value()
                var attached = False
                for session in self.model.snapshot.sessions:
                    if self.model.session == Optional[UInt64](session.id):
                        for window in session.windows:
                            if pane in window.panes:
                                attached = True
                if not attached:
                    continue
                self.backend.palette_open(
                    self.focus_epoch,
                    (self.model.source_server, self.model.source_client),
                )
                self.palette.restore_output = (
                    self.model.source_server,
                    self.model.source_client,
                    self.output.id.value(),
                ) if self.output_input and self.output.id else Optional[
                    Tuple[UInt64, UInt64, UInt64]
                ](
                    None
                )
                self.blur_pointer(True)
                self.marked = None
                self.palette.open(ui, self.model, self.backend.online)
                self.windows.local_opened(self.model.dialogs.chooser_revision)
                self.palette.insert_pane = pane
                var body = String()
                if delivery.message.dir:
                    var text = delivery.message.dir.value().copy()
                    if text.startswith("/") or text.startswith("~"):
                        body = start_dir(text, self.palette.home)
                self.palette.set_files_body(body)
            else:
                _ = self.palette.files.apply(self.backend, delivery^)
        self.palette.sync_files(ui, self.backend, self.model, self.focus_epoch)
        if self.palette.files.close_ready:
            self.close_palette(ui)

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
        self.search.ensure(
            target,
            keys,
            TextStyle(self.theme.flat.font, 13, self.theme.flat.fg, False),
        )

    def search_valid(self, target: SearchTarget) -> Bool:
        if target.command_output:
            return self.output.id == Optional[UInt64](
                target.owner
            ) and self.output.owns(target.pane)
        return target.pane in self.model.terminals

    def observe_search(mut self, mut ui: Ui) raises:
        var previous = self.search.target_for_field(ui.focus)
        var valid = List[SearchTarget]()
        for pane in self.model.terminals.keys():
            valid.append(SearchTarget(pane, False, pane))
        if self.output.id and self.output.pane:
            valid.append(
                SearchTarget(
                    self.output.pane.value(), True, self.output.id.value()
                )
            )
        self.search.observe(
            self.model.source_server, self.model.source_client, Span(valid)
        )
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
        self.backend.input(
            target.pane,
            frame,
            epoch,
            passive or target.command_output,
            source,
            Optional[UInt64](
                target.owner
            ) if target.command_output else Optional[UInt64](None),
        )
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
                if self.output_input or self.model.pane != Optional[UInt64](
                    target.pane
                ):
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
        self.send_search(
            target,
            effect.frame,
            epoch.value() if epoch else self.focus_epoch,
            passive,
            source,
        )
        if effect.redraw or len(effect.frame) > 0:
            ui.request_wake(ui.time)

    def search_results(mut self, mut ui: Ui, target: SearchTarget) raises:
        var effects = self.search_frame.effects^
        self.search_frame.effects = List[SearchEffect]()
        for effect in effects:
            self.apply_search_effect(ui, target, effect)
        if self.search_frame.limited:
            self.model.error = "Terminal search input exceeds its byte limit"

    def prepare_search_editor(
        mut self,
        mut ui: Ui,
        input: TextInput,
        target: SearchTarget,
        pointer: Bool = True,
    ) raises:
        var name = target.name()
        if (
            name in self.search_fields
            or not self.search.editors[name].session.query
        ):
            return
        var chooser_pending = (
            self.backend.job[].native_chooser
            and Bool(self.model.dialogs.chooser)
            and self.model.dialogs.chooser.value().kind != 0
        )
        var prepared = self.search.editors[name].prepare(
            ui,
            input,
            self.search_clipboard,
            target.pane,
            target.command_output,
            self.search.keys[name].container if pointer else UInt64(0),
            len(input.primaries) == 0
            and len(input.pointers) == 0
            and not chooser_pending,
        )
        for effect in prepared.effects:
            self.search_frame.append(effect.copy())
        self.search_frame.limited = (
            self.search_frame.limited or prepared.limited
        )
        self.search_fields[name] = True
        self.search_results(ui, target)

    def prepare_search_input[
        Host: Clipboard
    ](mut self, mut ui: Ui, input: TextInput, mut host: Host) raises:
        self.poll(ui.time)
        _ = self.appearance.update(
            ui,
            self.model.appearance,
            self.model.appearance_provenance,
            self.model.appearance_revision,
            self.model.source_server,
            self.model.source_client,
        )
        if self.appearance.error.byte_length() > 0:
            self.model.error = self.appearance.error
        self.observe_output(ui)
        self.observe_search(ui)
        self.search_frame = SearchInputResult()
        self.search_clipboard = SearchClipboard()
        self.search_clipboard.primary_supported = host.supports_primary()
        self.search_fields.clear()
        var commands = self.backend.search_commands.take()
        for command in commands:
            if (
                command.server != self.model.source_server
                or command.client != self.model.source_client
            ):
                continue
            var target = SearchTarget(command.pane, False, command.pane)
            if command.output_owner:
                target = SearchTarget(
                    command.pane, True, command.output_owner.value()
                )
            elif self.output.owns(command.pane):
                continue
            if not self.search_valid(target):
                continue
            self.ensure_search(target)
            var effect = self.search.begin(
                target, command.direction, SEARCH_ACCEPT_AND_CLOSE
            )
            self.apply_search_effect(ui, target, effect)
        var target = self.search.target_for_field(ui.focus)
        if target and self.search_valid(target.value()):
            var name = target.value().name()
            if (
                input.primary
                and len(input.primaries) == 0
                and len(input.pointers) == 0
                and self.search_clipboard.primary_supported
            ):
                var event = input.primary.value()
                var hit = ui.hit_test(event.x, event.y)
                if (
                    hit == self.search.keys[name].field
                    or hit == self.search.keys[name].container
                ):
                    self.search_clipboard.primary_text = (
                        host.get_primary_selection_bounded(SEARCH_TEXT_BYTES)
                    )
            self.prepare_search_editor(ui, input, target.value())
        self.search_clipboard.deliver(host)
        self.search_prepared = True

    def search_event[
        Host: Clipboard
    ](
        mut self, mut ui: Ui, input: TextInput, event: TextEvent, mut host: Host
    ) raises -> Bool:
        var target = self.search.target_for_field(ui.focus)
        if not target or not self.search_valid(target.value()):
            return False
        var name = target.value().name()
        if not self.search.editors[name].session.query:
            return False
        if event.kind == 1 and event.cmd == CMD_PASTE:
            self.search_clipboard.load(host, SEARCH_TEXT_BYTES)
        var consumed = self.search.editors[name].event(
            ui,
            input,
            event,
            self.search_clipboard,
            target.value().pane,
            self.search_frame,
            target.value().command_output,
        )
        if consumed:
            self.search.touch(name)
        self.search_results(ui, target.value())
        self.search_clipboard.deliver(host)
        return consumed

    def plan_search(
        mut self,
        mut ui: Ui,
        mut keyboard: KeyboardController,
        key: KeyEvent,
        bytes: Span[UInt8, _],
    ) raises -> Optional[SearchPlan]:
        self.observe_search(ui)
        var input = mjui_key_input(
            key.key, key.ch, key.unshifted, key.mods, key.action()
        )
        var closing_key = UInt8(
            2
        ) if key.key == Key.ENTER or key.key == Key.KP_ENTER else UInt8(
            4
        ) if key.key == Key.ESCAPE else UInt8(
            0
        )
        if key.release and key.key in self.search.closing_owners:
            var owner = self.search.closing_owners.pop(key.key)
            if owner.name() in self.search.editors:
                _ = self.search.editors[owner.name()].key_released(closing_key)
            keyboard.claim_text(key, bytes)
            return SearchPlan(
                owner,
                2,
                input^,
                self.focus_epoch,
                self.model.source_server,
                self.model.source_client,
            )
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
        if (
            key.text_lo >= 0
            and key.text_hi > key.text_lo
            and Int(key.text_hi) <= len(bytes)
        ):
            input.text = String(
                from_utf8=bytes[Int(key.text_lo) : Int(key.text_hi)]
            )
        var name = target.value().name()
        if (
            (ui.focus == TERMINAL_KEY or field_target or chrome_target)
            and name in self.search.editors
            and self.search.editors[name].session.swallow_key(
                closing_key if closing_key != 0 else input.key, key.release
            )
        ):
            keyboard.claim_text(key, bytes)
            keyboard.claim_text_command(key)
            return SearchPlan(
                target.value(),
                2,
                input^,
                self.focus_epoch,
                self.model.source_server,
                self.model.source_client,
            )
        if key.release:
            return None
        if (
            field_target
            and closing_key != 0
            and (
                self.search.editors[name].session.query
                or self.search.editors[name].pending_begin
            )
        ):
            self.search.editors[name].key_pressed(closing_key)
            self.search.closing_owners[key.key] = target.value()
        var modal = False
        for session in self.model.snapshot.sessions:
            if self.model.session == Optional[UInt64](session.id):
                for window in session.windows:
                    if self.model.window == Optional[UInt64](window.id):
                        modal = Bool(window.modal)
        var prefix = (
            not target.value().command_output
            and self.model.routing.claims_prefix_input(input)
        )
        var kind: UInt8 = 255
        if (
            ui.focus == TERMINAL_KEY or field_target or chrome_target
        ) and search_shortcut(input, prefix, modal):
            self.ensure_search(target.value())
            self.search.editors[name].pending_begin = True
            self.marked = None
            _ = self.cancel_local_scroll(
                target.value().pane
            ) if not target.value().command_output else False
            ui.focus = self.search.keys[name].field
            kind = 0
        elif (
            name in self.search.editors
            and (field_target or chrome_target)
            and (
                self.search.editors[name].session.query
                or self.search.editors[name].pending_begin
            )
        ):
            var probe = SearchQuery()
            if search_key_action(probe, input):
                kind = 1
            elif (
                chrome_target
                and not field_target
                and (
                    input.key == 0
                    or input.key == 1
                    or input.key == 4
                    or input.key == 5
                )
            ):
                ui.focus = self.search.keys[name].field
                keyboard.claim_text_command(key)
                kind = 4
        if kind == 255:
            return None
        keyboard.claim_text(key, bytes)
        ui.request_wake(ui.time)
        return SearchPlan(
            target.value(),
            kind,
            input^,
            self.focus_epoch,
            self.model.source_server,
            self.model.source_client,
        )

    def dispatch_search(
        mut self, mut ui: Ui, input: TextInput, plan: SearchPlan
    ) raises:
        if (
            plan.server != self.model.source_server
            or plan.client != self.model.source_client
            or not self.search_valid(plan.target)
        ):
            return
        var name = plan.target.name()
        if name not in self.search.editors:
            return
        var effect = SearchEffect()
        if plan.kind == 0:
            effect = self.search.begin(plan.target)
            self.search.editors[name].pending_begin = False
        elif plan.kind == 1:
            effect = self.search.editors[name].session.key_action(
                plan.target.pane, plan.input, plan.target.command_output
            )
        elif plan.kind == 4:
            effect.focus = SEARCH_FOCUS
            effect.redraw = True
        self.apply_search_effect(
            ui,
            plan.target,
            effect,
            epoch=plan.epoch,
            source=(plan.server, plan.client),
        )
        self.prepare_search_editor(ui, input, plan.target, False)

    def finish_search(mut self, mut ui: Ui) raises:
        var target = self.search.target_for_field(ui.focus)
        var protected: Optional[String] = None
        if target:
            protected = target.value().name()
        var effects = self.search.enforce(protected)
        for effect in effects:
            self.apply_search_effect(ui, effect.target, effect.effect, True)

    def paint_search_chrome(
        mut self,
        mut ui: Ui,
        target: SearchTarget,
        mode: TerminalMode,
        status: Optional[TerminalSearch],
        unseen: UInt32,
        width: Float32,
    ) raises:
        var top: Float32 = 8
        var name = target.name()
        if (
            name in self.search.editors
            and self.search.editors[name].session.query
        ):
            var bar_width = min(Float32(460), max(width - 16, Float32(0)))
            self.search.editors[name].field.focused = (
                ui.focus == self.search.keys[name].field
            )
            var result = build_search_chrome(
                self.search.editors[name],
                ui,
                self.theme,
                self.search.keys[name],
                target.pane,
                max(width - bar_width - 8, Float32(0)),
                top,
                bar_width,
                status,
                target.command_output,
                True,
            )
            top += result.height + 4
            for effect in result.effects:
                self.apply_search_effect(ui, target, effect)
        var mode_key = self.widget_key(
            7 if target.command_output else 16, target.owner
        )
        build_mode_indicator(
            ui,
            self.theme,
            mode_key,
            mode,
            unseen,
            max(width - 8, Float32(0)),
            top,
            True,
        )

    def terminal_chrome_hit(
        self, ui: Ui, x: Float32, y: Float32
    ) raises -> Bool:
        var hit = ui.hit_test(x, y)
        if self.search.owns_key(hit):
            return True
        for entry in self.keys.items():
            if entry.value == hit and (
                entry.key.startswith("7:") or entry.key.startswith("16:")
            ):
                return True
        return False

    def search_preserves_output(self, ui: Ui) raises -> Bool:
        var target = self.search.target_for_field(ui.focus)
        return (
            Bool(target)
            and target.value().command_output
            and self.search_valid(target.value())
        )

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
        if (
            self.input_pane() == Optional[UInt64](pane)
            and output == self.output_input
            and (not epoch or epoch.value() == self.focus_epoch)
            and (
                not source
                or (
                    source.value()[0] == self.model.source_server
                    and source.value()[1] == self.model.source_client
                )
            )
        ):
            self.cursor_epoch += 1
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
        self.cursor_epoch += 1
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
            if not self.palette.opened() and not self.modal_open():
                ui.focus = TERMINAL_KEY
            self.focus_epoch += 1

    def focus_output(mut self, mut ui: Ui) raises:
        if self.palette.opened() or self.modal_open():
            return
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
            self.primary.reset()
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

    def pointer_surface_hit(
        self, ui: Ui, event: MouseEvent
    ) raises -> Optional[PointerSurface]:
        if self.painted_source != (
            self.model.source_server,
            self.model.source_client,
        ):
            return None
        var hit = ui.hit_test(event.x, event.y)
        var surfaces = self.pointer_surfaces(ui)
        for index in range(len(surfaces) - 1, -1, -1):
            var surface = surfaces[index]
            if not surface.grid.contains(event.x, event.y):
                continue
            var grid_name = String(
                6, ":", self.output.id.value()
            ) if surface.command_output and self.output.id else String(
                4, ":", surface.pane
            )
            var pane_name = String(5, ":", surface.pane)
            if (grid_name in self.keys and hit == self.keys[grid_name]) or (
                pane_name in self.keys and hit == self.keys[pane_name]
            ):
                return surface
        return None

    def primary_focus(mut self, mut ui: Ui, surface: PointerSurface) raises:
        if surface.command_output:
            self.focus_output(ui)
        else:
            if self.output_input or self.model.pane != Optional[UInt64](
                surface.pane
            ):
                self.blur_pointer()
                self.model.select_pane(surface.pane)
                self.focus_epoch += 1
            ui.focus = TERMINAL_KEY
            self.pointer_focus_pane = surface.pane
            self.pointer_focus_output = False
        self.cursor_epoch += 1

    def primary_notice(mut self, mut ui: Ui, text: String):
        try:
            self.palette.notify(ui, text)
        except error:
            self.model.error = String(error)

    def primary_press[
        Host: Clipboard
    ](mut self, mut ui: Ui, event: MouseEvent, mut host: Host) raises -> Bool:
        self.primary.reset()
        if not self.active or not self.model.session:
            return False
        var surface = self.pointer_surface_hit(ui, event)
        if not surface:
            return False
        var owner_changed = (
            ui.focus != TERMINAL_KEY
            or self.input_pane() != Optional[UInt64](surface.value().pane)
            or self.output_input != surface.value().command_output
        )
        if (
            not surface.value().command_output
            and surface.value().pane not in self.model.terminals
        ):
            return False
        self.primary_focus(ui, surface.value())
        if owner_changed:
            self.marked = None
        var pane = surface.value().pane
        var tracking = (
            self.model.command_output.viewport.value()
            .metadata.mouse_tracking if surface.value()
            .command_output else self.model.terminals[pane]
            .metadata.mouse_tracking
        )
        var target = PrimaryTarget(
            pane,
            self.model.source_server,
            self.model.source_client,
            self.focus_epoch,
        )
        if not self.primary.press(
            target,
            surface.value().command_output,
            tracking,
            event.mods & MOD_SHIFT != 0,
        ):
            var middle = event
            middle.kind = MOUSE_MIDDLE_DOWN
            self.mouse(ui, middle)
            return owner_changed
        if not self.primary.accepts(target):
            return owner_changed
        try:
            var text = String()
            comptime if CompilationTarget.is_linux():
                if host.supports_primary():
                    text = host.get_primary_selection_bounded(
                        MAX_PENDING_INPUT_BYTES
                    )
            else:
                text = host.get_clipboard_bounded(MAX_PENDING_INPUT_BYTES)
            var paste = self.primary.prepare(target, text)
            if not paste:
                return owner_changed
            if (
                len(self.backend.queue) >= MAX_PENDING_ACTIONS
                or paste.value().text.byte_length()
                > MAX_PENDING_INPUT_BYTES - self.backend.queue.pending_bytes
            ):
                self.primary_notice(
                    ui, "pending terminal paste exceeds queue limit"
                )
                return owner_changed
            _ = self.cancel_local_scroll(pane)
            self.backend.paste(
                pane,
                paste.value().text,
                target.epoch,
                (target.server, target.client),
            )
            self.next_poll = 0
            ui.request_wake(ui.time)
        except error:
            self.primary.pending = None
            self.primary_notice(ui, String(error))
        return owner_changed

    def ordered_pointer[
        Host: Clipboard
    ](
        mut self, mut ui: Ui, input: TextInput, event: TextEvent, mut host: Host
    ) raises -> Bool:
        var pointer = pointer_payload(input, event)
        self.ordered_pointer_frame = True
        if event.kind == TEXT_PRIMARY:
            if pointer.kind == MOUSE_MIDDLE_UP:
                if self.primary.release():
                    return False
                self.mouse(ui, pointer)
                return False
            if pointer.kind == MOUSE_MIDDLE_DRAGGED:
                if not self.primary.pressed:
                    self.mouse(ui, pointer)
                return False
            if self.palette.opened():
                var owner = self.palette.component.key + 3
                if ui.retained_input_begin(
                    owner, self.palette.component.field_key
                ):
                    var previous_focus = ui.focus
                    var pasted = editor_primary(
                        self.palette.component.field,
                        ui,
                        pointer,
                        host,
                        PALETTE_INPUT_BYTES - len(input.marked),
                    )
                    ui.retained_input_end()
                    if pasted.limited:
                        self.primary_notice(ui, "Palette input exceeds 64 KiB")
                    if pasted.inserted:
                        if previous_focus == self.palette.component.field_key:
                            editor_composition(
                                self.palette.component.field, ui, input
                            )
                        self.palette.component.changed(
                            self.palette.projection.fleet,
                            self.palette.projection.decorations,
                        )
                        self.palette.sync_files(
                            ui, self.backend, self.model, self.focus_epoch
                        )
                    return (
                        pasted.inserted
                        and previous_focus != self.palette.component.field_key
                    )
                return False
            if self.modal_open():
                return False
            var hit = ui.hit_test(pointer.x, pointer.y)
            var target = self.search.target_for_field(hit)
            if target and self.search_valid(target.value()):
                var name = target.value().name()
                if not self.search.editors[name].session.query:
                    return False
                var previous_focus = ui.focus
                var pasted = editor_primary(
                    self.search.editors[name].field,
                    ui,
                    pointer,
                    host,
                    SEARCH_TEXT_BYTES - len(input.marked),
                )
                if pasted.limited:
                    self.model.error = (
                        "Terminal search input exceeds its byte limit"
                    )
                if pasted.inserted:
                    if previous_focus == self.search.keys[name].field:
                        editor_composition(
                            self.search.editors[name].field, ui, input
                        )
                    self.search.editors[name].changed(
                        target.value().pane,
                        target.value().command_output,
                        self.search_frame,
                    )
                    self.search.touch(name)
                    self.search_results(ui, target.value())
                return (
                    pasted.inserted
                    and previous_focus != self.search.keys[name].field
                )
            return self.primary_press(ui, pointer, host)
        var before_focus = ui.focus
        var before_pane = self.input_pane()
        var before_output = self.output_input
        var scope = EditorPointerScope(ui)
        scope.apply(ui, pointer)
        var single = TextInput()
        single.marked = input.marked.copy()
        single.marked_sel_a = input.marked_sel_a
        single.marked_sel_b = input.marked_sel_b
        try:
            if self.palette.opened():
                var result = self.palette.prepare(
                    ui, single, host, self.backend, self.model, self.focus_epoch
                )
                self.palette.component.clicked_handled = True
                if result.closed or self.palette.files.close_ready:
                    self.close_palette(ui)
                self.palette_effect(ui, result.effect)
                if result.limited:
                    self.primary_notice(ui, "Palette input exceeds 64 KiB")
            else:
                var hit = ui.hit_test(pointer.x, pointer.y)
                var target = self.search.target_for_field(hit)
                if target and self.search_valid(target.value()):
                    var name = target.value().name()
                    var prepared = self.search.editors[name].prepare(
                        ui,
                        single,
                        self.search_clipboard,
                        target.value().pane,
                        target.value().command_output,
                        self.search.keys[name].container,
                    )
                    for effect in prepared.effects:
                        self.search_frame.append(effect.copy())
                    self.search_frame.limited = (
                        self.search_frame.limited or prepared.limited
                    )
                    self.search_results(ui, target.value())
                    self.search_clipboard.deliver(host)
                else:
                    self.mouse(ui, pointer)
        except error:
            scope.restore(ui)
            raise Error(String(error))
        if pointer.kind == MOUSE_UP:
            ui.active = 0
        scope.restore(ui)
        return (
            self.palette_closed_frame
            or before_focus != ui.focus
            or before_pane != self.input_pane()
            or before_output != self.output_input
        )

    def mouse(mut self, mut ui: Ui, event: MouseEvent) raises:
        if self.painted_source != (
            self.model.source_server,
            self.model.source_client,
        ):
            return
        var captured_middle_release = (
            event.kind == MOUSE_MIDDLE_UP and UInt8(1) in self.pointer.captures
        )
        if (
            self.windows.opened()
            and not self.palette.opened()
            and not captured_middle_release
        ):
            return
        if (
            self.chooser.opened()
            and not self.palette.opened()
            and not captured_middle_release
        ):
            var choice = self.chooser.pointer(ui, event)
            if choice:
                self.dispatch_chooser(ui, choice.value())
            return
        if self.palette.opened() and not captured_middle_release:
            return
        if (
            self.terminal_chrome_hit(ui, event.x, event.y)
            and not captured_middle_release
        ):
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
        if (
            event.kind == MOUSE_DOWN
            or event.kind == MOUSE_RIGHT_DOWN
            or event.kind == MOUSE_MIDDLE_DOWN
        ):
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
        if (
            event.kind == MOUSE_DOWN
            or event.kind == MOUSE_RIGHT_DOWN
            or event.kind == MOUSE_UP
            or event.kind == MOUSE_RIGHT_UP
        ):
            for surface in surfaces:
                if surface.grid.contains(event.x, event.y):
                    self.cursor_epoch += 1
                    break
        var routed = self.pointer.route(
            self.model, Span(surfaces), event, self.focus_epoch
        )
        for input in routed:
            self.forward_pointer(ui, input)

    def scroll(
        mut self, mut ui: Ui, event: ScrollEvent, x: Float32, y: Float32
    ) raises:
        if self.palette.opened():
            return
        if self.windows.opened():
            if self.windows.wheel(ui, x, y, event.dx, event.dy, event.precise):
                ui.request_wake(ui.time)
            return
        if self.chooser.opened():
            if self.chooser.wheel(ui, x, y, event.dx, event.dy, event.precise):
                ui.request_wake(ui.time)
            return
        if self.sidebar_hit(ui, x, y):
            if self.sidebar_wheel(ui, event.dy, event.precise):
                ui.request_wake(ui.time)
            return
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
        if self.palette.opened() or self.modal_open():
            return
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
        var width = max(
            ui.width - (SIDEBAR_WIDTH if self.sidebar_shown else 0), Float32(0)
        )
        var height = max(ui.height - TOOLBAR_HEIGHT - STATUS_HEIGHT, Float32(0))
        var options = self.appearance.options(
            ui, False, blink_epoch=self.cursor_epoch
        )
        var metrics = cell_metrics_options(
            ui.shaper.fonts[Int(options.font)], options, ui.scale
        )
        var rects = List[PaneRect]()
        var kinds = Dict[UInt64, UInt8]()
        for session in self.model.snapshot.sessions:
            for window in session.windows:
                if self.model.window and self.model.window.value() == window.id:
                    rects = window_rects(
                        window, width, height, metrics.width(), metrics.height()
                    )
                    for entry in window.panes.items():
                        kinds[entry.key] = entry.value.kind.kind
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
            var interaction = ui.interact(
                key,
                focusable=True,
                focus_on_press=not self.ordered_pointer_frame,
            )
            if interaction.pressed and not self.ordered_pointer_frame:
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
            var pad_l = max(item.inset_x, self.appearance.active.padding_left)
            var pad_r = max(item.inset_x, self.appearance.active.padding_right)
            var pad_t = max(item.inset_y, self.appearance.active.padding_top)
            var pad_b = max(item.inset_y, self.appearance.active.padding_bottom)
            var inner_width = bounds.width - pad_l - pad_r
            var inner_height = bounds.height - pad_t - pad_b
            options.focused = (
                selected and ui.focus == TERMINAL_KEY and self.active
            )
            var pane_style = (
                Style.col()
                .at(bounds.x, bounds.y)
                .width(bounds.width)
                .height(bounds.height)
                .pad(pad_l, pad_t)
                .fill(background)
                .clipped()
                .id(self.widget_key(5, item.pane))
            )
            pane_style.pad_r = pad_r
            pane_style.pad_b = pad_b
            var columns = grid_dimension(inner_width, metrics.width(), ui.scale)
            var rows = grid_dimension(inner_height, metrics.height(), ui.scale)
            if (
                not output
                and item.pane in self.model.terminals
                and grid_placement(
                    self.model.terminals[item.pane],
                    scrolled_locally=Bool(self.local_scroll.target(item.pane))
                    or self.local_scroll.sub_row(item.pane) > 0,
                ).bottom_anchored
            ):
                pane_style.pad_t += max(
                    inner_height - Float32(rows) * metrics.height(),
                    Float32(0),
                )
            ui.open(pane_style)
            if output:
                if self.output_seen != self.output.id:
                    var search_focus = self.search.target_for_field(ui.focus)
                    self.focus_output(ui)
                    self.output_seen = self.output.id
                    if (
                        search_focus
                        and search_focus.value().command_output
                        and search_focus.value().owner == self.output.id.value()
                    ):
                        ui.focus = self.search.keys[
                            search_focus.value().name()
                        ].field
                        self.blur_pointer(True)
                options.focused = (
                    self.output.focused
                    and ui.focus == TERMINAL_KEY
                    and self.active
                )
                _ = self.output.render(
                    ui,
                    key,
                    self.model.command_output,
                    options.font,
                    options.size,
                    options.focused,
                    Int(columns),
                    Int(rows),
                    self.marked if self.output_input else Optional[String](
                        None
                    ),
                    options,
                )
                self.visible_panes.append(item.pane)
                var resized = self.output.resize(
                    GridSize(
                        columns, rows, UInt32(metrics.w), UInt32(metrics.h), 0
                    ),
                    Bool(self.model.session),
                    inner_width >= metrics.width()
                    and inner_height >= metrics.height(),
                )
                if resized:
                    self.backend.input(
                        item.pane, resized.value(), self.focus_epoch, True
                    )
                var mode = (
                    self.model.command_output.viewport.value().metadata.mode
                )
                var status = (
                    self.model.command_output.viewport.value().metadata.search
                )
                var unseen = (
                    self.model.command_output.viewport.value().metadata.unseen_output
                )
                self.paint_search_chrome(
                    ui,
                    SearchTarget(item.pane, True, self.output.id.value()),
                    mode,
                    status,
                    unseen,
                    max(inner_width, Float32(0)),
                )
            elif item.pane in self.model.terminals:
                ref viewport = self.model.terminals[item.pane]
                if item.pane in self.model.histories:
                    _ = render_retained_viewport_opts(
                        ui,
                        key,
                        viewport,
                        self.model.histories[item.pane],
                        self.local_scroll.target(item.pane),
                        self.local_scroll.sub_row(item.pane),
                        options,
                        Int(columns),
                        Int(rows),
                        self.marked if selected else Optional[String](None),
                    )
                else:
                    _ = render_viewport_opts(
                        ui,
                        key,
                        viewport,
                        options,
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
                var unseen = self.model.terminals[
                    item.pane
                ].metadata.unseen_output
                self.paint_search_chrome(
                    ui,
                    SearchTarget(item.pane, False, item.pane),
                    mode,
                    status,
                    unseen,
                    max(inner_width, Float32(0)),
                )
            elif kinds.get(item.pane, UInt8(1)) == 0:
                self.paint_picker(ui, item.pane)
            else:
                label(ui, self.theme, "Connecting to terminal…", flat.dim)
            ui.close()
        ui.close()

    def host_state(self) -> HostState:
        if self.backend.online:
            return HostState(HOST_CONNECTED)
        var reason = self.model.error.copy()
        if reason.byte_length() == 0:
            return HostState(HOST_CONNECTING)
        if reason.byte_length() > HOST_REASON_BYTES:
            reason = String("the daemon connection failed")
        return HostState.unreachable(reason^)

    def sync_sidebar(mut self, mut ui: Ui):
        var state = self.host_state()
        var attached = self.model.attached_session
        var seen = String(
            self.model.generation,
            ":",
            self.model.source_server,
            ":",
            self.model.source_client,
            ":",
            state.kind,
            ":",
            self.backend.platform.kind,
            ":",
            self.backend.platform.distro,
            ":",
            attached.value() if attached else UInt64(0),
            ":",
            Int(Bool(attached)),
            ":",
            state.reason,
        )
        if seen == self.sidebar_seen:
            return
        self.sidebar_seen = seen^
        var snapshot: Optional[MuxSnapshot] = None
        if self.model.generation != 0:
            snapshot = self.model.snapshot.copy()
        var hosts: List[FleetHostInput] = [
            FleetHostInput(
                LOCAL_HOST,
                self.sidebar.local_hostname.copy(),
                state^,
                self.backend.platform,
                snapshot^,
            )
        ]
        try:
            var scroll = self.sidebar.update(Span(hosts), LOCAL_HOST, attached)
            if scroll:
                self.sidebar_reveal(scroll.value(), ui)
        except error:
            self.primary_notice(ui, String(error))

    def sidebar_capacity(self, ui: Ui) -> Int:
        var height = max(ui.height - TOOLBAR_HEIGHT - STATUS_HEIGHT - 16, 0)
        return max(Int(height / SIDEBAR_ROW), 1)

    def sidebar_reveal(mut self, row: Int, ui: Ui):
        var capacity = self.sidebar_capacity(ui)
        if row < self.sidebar_first:
            self.sidebar_first = row
        elif row >= self.sidebar_first + capacity:
            self.sidebar_first = row - capacity + 1

    def sidebar_hit(self, ui: Ui, x: Float32, y: Float32) -> Bool:
        if not self.sidebar_shown:
            return False
        var rect = ui.rect_of(SIDEBAR_KEY)
        return x >= rect.x0 and x < rect.x1 and y >= rect.y0 and y < rect.y1

    def sidebar_wheel(mut self, ui: Ui, dy: Float32, precise: Bool) -> Bool:
        var lines = Int(-dy / SIDEBAR_ROW) if precise else Int(-dy)
        if lines == 0:
            lines = -1 if dy > 0 else 1 if dy < 0 else 0
        var last = max(self.sidebar.row_count() - self.sidebar_capacity(ui), 0)
        var first = min(max(self.sidebar_first + lines, 0), last)
        var moved = first != self.sidebar_first
        self.sidebar_first = first
        return moved

    def nav_source(self) -> DesktopNavSource:
        var scope = self.backend.path_source.value().epoch if self.backend.path_source else UInt64(
            0
        )
        return DesktopNavSource(
            scope,
            self.model.source_server,
            self.model.source_client,
            self.focus_epoch,
        )

    def nav_capabilities(self) -> List[String]:
        var capabilities = List[String]()
        if self.backend.capabilities & NEW_SESSION_ATTACH_BIT != 0:
            capabilities.append(String("new-session-attach-v1"))
        return capabilities^

    def apply_nav(mut self, mut ui: Ui, intent: DesktopNavIntent) raises:
        if intent.scroll:
            self.sidebar_reveal(intent.scroll.value(), ui)
        if intent.activation:
            var admission = stage_navigation(
                self.backend.queue,
                intent.activation.value(),
                self.sidebar.attached_host,
                self.nav_source(),
            )
            if admission.host_request:
                self.primary_notice(
                    ui, "This client is attached to one daemon at a time"
                )
            commit_navigation(self.backend.queue, admission^, self.nav_source())
            self.next_poll = 0
            ui.request_wake(ui.time)
        if intent.release_focus or intent.focus_pane:
            ui.focus = TERMINAL_KEY

    def nav(mut self, mut ui: Ui, intent: DesktopNavIntent):
        try:
            self.apply_nav(ui, intent)
        except error:
            self.primary_notice(ui, String(error))

    def chrome_dispatch(
        mut self, mut ui: Ui, dispatch: DesktopChromeDispatch
    ) raises -> Bool:
        if dispatch.kind >= 1 and dispatch.kind <= 6:
            var intent = self.sidebar.chrome_intent(
                dispatch, self.nav_capabilities(), current_directory()
            )
            if intent:
                self.apply_nav(ui, intent.value())
            return True
        if dispatch.kind == 7:
            self.sidebar_shown = not self.sidebar_shown
            ui.request_wake(ui.time)
            return True
        if dispatch.kind == 8:
            self.primary_notice(ui, "Settings are not available in this client yet")
            return True
        return False

    def modal_capture(self) -> Bool:
        for session in self.model.snapshot.sessions:
            if self.model.session == Optional[UInt64](session.id):
                for window in session.windows:
                    if self.model.window == Optional[UInt64](window.id):
                        return Bool(window.modal)
        return False

    def pane_kind(self, pane: UInt64) -> Optional[UInt8]:
        for session in self.model.snapshot.sessions:
            for window in session.windows:
                if pane in window.panes:
                    try:
                        return window.panes[pane].kind.kind
                    except:
                        return None
        return None

    def picker_choices(self) -> List[String]:
        var choices: List[String] = [String("terminal")]
        return choices^

    def picker_pane(self, ui: Ui) -> Optional[UInt64]:
        if ui.focus != TERMINAL_KEY or not self.active or self.output_input:
            return None
        if not self.model.pane:
            return None
        var pane = self.model.pane.value()
        if self.pane_kind(pane) != Optional[UInt8](0):
            return None
        return pane

    def picker_command(
        mut self, mut ui: Ui, name: String, arguments: List[String]
    ) raises:
        self.backend.palette_command(
            nav_command(name, arguments),
            self.focus_epoch,
            (self.model.source_server, self.model.source_client),
        )
        self.next_poll = 0
        ui.request_wake(ui.time)

    def picker_choose(mut self, mut ui: Ui, pane: UInt64, choice: Int) raises:
        var choices = self.picker_choices()
        if choice < 0 or choice >= len(choices):
            return
        self.picker_command(
            ui,
            "select-pane-kind",
            [String("-t"), String("%", pane), choices[choice].copy()],
        )

    def picker_key(mut self, mut ui: Ui, key: KeyEvent) raises -> Bool:
        if key.release or key.consumed:
            return False
        var pane = self.picker_pane(ui)
        if not pane:
            return False
        if key.mods & (MOD_CTRL | MOD_ALT | MOD_CMD) != 0:
            return False
        var choices = self.picker_choices()
        var selected = min(
            self.picker_selected.get(pane.value(), 0), len(choices) - 1
        )
        if key.key == Key.T:
            self.picker_choose(ui, pane.value(), 0)
        elif key.key == Key.J or key.key == Key.DOWN:
            self.picker_selected[pane.value()] = (selected + 1) % len(choices)
        elif key.key == Key.K or key.key == Key.UP:
            self.picker_selected[pane.value()] = (
                selected + len(choices) - 1
            ) % len(choices)
        elif key.key == Key.ENTER or key.key == Key.KP_ENTER:
            self.picker_choose(ui, pane.value(), selected)
        elif key.key == Key.ESCAPE:
            self.picker_command(
                ui, "kill-pane", [String("-t"), String("%", pane.value())]
            )
        else:
            return False
        ui.request_wake(ui.time)
        return True

    def paint_picker(mut self, mut ui: Ui, pane: UInt64) raises:
        var flat = self.theme.flat
        var choices = self.picker_choices()
        var selected = min(self.picker_selected.get(pane, 0), len(choices) - 1)
        ui.open(Style.col().grow().grow_h().center().spacing(8))
        label(ui, self.theme, "New pane", flat.dim, strong=True)
        for index in range(len(choices)):
            var title = String("Terminal  t")
            var row_key = self.name_key(String("picker:", pane, ":", index))
            if row(
                ui,
                self.theme,
                row_key,
                title,
                selected=index == selected,
                height=28,
            ):
                self.picker_selected[pane] = index
                if self.model.pane != Optional[UInt64](pane):
                    self.picker_command(
                        ui, "select-pane", [String("-t"), String("%", pane)]
                    )
                self.picker_choose(ui, pane, index)
        ui.close()

    def paint_sidebar(mut self, mut ui: Ui) raises:
        if not self.sidebar_shown:
            return
        var flat = self.theme.flat
        panel_begin(ui, self.theme, width=SIDEBAR_WIDTH)
        ui.open(
            Style.col()
            .grow()
            .grow_h()
            .pad(4, 8)
            .clipped()
            .id(SIDEBAR_KEY)
        )
        var capacity = self.sidebar_capacity(ui)
        var last = max(self.sidebar.row_count() - capacity, 0)
        self.sidebar_first = min(max(self.sidebar_first, 0), last)
        var rows = self.sidebar.rows(self.sidebar_first, capacity, False)
        var clicked: Optional[NavNode] = None
        var toggled: Optional[NavNode] = None
        for item in rows:
            var shown = PaletteRow(item.label.copy())
            shown.indent = Float32(item.depth) * 12
            if item.expandable:
                shown.expanded = item.expanded
            if item.indicator:
                if item.indicator.value().failed:
                    shown.detail = self.host_state().label()
                else:
                    shown.running = True
            elif item.bell:
                shown.running = True
            if item.shortcut:
                shown.right = String(Int(item.shortcut.value()) + 1)
            var name = String(
                item.node.host, ":", item.node.kind, ":", item.node.id
            )
            var row_key = self.name_key(String("sidebar:", name))
            var disclosure = self.name_key(String("sidebar-toggle:", name))
            var response = palette_tree_row(
                ui,
                self.theme,
                row_key,
                shown,
                item.active or item.selected,
                SIDEBAR_WIDTH - 8,
                disclosure,
            )
            if response.toggled:
                toggled = item.node
            elif response.clicked:
                clicked = item.node
        if len(rows) == 0:
            ui.open(Style.row().height(28).pad(8, 0).center())
            label(ui, self.theme, "Connecting to zz", flat.dim)
            ui.close()
        ui.close()
        panel_end(ui)
        if toggled:
            self.sidebar.disclose(toggled.value())
        elif clicked:
            self.nav(ui, self.sidebar.click(clicked.value()))

    def paint_which_key(mut self, mut ui: Ui) raises:
        var attached = Bool(self.model.attached_session) and Bool(
            self.model.key_tables.snapshot
        )
        var prefix: Optional[String] = None
        if attached and UInt8(0) in self.model.routing.options:
            prefix = canonical_key(self.model.routing.options[UInt8(0)].value)
        var context = WhichKeyContext(
            self.model.routing.table.copy(),
            self.model.routing.repeat,
            prefix^,
            self.palette.opened()
            or self.modal_open()
            or Bool(self.output.id),
            TOOLBAR_HEIGHT,
            STATUS_HEIGHT,
        )
        var refusal: Optional[String]
        if attached:
            refusal = self.which_key.frame(
                ui,
                self.theme,
                self.model.key_tables.snapshot.value()[].tables,
                context,
            ).refusal
        else:
            refusal = self.which_key.frame(
                ui, self.theme, List[PrefixTable](), context
            ).refusal
        if refusal:
            self.primary_notice(ui, refusal.value())

    def build(mut self, mut ui: Ui, input: TextInput) raises:
        var search_skip = 0
        if not self.palette_prepared:
            var host = SearchClipboard()
            self.prepare_palette_input(ui, input, host)
            for event in input.events:
                if not self.palette_event(ui, input, event, host):
                    break
                search_skip += 1
        self.palette_prepared = False
        if not self.search_prepared:
            var host = SearchClipboard()
            self.prepare_search_input(ui, input, host)
            for index in range(search_skip, len(input.events)):
                if not self.search_event(ui, input, input.events[index], host):
                    break
                search_skip += 1
            self.finish_search(ui)
        self.search_prepared = False
        if not self.chooser_prepared:
            self.prepare_chooser_input(ui, input)
            for index in range(search_skip, len(input.events)):
                if not self.chooser_event(ui, input, input.events[index]):
                    break
                search_skip += 1
        self.chooser_prepared = False
        if not self.windows_prepared:
            self.prepare_windows_input(ui, input)
            var host = SearchClipboard()
            for index in range(search_skip, len(input.events)):
                if not self.windows_event(ui, input, input.events[index], host):
                    break
                search_skip += 1
            self.compose_windows(ui, input)
        self.windows_prepared = False
        self.poll(ui.time)
        if self.config_notice:
            self.primary_notice(ui, self.config_notice.take())
        self.sync_sidebar(ui)
        if self.model.key_table_changes != self.which_key_changes:
            self.which_key_changes = self.model.key_table_changes
            try:
                self.which_key.apply(
                    WhichKeyIntent.table_changed(
                        UInt64(max(ui.time, Float64(0)) * 1_000_000_000),
                        self.model.routing.table,
                        self.model.routing.repeat,
                        self.which_key_delay,
                    )
                )
            except error:
                self.primary_notice(ui, String(error))
        var chooser_eligible = (
            self.backend.job[].native_chooser
            and self.backend.online
            and Bool(self.model.dialogs.chooser)
        )
        if chooser_eligible:
            chooser_eligible = self.model.dialogs.chooser.value().kind != 0
        if chooser_eligible != self.chooser.session.opened() or (
            chooser_eligible
            and (
                self.model.dialogs.chooser_revision
                != self.chooser.session.input.revision
                or self.model.dialogs.chooser_closed_revision
                != self.chooser.session.closed_revision
                or not self.chooser.session.source.matches(
                    self.chooser_source()
                )
            )
        ):
            var pointer_handled = self.chooser.pointer_handled
            var unmark = self.chooser_unmark
            var closed = self.chooser_closed_frame
            self.prepare_chooser_input(ui, input)
            self.chooser.pointer_handled = (
                self.chooser.pointer_handled or pointer_handled
            )
            self.chooser_unmark = self.chooser_unmark or unmark
            self.chooser_closed_frame = self.chooser_closed_frame or closed
            self.chooser_prepared = False
        self.pump_palette_files(ui)
        if self.palette.component.opened():
            self.palette.refresh(self.model, self.backend.online)
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
        self.marked = String(
            from_utf8=Span(input.marked)
        ) if ui.focus == TERMINAL_KEY and len(input.marked) > 0 else Optional[
            String
        ](
            None
        )
        self.observe_output(ui)
        if ui.focus != TERMINAL_KEY or (
            self.active and self.pointer_focus_pane != self.input_pane()
        ):
            self.blur_pointer(
                self.search_preserves_output(ui)
                or self.palette.opened()
                or self.modal_open()
            )
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
        var toolbar: Optional[DesktopChromeDispatch] = None
        if button(ui, self.theme, 1, "New session"):
            toolbar = DesktopChromeDispatch(1, 0)
        if button(
            ui,
            self.theme,
            5,
            "New window",
            disabled=not self.sidebar.attached_session,
        ):
            toolbar = DesktopChromeDispatch(2, 0)
        if button(
            ui,
            self.theme,
            2,
            "Split right",
            disabled=not self.sidebar.active_pane,
        ):
            toolbar = DesktopChromeDispatch(4, 0)
        if button(
            ui,
            self.theme,
            3,
            "Split down",
            disabled=not self.sidebar.active_pane,
        ):
            toolbar = DesktopChromeDispatch(5, 0)
        if button(ui, self.theme, 4, "Reconnect"):
            try:
                self.backend.enqueue(
                    RECONNECT, selection_epoch=self.focus_epoch
                )
                ui.focus = TERMINAL_KEY
            except error:
                self.model.error = String(error)
        ui.close()
        if toolbar:
            try:
                _ = self.chrome_dispatch(ui, toolbar.value())
                ui.focus = TERMINAL_KEY
            except error:
                self.primary_notice(ui, String(error))
        ui.open(Style.row().grow().grow_h())
        self.paint_sidebar(ui)
        ui.open(Style.col().grow().grow_h())
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
        elif self.model.message:
            label(
                ui,
                self.theme,
                self.model.message.value().text,
                flat.danger if self.model.message.value().kind
                == 2 else flat.dim,
            )
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
            self.blur_pointer(
                self.search_preserves_output(ui)
                or self.palette.opened()
                or self.modal_open()
            )
        self.pointer_focus_pane = (
            self.input_pane() if ui.focus == TERMINAL_KEY
            and self.active else Optional[UInt64](None)
        )
        self.pointer_focus_output = self.output_input and Bool(
            self.pointer_focus_pane
        )
        if ui.focus != TERMINAL_KEY and not self.search.target_for_chrome(
            ui.focus
        ):
            self.search.reset_keys()
        self.paint_which_key(ui)
        if self.chooser.opened():
            try:
                var choices = self.chooser.paint(
                    ui, self.theme, self.theme.flat.font_bold
                )
                for choice in choices:
                    self.dispatch_chooser(ui, choice)
            except error:
                self.chooser.reset(ui)
                ui.focus = TERMINAL_KEY
                self.primary_notice(ui, String(error))
        try:
            self.windows.paint(ui, self.theme)
        except error:
            self.windows.reset(ui)
            if not self.palette.opened():
                ui.focus = TERMINAL_KEY
            self.primary_notice(ui, String(error))
        if self.palette.component.opened():
            ui.focus = self.palette.component.field_key
            var result = self.palette.component.build(
                ui,
                self.theme,
                self.palette.projection.fleet,
                self.palette.projection.decorations,
                ui.width,
                ui.height,
            )
            if self.palette.files.files.active:
                if self.palette.component.model.controller.selected:
                    self.palette.files.files.ranking.selected = (
                        self.palette.component.model.controller.selected.value()
                    )
                if result.clicked:
                    self.palette.file_click(
                        ui, self.backend, self.model, result.clicked.value()
                    )
                self.palette.file_notices(ui)
                if self.palette.files.close_ready:
                    self.close_palette(ui)
            self.palette_effect(ui, result.effect)
        self.palette.paint_notifications(ui, self.theme)
        self.chooser.record_input_owner(ui)
        self.painted_source = (
            self.model.source_server,
            self.model.source_client,
        )
        self.ordered_pointer_frame = False
