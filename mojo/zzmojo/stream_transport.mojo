from .exec_protocol import PreparedInvocation, Invocation
from .palette_protocol import palette_command_request
from .client_attachment import (
    ClientDetached,
    decode_detached,
    decode_server_stopping,
)
from std.ffi import external_call
from std.memory import ArcPointer
from std.time import perf_counter_ns
from .transport import Connection, open_connection
from .protocol import (
    Response,
    MAX_FRAME_BYTES,
    Decoder,
    command_request,
    decode_response,
    validate_envelope,
)
from .stream_protocol import (
    WELCOME,
    BATCH,
    EVENT,
    SNAPSHOT,
    TREE_DELTA,
    CLIENT_VIEW,
    PING,
    HostPlatform,
    interactive_hello,
    decode_stream_frame,
    request_full,
    tree_sync,
    pong,
)
from .tree_model import MuxSnapshot, ClientView
from .tree_gate import TreeGate
from .tree_codec import eligible_delta, eligible_view
from .client_context import ClientContext
from .search_ui_commands import SearchUiCommands
from .terminal_search_protocol import TERMINAL_UI_COMMAND
from .appearance_protocol import TerminalAppearance, decode_appearance
from .terminal_model import TerminalViewport, TerminalHistory
from .terminal_wire import decode_terminal_frame
from .terminal_history import (
    HistorySnapshot,
    PreparedHistoryPatch,
    HISTORY_BYTE_BUDGET,
)
from .terminal_history_cache import TerminalHistoryCache
from .history_pacer import HISTORY_BACKFILL_QUIET_NS, bounded_history_range
from .terminal_history_protocol import history_request, decode_history_budget
from .input_protocol import InputRoutingState, decode_routing_event
from .desktop_key_tables import RetainedKeyTables, get_key_tables
from .prompt_protocol import decode_prompt_event
from .chooser_protocol import decode_choose_event
from .chooser_buffer_protocol import decode_choose_buffer
from .chooser_plane_protocol import decode_chooser_plane
from .palette_dialog_state import PaletteDialogs
from .path_protocol import decode_path_message, PATH_PICKER_EVENT
from .path_service import PathSource
from .path_mailbox import PathMailbox
from .native_cells import MAX_BEND_CELLS
from .client_events import (
    ClientEventState,
    ClientNotice,
    ClientMessage,
    decode_client_event,
)
from .clipboard_protocol import (
    ClipboardState,
    decode_clipboard_event,
    CLIPBOARD_WRITE,
    CLIPBOARD_QUERY,
)

comptime TERMINAL_CACHE_BYTES = 128 * 1024 * 1024
comptime UNHANDLED_BYTES = 4 * 1024 * 1024


@fieldwise_init
struct _PollFd(TrivialRegisterPassable):
    var fd: Int32
    var events: Int16
    var returned: Int16


def readable(fd: Int32) raises -> Bool:
    var descriptor = _PollFd(fd, 1, 0)
    var count = external_call["poll", Int32](
        Pointer(to=descriptor), UInt64(1), Int32(0)
    )
    if count < 0:
        raise Error("interactive socket polling failed")
    if count == 0:
        return False
    if descriptor.returned & 1 != 0:
        return True
    raise Error("interactive daemon disconnected")


def viewport_bytes(viewport: TerminalViewport) -> Int:
    return (
        len(viewport.cells) * 8
        + len(viewport.styles) * 16
        + len(viewport.grapheme_offsets) * 4
        + len(viewport.grapheme_bytes)
        + len(viewport.metadata.overlays) * 8
        + len(viewport.metadata.kitty) * 72
    )


@fieldwise_init
struct InitialResponse(Copyable, Movable):
    var response: Response
    var rejected: Bool


