from std.math import floor
from mjui.ui import Ui, Style, Hit
from mjui.input import MouseEvent, MOUSE_DOWN, MOUSE_UP
from mjui.layers import MODAL, Placement
from mjui.widgets.key import sub
from mjui.widgets.penna import PennaTheme
from .chooser_component import render_chooser, measure_chooser, ChooserBounds
from .chooser_presentation import ChooserPresentation
from .chooser_session import (
    ChooserSession,
    ChooserSource,
    ChooserPlan,
    ChooserObservation,
)
from .palette_dialog_state import PaletteDialogs

comptime CHOOSER_KEY: UInt64 = 0x7500000000000000


struct LiveChooser(Movable):
    var session: ChooserSession
    var key: UInt64
    var layer: UInt64
    var field: UInt64
    var painted_scope: UInt64
    var panel: ChooserBounds
    var viewport: ChooserBounds
    var rows: List[Tuple[UInt64, UInt32]]
    var close_press: Optional[UInt64]
    var pointer_handled: Bool
    var parked: Bool
    var host_scope: Optional[UInt64]
    var host_source: ChooserSource

    def __init__(out self, key: UInt64 = CHOOSER_KEY):
        self.session = ChooserSession()
        self.key = key
        self.layer = sub(key, 3)
        self.field = sub(key, 4)
        self.painted_scope = 0
        self.panel = ChooserBounds(0, 0, 0, 0)
        self.viewport = ChooserBounds(0, 0, 0, 0)
        self.rows = List[Tuple[UInt64, UInt32]]()
        self.close_press = None
        self.pointer_handled = False
        self.parked = False
        self.host_scope = None
        self.host_source = ChooserSource(0, 0, 0)

    def opened(self) -> Bool:
        return self.session.opened() and not self.parked

    def input_scope(self, mut ui: Ui) -> Bool:
        if not self.opened() or ui.focus != self.field:
            return False
        if not ui.retained_input_begin(self.layer, self.field):
            return False
        ui.retained_input_end()
        return True

    def prepare(
        mut self,
        mut ui: Ui,
        dialogs: PaletteDialogs,
        source: ChooserSource,
        enabled: Bool,
        parked: Bool,
        marked_revision: UInt64,
    ) raises -> ChooserObservation:
        var result = self.session.observe(
            dialogs, source, enabled, marked_revision
        )
        self.parked = parked
        self.pointer_handled = False
        if result.owner_changed or result.closed:
            self.close_press = None
            self.rows.clear()
            self.painted_scope = 0
            ui.close_layer_only(self.layer)
        if result.closed:
            if ui.focus == self.field:
                ui.focus = 0
            return result
        if self.opened() and not ui.layer_is_open(self.layer):
            ui.open_layer(
                self.layer,
                MODAL,
                Placement.at(0, 0, shift=False),
                scrim=0,
                light_dismiss=False,
                dismiss_escape=False,
            )
            ui.focus = self.field
            ui.text_input = True
        elif self.opened() and ui.retained_input_begin(self.layer, self.field):
            ui.retained_input_end()
            ui.focus = self.field
            ui.text_input = True
        return result

    def record_input_owner(mut self, mut ui: Ui):
        if self.input_scope(ui):
            self.host_scope = self.session.scope
            self.host_source = self.session.source
        else:
            if self.host_scope:
                _ = self.session.input.unmark()
            self.host_scope = None

    def reset(mut self, mut ui: Ui):
        self.session.reset()
        self.close_press = None
        self.rows.clear()
        self.painted_scope = 0
        ui.close_layer_only(self.layer)

    def row_hit(self, hit: UInt64) -> Optional[UInt32]:
        for row in self.rows:
            if row[0] == hit:
                return row[1]
        return None

    def pointer(
        mut self, mut ui: Ui, event: MouseEvent
    ) raises -> Optional[ChooserPlan]:
        self.pointer_handled = True
        if not self.opened() or self.painted_scope != self.session.scope:
            self.close_press = None
            return None
        var hit = ui.hit_test(event.x, event.y)
        if event.kind == MOUSE_DOWN:
            self.close_press = None
            if hit == sub(self.key, 1):
                return self.session.close()
            if hit == sub(self.key, 2):
                self.close_press = self.session.scope
                return None
            var row = self.row_hit(hit)
            if row:
                ui.focus = self.field
                return self.session.select(row.value(), event.clicks >= 2)
        elif event.kind == MOUSE_UP:
            var pressed = self.close_press
            self.close_press = None
            if pressed == Optional[UInt64](self.session.scope) and hit == sub(
                self.key, 2
            ):
                return self.session.close()
        return None

    def wheel(
        mut self,
        ui: Ui,
        x: Float32,
        y: Float32,
        dx: Float32,
        dy: Float32,
        precise: Bool,
    ) raises -> Bool:
        if not self.opened() or self.painted_scope != self.session.scope:
            return False
        if (
            x < self.viewport.x
            or y < self.viewport.y
            or x >= self.viewport.x + self.viewport.width
            or y >= self.viewport.y + self.viewport.height
        ):
            return False
        var hit = ui.hit_test(x, y)
        if hit != self.key and not self.row_hit(hit):
            return False
        self.session.wheel(dx, dy, precise)
        return True

    def paint(
        mut self, mut ui: Ui, theme: PennaTheme, medium: Int32 = -1
    ) raises -> List[ChooserPlan]:
        var effects = List[ChooserPlan]()
        if not self.opened():
            self.record_input_owner(ui)
            return effects^
        var model = ChooserPresentation(self.session.snapshot.value().copy())
        var geometry = measure_chooser(ui, theme, model)
        var row_height = geometry.row_height
        self.session.geometry(
            geometry.viewport.height, row_height, geometry.line_height
        )
        var marked = (
            self.session.input.marked.value().copy() if self.session.input.marked else String()
        )
        _ = model.search(marked)
        var count = len(self.session.snapshot.value().items[].values)
        var first = Int(floor(self.session.scroll / row_height))
        for index in range(first, min(count, first + 12)):
            _ = model.row(index)
        if not ui.layer_begin(
            self.layer, Style.col().width(ui.width).height(ui.height)
        ):
            return effects^
        var result = render_chooser(
            ui,
            theme,
            self.key,
            model,
            self.session.scroll,
            medium,
            marked,
            False,
        )
        self.panel = result.panel
        self.viewport = result.viewport
        self.rows.clear()
        for index in range(first, min(count, first + 12)):
            self.rows.append((sub(self.key, index + 64), UInt32(index)))
        self.painted_scope = self.session.scope
        ui.open(
            Style.col()
            .width(0)
            .height(0)
            .at(self.panel.x, self.panel.y)
            .id(self.field)
        )
        _ = ui.interact(self.field, focusable=True, focus_on_press=False)
        ui.text_field(self.field)
        ui.close()
        ui.layer_end()
        ui.focus = self.field
        ui.text_input = True
        ui.caret_rect = Hit(
            self.field, self.panel.x, self.panel.y, self.panel.x, self.panel.y
        )
        self.record_input_owner(ui)
        if not self.pointer_handled:
            if result.selected:
                effects.append(
                    self.session.select(
                        result.selected.value(), result.activate
                    )
                )
            if result.close:
                effects.append(self.session.close())
        return effects^
