from .protocol import Decoder, append_bytes, append_text, append_uint, envelope
from .tree_model import TmuxColour
from .tree_wire import (
    read_bool,
    read_uint,
    read_enum,
    read_count,
    read_text,
    read_optional_text,
    read_optional_colour,
)

comptime EVENT_KINDS = 66
comptime MESSAGE_KINDS = 76

comptime EVENT_DISPLAY_PANES: UInt64 = 15
comptime EVENT_OPEN_URI: UInt64 = 19
comptime EVENT_FOCUS_SIDEBAR: UInt64 = 20
comptime EVENT_BELL: UInt64 = 24
comptime EVENT_AGENT_UPDATES: UInt64 = 29
comptime EVENT_AGENT_STATE: UInt64 = 30
comptime EVENT_AGENT_LAGGED: UInt64 = 31
comptime EVENT_MENU: UInt64 = 36
comptime EVENT_CONFIRM: UInt64 = 37
comptime EVENT_PROVIDERS: UInt64 = 65

comptime ATTENTION_IDLE: UInt8 = 0
comptime ATTENTION_WORKING: UInt8 = 1
comptime ATTENTION_NEEDS_INPUT: UInt8 = 2
comptime ATTENTION_FAILED: UInt8 = 3

comptime EDGE_REQUEST: UInt8 = 0
comptime EDGE_DONE: UInt8 = 1
comptime EDGE_FAILED: UInt8 = 2
comptime EDGE_NONE: UInt8 = 255

comptime AGENT_WATCH_LIMIT = 256
comptime AGENT_PANE_LIMIT = 4096
comptime AGENT_BATCH_BYTES = 9 * 1024 * 1024
comptime AGENT_BATCH_ITEMS = 65536
comptime AGENT_QUEUE_BYTES = 48 * 1024 * 1024
comptime AGENT_QUEUE_PAUSE = 8 * 1024 * 1024
comptime AGENT_QUEUE_BATCHES = 4096
comptime AGENT_STATE_BYTES = 32 * 1024 * 1024
comptime AGENT_EDGE_LIMIT = 1024
comptime AGENT_BLOB_BYTES = 256 * 1024
comptime AGENT_PERMISSION_BYTES = 64 * 1024
comptime AGENT_SESSION_ID_BYTES = 16 * 1024
comptime AGENT_OPTION_BYTES = 4 * 1024
comptime AGENT_ANSWER_BYTES = 16 * 1024
comptime AGENT_TASK_LIMIT = 64
comptime AGENT_PROMPT_BYTES = 6 * 1024 * 1024
comptime AGENT_PROMPT_IMAGES = 64
comptime AGENT_IMAGE_FORMAT_BYTES = 64
comptime AGENT_QUESTION_ANSWERS = 32

comptime OPEN_URI_LIMIT = 64
comptime OPEN_URI_BYTES = 64 * 1024
comptime INDICATOR_LIMIT = 1024
comptime INDICATOR_LABEL_BYTES = 1024
comptime MENU_ITEM_LIMIT = 1024
comptime MENU_TEXT_BYTES = 16 * 1024
comptime CONFIRM_PROMPT_BYTES = 64 * 1024
comptime PROVIDER_LIMIT = 16
comptime PROVIDER_TEXT_BYTES = 64 * 1024
comptime NOTICE_LIMIT = 64
comptime NOTICE_BYTES = 4 * 1024

comptime AGENT_OUTBOX_BYTES = 64 * 1024 * 1024
comptime AGENT_OUTBOX_FRAMES = 1024
comptime AGENT_SEND_OK: UInt8 = 0
comptime AGENT_SEND_OFFLINE: UInt8 = 1
comptime AGENT_SEND_FULL: UInt8 = 2


@fieldwise_init
struct AgentTask(Copyable, Movable):
    var id: String
    var kind: String
    var description: String
    var tool_call_id: Optional[String]


@fieldwise_init
struct AgentGit(Copyable, Movable):
    var branch: Optional[String]
    var changed_files: UInt32
    var additions: UInt32
    var deletions: UInt32


@fieldwise_init
struct AgentPermission(Copyable, Movable):
    var request_id: UInt64
    var payload: String


struct AgentWireState(Copyable, Movable):
    var phase: UInt8
    var failure: String
    var queued_prompts: UInt32
    var session_id: Optional[String]
    var title: Optional[String]
    var error: Optional[String]
    var auth_methods: String
    var config_options: String
    var modes: String
    var permission: Optional[AgentPermission]
    var git: Optional[AgentGit]
    var tasks: List[AgentTask]
    var bytes: Int

    def __init__(out self):
        self.phase = 0
        self.failure = String()
        self.queued_prompts = 0
        self.session_id = None
        self.title = None
        self.error = None
        self.auth_methods = String()
        self.config_options = String()
        self.modes = String()
        self.permission = None
        self.git = None
        self.tasks = List[AgentTask]()
        self.bytes = 0

    def attention(self) -> UInt8:
        if self.permission:
            return ATTENTION_NEEDS_INPUT
        if self.phase == 2 or self.phase == 3:
            return ATTENTION_WORKING
        if self.phase == 4:
            return ATTENTION_FAILED
        return ATTENTION_IDLE


