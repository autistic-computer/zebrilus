from mjui.ui import Ui, TextStyle
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_notification_state import Notification, NotificationHost
from mjui.widgets.penna_notification_host import notification_host
from std.sys.info import CompilationTarget
from mjui.input import (
    Clipboard,
    TextInput,
    TextEvent,
    TEXT_COMMAND,
    CMD_PASTE,
    CMD_UP,
    CMD_DOWN,
    CMD_LEFT,
    CMD_RIGHT,
    CMD_DOC_START,
    CMD_DOC_END,
    CMD_NEWLINE,
    CMD_NEWLINE_RAW,
    CMD_TAB,
    CMD_BACKTAB,
    CMD_DELETE_BACK,
    CMD_EXTEND,
    CMD_LOGICAL,
    MOD_ALT,
    MOD_CMD,
    MOD_CTRL,
    MOD_SHIFT,
)
from .palette_component import PaletteComponent, PaletteComponentResult
from .palette_controller import PALETTE_INPUT_BYTES
from .palette_snapshot import PaletteProjection, project_palette_snapshot
from .palette_places_model import PaletteFleet
from .palette_presentation import PaletteDecorations
from .palette_programs import PalettePrograms
from .palette_verbs import palette_verbs
from .palette_host import palette_local_hostname
from .search_clipboard import SearchClipboard
from .state import AppModel
from .backend import AsyncBackend
from .palette_files_runtime import PaletteFilesRuntime
from .palette_files_component import palette_file_rows, palette_file_hints
from .palette_files import (
    FilesEffect,
    FILES_UNHANDLED,
    FILES_SET_REST,
    FILES_ENTER,
    FILES_TAB,
    FILES_SHIFT_TAB,
    FILES_LEFT,
    FILES_RIGHT,
    FILES_UP,
    FILES_DOWN,
    FILES_FIRST,
    FILES_LAST,
    FILES_BACKSPACE,
    FILES_COPY,
)
from .path_context import files_place
from .palette_controller import PaletteDraft
from .cli_default_home import resolve_home
from std.os import getenv

comptime PALETTE_KEY: UInt64 = 0x6000000000000000


@fieldwise_init
struct PalettePlan(Copyable, Movable):
    var kind: UInt8
    var server: UInt64
    var client: UInt64
    var epoch: UInt64


