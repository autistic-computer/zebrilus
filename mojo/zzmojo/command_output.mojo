from mjui.input import TextInput, TextEvent, TEXT_INSERT, INSERT_COMPOSED
from mjui.ui import Ui
from mjui.term.grid import CellMetrics, TermOpts
from .client_events import CommandOutputState
from .geometry import GridSize
from .terminal_painter import render_viewport, render_viewport_opts
from .input_protocol import (
    resize_command_output,
    input_focus,
    input_copy_selection,
)
from .terminal_commit import pasted_commit, terminal_commit


@fieldwise_init
struct OutputTransition(Copyable, Movable):
    var kind: UInt8
    var id: Optional[UInt64]
    var pane: Optional[UInt64]
    var previous_pane: Optional[UInt64]


struct CommandOutputPresentation(Movable):
    var id: Optional[UInt64]
    var pane: Optional[UInt64]
    var focused: Bool
    var reported: Optional[GridSize]
    var server: UInt64
    var client: UInt64

    def __init__(out self):
        self.id = None
        self.pane = None
        self.focused = False
        self.reported = None
        self.server = 0
        self.client = 0

    def observe(
        mut self,
        output: CommandOutputState,
        server: UInt64 = 0,
        client: UInt64 = 0,
    ) -> OutputTransition:
        var id = output.id
        var pane = output.pane
        if not id or not pane or not output.viewport:
            id = None
            pane = None
        var previous = self.pane
        if (
            self.id == id
            and self.pane == pane
            and self.server == server
            and self.client == client
        ):
            return OutputTransition(0, id, pane, previous)
        var kind = UInt8(1 if id else 2 if previous else 0)
        self.id = id
        self.pane = pane
        self.focused = False
        self.reported = None
        self.server = server
        self.client = client
        return OutputTransition(kind, id, pane, previous)

    def owns(self, pane: UInt64) -> Bool:
        return Bool(self.id) and self.pane == Optional[UInt64](pane)

    def resize(
        mut self, size: GridSize, attached: Bool, measurable: Bool
    ) raises -> Optional[List[UInt8]]:
        if not self.id or not self.pane or not attached or not measurable:
            return None
        var dimensions = size
        dimensions.generation = 0
        if self.reported and self.reported.value().same(dimensions):
            return None
        var frame = resize_command_output(
            dimensions.columns,
            dimensions.rows,
            dimensions.cell_width,
            dimensions.cell_height,
        )
        self.reported = dimensions
        return frame^

    def focus(mut self, focused: Bool) raises -> Optional[List[UInt8]]:
        if not self.id or not self.pane or self.focused == focused:
            return None
        var frame = input_focus(self.pane.value(), focused, True)
        self.focused = focused
        return frame^

    def commit(
        self, text: String, composed: Bool = False
    ) raises -> Optional[List[UInt8]]:
        if (
            not self.id
            or not self.pane
            or not self.focused
            or text.byte_length() == 0
        ):
            return None
        return terminal_commit(self.pane.value(), text, composed, True)

    def text_event(
        self, input: TextInput, event: TextEvent
    ) raises -> Optional[List[UInt8]]:
        if event.kind != TEXT_INSERT or event.end <= event.start:
            return None
        var text = String(
            from_utf8=Span(input.bytes)[Int(event.start) : Int(event.end)]
        )
        return self.commit(text, event.cmd & INSERT_COMPOSED != 0)

    def copy_selection(
        self, request_id: UInt64 = 1, target: UInt8 = 0
    ) raises -> Optional[List[UInt8]]:
        if not self.id or not self.pane:
            return None
        return input_copy_selection(self.pane.value(), request_id, target, True)

    def render(
        self,
        mut ui: Ui,
        key: UInt64,
        output: CommandOutputState,
        font: Int32,
        size: Float32,
        focused: Bool,
        columns: Int,
        rows: Int,
        marked: Optional[String] = None,
        options: Optional[TermOpts] = None,
    ) raises -> CellMetrics:
        if (
            not self.id
            or self.id != output.id
            or self.pane != output.pane
            or not output.viewport
        ):
            raise Error("command-output presentation has no current viewport")
        if options:
            return render_viewport_opts(ui, key, output.viewport.value(), options.value(), columns, rows, True, marked)
        return render_viewport(
            ui,
            key,
            output.viewport.value(),
            font,
            size,
            focused,
            columns,
            rows,
            True,
            marked,
        )