def attention_edge(previous: UInt8, current: UInt8) -> UInt8:
    if current == ATTENTION_NEEDS_INPUT and previous != ATTENTION_NEEDS_INPUT:
        return EDGE_REQUEST
    if previous == ATTENTION_WORKING and current == ATTENTION_IDLE:
        return EDGE_DONE
    if current == ATTENTION_FAILED and previous != ATTENTION_FAILED:
        return EDGE_FAILED
    return EDGE_NONE


@fieldwise_init
struct AgentBatch(Copyable, Movable):
    var pane: UInt64
    var first_seq: UInt64
    var items: List[List[UInt8]]
    var bytes: Int


@fieldwise_init
struct AgentOutbound(Copyable, Movable):
    var server: UInt64
    var client: UInt64
    var frame: List[UInt8]


@fieldwise_init
struct AgentAttention(Copyable, Movable):
    var pane: UInt64
    var edge: UInt8


@fieldwise_init
struct OpenUri(Copyable, Movable):
    var pane: UInt64
    var uri: String


@fieldwise_init
struct PaneIndicator(Copyable, Movable):
    var pane: UInt64
    var index: UInt32
    var select_key: UInt8
    var flags: UInt8
    var label: String


@fieldwise_init
struct DisplayPanes(Copyable, Movable):
    var window: UInt64
    var duration_ms: UInt32
    var indicators: List[PaneIndicator]
    var colour: Optional[TmuxColour]
    var active_colour: Optional[TmuxColour]


@fieldwise_init
struct MenuItem(Copyable, Movable):
    var name: String
    var key: Optional[String]
    var annotation: Optional[String]
    var enabled: Bool


struct MenuState(Copyable, Movable):
    var left: UInt16
    var top: UInt16
    var width: UInt16
    var height: UInt16
    var client_columns: UInt16
    var client_rows: UInt16
    var cell_width_px: UInt32
    var cell_height_px: UInt32
    var title: String
    var style: String
    var selected_style: String
    var border_style: String
    var border_lines: UInt8
    var items: List[Optional[MenuItem]]
    var selected: Optional[UInt32]
    var stay_open: Bool
    var mouse_keys: Bool

    def __init__(out self):
        self.left = 0
        self.top = 0
        self.width = 0
        self.height = 0
        self.client_columns = 0
        self.client_rows = 0
        self.cell_width_px = 0
        self.cell_height_px = 0
        self.title = String()
        self.style = String()
        self.selected_style = String()
        self.border_style = String()
        self.border_lines = 0
        self.items = List[Optional[MenuItem]]()
        self.selected = None
        self.stay_open = False
        self.mouse_keys = False


@fieldwise_init
struct ConfirmState(Copyable, Movable):
    var prompt: String
    var confirm_key: UInt8
    var default_yes: Bool
    var cursor_style: UInt8
    var cursor_colour: Optional[TmuxColour]


@fieldwise_init
struct ProviderStatus(Copyable, Movable):
    var provider: UInt8
    var path: Optional[String]
    var version: Optional[String]
    var latest: Optional[String]
    var manual: Bool
    var manual_owner: String
    var manual_command: Optional[String]
    var task: UInt8
    var task_text: String


@fieldwise_init
struct ProvidersReport(Copyable, Movable):
    var checking: Bool
    var providers: List[ProviderStatus]


def typed_event(kind: UInt64) -> Bool:
    return (
        kind == EVENT_DISPLAY_PANES
        or kind == EVENT_OPEN_URI
        or kind == EVENT_FOCUS_SIDEBAR
        or kind == EVENT_BELL
        or kind == EVENT_AGENT_UPDATES
        or kind == EVENT_AGENT_STATE
        or kind == EVENT_AGENT_LAGGED
        or kind == EVENT_MENU
        or kind == EVENT_CONFIRM
        or kind == EVENT_PROVIDERS
    )


def drained_event(kind: UInt64) -> Bool:
    return (
        (kind >= 26 and kind <= 28)
        or kind == 32
        or kind == 35
        or (kind >= 38 and kind <= 44)
        or (kind >= 46 and kind <= 50)
        or kind == 52
        or kind == 53
        or kind == 61
        or kind == 62
    )


def _soft_text(
    mut reader: Decoder, maximum: Int, mut over: Bool
) raises -> String:
    var size = read_count(reader)
    if size > maximum:
        over = True
        reader.offset += size
        return String()
    var result = String(
        from_utf8=Span(reader.data)[reader.offset : reader.offset + size]
    )
    reader.offset += size
    return result^