struct PaletteRuntime(Movable):
    var component: PaletteComponent
    var programs: PalettePrograms
    var hostname: String
    var projection: PaletteProjection
    var source: Tuple[UInt64, UInt64]
    var generation: UInt64
    var session: Optional[UInt64]
    var window: Optional[UInt64]
    var pane: Optional[UInt64]
    var online: Bool
    var aliases: String
    var pending_open: Bool
    var pending_close: Bool
    var restore_output: Optional[Tuple[UInt64, UInt64, UInt64]]
    var held: Dict[UInt16, Bool]
    var clipboard: SearchClipboard
    var notifications: NotificationHost
    var files: PaletteFilesRuntime
    var files_rest: String
    var insert_pane: Optional[UInt64]
    var home: Optional[String]

    def __init__(out self) raises:
        self.component = PaletteComponent(
            PALETTE_KEY, TextStyle(0, 12, 0xFFFFFFFF, False)
        )
        self.programs = PalettePrograms()
        self.hostname = palette_local_hostname()
        self.projection = PaletteProjection(
            PaletteFleet(), PaletteDecorations()
        )
        self.source = (0, 0)
        self.generation = 0
        self.session = None
        self.window = None
        self.pane = None
        self.online = False
        self.aliases = String()
        self.pending_open = False
        self.pending_close = False
        self.restore_output = None
        self.held = Dict[UInt16, Bool]()
        self.clipboard = SearchClipboard()
        self.notifications = NotificationHost()
        self.files = PaletteFilesRuntime()
        self.files_rest = String()
        self.insert_pane = None
        var home = getenv("HOME", "")
        if home.byte_length() > 16384:
            raise Error("palette home path exceeds 16 KiB")
        self.home = home.copy() if home.byte_length() > 0 else resolve_home()

    def opened(self) -> Bool:
        return self.pending_open or (
            self.component.opened() and not self.pending_close
        )

    def owns_input(self, ui: Ui) -> Bool:
        return self.opened() and ui.focus == self.component.field_key

    def reset_keys(mut self):
        self.held.clear()

    def refresh(
        mut self, model: AppModel, online: Bool, force: Bool = False
    ) raises:
        var aliases = (
            model.routing.options[20].value.copy() if 20
            in model.routing.options else String()
        )
        var changed = (
            force
            or self.generation != model.snapshot.generation
            or self.source != (model.source_server, model.source_client)
            or self.session != model.session
            or self.window != model.window
            or self.pane != model.pane
            or self.online != online
        )
        var verbs_changed = aliases != self.aliases
        if changed:
            self.projection = project_palette_snapshot(
                model.snapshot,
                model.session,
                self.hostname,
                self.programs,
                online=online,
                apple=CompilationTarget.is_macos(),
            )
            self.source = (model.source_server, model.source_client)
            self.generation = model.snapshot.generation
            self.session = model.session
            self.window = model.window
            self.pane = model.pane
            self.online = online
        if verbs_changed:
            self.component.model.verbs = palette_verbs(
                self.component.model.commands, False, False, True, aliases
            )
            self.aliases = aliases^
        if changed or verbs_changed:
            self.component.refresh(
                self.projection.fleet, self.projection.decorations
            )

    def open(mut self, mut ui: Ui, model: AppModel, online: Bool) raises:
        self.pending_open = False
        self.pending_close = False
        try:
            self.refresh(model, online, True)
            self.component.open(
                ui, self.projection.fleet, self.projection.decorations
            )
        except error:
            self.component.close(ui)
            self.notify(ui, String(error))

    def notify(mut self, mut ui: Ui, text: String) raises:
        _ = self.notifications.push(
            Notification(text.copy(), autohide=Optional[Float64](2.6)), ui.time
        )
        ui.request_wake(ui.time)

    def paint_notifications(mut self, mut ui: Ui, theme: PennaTheme) raises:
        notification_host(ui, theme, self.notifications, PALETTE_KEY + 4)

    def close(mut self, mut ui: Ui):
        self.pending_open = False
        self.pending_close = False
        self.restore_output = None
        self.insert_pane = None
        self.component.close(ui)

    def prepare[
        Host: Clipboard
    ](
        mut self,
        mut ui: Ui,
        input: TextInput,
        mut host: Host,
        mut backend: AsyncBackend,
        model: AppModel,
        epoch: UInt64,
    ) raises -> PaletteComponentResult:
        self.clipboard.deliver(host)
        self.clipboard = SearchClipboard()
        var result = PaletteComponentResult()
        if not self.component.opened():
            return result^
        self.clipboard.primary_supported = host.supports_primary()
        var allow_primary = True
        var primary_owned = True
        if input.primary:
            var event = input.primary.value()
            primary_owned = (
                ui.hit_test(event.x, event.y) == self.component.field_key
            )
        if input.primary and primary_owned and self.clipboard.primary_supported:
            try:
                self.clipboard.primary_text = (
                    host.get_primary_selection_bounded(PALETTE_INPUT_BYTES)
                )
            except:
                allow_primary = False
        result = self.component.prepare(
            ui,
            input,
            self.clipboard,
            self.projection.fleet,
            self.projection.decorations,
            allow_primary,
            primary_owned,
        )
        self.sync_files(ui, backend, model, epoch)
        if result.clicked and self.files.files.active:
            self.file_click(ui, backend, model, result.clicked.value())
        self.file_notices(ui)
        self.clipboard.deliver(host)
        return result^

    def event[
        Host: Clipboard
    ](
        mut self,
        mut ui: Ui,
        input: TextInput,
        event: TextEvent,
        mut host: Host,
        mut backend: AsyncBackend,
        model: AppModel,
        epoch: UInt64,
    ) raises -> PaletteComponentResult:
        var result = PaletteComponentResult()
        if (
            not self.component.opened()
            or ui.layers.modal != self.component.key + 3
        ):
            return result^
        if self.files.files.active and event.kind == TEXT_COMMAND:
            var action = self.file_key(event)
            if action:
                var effect = self.files.files.key(
                    action.value(), self.files.inserts(model)
                )
                if effect.kind != FILES_UNHANDLED:
                    self.file_effect(ui, backend, model, effect)
                    result.consumed = True
                    self.project_files(model)
                    self.file_notices(ui)
                    self.clipboard.deliver(host)
                    return result^
        if event.kind == TEXT_COMMAND and event.cmd == CMD_PASTE:
            try:
                self.clipboard.load(host, PALETTE_INPUT_BYTES)
            except:
                result.consumed = True
                result.limited = True
                return result^
        result = self.component.event(
            ui,
            input,
            event,
            self.clipboard,
            self.projection.fleet,
            self.projection.decorations,
        )
        self.sync_files(ui, backend, model, epoch)
        self.file_notices(ui)
        self.clipboard.deliver(host)
        return result^

    def set_files_body(mut self, text: String) raises:
        var slot = self.component.model.controller.next_slot()
        if not slot or slot.value().kind != 3:
            for verb in self.component.model.verbs:
                if verb.run == 3:
                    self.component.model.controller.draft = PaletteDraft(verb)
                    break
        self.component.model.controller.set_query(text)
        self.component.sync_field()

    def project_files(mut self, model: AppModel) raises:
        if not self.files.files.active:
            return
        self.component.external_rows = True
        self.component.model.rows = palette_file_rows(self.files.files)
        var selected = self.files.files.ranking.selected
        self.component.model.controller.selected = (
            selected if selected >= 0
            and selected
            < len(self.component.model.rows) else Optional[Int](None)
        )
        self.component.external_empty = self.files.files.empty_message()
        self.component.external_hints = palette_file_hints(
            self.files.files, self.files.inserts(model)
        )
        if self.files.files.scroll != 0:
            self.component.scroll_selected = True
            self.files.files.scroll = 0

    def sync_files(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        model: AppModel,
        epoch: UInt64,
    ) raises:
        var slot = self.component.model.controller.next_slot()
        if not self.component.opened() or not slot or slot.value().kind != 3:
            if self.files.files.active:
                self.files.close(backend)
                self.component.external_rows = False
                self.component.model.rebuild(
                    self.projection.fleet, self.projection.decorations
                )
            return
        if not backend.path_source:
            if self.files.files.active:
                _ = self.files.tick()
                self.project_files(model)
                if self.files.busy():
                    ui.request_wake(ui.time + 0.016)
            return
        self.files.observe_source(backend)
        var written = UInt8(0)
        for chip in self.component.model.controller.chips:
            if not chip.automatic:
                written |= UInt8(1) << chip.level
        var place = files_place(
            self.component.model.controller.scope(),
            written,
            self.projection.fleet.here,
        )
        var same = False
        if self.files.files.active and self.files.files.place:
            ref prior = self.files.files.place.value()
            same = (
                prior.host == place.host
                and prior.session == place.session
                and prior.window == place.window
                and prior.pane == place.pane
            )
        if not same:
            var host = self.projection.fleet.host_index(place.host)
            if not host or place.host != self.projection.fleet.here:
                self.notify(ui, "File listing is not connected to this host")
                return
            var start = self.files.open(
                backend,
                model,
                backend.path_source.value(),
                place,
                self.projection.fleet.hosts[host.value()].name,
                True,
                self.insert_pane if self.insert_pane else model.pane,
                self.home,
                None,
                epoch,
            )
            self.files_rest = String()
            if self.component.model.controller.query.byte_length() == 0:
                self.component.model.controller.set_query(start)
                self.component.sync_field()
            self.component.scroll_top = True
        var query = self.component.model.controller.query.copy()
        if query != self.files_rest or not self.files.files.service.listing:
            try:
                self.files.rest(backend, model, query)
            except error:
                self.component.model.controller.set_query(self.files_rest)
                self.component.sync_field()
                self.notify(ui, String(error))
                return
            self.files_rest = query^
        _ = self.files.tick()
        self.project_files(model)
        if self.files.busy():
            ui.request_wake(ui.time + 0.016)
        if self.files.pick_ready:
            self.files.pick_ready = False
            self.file_effect(
                ui,
                backend,
                model,
                self.files.files.pick(False, self.files.inserts(model)),
            )
        self.file_notices(ui)

    def file_key(self, event: TextEvent) -> Optional[UInt8]:
        if event.cmd & CMD_EXTEND != 0:
            return None
        var command = event.cmd & ~(CMD_EXTEND | CMD_LOGICAL)
        var mods = event.mods & (MOD_SHIFT | MOD_CTRL | MOD_ALT | MOD_CMD)
        var end = (
            self.component.field.caret == len(self.component.field.buf)
            and self.component.field.anchor == self.component.field.caret
        )
        if (
            command == CMD_NEWLINE or command == CMD_NEWLINE_RAW
        ) and mods == MOD_ALT:
            return UInt8(FILES_COPY)
        if (command == CMD_UP or command == CMD_DOWN) and mods & (
            MOD_ALT | MOD_CMD
        ) == 0:
            return UInt8(FILES_UP if command == CMD_UP else FILES_DOWN)
        if command == CMD_DOC_START or command == CMD_DOC_END:
            return UInt8(
                FILES_FIRST if command == CMD_DOC_START else FILES_LAST
            )
        if command == CMD_TAB:
            return UInt8(FILES_TAB)
        if command == CMD_BACKTAB:
            return UInt8(FILES_SHIFT_TAB)
        if mods == 0:
            if command == CMD_NEWLINE or command == CMD_NEWLINE_RAW:
                return UInt8(FILES_ENTER)
            if end:
                if command == CMD_LEFT:
                    return UInt8(FILES_LEFT)
                if command == CMD_RIGHT:
                    return UInt8(FILES_RIGHT)
                if command == CMD_DELETE_BACK:
                    return UInt8(FILES_BACKSPACE)
        return None

    def file_effect(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        model: AppModel,
        effect: FilesEffect,
    ) raises:
        if effect.kind == FILES_SET_REST:
            self.component.model.controller.set_query(effect.text)
            self.component.sync_field()
            self.component.scroll_top = True
            self.files.rest(backend, model, effect.text)
            self.files_rest = effect.text.copy()
        else:
            self.files.effect(backend, model, effect)

    def file_click(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        model: AppModel,
        index: Int,
    ) raises:
        if index < 0 or index >= len(self.files.files.ranking.rows):
            return
        self.files.files.ranking.selected = index
        self.file_effect(
            ui,
            backend,
            model,
            self.files.files.pick(
                self.files.files.ranking.rows[index].entry.kind == 1,
                self.files.inserts(model),
            ),
        )

    def file_notices(mut self, mut ui: Ui) raises:
        if self.files.clipboard:
            self.clipboard.set_clipboard(self.files.clipboard.take())
        if self.files.notice:
            self.notify(ui, self.files.notice.take())
