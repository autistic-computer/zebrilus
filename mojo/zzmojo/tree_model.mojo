struct TmuxColour(TrivialRegisterPassable):
    var kind: UInt8
    var value: UInt32

    def __init__(out self):
        self.kind = 3
        self.value = 0


struct PaneKind(Copyable, Movable):
    var kind: UInt8
    var tabs: List[String]
    var active_tab: UInt64
    var profile: String
    var path: Optional[String]
    var cwd: Optional[String]
    var provider: UInt8
    var session_id: Optional[String]
    var model: Optional[String]
    var effort: Optional[String]

    def __init__(out self):
        self.kind = 1
        self.tabs = List[String]()
        self.active_tab = 0
        self.profile = String()
        self.path = None
        self.cwd = None
        self.provider = 0
        self.session_id = None
        self.model = None
        self.effort = None


struct PaneSummary(Copyable, Movable):
    var changed_at: UInt64
    var unseen: Bool
    var command: Optional[UInt8]
    var command_exit: UInt8
    var name: String
    var cwd: String
    var repo: String
    var branch: String
    var process: List[String]
    var flip_armed: Bool

    def __init__(out self):
        self.changed_at = 0
        self.unseen = False
        self.command = None
        self.command_exit = 0
        self.name = String()
        self.cwd = String()
        self.repo = String()
        self.branch = String()
        self.process = List[String]()
        self.flip_armed = False


struct PaneStatus(Copyable, Movable):
    var state: UInt8
    var kind: Optional[UInt8]
    var progress: Optional[UInt8]
    var app: String
    var title: String
    var message: String

    def __init__(out self):
        self.state = 0
        self.kind = None
        self.progress = None
        self.app = String()
        self.title = String()
        self.message = String()


struct PaneSnapshot(Copyable, Movable):
    var id: UInt64
    var title: String
    var kind: PaneKind
    var synchronized_input: Bool
    var bell: Bool
    var dead: Bool
    var dead_status: Optional[UInt32]
    var border_colour: Optional[TmuxColour]
    var active_border_colour: Optional[TmuxColour]
    var border_status_text: String
    var mode_kind: Optional[UInt8]
    var mode_bytes: List[UInt8]
    var status: Optional[PaneStatus]
    var summary: PaneSummary

    def __init__(out self):
        self.id = 0
        self.title = String()
        self.kind = PaneKind()
        self.synchronized_input = False
        self.bell = False
        self.dead = False
        self.dead_status = None
        self.border_colour = None
        self.active_border_colour = None
        self.border_status_text = String()
        self.mode_kind = None
        self.mode_bytes = List[UInt8]()
        self.status = None
        self.summary = PaneSummary()


struct LayoutNode(TrivialRegisterPassable):
    var kind: UInt8
    var id: UInt64
    var axis: UInt8
    var ratio: Float32
    var first: Int
    var second: Int

    def __init__(out self):
        self.kind = 2
        self.id = 0
        self.axis = 0
        self.ratio = 0.5
        self.first = -1
        self.second = -1


struct LayoutTree(Copyable, Movable):
    var nodes: List[LayoutNode]
    var root: Int

    def __init__(out self):
        self.nodes = List[LayoutNode]()
        self.root = -1

    def panes(self) -> List[UInt64]:
        var output = List[UInt64]()
        if self.root < 0:
            return output^
        var pending: List[Int] = [self.root]
        while len(pending) > 0:
            var index = pending.pop()
            var node = self.nodes[index].copy()
            if node.kind == 0:
                output.append(node.id)
            elif node.kind == 1:
                pending.append(node.second)
                pending.append(node.first)
        return output^


struct FloatingPane(TrivialRegisterPassable):
    var pane: UInt64
    var xoff: Int32
    var yoff: Int32
    var sx: UInt16
    var sy: UInt16
    var visible: Bool
    var border_lines: UInt8
    var border_status: UInt8

    def __init__(out self):
        self.pane = 0
        self.xoff = 0
        self.yoff = 0
        self.sx = 0
        self.sy = 0
        self.visible = False
        self.border_lines = 0
        self.border_status = 0