struct StreamModel(Copyable, Movable):
    var tree: MuxSnapshot
    var view: ClientView
    var snapshot: MuxSnapshot
    var terminals: Dict[UInt64, TerminalViewport]
    var histories: Dict[UInt64, TerminalHistory]
    var retained_histories: Dict[UInt64, ArcPointer[HistorySnapshot]]
    var history_invalidations: Dict[UInt64, UInt64]
    var history_updates: UInt64
    var events: ClientEventState
    var notice: Optional[ClientNotice]
    var notice_revision: UInt64
    var message: Optional[ClientMessage]
    var bend_cell_updates: UInt64
    var bend_tree_updates: UInt64
    var unhandled: List[List[UInt8]]
    var unhandled_bytes: Int
    var revision: UInt64
    var routing: InputRoutingState
    var key_table_changes: UInt64
    var key_tables: RetainedKeyTables
    var appearance: Optional[TerminalAppearance]
    var appearance_provenance: Dict[UInt8, UInt8]
    var appearance_revision: UInt64
    var clipboard: ClipboardState
    var search_commands: SearchUiCommands
    var dialogs: PaletteDialogs
    var paths: PathMailbox
    var attachment_blocked: Bool
    var detached: Optional[ClientDetached]
    var server_stopped: Bool

    def __init__(out self):
        self.tree = MuxSnapshot()
        self.view = ClientView()
        self.snapshot = MuxSnapshot()
        self.terminals = Dict[UInt64, TerminalViewport]()
        self.histories = Dict[UInt64, TerminalHistory]()
        self.retained_histories = Dict[UInt64, ArcPointer[HistorySnapshot]]()
        self.history_invalidations = Dict[UInt64, UInt64]()
        self.history_updates = 0
        self.events = ClientEventState()
        self.clipboard = ClipboardState()
        self.search_commands = SearchUiCommands()
        self.dialogs = PaletteDialogs()
        self.paths = PathMailbox()
        self.attachment_blocked = False
        self.detached = None
        self.server_stopped = False
        self.notice = None
        self.notice_revision = 0
        self.message = None
        self.bend_cell_updates = 0
        self.bend_tree_updates = 0
        self.unhandled = List[List[UInt8]]()
        self.unhandled_bytes = 0
        self.revision = 0
        self.routing = InputRoutingState()
        self.key_table_changes = 0
        self.key_tables = RetainedKeyTables()
        self.appearance = None
        self.appearance_provenance = Dict[UInt8, UInt8]()
        for key in range(30):
            self.appearance_provenance[UInt8(key)] = 0
        self.appearance_revision = 0

    def take_events(mut self) -> List[List[UInt8]]:
        var events = self.unhandled^
        self.unhandled = List[List[UInt8]]()
        self.unhandled_bytes = 0
        return events^