def _soft_optional(
    mut reader: Decoder, maximum: Int, mut over: Bool
) raises -> Optional[String]:
    if not read_bool(reader):
        return None
    return Optional[String](_soft_text(reader, maximum, over))


def read_agent_state(mut reader: Decoder) raises -> AgentWireState:
    var state = AgentWireState()
    state.phase = read_enum(reader, 4)
    if state.phase == 4:
        state.failure = read_text(reader, AGENT_BLOB_BYTES)
    state.queued_prompts = UInt32(read_uint(reader, 4294967295))
    state.session_id = read_optional_text(reader, AGENT_SESSION_ID_BYTES)
    state.title = read_optional_text(reader, AGENT_OPTION_BYTES)
    state.error = read_optional_text(reader, AGENT_BLOB_BYTES)
    state.auth_methods = read_text(reader, AGENT_BLOB_BYTES)
    state.config_options = read_text(reader, AGENT_BLOB_BYTES)
    state.modes = read_text(reader, AGENT_BLOB_BYTES)
    if read_bool(reader):
        var request = reader.uint()
        state.permission = AgentPermission(
            request, read_text(reader, AGENT_PERMISSION_BYTES)
        )
    if read_bool(reader):
        var branch = read_optional_text(reader, AGENT_OPTION_BYTES)
        var changed = UInt32(read_uint(reader, 4294967295))
        var additions = UInt32(read_uint(reader, 4294967295))
        var deletions = UInt32(read_uint(reader, 4294967295))
        state.git = AgentGit(branch^, changed, additions, deletions)
    for _ in range(read_count(reader, AGENT_TASK_LIMIT)):
        var id = read_text(reader, AGENT_OPTION_BYTES)
        var kind = read_text(reader, AGENT_OPTION_BYTES)
        var description = read_text(reader, AGENT_OPTION_BYTES)
        var tool = read_optional_text(reader, AGENT_OPTION_BYTES)
        state.tasks.append(AgentTask(id^, kind^, description^, tool^))
    return state^


def read_display_panes(
    mut reader: Decoder, mut over: Bool
) raises -> Optional[DisplayPanes]:
    if not read_bool(reader):
        return None
    var window = reader.uint()
    var duration = UInt32(read_uint(reader, 4294967295))
    var count = read_count(reader)
    if count > INDICATOR_LIMIT:
        over = True
        return None
    var indicators = List[PaneIndicator]()
    for _ in range(count):
        var pane = reader.uint()
        var index = UInt32(read_uint(reader, 4294967295))
        var key = reader.byte()
        var flags = reader.byte()
        var label = read_text(reader, INDICATOR_LABEL_BYTES)
        indicators.append(PaneIndicator(pane, index, key, flags, label^))
    var colour = read_optional_colour(reader)
    var active = read_optional_colour(reader)
    return Optional[DisplayPanes](
        DisplayPanes(window, duration, indicators^, colour, active)
    )


def read_menu(mut reader: Decoder, mut over: Bool) raises -> Optional[MenuState]:
    if not read_bool(reader):
        return None
    var menu = MenuState()
    menu.left = UInt16(read_uint(reader, 65535))
    menu.top = UInt16(read_uint(reader, 65535))
    menu.width = UInt16(read_uint(reader, 65535))
    menu.height = UInt16(read_uint(reader, 65535))
    menu.client_columns = UInt16(read_uint(reader, 65535))
    menu.client_rows = UInt16(read_uint(reader, 65535))
    menu.cell_width_px = UInt32(read_uint(reader, 4294967295))
    menu.cell_height_px = UInt32(read_uint(reader, 4294967295))
    menu.title = _soft_text(reader, MENU_TEXT_BYTES, over)
    menu.style = _soft_text(reader, MENU_TEXT_BYTES, over)
    menu.selected_style = _soft_text(reader, MENU_TEXT_BYTES, over)
    menu.border_style = _soft_text(reader, MENU_TEXT_BYTES, over)
    menu.border_lines = read_enum(reader, 6)
    var count = read_count(reader)
    if count > MENU_ITEM_LIMIT:
        over = True
        return None
    for _ in range(count):
        if not read_bool(reader):
            menu.items.append(None)
            continue
        var name = _soft_text(reader, MENU_TEXT_BYTES, over)
        var key = _soft_optional(reader, MENU_TEXT_BYTES, over)
        var annotation = _soft_optional(reader, MENU_TEXT_BYTES, over)
        var enabled = read_bool(reader)
        menu.items.append(
            Optional[MenuItem](MenuItem(name^, key^, annotation^, enabled))
        )
    if read_bool(reader):
        menu.selected = UInt32(read_uint(reader, 4294967295))
    menu.stay_open = read_bool(reader)
    menu.mouse_keys = read_bool(reader)
    if over:
        return None
    return Optional[MenuState](menu^)