struct ModalPane(TrivialRegisterPassable):
    var pane: UInt64
    var capture_keys: Bool
    var close_on_click: Bool
    var close_on_cancel: Bool

    def __init__(out self):
        self.pane = 0
        self.capture_keys = False
        self.close_on_click = False
        self.close_on_cancel = False


struct WindowSnapshot(Copyable, Movable):
    var id: UInt64
    var index: UInt32
    var name: String
    var automatic_rename: Bool
    var active_pane: UInt64
    var zoomed_pane: Optional[UInt64]
    var layout: LayoutTree
    var panes: Dict[UInt64, PaneSnapshot]
    var layout_dump: String
    var visible_layout_dump: String
    var status_label: String
    var activity: Bool
    var silence: Bool
    var pane_border_status: UInt8
    var pane_border_lines: UInt8
    var pane_border_indicators: UInt8
    var pane_order: List[UInt64]
    var pane_z_order: List[UInt64]
    var floating: List[FloatingPane]
    var modal: Optional[ModalPane]
    var sx: UInt16
    var sy: UInt16
    var pane_base_index: UInt32

    def __init__(out self):
        self.id = 0
        self.index = 0
        self.name = String()
        self.automatic_rename = False
        self.active_pane = 0
        self.zoomed_pane = None
        self.layout = LayoutTree()
        self.panes = Dict[UInt64, PaneSnapshot]()
        self.layout_dump = String()
        self.visible_layout_dump = String()
        self.status_label = String()
        self.activity = False
        self.silence = False
        self.pane_border_status = 0
        self.pane_border_lines = 0
        self.pane_border_indicators = 1
        self.pane_order = List[UInt64]()
        self.pane_z_order = List[UInt64]()
        self.floating = List[FloatingPane]()
        self.modal = None
        self.sx = 0
        self.sy = 0
        self.pane_base_index = 0


struct SessionViewer(Copyable, Movable):
    var name: String
    var window: UInt64
    var is_self: Bool

    def __init__(out self):
        self.name = String()
        self.window = 0
        self.is_self = False


struct SessionSnapshot(Copyable, Movable):
    var id: UInt64
    var name: String
    var active_window: UInt64
    var windows: List[WindowSnapshot]
    var viewers: List[SessionViewer]

    def __init__(out self):
        self.id = 0
        self.name = String()
        self.active_window = 0
        self.windows = List[WindowSnapshot]()
        self.viewers = List[SessionViewer]()


struct MuxSnapshot(Copyable, Movable):
    var generation: UInt64
    var sessions: List[SessionSnapshot]
    var focused_window: Optional[UInt64]

    def __init__(out self):
        self.generation = 0
        self.sessions = List[SessionSnapshot]()
        self.focused_window = None

    def session_index(self, id: UInt64) raises -> Int:
        for index in range(len(self.sessions)):
            if self.sessions[index].id == id:
                return index
        raise Error("missing session: " + String(id))

    def window_index(self, session: Int, id: UInt64) raises -> Int:
        for index in range(len(self.sessions[session].windows)):
            if self.sessions[session].windows[index].id == id:
                return index
        raise Error("missing window: " + String(id))

    def focused_window_for(self, session: SessionSnapshot) -> UInt64:
        if self.focused_window:
            for window in session.windows:
                if window.id == self.focused_window.value():
                    return window.id
        return session.active_window

    def apply(mut self, delta: TreeDelta) raises:
        if self.generation != delta.base:
            raise Error("tree generation mismatch")
        self.apply_overlay(delta.ops)
        self.generation = delta.version

    def apply_overlay(mut self, ops: List[TreeOp]) raises:
        if len(ops) == 0:
            return
        var next = self.copy()
        for op in ops:
            _apply_op(next, op)
        _sort_tree(next)
        self = next^


