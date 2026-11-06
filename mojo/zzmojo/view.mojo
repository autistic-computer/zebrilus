from mjui.ui import Ui, Style
from mjui.input import TextInput, TEXT_INSERT, MouseEvent
from mjui.scroll import ScrollEvent
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
    TEXT,
    CREATE_SESSION,
    SPLIT_RIGHT,
    SPLIT_DOWN,
    RECONNECT,
)
from .state import AppModel
from .terminal_painter import render_viewport
from .workspace_layout import window_rects, PaneRect
from .geometry import GridSize, grid_dimension
from mjui.term.grid import cell_metrics
from .input_protocol import KeyInput, input_key, mjui_key_input
from .terminal_projection import grid_placement
from .pointer_controller import PointerController, PointerSurface, RoutedPointer
from .terminal_view import PointerGrid

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
    var keys: Dict[String, UInt64]
    var next_key: UInt64
    var reported: Dict[UInt64, GridSize]
    var reported_window: Optional[GridSize]
    var reported_session: Optional[UInt64]
    var reported_client: UInt64
    var reported_server: UInt64

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
        self.keys = Dict[String, UInt64]()
        self.next_key = 100
        self.reported = Dict[UInt64, GridSize]()
        self.reported_window = None
        self.reported_session = None
        self.reported_client = 0
        self.reported_server = 0
        self.backend.submit(self.model, self.focus_epoch)

    def widget_key(mut self, kind: Int, id: UInt64) raises -> UInt64:
        var name = String(kind, ":", id)
        if name not in self.keys:
            if self.next_key == TERMINAL_KEY:
                raise Error("widget identifiers exhausted")
            self.keys[name] = self.next_key
            self.next_key += 1
        return self.keys[name]

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
            if self.model.pane:
                self.forward_key(
                    self.model.pane.value(),
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
    ) raises:
        self.backend.input(
            pane,
            input_key(pane, key),
            epoch.value() if epoch else self.focus_epoch,
            release,
        )
        self.next_poll = 0

    def pointer_surfaces(self, ui: Ui) raises -> List[PointerSurface]:
        var surfaces = List[PointerSurface](capacity=len(self.visible_panes))
        for pane in self.visible_panes:
            var name = String(4, ":", pane)
            if name not in self.keys:
                continue
            var key = self.keys[name]
            var index = ui.term.find(key)
            if index < 0 or ui.term.views[index].gen < 0:
                continue
            ref grid = ui.term.views[index]
            var cell = ui.grid_cell_rect(key, 0, 0)
            surfaces.append(
                PointerSurface(
                    pane,
                    PointerGrid(
                        cell.x0,
                        cell.y0,
                        Float32(grid.cols) * grid.m.width(),
                        Float32(grid.rows) * grid.m.height(),
                        grid.m.width(),
                        grid.m.height(),
                        0,
                        ui.scale,
                        grid.cols,
                        grid.rows,
                    ),
                )
            )
        return surfaces^

    def blur_pointer(mut self) raises:
        if self.pointer_focus_pane:
            var clear = self.pointer.blur(
                self.model,
                self.pointer_focus_pane.value(),
                epoch=self.focus_epoch,
            )
            if clear:
                ref event = clear.value()
                self.backend.input(event.pane, event.frame, event.epoch, True)
        self.pointer_focus_pane = None

    def forward_pointer(mut self, mut ui: Ui, event: RoutedPointer) raises:
        if event.activate:
            if self.model.pane != Optional[UInt64](event.pane):
                self.blur_pointer()
                self.model.select_pane(event.pane)
                self.focus_epoch += 1
            ui.focus = TERMINAL_KEY
            self.pointer_focus_pane = event.pane
        self.backend.input(event.pane, event.frame, event.epoch, event.passive)
        self.next_poll = 0
        ui.request_wake(ui.time)

    def mouse(mut self, mut ui: Ui, event: MouseEvent) raises:
        var surfaces = self.pointer_surfaces(ui)
        var routed = self.pointer.route(
            self.model, Span(surfaces), event, self.focus_epoch
        )
        for input in routed:
            self.forward_pointer(ui, input)

    def scroll(
        mut self, mut ui: Ui, event: ScrollEvent, x: Float32, y: Float32
    ) raises:
        var surfaces = self.pointer_surfaces(ui)
        var routed = self.pointer.scroll(
            self.model, Span(surfaces), event, x, y, ui.mods, self.focus_epoch
        )
        if routed:
            self.forward_pointer(ui, routed.value())

    def modifiers(mut self, mut ui: Ui) raises:
        if not ui.pointer_in:
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
        if time >= self.next_poll or len(self.backend.queue) > 0:
            self.backend.submit(self.model, self.focus_epoch)
            self.next_poll = Float64.MAX if self.event_driven else time + 0.25

    def wait(mut self):
        _ = self.backend.collect(self.model, self.focus_epoch, wait=True)

    def flush(mut self):
        while self.backend.busy or len(self.backend.queue) > 0:
            if self.backend.busy:
                self.wait()
            if len(self.backend.queue) > 0:
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
            var key = self.widget_key(4, item.pane)
            var interaction = ui.interact(key, focusable=True)
            if interaction.pressed:
                if self.model.pane != Optional[UInt64](item.pane):
                    self.blur_pointer()
                    self.model.select_pane(item.pane)
                    self.focus_epoch += 1
                self.next_poll = 0
                ui.focus = TERMINAL_KEY
            var selected = (
                self.model.pane and self.model.pane.value() == item.pane
            )
            var background = flat.bg
            if item.pane in self.model.terminals:
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
                item.pane in self.model.terminals
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
            if item.pane in self.model.terminals:
                ref viewport = self.model.terminals[item.pane]
                _ = render_viewport(
                    ui,
                    key,
                    viewport,
                    flat.mono,
                    13,
                    selected and ui.focus == TERMINAL_KEY and self.active,
                    Int(columns),
                    Int(rows),
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
            else:
                label(ui, self.theme, "Connecting to terminal…", flat.dim)
            ui.close()
        ui.close()

    def build(mut self, mut ui: Ui, input: TextInput) raises:
        self.poll(ui.time)
        if (
            ui.focus != TERMINAL_KEY
            or self.pointer_focus_pane != self.model.pane
        ):
            self.blur_pointer()
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
            for event in input.events:
                if event.kind == TEXT_INSERT and event.end > event.start:
                    var text = String(
                        from_utf8=Span(input.bytes)[
                            Int(event.start) : Int(event.end)
                        ]
                    )
                    try:
                        self.backend.enqueue(
                            TEXT, self.model.pane, text, self.focus_epoch
                        )
                        self.next_poll = 0
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
        if len(self.backend.queue) > 0:
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
        if (
            ui.focus != TERMINAL_KEY
            or self.pointer_focus_pane != self.model.pane
        ):
            self.blur_pointer()
        self.pointer_focus_pane = (
            self.model.pane if ui.focus == TERMINAL_KEY
            and self.active else Optional[UInt64](None)
        )