def read_confirm(
    mut reader: Decoder, mut over: Bool
) raises -> Optional[ConfirmState]:
    if not read_bool(reader):
        return None
    var prompt = _soft_text(reader, CONFIRM_PROMPT_BYTES, over)
    var key = reader.byte()
    var default_yes = read_bool(reader)
    var style = reader.byte()
    var colour = read_optional_colour(reader)
    if over:
        return None
    return Optional[ConfirmState](
        ConfirmState(prompt^, key, default_yes, style, colour)
    )


def read_providers(
    mut reader: Decoder, mut over: Bool
) raises -> Optional[ProvidersReport]:
    var checking = read_bool(reader)
    var count = read_count(reader)
    if count > PROVIDER_LIMIT:
        over = True
        return None
    var providers = List[ProviderStatus]()
    for _ in range(count):
        var provider = read_enum(reader, 1)
        var path = _soft_optional(reader, PROVIDER_TEXT_BYTES, over)
        var version = _soft_optional(reader, PROVIDER_TEXT_BYTES, over)
        var latest = _soft_optional(reader, PROVIDER_TEXT_BYTES, over)
        var manual = read_bool(reader)
        var owner = String()
        var command: Optional[String] = None
        if manual:
            owner = _soft_text(reader, PROVIDER_TEXT_BYTES, over)
            command = _soft_optional(reader, PROVIDER_TEXT_BYTES, over)
        var task = read_enum(reader, 2)
        var text = String()
        if task != 0:
            text = _soft_text(reader, PROVIDER_TEXT_BYTES, over)
        providers.append(
            ProviderStatus(
                provider,
                path^,
                version^,
                latest^,
                manual,
                owner^,
                command^,
                task,
                text^,
            )
        )
    if over:
        return None
    return Optional[ProvidersReport](ProvidersReport(checking, providers^))


def _contains(panes: List[UInt64], pane: UInt64) -> Bool:
    for item in panes:
        if item == pane:
            return True
    return False


def _remove(mut panes: List[UInt64], pane: UInt64) -> Bool:
    var kept = List[UInt64]()
    var found = False
    for item in panes:
        if item == pane:
            found = True
        else:
            kept.append(item)
    swap(panes, kept)
    return found


def _restore(mut source: List[AgentBatch], mut target: List[AgentBatch]):
    while len(source) > 0:
        target.append(source.pop())


