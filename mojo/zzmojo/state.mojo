from std.memory import ArcPointer
from .terminal_history import HistorySnapshot
from .stream_transport import InteractiveConnection as Connection
from .tree_model import MuxSnapshot, WindowSnapshot, _Order, _ordered
from .terminal_model import TerminalViewport, GRAPHEME
from .stream_protocol import input_text
from .input_protocol import InputRoutingState
from .desktop_key_tables import RetainedKeyTables
from .client_events import StatusLine, CommandOutputState, ClientMessage
from .appearance_protocol import TerminalAppearance
from .palette_dialog_state import PaletteDialogs
from .client_attachment import ClientDetached


def format_rows(
    output: Span[UInt8, _], columns: Int
) raises -> List[List[String]]:
    var rows = List[List[String]]()
    var fields = List[String]()
    var value = List[UInt8]()
    var escaped = False
    for byte in output:
        if escaped:
            value.append(byte)
            escaped = False
        elif byte == 92:
            escaped = True
        elif byte == 9 or byte == 10:
            fields.append(String(from_utf8=Span(value)))
            value.clear()
            if byte == 10:
                if len(fields) != columns:
                    raise Error("invalid daemon format row")
                rows.append(fields^)
                fields = List[String]()
        else:
            value.append(byte)
    if escaped:
        raise Error("truncated escaped daemon field")
    if len(value) > 0 or len(fields) > 0:
        fields.append(String(from_utf8=Span(value)))
        if len(fields) != columns:
            raise Error("invalid daemon format row")
        rows.append(fields^)
    return rows^


@fieldwise_init
struct Session(Copyable, Movable):
    var id: UInt64
    var name: String


@fieldwise_init
struct Window(Copyable, Movable):
    var id: UInt64
    var session: UInt64
    var name: String
    var active: Bool


@fieldwise_init
struct Pane(Copyable, Movable):
    var id: UInt64
    var window: UInt64
    var title: String
    var command: String
    var active: Bool


def _reconciled(
    sessions: List[Session],
    windows: List[Window],
    panes: List[Pane],
    var session: Optional[UInt64],
    var window: Optional[UInt64],
    var pane: Optional[UInt64],
) -> Tuple[Optional[UInt64], Optional[UInt64], Optional[UInt64]]:
    var has_session = False
    for candidate in sessions:
        if session and candidate.id == session.value():
            has_session = True
    if not has_session:
        session = sessions[0].id if len(sessions) > 0 else Optional[UInt64](
            None
        )
    var has_window = False
    for candidate in windows:
        if (
            session
            and candidate.session == session.value()
            and window
            and candidate.id == window.value()
        ):
            has_window = True
    if not has_window:
        window = None
        for candidate in windows:
            if session and candidate.session == session.value():
                if not window or candidate.active:
                    window = candidate.id
    var has_pane = False
    for candidate in panes:
        if (
            window
            and candidate.window == window.value()
            and pane
            and candidate.id == pane.value()
        ):
            has_pane = True
    if not has_pane:
        pane = None
        for candidate in panes:
            if window and candidate.window == window.value():
                if not pane or candidate.active:
                    pane = candidate.id
    return (session, window, pane)


@inline(.never)
def terminal_text(viewport: TerminalViewport) raises -> String:
    var output = List[UInt8](capacity=len(viewport.cells))
    var columns = Int(viewport.columns)
    var rows = Int(viewport.rows)
    if len(viewport.cells) != columns * rows:
        raise Error("terminal plane dimensions do not match its cells")
    for row in range(rows):
        var start = len(output)
        for column in range(columns):
            var cell = viewport.cells[row * columns + column]
            if cell.flags & 3 >= 2:
                continue
            var glyph = cell.glyph
            if glyph & GRAPHEME != 0:
                var index = Int(glyph & ~GRAPHEME)
                if index + 1 >= len(viewport.grapheme_offsets):
                    raise Error(
                        "terminal grapheme index exceeds its dictionary"
                    )
                var first = Int(viewport.grapheme_offsets[index])
                var end = Int(viewport.grapheme_offsets[index + 1])
                if first > end or end > len(viewport.grapheme_bytes):
                    raise Error(
                        "terminal grapheme offsets exceed their dictionary"
                    )
                output.extend(Span(viewport.grapheme_bytes)[first:end])
            elif glyph != 0:
                output.extend(chr(Int(glyph)).as_bytes())
            else:
                output.append(32)
        while len(output) > start and output[len(output) - 1] == 32:
            _ = output.pop()
        output.append(10)
    return String(from_utf8=Span(output))


def _pane_keys(window: WindowSnapshot) -> List[UInt64]:
    var output = List[UInt64]()
    var seen = Dict[UInt64, Bool]()
    for id in window.layout.panes():
        if id not in seen and id in window.panes:
            seen[id] = True
            output.append(id)
    var remaining = List[_Order]()
    for entry in window.panes.items():
        if entry.key not in seen:
            remaining.append(_Order(entry.key, 0, 0))
    var sorted = _ordered(remaining^)
    for entry in sorted:
        output.append(entry.major)
    return output^