struct TreeOp(Copyable, Movable):
    var kind: UInt8
    var session: UInt64
    var window: UInt64
    var pane: UInt64
    var name: String
    var session_value: Optional[SessionSnapshot]
    var window_value: Optional[WindowSnapshot]
    var pane_value: Optional[PaneSnapshot]
    var viewers: List[SessionViewer]
    var layout: LayoutTree
    var zoomed_pane: Optional[UInt64]
    var layout_dump: String
    var visible_layout_dump: String
    var pane_order: List[UInt64]
    var pane_z_order: List[UInt64]
    var floating: List[FloatingPane]
    var modal: Optional[ModalPane]
    var sx: UInt16
    var sy: UInt16
    var index: UInt32
    var automatic_rename: Bool
    var activity: Bool
    var silence: Bool
    var pane_base_index: UInt32
    var pane_border_status: UInt8
    var pane_border_lines: UInt8
    var pane_border_indicators: UInt8
    var pane_kind: PaneKind
    var synchronized_input: Bool
    var bell: Bool
    var dead: Bool
    var dead_status: Optional[UInt32]
    var border_colour: Optional[TmuxColour]
    var active_border_colour: Optional[TmuxColour]
    var mode_kind: Optional[UInt8]
    var mode_bytes: List[UInt8]
    var status: Optional[PaneStatus]
    var summary: PaneSummary

    def __init__(out self):
        self.kind = 0
        self.session = 0
        self.window = 0
        self.pane = 0
        self.name = String()
        self.session_value = None
        self.window_value = None
        self.pane_value = None
        self.viewers = List[SessionViewer]()
        self.layout = LayoutTree()
        self.zoomed_pane = None
        self.layout_dump = String()
        self.visible_layout_dump = String()
        self.pane_order = List[UInt64]()
        self.pane_z_order = List[UInt64]()
        self.floating = List[FloatingPane]()
        self.modal = None
        self.sx = 0
        self.sy = 0
        self.index = 0
        self.automatic_rename = False
        self.activity = False
        self.silence = False
        self.pane_base_index = 0
        self.pane_border_status = 0
        self.pane_border_lines = 0
        self.pane_border_indicators = 1
        self.pane_kind = PaneKind()
        self.synchronized_input = False
        self.bell = False
        self.dead = False
        self.dead_status = None
        self.border_colour = None
        self.active_border_colour = None
        self.mode_kind = None
        self.mode_bytes = List[UInt8]()
        self.status = None
        self.summary = PaneSummary()


struct TreeDelta(Copyable, Movable):
    var base: UInt64
    var version: UInt64
    var ops: List[TreeOp]

    def __init__(out self):
        self.base = 0
        self.version = 0
        self.ops = List[TreeOp]()


struct ClientView(Copyable, Movable):
    var session: Optional[UInt64]
    var focused_window: Optional[UInt64]
    var read_only: Bool
    var client_flags: String
    var layout_generation: UInt64
    var attachment_generation: UInt64
    var overlay: List[TreeOp]

    def __init__(out self):
        self.session = None
        self.focused_window = None
        self.read_only = False
        self.client_flags = String()
        self.layout_generation = 0
        self.attachment_generation = 0
        self.overlay = List[TreeOp]()

    def apply(self, mut snapshot: MuxSnapshot) raises:
        snapshot.apply_overlay(self.overlay)
        snapshot.focused_window = self.focused_window


@fieldwise_init
struct _Order(TrivialRegisterPassable):
    var major: UInt64
    var minor: UInt64
    var index: Int


def _ordered(var values: List[_Order]) -> List[_Order]:
    var buffer = values.copy()
    var width = 1
    var size = len(values)
    while width < size:
        var start = 0
        while start < size:
            var middle = min(start + width, size)
            var end = min(start + 2 * width, size)
            var left = start
            var right = middle
            for target in range(start, end):
                var take_left = right >= end
                if not take_left and left < middle:
                    take_left = values[left].major < values[right].major or (
                        values[left].major == values[right].major
                        and values[left].minor <= values[right].minor
                    )
                if take_left:
                    buffer[target] = values[left]
                    left += 1
                else:
                    buffer[target] = values[right]
                    right += 1
            start = end
        var previous = values^
        values = buffer^
        buffer = previous^
        width *= 2
    return values^


def _sort_tree(mut snapshot: MuxSnapshot):
    var order = List[_Order]()
    for index in range(len(snapshot.sessions)):
        order.append(_Order(snapshot.sessions[index].id, 0, index))
    var sorted = _ordered(order^)
    var sessions = List[SessionSnapshot]()
    for item in sorted:
        var session = snapshot.sessions[item.index].copy()
        var windows_order = List[_Order]()
        for index in range(len(session.windows)):
            windows_order.append(
                _Order(
                    UInt64(session.windows[index].index),
                    session.windows[index].id,
                    index,
                )
            )
        var sorted_windows = _ordered(windows_order^)
        var windows = List[WindowSnapshot]()
        for window in sorted_windows:
            windows.append(session.windows[window.index].copy())
        session.windows = windows^
        sessions.append(session^)
    snapshot.sessions = sessions^