struct AgentQueue(Copyable, Movable):
    var watched: List[UInt64]
    var batches: List[AgentBatch]
    var batch_bytes: Int
    var states: Dict[UInt64, AgentWireState]
    var state_bytes: Int
    var fresh: List[UInt64]
    var status: Dict[UInt64, UInt8]
    var status_revision: UInt64
    var edges: List[AgentAttention]
    var lagging: List[UInt64]
    var cleared: Bool
    var online: Bool
    var disconnected: Bool
    var dropped_batches: UInt64
    var refused_batches: UInt64
    var refused_states: UInt64
    var evicted: UInt64
    var dropped_edges: UInt64

    def __init__(out self):
        self.watched = List[UInt64]()
        self.batches = List[AgentBatch]()
        self.batch_bytes = 0
        self.states = Dict[UInt64, AgentWireState]()
        self.state_bytes = 0
        self.fresh = List[UInt64]()
        self.status = Dict[UInt64, UInt8]()
        self.status_revision = 0
        self.edges = List[AgentAttention]()
        self.lagging = List[UInt64]()
        self.cleared = False
        self.online = False
        self.disconnected = False
        self.dropped_batches = 0
        self.refused_batches = 0
        self.refused_states = 0
        self.evicted = 0
        self.dropped_edges = 0

    def watches(self, pane: UInt64) -> Bool:
        return _contains(self.watched, pane)

    def saturated(self) -> Bool:
        return self.batch_bytes >= AGENT_QUEUE_PAUSE

    def _lag(mut self, pane: UInt64):
        if not _contains(self.lagging, pane):
            self.lagging.append(pane)

    def _drop_batches(mut self, pane: UInt64) -> List[AgentBatch]:
        var taken = List[AgentBatch]()
        var kept = List[AgentBatch]()
        while len(self.batches) > 0:
            var batch = self.batches.pop()
            if batch.pane == pane:
                self.batch_bytes -= batch.bytes
                taken.append(batch^)
            else:
                kept.append(batch^)
        _restore(kept, self.batches)
        var ordered = List[AgentBatch]()
        _restore(taken, ordered)
        return ordered^

    def _drop_state(mut self, pane: UInt64) raises:
        if pane in self.states:
            self.state_bytes -= self.states[pane].bytes
            _ = self.states.pop(pane)
        _ = _remove(self.fresh, pane)

    def _heaviest(self) raises -> UInt64:
        var totals = Dict[UInt64, Int]()
        var pane: UInt64 = 0
        var most = -1
        for batch in self.batches:
            var total = batch.bytes
            if batch.pane in totals:
                total += totals[batch.pane]
            totals[batch.pane] = total
            if total > most:
                most = total
                pane = batch.pane
        return pane

    def push_batch(mut self, var batch: AgentBatch) raises:
        if not self.watches(batch.pane) or _contains(self.lagging, batch.pane):
            self.dropped_batches += 1
            return
        while len(self.batches) > 0 and (
            batch.bytes > AGENT_QUEUE_BYTES - self.batch_bytes
            or len(self.batches) >= AGENT_QUEUE_BATCHES
        ):
            var pane = self._heaviest()
            _ = self._drop_batches(pane)
            self._lag(pane)
            self.evicted += 1
            if pane == batch.pane:
                self.dropped_batches += 1
                return
        self.batch_bytes += batch.bytes
        self.batches.append(batch^)

    def push_state(mut self, pane: UInt64, var state: AgentWireState) raises:
        var current = state.attention()
        if pane in self.status:
            var previous = self.status[pane]
            var edge = attention_edge(previous, current)
            if edge != EDGE_NONE:
                if len(self.edges) >= AGENT_EDGE_LIMIT:
                    _ = self.edges.pop(0)
                    self.dropped_edges += 1
                self.edges.append(AgentAttention(pane, edge))
            if previous != current:
                self.status[pane] = current
                self.status_revision += 1
        elif len(self.status) >= AGENT_PANE_LIMIT:
            self.refused_states += 1
            return
        else:
            self.status[pane] = current
            self.status_revision += 1
        self._store(pane, state^)

    def _store(mut self, pane: UInt64, var state: AgentWireState) raises:
        self._drop_state(pane)
        if state.bytes > AGENT_STATE_BYTES - self.state_bytes:
            self.refused_states += 1
            return
        self.state_bytes += state.bytes
        self.states[pane] = state^
        self.fresh.append(pane)

    def push_lagged(mut self, pane: UInt64):
        if self.watches(pane):
            self._lag(pane)

    def forget(mut self, pane: UInt64) raises:
        _ = self._drop_batches(pane)
        self._drop_state(pane)
        _ = _remove(self.lagging, pane)
        if pane in self.status:
            _ = self.status.pop(pane)
            self.status_revision += 1

    def prune(mut self, live: Dict[UInt64, Bool]) raises:
        var retired = List[UInt64]()
        for pane in self.status:
            if pane not in live:
                retired.append(pane)
        for batch in self.batches:
            if batch.pane not in live and not _contains(retired, batch.pane):
                retired.append(batch.pane)
        for pane in retired:
            self.forget(pane)

    def _clear(mut self):
        self.batches.clear()
        self.batch_bytes = 0
        self.states.clear()
        self.state_bytes = 0
        self.fresh.clear()
        self.lagging.clear()
        if len(self.status) > 0:
            self.status.clear()
            self.status_revision += 1

    def reset(mut self):
        self._clear()
        self.edges.clear()
        self.cleared = True

    def watch(mut self, pane: UInt64) -> Bool:
        if self.watches(pane):
            return True
        if len(self.watched) >= AGENT_WATCH_LIMIT:
            return False
        self.watched.append(pane)
        self._lag(pane)
        if pane in self.states and not _contains(self.fresh, pane):
            self.fresh.append(pane)
        return True

    def unwatch(mut self, pane: UInt64):
        if not _remove(self.watched, pane):
            return
        _ = self._drop_batches(pane)
        _ = _remove(self.lagging, pane)

    def rebind(mut self, online: Bool):
        self._clear()
        self.edges.clear()
        for pane in self.watched:
            self.lagging.append(pane)
        if self.online:
            self.disconnected = True
        self.online = online

    def merge(mut self, mut other: AgentQueue) raises:
        if other.cleared:
            other.cleared = False
            self._clear()
            for pane in self.watched:
                self.lagging.append(pane)
        var batches = List[AgentBatch]()
        while len(other.batches) > 0:
            batches.append(other.batches.pop())
        other.batch_bytes = 0
        while len(batches) > 0:
            self.push_batch(batches.pop())
        for pane in other.fresh:
            if pane in other.states:
                self._store(pane, other.states[pane].copy())
        other.states.clear()
        other.state_bytes = 0
        other.fresh.clear()
        for pane in other.lagging:
            if self.watches(pane):
                self._lag(pane)
        other.lagging.clear()
        for edge in other.edges:
            if len(self.edges) >= AGENT_EDGE_LIMIT:
                _ = self.edges.pop(0)
                self.dropped_edges += 1
            self.edges.append(edge.copy())
        other.edges.clear()
        if other.status_revision != 0:
            self.status = other.status.copy()
            self.status_revision += 1
            other.status_revision = 0
            var retired = List[UInt64]()
            for pane in self.states:
                if pane not in self.status:
                    retired.append(pane)
            for pane in retired:
                self._drop_state(pane)
        self.dropped_batches += other.dropped_batches
        self.refused_batches += other.refused_batches
        self.refused_states += other.refused_states
        self.evicted += other.evicted
        self.dropped_edges += other.dropped_edges
        other.dropped_batches = 0
        other.refused_batches = 0
        other.refused_states = 0
        other.evicted = 0
        other.dropped_edges = 0

    def take_updates(mut self, pane: UInt64) -> List[AgentBatch]:
        return self._drop_batches(pane)

    def take_state(mut self, pane: UInt64) raises -> Optional[AgentWireState]:
        if not _remove(self.fresh, pane) or pane not in self.states:
            return None
        return Optional[AgentWireState](self.states[pane].copy())

    def state(self, pane: UInt64) raises -> Optional[AgentWireState]:
        if pane not in self.states:
            return None
        return Optional[AgentWireState](self.states[pane].copy())

    def take_lagged(mut self, pane: UInt64) -> Bool:
        if not self.online:
            return False
        return _remove(self.lagging, pane)

    def take_attention(mut self) -> List[AgentAttention]:
        var edges = List[AgentAttention]()
        swap(edges, self.edges)
        return edges^

    def take_disconnected(mut self) -> Bool:
        var value = self.disconnected
        self.disconnected = False
        return value


