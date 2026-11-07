from mjui.input import (
    Clipboard,
    KeyEvent,
    TextInput,
    TextEvent,
    TEXT_INSERT,
    TEXT_COMMAND,
    CMD_COPY,
    CMD_PASTE,
    CMD_SELECT_ALL,
)
from mjui.ui import Ui
from .view import WorkspaceView, TERMINAL_KEY
from .keyboard_controller import KeyboardController, RoutedKey
from .clipboard_controller import ClipboardController, ClipboardPlan
from .actions import MAX_PENDING_ACTIONS, MAX_PENDING_INPUT_BYTES
from .terminal_search_controller import SearchPlan
from .palette_runtime import PalettePlan

comptime TEXT_ROUTED: UInt8 = 255


@fieldwise_init
struct _Pending(Copyable, Movable):
    var position: Int32
    var byte_position: Int32
    var key: Optional[RoutedKey]
    var clipboard: Optional[ClipboardPlan]
    var server: UInt64
    var client: UInt64
    var output: Bool
    var search: Optional[SearchPlan]
    var palette: Optional[PalettePlan]


struct InputTimeline(Movable):
    var pending: List[_Pending]
    var pending_bytes: Int
    var initial_focus: Optional[UInt64]
    var final_focus: Optional[UInt64]

    def __init__(out self):
        self.pending = List[_Pending]()
        self.pending_bytes = 0
        self.initial_focus = None
        self.final_focus = None

    def append(mut self, event: _Pending, bytes: Int) raises:
        if (
            len(self.pending) >= MAX_PENDING_ACTIONS
            or bytes > MAX_PENDING_INPUT_BYTES - self.pending_bytes
        ):
            raise Error("pending input timeline exceeds queue limit")
        self.pending.append(event.copy())
        self.pending_bytes += bytes

    def key(
        mut self, view: WorkspaceView, routed: RoutedKey, event: KeyEvent
    ) raises:
        var size = (
            routed.input.text.value().byte_length() if routed.input.text else 0
        )
        var output = view.model.command_output.pane == Optional[UInt64](
            routed.pane
        ) and Bool(view.model.command_output.viewport)
        self.append(
            _Pending(
                event.text_event,
                event.text_lo,
                routed.copy(),
                None,
                view.model.source_server,
                view.model.source_client,
                output,
                None,
                None,
            ),
            size,
        )

    def clipboard(mut self, plan: ClipboardPlan, event: KeyEvent) raises:
        self.append(
            _Pending(
                event.text_event,
                event.text_lo,
                None,
                plan.copy(),
                plan.server,
                plan.client,
                plan.output,
                None,
                None,
            ),
            0,
        )

    def search(mut self, plan: SearchPlan, event: KeyEvent, before_focus: UInt64, after_focus: UInt64) raises:
        self.append(_Pending(event.text_event, event.text_lo, None, None, plan.server, plan.client, plan.target.command_output, plan.copy(), None), plan.input.text.value().byte_length() if plan.input.text else 0)
        if not self.initial_focus:
            self.initial_focus = before_focus
        self.final_focus = after_focus

    def palette(mut self, plan: PalettePlan, event: KeyEvent, before_focus: UInt64, after_focus: UInt64) raises:
        self.append(_Pending(event.text_event, event.text_lo, None, None, plan.server, plan.client, False, None, plan.copy()), 0)
        if not self.initial_focus:
            self.initial_focus = before_focus
        self.final_focus = after_focus

    def position(
        self, index: Int, input: TextInput, consumed_prefix: Int
    ) -> Int:
        if self.pending[index].position >= 0:
            return min(
                max(Int(self.pending[index].position) - consumed_prefix, 0),
                len(input.events),
            )
        var byte = self.pending[index].byte_position
        for event_index in range(len(input.events)):
            if (
                input.events[event_index].kind == TEXT_INSERT
                and input.events[event_index].start >= byte
            ):
                return event_index
        return len(input.events)

    def merge(mut self, mut input: TextInput, consumed_prefix: Int = 0):
        var events = List[TextEvent]()
        var next = 0
        for index in range(len(input.events) + 1):
            while (
                next < len(self.pending)
                and self.position(next, input, consumed_prefix) <= index
            ):
                events.append(
                    TextEvent(TEXT_ROUTED, 0, 0, Int32(next), Int32(next))
                )
                next += 1
            if index < len(input.events):
                var event = input.events[index]
                if event.kind == TEXT_COMMAND:
                    event.start = Int32(index + consumed_prefix)
                    event.end = Int32(index + consumed_prefix)
                events.append(event)
        input.events = events^

    def process[
        Host: Clipboard
    ](
        mut self,
        mut view: WorkspaceView,
        mut ui: Ui,
        mut host: Host,
        mut keyboard: KeyboardController,
        mut clipboard: ClipboardController,
        mut input: TextInput,
        consumed_prefix: Int = 0,
    ) raises -> Bool:
        var restore_focus: Optional[UInt64] = None
        if self.initial_focus:
            if self.final_focus and ui.focus != self.final_focus.value():
                restore_focus = ui.focus
            ui.focus = self.initial_focus.value()
        self.initial_focus = None
        self.final_focus = None
        view.prepare_palette_input(ui, input, host)
        view.prepare_search_input(ui, input, host)
        self.merge(input, consumed_prefix)
        keyboard.filter_text(input, stamped_commands=True)
        var pending = List[_Pending]()
        swap(pending, self.pending)
        self.pending_bytes = 0
        var remaining = List[TextEvent]()
        var unmark = view.palette_closed_frame
        var composed = len(input.marked) > 0
        for event in input.events:
            if event.kind == TEXT_ROUTED:
                var index = Int(event.start)
                if index < 0 or index >= len(pending):
                    raise Error("input timeline marker has no captured event")
                ref captured = pending[index]
                if (
                    captured.server != view.model.source_server
                    or captured.client != view.model.source_client
                ):
                    if captured.palette:
                        view.palette.pending_open = False
                        view.palette.pending_close = False
                        if ui.focus == view.palette.component.field_key:
                            ui.focus = TERMINAL_KEY
                    continue
                if captured.key:
                    ref key = captured.key.value()
                    view.forward_key(
                        key.pane,
                        key.input,
                        key.epoch,
                        key.release,
                        captured.output,
                        (captured.server, captured.client),
                    )
                elif captured.clipboard:
                    var plan = captured.clipboard.value().copy()
                    var result = clipboard.act(
                        view, ui, host, plan.action, plan.composed, plan.copy()
                    )
                    if result.unmark:
                        unmark = True
                        composed = False
                elif captured.search:
                    view.dispatch_search(ui, input, captured.search.value())
                elif captured.palette:
                    view.dispatch_palette(ui, input, captured.palette.value())
                    if view.palette_closed_frame:
                        unmark = True
                        composed = False
                continue
            if view.palette_event(ui, input, event, host):
                if view.palette_closed_frame:
                    unmark = True
                    composed = False
                continue
            if view.search_event(ui, input, event, host):
                continue
            if ui.focus == TERMINAL_KEY and view.active and view.input_pane():
                if event.kind == TEXT_INSERT:
                    view.commit_text_event(input, event)
                    continue
                if event.kind == TEXT_COMMAND:
                    var action: UInt8 = 0
                    if event.cmd == CMD_COPY:
                        action = 1
                    elif event.cmd == CMD_PASTE:
                        action = 2
                    elif event.cmd == CMD_SELECT_ALL:
                        action = 3
                    if action != 0:
                        var result = clipboard.act(
                            view, ui, host, action, composed
                        )
                        if result.unmark:
                            unmark = True
                            composed = False
                        continue
            remaining.append(event)
        input.events = remaining^
        self.pending.clear()
        self.pending_bytes = 0
        if restore_focus:
            ui.focus = restore_focus.value()
        view.finish_search(ui)
        if unmark:
            input.unmark()
        return unmark
