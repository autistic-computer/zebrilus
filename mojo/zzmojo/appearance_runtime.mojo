from std.math import isfinite
from mjui.ui import Ui
from mjui.text.fonts import FontCatalog
from mjui.text.font_group import PreparedFonts
from mjui.term.grid import TermOpts
from .appearance_protocol import TerminalAppearance
from .appearance_fonts import terminal_font_specs, terminal_font_key, terminal_grid_metrics
from .terminal_settings import localized_appearance, display_size
from .terminal_appearance import terminal_options


def _same_bytes(left: List[UInt8], right: List[UInt8]) -> Bool:
    if len(left) != len(right):
        return False
    for index in range(len(left)):
        if left[index] != right[index]:
            return False
    return True


struct AppearanceRuntime(Movable):
    var catalog: FontCatalog
    var available: List[String]
    var catalog_loaded: Bool
    var active: TerminalAppearance
    var prepared: Optional[PreparedFonts]
    var applied_key: List[UInt8]
    var pending_key: List[UInt8]
    var slots: Tuple[Int32, Int32, Int32, Int32]
    var fonts_ready: Bool
    var font_generation: UInt64
    var source_server: UInt64
    var source_client: UInt64
    var revision: UInt64
    var observed: Bool
    var dirty: Bool
    var preferred: Optional[String]
    var font_scale: Float32
    var fallback: String
    var error: String

    def __init__(out self, fallback_font: Int32, fallback: String = String("monospace")):
        self.catalog = FontCatalog()
        self.available = List[String]()
        self.catalog_loaded = False
        self.active = TerminalAppearance()
        self.prepared = None
        self.applied_key = List[UInt8]()
        self.pending_key = List[UInt8]()
        self.slots = (fallback_font, Int32(-1), Int32(-1), Int32(-1))
        self.fonts_ready = False
        self.font_generation = 0
        self.source_server = 0
        self.source_client = 0
        self.revision = 0
        self.observed = False
        self.dirty = True
        self.preferred = None
        self.font_scale = 1
        self.fallback = fallback
        self.error = String()

    def preferences(mut self, preferred: Optional[String], scale: Float32) raises:
        if not isfinite(scale) or scale <= 0:
            raise Error("invalid terminal font scale")
        self.preferred = preferred.copy()
        self.font_scale = scale
        self.dirty = True

    def update(
        mut self,
        mut ui: Ui,
        appearance: Optional[TerminalAppearance],
        provenance: Dict[UInt8, UInt8],
        revision: UInt64,
        server: UInt64,
        client: UInt64,
    ) raises -> Bool:
        if not self.catalog_loaded:
            if not self.catalog.ready():
                ui.request_wake(ui.time + Float64(0.01))
                return False
            try:
                for family in self.catalog.families():
                    self.available.append(family)
            except error:
                self.error = String(error)
            self.catalog_loaded = True
        if self.font_generation != ui.font_generation:
            self.applied_key.clear()
            self.font_generation = ui.font_generation
            self.dirty = True
            self.prepared = None
            self.pending_key.clear()
        var changed = False
        if self.dirty or not self.observed or self.revision != revision or self.source_server != server or self.source_client != client:
            var base = appearance.value().copy() if appearance else TerminalAppearance()
            var local = localized_appearance(base, provenance, self.available, self.fallback, self.preferred, self.font_scale)
            var specs = terminal_font_specs(local, self.fallback, display_size(local))
            var key = terminal_font_key(specs)
            self.active = local^
            self.revision = revision
            self.source_server = server
            self.source_client = client
            self.observed = True
            self.dirty = False
            changed = True
            if _same_bytes(key, self.applied_key):
                self.prepared = None
                self.pending_key.clear()
            elif not self.prepared or not _same_bytes(key, self.pending_key):
                self.prepared = None
                self.pending_key.clear()
                try:
                    self.prepared = ui.prepare_fonts(specs)
                    self.pending_key = key^
                    self.error = String()
                except error:
                    self.error = String(error)
        if self.prepared:
            var pending = self.prepared.take()
            if ui.apply_fonts(pending):
                self.slots = ui.font_group().slots()
                self.font_generation = ui.font_generation
                self.fonts_ready = True
                swap(self.applied_key, self.pending_key)
                self.pending_key.clear()
                changed = True
            else:
                self.prepared = pending^
                ui.request_wake(ui.time + Float64(0.01))
        return changed

    def options(self, mut ui: Ui, focused: Bool, delta: Float32 = 0, blink_epoch: UInt64 = 0) -> TermOpts:
        var size = display_size(self.active, delta)
        var metrics = terminal_grid_metrics(ui, self.slots[0], size, self.active)
        return terminal_options(self.active, self.slots, size, focused, metrics, blink_epoch)