struct StreamQueues(Copyable, Movable):
    var agent: AgentQueue
    var bells: UInt64
    var bell_pane: UInt64
    var bell_revision: UInt64
    var sidebar_focus: UInt64
    var sidebar_focus_revision: UInt64
    var open_uris: List[OpenUri]
    var refused_uris: UInt64
    var display_panes: Optional[DisplayPanes]
    var display_panes_changed: Bool
    var display_panes_revision: UInt64
    var menu: Optional[MenuState]
    var menu_changed: Bool
    var menu_opens: UInt64
    var menu_revision: UInt64
    var menu_opened: UInt64
    var confirm: Optional[ConfirmState]
    var confirm_changed: Bool
    var confirm_revision: UInt64
    var providers: Optional[ProvidersReport]
    var providers_changed: Bool
    var providers_revision: UInt64
    var refused_overlays: UInt64
    var refused_providers: UInt64
    var notices: List[String]
    var dropped_notices: UInt64
    var ignored_responses: UInt64
    var settled_responses: UInt64
    var drained_events: List[UInt64]
    var drained_messages: List[UInt64]
    var drained_bytes: UInt64

    def __init__(out self):
        self.agent = AgentQueue()
        self.bells = 0
        self.bell_pane = 0
        self.bell_revision = 0
        self.sidebar_focus = 0
        self.sidebar_focus_revision = 0
        self.open_uris = List[OpenUri]()
        self.refused_uris = 0
        self.display_panes = None
        self.display_panes_changed = False
        self.display_panes_revision = 0
        self.menu = None
        self.menu_changed = False
        self.menu_opens = 0
        self.menu_revision = 0
        self.menu_opened = 0
        self.confirm = None
        self.confirm_changed = False
        self.confirm_revision = 0
        self.providers = None
        self.providers_changed = False
        self.providers_revision = 0
        self.refused_overlays = 0
        self.refused_providers = 0
        self.notices = List[String]()
        self.dropped_notices = 0
        self.ignored_responses = 0
        self.settled_responses = 0
        self.drained_events = List[UInt64](length=EVENT_KINDS, fill=0)
        self.drained_messages = List[UInt64](length=MESSAGE_KINDS, fill=0)
        self.drained_bytes = 0

    def drain(mut self, message: UInt64, event: Optional[UInt64], size: Int):
        if event and event.value() < UInt64(EVENT_KINDS):
            self.drained_events[Int(event.value())] += 1
        elif message < UInt64(MESSAGE_KINDS):
            self.drained_messages[Int(message)] += 1
        self.drained_bytes += UInt64(size)

    def drained(self) -> UInt64:
        var total: UInt64 = 0
        for count in self.drained_events:
            total += count
        for count in self.drained_messages:
            total += count
        return total

    def notice(mut self, text: String):
        if text.byte_length() > NOTICE_BYTES:
            self.dropped_notices += 1
            return
        if len(self.notices) >= NOTICE_LIMIT:
            _ = self.notices.pop(0)
            self.dropped_notices += 1
        self.notices.append(text.copy())

    def take_notices(mut self) -> List[String]:
        var notices = List[String]()
        swap(notices, self.notices)
        return notices^

    def take_open_uris(mut self) -> List[OpenUri]:
        var uris = List[OpenUri]()
        swap(uris, self.open_uris)
        return uris^

    def event(mut self, kind: UInt64, var frame: List[UInt8]) raises:
        var size = len(frame)
        if not typed_event(kind):
            self.drain(10, Optional[UInt64](kind), size)
            return
        var reader = Decoder(frame^)
        _ = reader.uint()
        _ = reader.uint()
        _ = reader.uint()
        var over = False
        if kind == EVENT_BELL:
            self.bell_pane = reader.uint()
            self.bells += 1
        elif kind == EVENT_FOCUS_SIDEBAR:
            self.sidebar_focus += 1
        elif kind == EVENT_OPEN_URI:
            var pane = reader.uint()
            var uri = _soft_text(reader, OPEN_URI_BYTES, over)
            if over or len(self.open_uris) >= OPEN_URI_LIMIT:
                self.refused_uris += 1
            else:
                self.open_uris.append(OpenUri(pane, uri^))
        elif kind == EVENT_AGENT_UPDATES:
            var pane = reader.uint()
            var first_seq = reader.uint()
            var count = read_count(reader)
            if count > AGENT_BATCH_ITEMS:
                self.agent.refused_batches += 1
                return
            var items = List[List[UInt8]]()
            var total = 0
            for _ in range(count):
                var item = reader.bytes()
                total += len(item)
                if total > AGENT_BATCH_BYTES:
                    raise Error("agent updates exceed their byte limit")
                items.append(item^)
            reader.finish()
            self.agent.push_batch(AgentBatch(pane, first_seq, items^, total))
            return
        elif kind == EVENT_AGENT_STATE:
            var pane = reader.uint()
            var state = read_agent_state(reader)
            state.bytes = size
            reader.finish()
            self.agent.push_state(pane, state^)
            return
        elif kind == EVENT_AGENT_LAGGED:
            var pane = reader.uint()
            _ = reader.uint()
            self.agent.push_lagged(pane)
        elif kind == EVENT_DISPLAY_PANES:
            var state = read_display_panes(reader, over)
            if over:
                self.refused_overlays += 1
                return
            self.display_panes = state^
            self.display_panes_changed = True
        elif kind == EVENT_MENU:
            var state = read_menu(reader, over)
            if over:
                self.refused_overlays += 1
                return
            if state and not self.menu:
                self.menu_opens += 1
            self.menu = state^
            self.menu_changed = True
        elif kind == EVENT_CONFIRM:
            var state = read_confirm(reader, over)
            if over:
                self.refused_overlays += 1
                return
            self.confirm = state^
            self.confirm_changed = True
        else:
            var report = read_providers(reader, over)
            if over:
                self.refused_providers += 1
                return
            self.providers = report^
            self.providers_changed = True
        reader.finish()

    def reset_session(mut self):
        if self.display_panes:
            self.display_panes = None
            self.display_panes_changed = True
        if self.menu:
            self.menu = None
            self.menu_changed = True
        if self.confirm:
            self.confirm = None
            self.confirm_changed = True
        self.agent.reset()

    def merge(mut self, mut other: StreamQueues) raises:
        self.agent.merge(other.agent)
        if other.bells > 0:
            self.bell_revision += other.bells
            if self.bell_revision == 0:
                self.bell_revision = 1
            self.bell_pane = other.bell_pane
            other.bells = 0
        if other.sidebar_focus > 0:
            self.sidebar_focus_revision += other.sidebar_focus
            other.sidebar_focus = 0
        for uri in other.open_uris:
            if len(self.open_uris) >= OPEN_URI_LIMIT:
                self.refused_uris += 1
            else:
                self.open_uris.append(uri.copy())
        other.open_uris.clear()
        if other.display_panes_changed:
            other.display_panes_changed = False
            self.display_panes = other.display_panes.copy()
            self.display_panes_revision += 1
        if other.menu_changed:
            other.menu_changed = False
            self.menu = other.menu.copy()
            self.menu_revision += 1
        self.menu_opened += other.menu_opens
        other.menu_opens = 0
        if other.confirm_changed:
            other.confirm_changed = False
            self.confirm = other.confirm.copy()
            self.confirm_revision += 1
        if other.providers_changed:
            other.providers_changed = False
            self.providers = other.providers.copy()
            self.providers_revision += 1
        for text in other.notices:
            self.notice(text)
        other.notices.clear()
        self.refused_uris += other.refused_uris
        self.refused_overlays += other.refused_overlays
        self.refused_providers += other.refused_providers
        self.dropped_notices += other.dropped_notices
        self.ignored_responses += other.ignored_responses
        self.settled_responses += other.settled_responses
        self.drained_bytes += other.drained_bytes
        other.refused_uris = 0
        other.refused_overlays = 0
        other.refused_providers = 0
        other.dropped_notices = 0
        other.ignored_responses = 0
        other.settled_responses = 0
        other.drained_bytes = 0
        for index in range(EVENT_KINDS):
            self.drained_events[index] += other.drained_events[index]
            other.drained_events[index] = 0
        for index in range(MESSAGE_KINDS):
            self.drained_messages[index] += other.drained_messages[index]
            other.drained_messages[index] = 0

    def disconnect(mut self):
        self.agent.rebind(False)
        self.display_panes = None
        self.menu = None
        self.confirm = None
        self.display_panes_revision += 1
        self.menu_revision += 1
        self.confirm_revision += 1