def _selected_text(
    terminals: Dict[UInt64, TerminalViewport], pane: Optional[UInt64]
) raises -> String:
    if pane and pane.value() in terminals:
        return terminal_text(terminals[pane.value()])
    return String()


def _execute(
    mut connection: Connection, name: String, args: List[String]
) raises:
    var response = connection.command(name, Span(args))
    if response.exit_code != 0:
        raise Error(response.error)


struct AppModel(Copyable, Movable):
    var sessions: List[Session]
    var windows: List[Window]
    var panes: List[Pane]
    var session: Optional[UInt64]
    var window: Optional[UInt64]
    var pane: Optional[UInt64]
    var terminal: String
    var error: String
    var generation: UInt64
    var snapshot: MuxSnapshot
    var attached_session: Optional[UInt64]
    var terminals: Dict[UInt64, TerminalViewport]
    var histories: Dict[UInt64, ArcPointer[HistorySnapshot]]
    var history_invalidations: Dict[UInt64, UInt64]
    var focus_dirty: Bool
    var source_server: UInt64
    var source_client: UInt64
    var layout_generation: UInt64
    var routing: InputRoutingState
    var key_table_changes: UInt64
    var key_tables: RetainedKeyTables
    var status: StatusLine
    var command_output: CommandOutputState
    var message: Optional[ClientMessage]
    var text_pane: Optional[UInt64]
    var appearance: Optional[TerminalAppearance]
    var appearance_provenance: Dict[UInt8, UInt8]
    var appearance_revision: UInt64
    var dialogs: PaletteDialogs
    var attachment_blocked: Bool
    var detached: Optional[ClientDetached]
    var server_stopped: Bool

    def __init__(out self):
        self.sessions = List[Session]()
        self.windows = List[Window]()
        self.panes = List[Pane]()
        self.session = None
        self.window = None
        self.pane = None
        self.terminal = String()
        self.error = String()
        self.generation = 0
        self.snapshot = MuxSnapshot()
        self.attached_session = None
        self.terminals = Dict[UInt64, TerminalViewport]()
        self.histories = Dict[UInt64, ArcPointer[HistorySnapshot]]()
        self.history_invalidations = Dict[UInt64, UInt64]()
        self.focus_dirty = False
        self.source_server = 0
        self.source_client = 0
        self.layout_generation = 0
        self.routing = InputRoutingState()
        self.key_table_changes = 0
        self.key_tables = RetainedKeyTables()
        self.status = StatusLine()
        self.command_output = CommandOutputState()
        self.message = None
        self.text_pane = None
        self.appearance = None
        self.appearance_provenance = Dict[UInt8, UInt8]()
        for key in range(30):
            self.appearance_provenance[UInt8(key)] = 0
        self.appearance_revision = 0
        self.dialogs = PaletteDialogs()
        self.attachment_blocked = False
        self.detached = None
        self.server_stopped = False

    def reconcile(mut self):
        if self.attachment_blocked and not self.focus_dirty:
            self.session = None
            self.window = None
            self.pane = None
            self.terminal = String()
            self.text_pane = None
            return
        var session, window, pane = _reconciled(
            self.sessions,
            self.windows,
            self.panes,
            self.session,
            self.window,
            self.pane,
        )
        if pane != self.pane or not pane:
            self.terminal = String()
            self.text_pane = None
        self.session = session
        self.window = window
        self.pane = pane

    def select_session(mut self, id: UInt64) raises:
        for session in self.sessions:
            if session.id == id:
                if self.session != Optional[UInt64](id):
                    self.window = None
                    self.pane = None
                self.session = id
                self.focus_dirty = True
                self.reconcile()
                return
        raise Error("session no longer exists")

    def select_window(mut self, id: UInt64) raises:
        for window in self.windows:
            if (
                window.id == id
                and self.session
                and window.session == self.session.value()
            ):
                if self.window != Optional[UInt64](id):
                    self.pane = None
                self.window = id
                self.focus_dirty = True
                self.reconcile()
                return
        raise Error("window no longer exists")

    def select_pane(mut self, id: UInt64) raises:
        for pane in self.panes:
            if (
                pane.id == id
                and self.window
                and pane.window == self.window.value()
            ):
                self.pane = id
                self.focus_dirty = True
                return
        raise Error("pane no longer exists")

    @inline(.never)
    def refresh(mut self, mut connection: Connection) raises:
        connection.drain()
        if (
            self.generation != 0
            and self.generation == connection.model.revision
            and self.source_server == connection.server_id
            and self.source_client == connection.client_id
        ):
            self.reconcile()
            if self.text_pane != self.pane:
                self.capture()
            return
        var snapshot = connection.model.snapshot.copy()
        var terminals = connection.model.terminals.copy()
        var sessions = List[Session]()
        var windows = List[Window]()
        var panes = List[Pane]()
        var session = self.session
        var window = self.window
        var pane = self.pane
        var blocked = (
            connection.model.attachment_blocked and not self.focus_dirty
        )
        if blocked:
            session = None
            window = None
            pane = None
        if not self.focus_dirty and connection.model.view.session:
            session = connection.model.view.session
        for record in snapshot.sessions:
            sessions.append(Session(record.id, record.name))
            var active_window = snapshot.focused_window_for(record)
            for item in record.windows:
                windows.append(
                    Window(
                        item.id, record.id, item.name, item.id == active_window
                    )
                )
                for id in _pane_keys(item):
                    ref source = item.panes[id]
                    var command: String
                    if len(source.summary.process) > 0:
                        command = source.summary.process[0]
                    elif source.kind.kind == 0:
                        command = "new pane"
                    elif source.kind.kind == 1:
                        command = "terminal"
                    elif source.kind.kind == 2:
                        command = "browser"
                    elif source.kind.kind == 3:
                        command = "editor"
                    else:
                        command = "agent"
                    panes.append(
                        Pane(
                            source.id,
                            item.id,
                            source.title,
                            command,
                            source.id == item.active_pane,
                        )
                    )
                if (
                    not self.focus_dirty
                    and session
                    and record.id == session.value()
                    and item.id == active_window
                ):
                    window = item.id
                    pane = item.active_pane
        var reconciled_session, reconciled_window, reconciled_pane = (
            _reconciled(sessions, windows, panes, session, window, pane)
        )
        if blocked:
            reconciled_session = None
            reconciled_window = None
            reconciled_pane = None
        var text = _selected_text(terminals, reconciled_pane)
        self.snapshot = snapshot^
        self.terminals = terminals^
        self.histories = connection.model.retained_histories.copy()
        self.history_invalidations = (
            connection.model.history_invalidations.copy()
        )
        self.sessions = sessions^
        self.windows = windows^
        self.panes = panes^
        self.session = reconciled_session
        self.window = reconciled_window
        self.pane = reconciled_pane
        self.terminal = text^
        self.generation = connection.model.revision
        self.source_server = connection.server_id
        self.source_client = connection.client_id
        self.layout_generation = connection.model.view.layout_generation
        self.attached_session = connection.model.view.session
        self.routing = connection.model.routing.copy()
        self.key_table_changes = connection.model.key_table_changes
        self.key_tables = connection.model.key_tables.copy()
        self.status = connection.model.events.status.copy()
        self.command_output = connection.model.events.output.copy()
        self.message = connection.model.message.copy()
        self.text_pane = self.pane
        self.appearance = connection.model.appearance.copy()
        self.appearance_provenance = (
            connection.model.appearance_provenance.copy()
        )
        self.appearance_revision = connection.model.appearance_revision
        self.dialogs = connection.model.dialogs.copy()
        self.attachment_blocked = connection.model.attachment_blocked
        self.detached = connection.model.detached.copy()
        self.server_stopped = connection.model.server_stopped
        self.error = "zz daemon stopped" if self.server_stopped else String()

    def capture(mut self) raises:
        self.terminal = _selected_text(self.terminals, self.pane)
        self.text_pane = self.pane

    def _owning_focus(mut self):
        if not self.focus_dirty or not self.pane:
            return
        for session in self.snapshot.sessions:
            for window in session.windows:
                if self.pane.value() in window.panes:
                    self.session = session.id
                    self.window = window.id
                    return

    @inline(.never)
    def sync_focus(mut self, mut connection: Connection) raises:
        self._owning_focus()
        var requested_session = self.session
        var requested_window = self.window
        var requested_pane = self.pane
        var requested = self.focus_dirty
        self.refresh(connection)
        if requested:
            self.session = requested_session
            self.window = requested_window
            self.pane = requested_pane
        elif (
            connection.model.view.session
            or connection.tree_pending
            or connection.model.attachment_blocked
        ):
            return
        if not self.session:
            return
        var session = self.session.value()
        var window = self.window
        var pane = self.pane
        if connection.model.view.session != self.session:
            var args: List[String] = ["-t", "$" + String(session)]
            _execute(
                connection,
                "switch-client" if connection.model.view.session else "attach-session",
                args,
            )
        if window:
            var actual = connection.model.view.focused_window
            if actual != window:
                var args: List[String] = ["-t", "@" + String(window.value())]
                _execute(connection, "select-window", args)
        if pane:
            var actual: Optional[UInt64] = None
            for record in connection.model.snapshot.sessions:
                if record.id == session:
                    for item in record.windows:
                        if window and item.id == window.value():
                            actual = item.active_pane
            if actual != pane:
                var args: List[String] = ["-t", "%" + String(pane.value())]
                _execute(connection, "select-pane", args)
        self.focus_dirty = False
        self.refresh(connection)

    def send_key(
        mut self, mut connection: Connection, key: String, literal: Bool = False
    ) raises:
        if not self.pane:
            return
        if literal:
            var frame = input_text(self.pane.value(), key)
            connection.transport.write(Span(frame))
            return
        var args: List[String] = ["-t", "%" + String(self.pane.value())]
        args.append("--")
        args.append(key)
        _execute(connection, "send-keys", args)