@inline(.never)
def _apply_op(mut snapshot: MuxSnapshot, op: TreeOp) raises:
    if op.kind == 0:
        if not op.session_value:
            raise Error("session upsert has no value")
        var next = op.session_value.value().copy()
        for index in range(len(snapshot.sessions)):
            if snapshot.sessions[index].id == next.id:
                snapshot.sessions[index] = next^
                return
        snapshot.sessions.append(next^)
        return
    if op.kind == 1:
        for index in range(len(snapshot.sessions) - 1, -1, -1):
            if snapshot.sessions[index].id == op.session:
                _ = snapshot.sessions.pop(index)
        return
    var session_index = snapshot.session_index(op.session)
    ref session = snapshot.sessions[session_index]
    if op.kind == 2:
        session.name = op.name
        return
    if op.kind == 3:
        session.active_window = op.window
        return
    if op.kind == 4:
        session.viewers = op.viewers.copy()
        return
    if op.kind == 5:
        if not op.window_value:
            raise Error("window upsert has no value")
        var next = op.window_value.value().copy()
        for index in range(len(session.windows)):
            if session.windows[index].id == next.id:
                session.windows[index] = next^
                return
        session.windows.append(next^)
        return
    if op.kind == 6:
        for index in range(len(session.windows) - 1, -1, -1):
            if session.windows[index].id == op.window:
                _ = session.windows.pop(index)
        return
    var window_index = -1
    for index in range(len(session.windows)):
        if session.windows[index].id == op.window:
            window_index = index
            break
    if window_index < 0:
        raise Error("missing window: " + String(op.window))
    ref window = session.windows[window_index]
    if op.kind == 7:
        window.name = op.name
        return
    if op.kind == 8:
        window.active_pane = op.pane
        return
    if op.kind == 9:
        window.layout = op.layout.copy()
        window.zoomed_pane = op.zoomed_pane
        window.layout_dump = op.layout_dump
        window.visible_layout_dump = op.visible_layout_dump
        window.pane_order = op.pane_order.copy()
        window.pane_z_order = op.pane_z_order.copy()
        window.floating = op.floating.copy()
        window.modal = op.modal
        window.sx = op.sx
        window.sy = op.sy
        return
    if op.kind == 10:
        window.index = op.index
        window.automatic_rename = op.automatic_rename
        window.activity = op.activity
        window.silence = op.silence
        window.pane_base_index = op.pane_base_index
        return
    if op.kind == 11:
        window.status_label = op.name
        window.pane_border_status = op.pane_border_status
        window.pane_border_lines = op.pane_border_lines
        window.pane_border_indicators = op.pane_border_indicators
        return
    if op.kind == 12:
        if not op.pane_value:
            raise Error("pane upsert has no value")
        var next = op.pane_value.value().copy()
        window.panes[next.id] = next^
        return
    if op.kind == 13:
        if op.pane in window.panes:
            _ = window.panes.pop(op.pane)
        return
    if op.pane not in window.panes:
        raise Error("missing pane: " + String(op.pane))
    ref pane = window.panes[op.pane]
    if op.kind == 14:
        pane.title = op.name
        return
    if op.kind == 15:
        pane.kind = op.pane_kind.copy()
        pane.synchronized_input = op.synchronized_input
        pane.bell = op.bell
        pane.dead = op.dead
        pane.dead_status = op.dead_status
        return
    if op.kind == 16:
        pane.border_colour = op.border_colour
        pane.active_border_colour = op.active_border_colour
        pane.border_status_text = op.name
        pane.mode_kind = op.mode_kind
        pane.mode_bytes = op.mode_bytes.copy()
        return
    if op.kind == 17:
        pane.status = op.status.copy()
        return
    if op.kind == 18:
        pane.summary = op.summary.copy()
        return
    raise Error("unknown tree operation")