def _option(mut payload: List[UInt8], text: String, limit: Int) raises:
    if text.byte_length() > limit:
        raise Error("agent request text exceeds its limit")
    append_text(payload, text)


def agent_prompt(
    pane: UInt64, text: String, formats: List[String], images: List[List[UInt8]]
) raises -> List[UInt8]:
    if text.byte_length() > AGENT_PROMPT_BYTES:
        raise Error("agent prompt exceeds its byte limit")
    if len(formats) != len(images) or len(images) > AGENT_PROMPT_IMAGES:
        raise Error("agent prompt images exceed their limit")
    var payload: List[UInt8] = [21]
    append_uint(payload, pane)
    append_text(payload, text)
    append_uint(payload, UInt64(len(images)))
    for index in range(len(images)):
        if formats[index].byte_length() > AGENT_IMAGE_FORMAT_BYTES:
            raise Error("agent image format exceeds its byte limit")
        if len(images[index]) > AGENT_PROMPT_BYTES:
            raise Error("agent image exceeds its byte limit")
        append_text(payload, formats[index])
        append_bytes(payload, Span(images[index]))
    return envelope(Span(payload))


def agent_cancel(pane: UInt64) raises -> List[UInt8]:
    var payload: List[UInt8] = [22]
    append_uint(payload, pane)
    return envelope(Span(payload))