struct InteractiveConnection(Movable):
    var transport: Connection
    var model: StreamModel
    var full_pending: Dict[UInt64, Bool]
    var tree_pending: Bool
    var tree_dirty: Bool
    var tree_received: Bool
    var initial_pending: Dict[UInt64, Bool]
    var initial_responses: List[InitialResponse]
    var initial_bytes: Int
    var response: Optional[Response]
    var waiting: Optional[UInt64]
    var server_id: UInt64
    var client_id: UInt64
    var instance: UInt64
    var capabilities: UInt64
    var clock: UInt64
    var platform: HostPlatform
    var tree_gate: Optional[TreeGate]
    var history_cache: TerminalHistoryCache
    var history_trickle: UInt64
    var history_deadlines: Dict[UInt64, UInt64]

    def __init__(out self, var transport: Connection, path_epoch: UInt64 = 0):
        self.transport = transport^
        self.model = StreamModel()
        self.model.paths = PathMailbox(PathSource(0, 0, path_epoch))
        self.full_pending = Dict[UInt64, Bool]()
        self.tree_pending = False
        self.tree_dirty = False
        self.tree_received = False
        self.initial_pending = Dict[UInt64, Bool]()
        self.initial_responses = List[InitialResponse]()
        self.initial_bytes = 0
        self.response = None
        self.waiting = None
        self.server_id = 0
        self.client_id = 0
        self.instance = 0
        self.capabilities = 0
        self.clock = 0
        self.platform = HostPlatform(0, 0)
        self.tree_gate = None
        self.history_cache = TerminalHistoryCache()
        self.history_trickle = 2000
        self.history_deadlines = Dict[UInt64, UInt64]()

    def request_history(
        mut self, pane: UInt64, prefetch: Optional[UInt32] = None
    ) raises:
        if pane not in self.model.terminals:
            return
        if pane in self.history_cache.states:
            self.history_cache.touch(pane)
        var context = self.history_cache.request_context(
            pane, self.model.terminals[pane]
        )
        var requested = self.history_cache.pacer.request(
            pane,
            prefetch,
            self.history_trickle,
            context,
            UInt64(perf_counter_ns()),
        )
        if requested:
            var bounded = bounded_history_range(
                requested.value(), self.model.terminals[pane].columns
            )
            var frame = history_request(pane, bounded.start, bounded.count)
            self.transport.write(Span(frame))

    def _history_follow(
        mut self, pane: UInt64, kind: UInt8, value: UInt64
    ) raises:
        if kind == 0:
            self.request_history(pane, UInt32(value))
        elif kind == 1:
            if self.history_cache.pacer.defer(pane, value):
                self.history_deadlines[pane] = (
                    UInt64(perf_counter_ns()) + HISTORY_BACKFILL_QUIET_NS
                )
        else:
            self.request_history(pane)

    def history_wait_ns(self) -> Optional[UInt64]:
        var result: Optional[UInt64] = None
        var now = UInt64(perf_counter_ns())
        for entry in self.history_deadlines.items():
            var wait = entry.value - now if entry.value > now else UInt64(0)
            if not result or wait < result.value():
                result = wait
        return result

    def _history_due(mut self) raises:
        var now = UInt64(perf_counter_ns())
        var due = List[UInt64]()
        for entry in self.history_deadlines.items():
            if entry.value <= now:
                due.append(entry.key)
        for pane in due:
            _ = self.history_deadlines.pop(pane)
            var resumed = self.history_cache.pacer.resume_deferred(
                pane, self.history_cache.mutations(pane)
            )
            if resumed == 0:
                self.request_history(pane)
            elif resumed == 1:
                self.history_deadlines[pane] = now + HISTORY_BACKFILL_QUIET_NS

    def _publish_history(mut self) raises:
        var live_bytes = 0
        for entry in self.model.terminals.items():
            live_bytes += viewport_bytes(entry.value)
        if self.model.events.output.viewport:
            live_bytes += viewport_bytes(
                self.model.events.output.viewport.value()
            )
        self.history_cache.byte_budget = min(
            HISTORY_BYTE_BUDGET, max(0, TERMINAL_CACHE_BYTES - live_bytes)
        )
        self.history_cache.enforce()
        self.model.retained_histories = self.history_cache.publish(
            self.model.terminals
        )
        self.model.history_invalidations = (
            self.history_cache.invalidations.copy()
        )

    def request_tree(mut self) raises:
        if not self.tree_pending:
            var frame = tree_sync()
            self.transport.write(Span(frame))
            self.tree_pending = True

    def request_key_tables(mut self) raises:
        if self.model.key_tables.needs_full():
            var frame = get_key_tables()
            self.transport.write(Span(frame))
            self.model.key_tables.requested()

    def request_viewport(mut self, pane: UInt64) raises:
        if pane not in self.full_pending:
            var frame = request_full(pane)
            self.transport.write(Span(frame))
            self.full_pending[pane] = True

    def materialize(mut self) raises:
        if not self.tree_dirty:
            return
        var snapshot = MuxSnapshot()
        if eligible_view(self.model.tree, self.model.view):
            if not self.tree_gate:
                self.tree_gate = TreeGate()
            var result = self.tree_gate.value().view(
                self.transport.gate, self.model.tree, self.model.view
            )
            if result.status != 0:
                self.request_tree()
                return
            swap(snapshot, result.snapshot)
            self.model.bend_tree_updates += 1
        else:
            snapshot = self.model.tree.copy()
            try:
                self.model.view.apply(snapshot)
            except:
                self.request_tree()
                return
        self.model.snapshot = snapshot^
        var live = Dict[UInt64, Bool]()
        for session in self.model.snapshot.sessions:
            for window in session.windows:
                for pane in window.panes:
                    live[pane] = True
        var retired = List[UInt64]()
        for pane in self.model.terminals:
            if pane not in live:
                retired.append(pane)
        for pane in retired:
            _ = self.model.terminals.pop(pane)
            self.history_cache.forget(pane)
            if pane in self.history_deadlines:
                _ = self.history_deadlines.pop(pane)
        retired.clear()
        for pane in self.full_pending:
            if pane not in live:
                retired.append(pane)
        for pane in retired:
            _ = self.full_pending.pop(pane)
        retired.clear()
        for pane in self.model.histories:
            if pane not in live:
                retired.append(pane)
        for pane in retired:
            _ = self.model.histories.pop(pane)
        if (
            self.model.events.output.pane
            and self.model.events.output.pane.value() not in live
        ):
            self.model.events.output.clear()
        self._publish_history()
        self.tree_dirty = False

    def _retain(mut self, var frame: List[UInt8]) raises:
        if len(frame) > UNHANDLED_BYTES - self.model.unhandled_bytes:
            raise Error(
                "unhandled interactive events exceed their memory limit"
            )
        self.model.unhandled_bytes += len(frame)
        self.model.unhandled.append(frame^)

    def _check_terminal_budget(self) raises:
        var size = 0
        for entry in self.model.terminals.items():
            size += viewport_bytes(entry.value)
        if self.model.events.output.viewport:
            size += viewport_bytes(self.model.events.output.viewport.value())
        for entry in self.model.histories.items():
            size += (
                len(entry.value.cells) * 8
                + len(entry.value.styles) * 16
                + len(entry.value.grapheme_offsets) * 4
                + len(entry.value.grapheme_bytes)
            )
        size += self.history_cache.owned_bytes()
        if size > TERMINAL_CACHE_BYTES:
            raise Error("retained terminal data exceeds its memory limit")

    @inline(.never)
    def clear_attachment_planes(mut self) raises:
        if self.model.routing.table:
            self.model.key_table_changes += 1
        self.model.routing.armed = False
        self.model.routing.table = None
        self.model.routing.repeat = False
        self.model.key_tables.reset_active()
        self.model.paths.close_picker()
        self.model.terminals.clear()
        self.model.histories.clear()
        self.model.retained_histories.clear()
        self.model.history_invalidations.clear()
        var panes = List[UInt64]()
        for pane in self.history_cache.last_used:
            panes.append(pane)
        for pane in panes:
            self.history_cache.invalidate(pane)
        self.history_deadlines.clear()
        self.model.events.output.clear()
        self.full_pending.clear()

    def consume(mut self, var frame: List[UInt8], nested: Bool = False) raises:
        if self.transport.gate.validate(Span(frame)) != 0:
            raise Error("Bend rejected an interactive frame")
        if validate_envelope(Span(frame)) == 1:
            var update = decode_terminal_frame(frame^)
            if update.kind == 0:
                if not update.viewport:
                    raise Error("full terminal frame contains no viewport")
                var viewport = update.viewport.take()
                if update.pane in self.history_cache.states:
                    self.history_cache.states[update.pane].replace(
                        viewport, self.history_cache.next_row_revision
                    )
                self.model.terminals[update.pane] = viewport^
                _ = self.history_cache.ensure(
                    update.pane, self.model.terminals[update.pane]
                )
                self.request_history(update.pane)
                if update.pane in self.full_pending:
                    _ = self.full_pending.pop(update.pane)
            elif update.kind == 1:
                if update.pane not in self.model.terminals or not update.patch:
                    self.request_viewport(update.pane)
                else:
                    ref retained = self.model.terminals[update.pane]
                    ref patch = update.patch.value()
                    var preparation: Optional[PreparedHistoryPatch] = None
                    if update.pane in self.history_cache.states:
                        preparation = self.history_cache.states[
                            update.pane
                        ].prepare_patch(
                            retained,
                            patch,
                            self.history_cache.next_row_revision,
                        )
                    var valid = True
                    try:
                        _ = retained.validate_patch(patch)
                    except:
                        valid = False
                    if valid:
                        if (len(patch.spans) > 0 or patch.scroll != 0) and len(
                            retained.cells
                        ) <= MAX_BEND_CELLS:
                            var cells = self.transport.gate.cells(
                                retained, patch
                            )
                            retained.apply_verified_cells(patch, cells^)
                            self.model.bend_cell_updates += 1
                        else:
                            if (
                                self.transport.gate.terminal(retained, patch)
                                != 0
                            ):
                                raise Error(
                                    "Bend rejected a validated terminal patch"
                                )
                            retained.apply(patch)
                        if preparation:
                            self.history_cache.states[update.pane].commit_patch(
                                retained,
                                patch,
                                preparation.take(),
                                self.history_cache.next_row_revision,
                            )
                        else:
                            _ = self.history_cache.ensure(update.pane, retained)
                        self.request_history(update.pane)
                    else:
                        _ = self.model.terminals.pop(update.pane)
                        self.history_cache.invalidate(update.pane)
                        if update.pane in self.history_deadlines:
                            _ = self.history_deadlines.pop(update.pane)
                        self.request_viewport(update.pane)
            elif update.kind == 2:
                self.model.events.output.apply_terminal(update)
            elif update.kind == 3:
                if not update.history:
                    raise Error("history frame contains no history")
                var arrival = self.history_cache.pacer.chunk_arrived(
                    update.pane, self.history_cache.mutations(update.pane)
                )
                if (
                    arrival.apply
                    and update.pane in self.model.terminals
                    and update.pane in self.history_cache.states
                ):
                    if self.history_cache.states[update.pane].apply_history(
                        self.model.terminals[update.pane],
                        update.history.take(),
                        self.history_cache.next_row_revision,
                    ):
                        self.model.history_updates += 1
                self._history_follow(
                    update.pane, arrival.follow_up.kind, arrival.follow_up.value
                )
            self._publish_history()
            self._check_terminal_budget()
            self.model.revision += 1
            return
        var reader = Decoder(frame.copy())
        var tag = reader.uint()
        if tag == 3:
            var rejected = reader.uint() == 1
            var request = reader.uint()
            var initial = request in self.initial_pending
            var response = decode_response(frame^, cli_errors=initial)
            if initial:
                var size = len(response.output) + response.error.byte_length()
                if size > MAX_FRAME_BYTES - self.initial_bytes:
                    raise Error("initial command responses exceed byte limit")
                self.initial_bytes += size
                _ = self.initial_pending.pop(response.request)
                self.initial_responses.append(
                    InitialResponse(response^, rejected)
                )
                return
            if (
                not self.waiting
                or response.request != self.waiting.value()
                or self.response
            ):
                raise Error("unexpected interactive command response")
            self.response = response^
            return
        if tag >= 44 and tag <= 46:
            var update = decode_path_message(frame^)
            if not update:
                raise Error("path listing reply has no decoded payload")
            if self.model.paths.route(update.take()):
                self.model.revision += 1
            return
        var event = decode_stream_frame(frame^)
        if event.kind == BATCH:
            if nested:
                raise Error("nested protocol batches are forbidden")
            var chooser_revision = self.model.dialogs.chooser_revision
            var chooser_closed = self.model.dialogs.chooser_closed_revision
            for item in event.batch:
                self.consume(item.copy(), True)
            self.model.dialogs.finish_group(chooser_revision, chooser_closed)
            self.materialize()
            return
        if event.kind == WELCOME:
            self.server_id = event.server_id
            self.client_id = event.client_id
            self.instance = event.instance
            self.capabilities = event.capabilities
            self.clock = event.clock
            self.platform = event.platform
            self.model.paths.connected(self.server_id, self.client_id)
            _ = self.model.key_tables.consume(event.raw.copy())
            self.request_key_tables()
        elif event.kind == PING:
            var reply = pong(event.ping_token)
            self.transport.write(Span(reply))
        elif event.snapshot:
            self.model.tree = event.snapshot.value().copy()
            self.tree_received = True
            if self.model.view.attachment_generation == 0:
                self.model.view.focused_window = self.model.tree.focused_window
            self.tree_pending = False
            self.tree_dirty = True
            self.full_pending.clear()
        elif event.delta:
            if self.tree_pending:
                self.request_tree()
            else:
                if eligible_delta(self.model.tree, event.delta.value()):
                    if not self.tree_gate:
                        self.tree_gate = TreeGate()
                    var result = self.tree_gate.value().delta(
                        self.transport.gate,
                        self.model.tree,
                        event.delta.value(),
                    )
                    if result.status == 0:
                        swap(self.model.tree, result.snapshot)
                        self.model.bend_tree_updates += 1
                        self.tree_dirty = True
                    else:
                        self.request_tree()
                else:
                    try:
                        self.model.tree.apply(event.delta.value())
                        self.tree_dirty = True
                    except:
                        self.request_tree()
        elif event.view:
            var view = event.view.value().copy()
            if self.model.view.session and not view.session:
                self.model.attachment_blocked = True
            elif view.session:
                self.model.attachment_blocked = False
            if (
                view.session != self.model.view.session
                or view.attachment_generation
                != self.model.view.attachment_generation
            ):
                self.model.dialogs.reset_session()
                self.clear_attachment_planes()
            self.model.view = view^
            self.tree_dirty = True
        elif event.event_kind == Optional[UInt64](22):
            var detached = decode_detached(event.raw.copy())
            if not detached:
                raise Error("detach event has no decoded payload")
            var matches = self.model.view.session == Optional[UInt64](
                detached.value().session
            )
            self.model.detached = detached.copy()
            if matches:
                self.model.attachment_blocked = True
                self.model.view.session = None
                self.model.view.focused_window = None
                self.clear_attachment_planes()
                self.model.dialogs.reset()
                self.model.routing.armed = False
                self.model.routing.table = None
                var message = (
                    "detach-client -E needs a terminal client: "
                    + detached.value()
                    .action.command if detached.value()
                    .action.kind
                    == 2 else "detached by "
                    + detached.value()
                    .by.value() if detached.value()
                    .by else String("session ended")
                )
                self.model.message = ClientMessage(
                    None, 1, message^, None, None
                )
                self.tree_dirty = True
        elif event.event_kind == Optional[UInt64](18):
            var stopped = decode_server_stopping(event.raw.copy())
            if not stopped:
                raise Error("server stopping event has no decoded payload")
            self.model.server_stopped = True
            self.model.attachment_blocked = True
            self.model.view.session = None
            self.model.view.focused_window = None
            self.clear_attachment_planes()
            self.model.dialogs.reset()
            self.model.routing.armed = False
            self.model.routing.table = None
            self.tree_dirty = True
        elif event.event_kind and (event.event_kind.value() == 25 or event.event_kind.value() == 56 or event.event_kind.value() == 59 or event.event_kind.value() == 54):
            var raw = List[UInt8]()
            swap(raw, event.raw)
            _ = self.model.key_tables.consume(raw^)
            if event.event_kind.value() == 54 and self.model.key_tables.active_known and self.model.key_tables.snapshot:
                ref prefix = self.model.key_tables.snapshot.value()[].prefix
                self.model.routing.table = prefix.active_table.copy()
                self.model.routing.repeat = prefix.repeat
                self.model.key_table_changes += 1
            self.request_key_tables()
        elif event.event_kind == Optional[UInt64](9):
            var raw = List[UInt8]()
            swap(raw, event.raw)
            var prompt = decode_prompt_event(raw^)
            if not prompt:
                raise Error("command prompt event has no decoded payload")
            self.model.dialogs.apply_prompt(prompt.take())
        elif event.event_kind == Optional[UInt64](
            11
        ) or event.event_kind == Optional[UInt64](12):
            var raw = List[UInt8]()
            swap(raw, event.raw)
            var chooser = decode_choose_event(raw^)
            if not chooser:
                raise Error("choose-tree event has no decoded payload")
            self.model.dialogs.apply_chooser(chooser.take())
        elif event.event_kind == Optional[UInt64](
            13
        ) or event.event_kind == Optional[UInt64](14):
            var raw = List[UInt8]()
            swap(raw, event.raw)
            var buffer = decode_choose_buffer(raw^)
            if not buffer:
                raise Error("choose-buffer event has no decoded payload")
            self.model.dialogs.apply_buffer(buffer.take())
        elif event.event_kind == Optional[UInt64](51):
            var raw = List[UInt8]()
            swap(raw, event.raw)
            var plane = decode_chooser_plane(raw^)
            if not plane:
                raise Error("chooser presentation event has no decoded payload")
            self.model.dialogs.apply_plane(plane.take())
        elif event.event_kind == Optional[UInt64](UInt64(PATH_PICKER_EVENT)):
            var raw = List[UInt8]()
            swap(raw, event.raw)
            var picker = decode_path_message(raw^)
            if not picker:
                raise Error("path picker event has no decoded payload")
            _ = self.model.paths.route(picker.take())
        elif event.event_kind == Optional[UInt64](TERMINAL_UI_COMMAND):
            var raw = List[UInt8]()
            swap(raw, event.raw)
            if not self.model.search_commands.consume(
                raw^,
                self.server_id,
                self.client_id,
                self.model.events.output.pane,
                self.model.events.output.id,
            ):
                raise Error("terminal search UI command has no decoded payload")
        elif event.event_kind and (
            event.event_kind.value() == UInt64(CLIPBOARD_WRITE)
            or event.event_kind.value() == UInt64(CLIPBOARD_QUERY)
        ):
            var raw = List[UInt8]()
            swap(raw, event.raw)
            var clipboard = decode_clipboard_event(raw^)
            if not clipboard:
                raise Error("clipboard event has no decoded payload")
            self.model.clipboard.apply(
                clipboard.value(), self.server_id, self.client_id
            )
        else:
            var routing = decode_routing_event(event.raw.copy())
            if routing:
                self.model.routing.apply(routing.value())
            else:
                var client_event = decode_client_event(event.raw.copy())
                if client_event:
                    var notice = self.model.events.apply(client_event.value())
                    if notice:
                        if notice.value().message:
                            self.model.message = notice.value().message.copy()
                        elif (
                            notice.value().cleared
                            and self.model.message
                            and self.model.message.value().message_id
                            == notice.value().cleared
                        ):
                            self.model.message = None
                        self.model.notice = notice^
                        self.model.notice_revision += 1
                    self._check_terminal_budget()
                else:
                    var appearance = decode_appearance(event.raw.copy())
                    if appearance:
                        ref update = appearance.value()
                        self.model.appearance = update.appearance.copy()
                        self.model.appearance_provenance = (
                            update.provenance.copy()
                        )
                        self.model.appearance_revision += 1
                    else:
                        var budget = decode_history_budget(event.raw.copy())
                        if budget:
                            self.history_trickle = budget.value()
                        else:
                            var retained = event.raw^
                            event.raw = List[UInt8]()
                            self._retain(retained^)
        if not nested:
            self.materialize()
        self.model.revision += 1

    def drain(mut self, limit: Int = 256) raises:
        if self.model.server_stopped:
            return
        self._history_due()
        for _ in range(limit):
            if not readable(self.transport.fd):
                break
            self.consume(self.transport.read_frame())
            if self.model.server_stopped:
                break

    def command(
        mut self, name: String, args: Span[String, _]
    ) raises -> Response:
        if self.waiting:
            raise Error("interactive command is already in flight")
        var request = self.transport.next_request
        if request == UInt64.MAX:
            raise Error("request identifiers exhausted")
        self.transport.next_request += 1
        self.waiting = request
        var frame = command_request(request, name, args)
        self.transport.write(Span(frame))
        for _ in range(4096):
            self.consume(self.transport.read_frame())
            if self.response:
                var response = self.response.value().copy()
                self.response = None
                self.waiting = None
                return response^
        raise Error("interactive command received too many unrelated frames")

    def command_invocation(mut self, command: Invocation) raises -> Response:
        if self.waiting:
            raise Error("interactive command is already in flight")
        var request = self.transport.next_request
        if request == UInt64.MAX:
            raise Error("request identifiers exhausted")
        var frame = palette_command_request(request, command)
        self.transport.next_request += 1
        self.waiting = request
        self.transport.write(Span(frame))
        for _ in range(4096):
            self.consume(self.transport.read_frame())
            if self.response:
                var response = self.response.value().copy()
                self.response = None
                self.waiting = None
                return response^
        raise Error("interactive command received too many unrelated frames")


