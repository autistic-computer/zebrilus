from mjui.ui import Ui, TextStyle
from std.sys.info import CompilationTarget
from mjui.input import Clipboard, TextInput, TextEvent, TEXT_COMMAND, CMD_PASTE
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
    var notice: String
    var notice_until: Float64

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
        self.notice = String()
        self.notice_until = 0

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

    def open(mut self, mut ui: Ui, model: AppModel, online: Bool):
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

    def notify(mut self, mut ui: Ui, text: String):
        self.notice = text
        self.notice_until = ui.time + 2.6
        ui.request_wake(self.notice_until)

    def close(mut self, mut ui: Ui):
        self.pending_open = False
        self.pending_close = False
        self.restore_output = None
        self.component.close(ui)

    def prepare[
        Host: Clipboard
    ](
        mut self, mut ui: Ui, input: TextInput, mut host: Host
    ) raises -> PaletteComponentResult:
        self.clipboard = SearchClipboard()
        var result = PaletteComponentResult()
        if not self.component.opened():
            return result^
        self.clipboard.primary_supported = host.supports_primary()
        var allow_primary = True
        if input.primary and self.clipboard.primary_supported:
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
        )
        self.clipboard.deliver(host)
        return result^

    def event[
        Host: Clipboard
    ](
        mut self, mut ui: Ui, input: TextInput, event: TextEvent, mut host: Host
    ) raises -> PaletteComponentResult:
        var result = PaletteComponentResult()
        if (
            not self.component.opened()
            or ui.layers.modal != self.component.key + 3
        ):
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
        self.clipboard.deliver(host)
        return result^