def agent_unqueue(pane: UInt64) raises -> List[UInt8]:
    var payload: List[UInt8] = [23]
    append_uint(payload, pane)
    return envelope(Span(payload))


def agent_permission(
    pane: UInt64, request: UInt64, option: Optional[String]
) raises -> List[UInt8]:
    var payload: List[UInt8] = [24]
    append_uint(payload, pane)
    append_uint(payload, request)
    if option:
        payload.append(1)
        _option(payload, option.value(), AGENT_OPTION_BYTES)
    else:
        payload.append(0)
    return envelope(Span(payload))


def agent_set_option(
    pane: UInt64, option: String, value: String
) raises -> List[UInt8]:
    var payload: List[UInt8] = [25]
    append_uint(payload, pane)
    _option(payload, option, AGENT_OPTION_BYTES)
    _option(payload, value, AGENT_OPTION_BYTES)
    return envelope(Span(payload))


def agent_set_mode(pane: UInt64, mode: String) raises -> List[UInt8]:
    var payload: List[UInt8] = [26]
    append_uint(payload, pane)
    _option(payload, mode, AGENT_OPTION_BYTES)
    return envelope(Span(payload))


def agent_authenticate(pane: UInt64, method: String) raises -> List[UInt8]:
    var payload: List[UInt8] = [27]
    append_uint(payload, pane)
    _option(payload, method, AGENT_OPTION_BYTES)
    return envelope(Span(payload))


def agent_replay(pane: UInt64, from_seq: UInt64) raises -> List[UInt8]:
    var payload: List[UInt8] = [29]
    append_uint(payload, pane)
    append_uint(payload, from_seq)
    return envelope(Span(payload))


def agent_acknowledge(pane: UInt64, reclaim: UInt64) raises -> List[UInt8]:
    var payload: List[UInt8] = [30]
    append_uint(payload, pane)
    append_uint(payload, reclaim)
    return envelope(Span(payload))


def agent_answer(
    pane: UInt64, request: UInt64, ids: List[String], answers: List[List[String]]
) raises -> List[UInt8]:
    if len(ids) != len(answers) or len(ids) > AGENT_QUESTION_ANSWERS:
        raise Error("agent answers exceed their limit")
    var payload: List[UInt8] = [66]
    append_uint(payload, pane)
    append_uint(payload, request)
    append_uint(payload, UInt64(len(ids)))
    for index in range(len(ids)):
        _option(payload, ids[index], AGENT_OPTION_BYTES)
        if len(answers[index]) > AGENT_QUESTION_ANSWERS:
            raise Error("agent answers exceed their limit")
        append_uint(payload, UInt64(len(answers[index])))
        for answer in answers[index]:
            _option(payload, answer, AGENT_ANSWER_BYTES)
    return envelope(Span(payload))


def agent_stop_task(pane: UInt64, task: String) raises -> List[UInt8]:
    var payload: List[UInt8] = [67]
    append_uint(payload, pane)
    _option(payload, task, AGENT_OPTION_BYTES)
    return envelope(Span(payload))


def agent_steer(pane: UInt64, text: String) raises -> List[UInt8]:
    var payload: List[UInt8] = [75]
    append_uint(payload, pane)
    _option(payload, text, AGENT_PROMPT_BYTES)
    return envelope(Span(payload))