def connect_interactive(
    path: String,
    session: String,
    columns: UInt16 = 80,
    rows: UInt16 = 24,
    cell_width_px: UInt32 = 8,
    cell_height_px: UInt32 = 16,
    native_search: Bool = False,
    native_paths: Bool = False,
    path_epoch: UInt64 = 0,
    native_chooser: Bool = False,
    native_keys: Bool = False,
) raises -> InteractiveConnection:
    return connect_interactive_transport(
        open_connection(path),
        session,
        columns,
        rows,
        cell_width_px,
        cell_height_px,
        native_search,
        native_paths=native_paths,
        path_epoch=path_epoch,
        native_chooser=native_chooser,
        native_keys=native_keys,
    )


def connect_interactive_transport(
    var transport: Connection,
    session: String,
    columns: UInt16 = 80,
    rows: UInt16 = 24,
    cell_width_px: UInt32 = 8,
    cell_height_px: UInt32 = 16,
    native_search: Bool = False,
    prepared: Optional[List[PreparedInvocation]] = None,
    native_paths: Bool = False,
    path_epoch: UInt64 = 0,
    native_chooser: Bool = False,
    native_keys: Bool = False,
) raises -> InteractiveConnection:
    var stream = InteractiveConnection(transport^, path_epoch)
    stream.model.key_tables.subscribed = native_keys
    if prepared:
        if len(prepared.value()) > 4096:
            raise Error("application resume exceeds command count limit")
        for index in range(len(prepared.value())):
            stream.initial_pending[UInt64(index + 1)] = True
        stream.transport.next_request = UInt64(len(prepared.value()) + 1)
    var process = external_call["getpid", UInt32]()
    var instance = UInt64(process)
    var context = ClientContext.capture()
    var hello = interactive_hello(
        instance,
        process,
        session,
        columns,
        rows,
        cell_width_px,
        cell_height_px,
        options=(UInt64(1) << 21) - 1,
        viewport=False,
        context=context^,
        native_search=native_search,
        prepared=prepared,
        native_paths=native_paths,
        native_chooser=native_chooser,
        native_keys=native_keys,
    )
    stream.transport.write(Span(hello))
    stream.consume(stream.transport.read_frame())
    if stream.instance != instance:
        raise Error("interactive welcome identity mismatch")
    for _ in range(4096):
        if stream.tree_received:
            return stream^
        var initial = stream.transport.read_frame()
        stream.consume(initial^)
    raise Error("interactive initialization did not supply its tree")
