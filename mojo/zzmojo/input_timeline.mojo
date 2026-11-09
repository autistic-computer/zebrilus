from mjui.input import (
    Clipboard,
    KeyEvent,
    TextInput,
    TextEvent,
    TEXT_INSERT,
    TEXT_COMMAND,
    TEXT_PRIMARY,
    TEXT_POINTER,
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
from .editor_pointer import validate_pointer_input
from .workspace_keys import (
    RawWorkspaceKey,
    WorkspaceKeyRoute,
    raw_workspace_key,
    route_workspace_key,
)

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
    var raw: Optional[RawWorkspaceKey]


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
                None,
            ),
            0,
        )

    def search(
        mut self,
        plan: SearchPlan,
        event: KeyEvent,
        before_focus: UInt64,
        after_focus: UInt64,
    ) raises:
        self.append(
            _Pending(
                event.text_event,
                event.text_lo,
                None,
                None,
                plan.server,
                plan.client,
                plan.target.command_output,
                plan.copy(),
                None,
                None,
            ),
            plan.input.text.value().byte_length() if plan.input.text else 0,
        )
        if not self.initial_focus:
            self.initial_focus = before_focus
        self.final_focus = after_focus

    def palette(
        mut self,
        plan: PalettePlan,
        event: KeyEvent,
        before_focus: UInt64,
        after_focus: UInt64,
    ) raises:
        self.append(
            _Pending(
                event.text_event,
                event.text_lo,
                None,
                None,
                plan.server,
                plan.client,
                False,
                None,
                plan.copy(),
                None,
            ),
            0,
        )
        if not self.initial_focus:
            self.initial_focus = before_focus
        self.final_focus = after_focus

    def raw_key(
        mut self, view: WorkspaceView, key: KeyEvent, input: TextInput
    ) raises:
        var bytes = max(Int(key.text_hi - key.text_lo), 0)
        var scope = Optional[UInt64](
            view.chooser.session.scope
        ) if view.chooser.opened() else Optional[UInt64](None)
        self.append(
            _Pending(
                key.text_event,
                key.text_lo,
                None,
                None,
                view.model.source_server,
                view.model.source_client,
                False,
                None,
                None,
                raw_workspace_key(key, input, scope),
            ),
            bytes,
        )

    def capture_route(
        mut self, view: WorkspaceView, route: WorkspaceKeyRoute, key: KeyEvent
    ) raises:
        if route.palette:
            self.palette(route.palette.value(), key, route.before, route.after)
        elif route.search:
            self.search(route.search.value(), key, route.before, route.after)
        elif route.clipboard:
            self.clipboard(route.clipboard.value(), key)
        elif route.key:
            self.key(view, route.key.value(), key)

    def dispatch_raw[
        Host: Clipboard
    ](
        mut self,
        mut view: WorkspaceView,
        mut ui: Ui,
        mut host: Host,
        mut keyboard: KeyboardController,
        mut clipboard: ClipboardController,
        mut input: TextInput,
        raw: RawWorkspaceKey,
    ) raises -> Bool:
        var route = route_workspace_key(
            view, ui, keyboard, clipboard, raw, input
        )
        var unmark = route.unmark
        if unmark:
            input.unmark()
        if route.palette:
            view.dispatch_palette(ui, input, route.palette.value())
            unmark = unmark or view.palette_closed_frame
        elif route.search:
            view.dispatch_search(ui, input, route.search.value())
        elif route.clipboard:
            var plan = route.clipboard.value().copy()
            var result = clipboard.act(
                view, ui, host, plan.action, plan.composed, plan.copy()
            )
            unmark = unmark or result.unmark
        elif route.key:
            ref key = route.key.value()
            var output = view.output_input and view.output.owns(key.pane)
            view.forward_key(
                key.pane,
                key.input,
                key.epoch,
                key.release,
                output,
                (view.model.source_server, view.model.source_client),
            )
        return unmark

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
        try:
            validate_pointer_input(input)
        except error:
            self.pending.clear()
            self.pending_bytes = 0
            self.initial_focus = None
            self.final_focus = None
            view.primary.reset()
            view.pointer.reset()
            keyboard.clear_text_claims()
            input.events.clear()
            input.primaries.clear()
            input.pointers.clear()
            input.primary = None
            raise Error(String(error))
        var restore_focus: Optional[UInt64] = None
        if self.initial_focus:
            if self.final_focus and ui.focus != self.final_focus.value():
                restore_focus = ui.focus
            ui.focus = self.initial_focus.value()
        self.initial_focus = None
        self.final_focus = None
        view.prepare_palette_input(ui, input, host)
        view.prepare_search_input(ui, input, host)
        view.prepare_chooser_input(ui, input)
        view.prepare_windows_input(ui, input)
        self.merge(input, consumed_prefix)
        var deferred = False
        for captured in self.pending:
            deferred = deferred or Bool(captured.raw)
        if not deferred:
            keyboard.filter_text(input, stamped_commands=True)
        var pending = List[_Pending]()
        swap(pending, self.pending)
        self.pending_bytes = 0
        var remaining = List[TextEvent]()
        var unmark = (
            view.palette_closed_frame
            or view.chooser_unmark
            or view.windows_unmark
        )
        var composed = len(input.marked) > 0
        var ordered = List[TextEvent]()
        swap(ordered, input.events)
        for event in ordered:
            if event.kind == TEXT_ROUTED:
                var index = Int(event.start)
                if index < 0 or index >= len(pending):
                    raise Error("input timeline marker has no captured event")
                ref captured = pending[index]
                if (
                    captured.server != view.model.source_server
                    or captured.client != view.model.source_client
                ):
                    if captured.raw:
                        keyboard.claim_text(
                            captured.raw.value().key, Span(input.bytes)
                        )
                        keyboard.claim_text_command(captured.raw.value().key)
                    if captured.palette:
                        view.palette.pending_open = False
                        view.palette.pending_close = False
                        if ui.focus == view.palette.component.field_key:
                            ui.focus = TERMINAL_KEY
                    continue
                if captured.raw:
                    if self.dispatch_raw(
                        view,
                        ui,
                        host,
                        keyboard,
                        clipboard,
                        input,
                        captured.raw.value(),
                    ):
                        unmark = True
                        composed = False
                        input.unmark()
                elif captured.key:
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
            if event.kind == TEXT_PRIMARY or event.kind == TEXT_POINTER:
                if view.windows_event(ui, input, event, host):
                    continue
                if view.ordered_pointer(ui, input, event, host):
                    unmark = True
                    composed = False
                    input.unmark()
                continue
            var filtered = List[TextEvent]()
            if deferred:
                filtered = keyboard.filter_event(event, event.start)
            else:
                filtered.append(event)
            for piece in filtered:
                if view.windows_event(ui, input, piece, host):
                    continue
                if view.chooser_event(ui, input, piece):
                    continue
                if view.palette_event(ui, input, piece, host):
                    if view.palette_closed_frame:
                        unmark = True
                        composed = False
                    continue
                if view.search_event(ui, input, piece, host):
                    continue
                if (
                    ui.focus == TERMINAL_KEY
                    and view.active
                    and view.input_pane()
                ):
                    if piece.kind == TEXT_INSERT:
                        view.commit_text_event(input, piece)
                        continue
                    if piece.kind == TEXT_COMMAND:
                        var action: UInt8 = 0
                        if piece.cmd == CMD_COPY:
                            action = 1
                        elif piece.cmd == CMD_PASTE:
                            action = 2
                        elif piece.cmd == CMD_SELECT_ALL:
                            action = 3
                        if action != 0:
                            var result = clipboard.act(
                                view, ui, host, action, composed
                            )
                            if result.unmark:
                                unmark = True
                                composed = False
                            continue
                remaining.append(piece)
        input.events = remaining^
        view.compose_windows(ui, input)
        unmark = unmark or view.windows_unmark
        self.pending.clear()
        self.pending_bytes = 0
        keyboard.clear_text_claims()
        if restore_focus:
            ui.focus = restore_focus.value()
        view.finish_search(ui)
        if unmark:
            input.unmark()
        return unmark
