from .tree_model import MuxSnapshot, SessionSnapshot, PaneSnapshot
from .palette_navigation_projection import (
    palette_ordered_panes,
    palette_pane_label,
    palette_session_place,
    _last_component,
    _pane_index,
)
from .palette_programs import PalettePrograms
from .palette_text import palette_trim
from .stream_protocol import HostPlatform
from .exec_protocol import Invocation
from .desktop_nav import (
    NavTarget,
    NavActivation,
    activation_for_target,
    rename_prompt_command,
)
from .desktop_nav_tree import (
    NavNode,
    NavTreeHost,
    NavTreeSession,
    NavTreeWindow,
    NAV_TREE_NODES,
    NAV_TREE_BYTES,
)
from .desktop_host import HostState, host_activation

comptime FLEET_NODE_BYTES = 128


@fieldwise_init
struct FleetPane(Copyable, Equatable, Movable):
    var id: UInt64
    var index: UInt32
    var label: String
    var cwd: String
    var kind: UInt8
    var provider: UInt8
    var bell: Bool
    var status: Optional[UInt8]
    var program: Optional[Int]

    def __eq__(self, other: Self) -> Bool:
        return (
            self.id == other.id
            and self.index == other.index
            and self.label == other.label
            and self.cwd == other.cwd
            and self.kind == other.kind
            and self.provider == other.provider
            and self.bell == other.bell
            and self.status == other.status
            and self.program == other.program
        )


@fieldwise_init
struct FleetWindow(Copyable, Equatable, Movable):
    var id: UInt64
    var index: UInt32
    var name: String
    var active_pane: UInt64
    var active: Bool
    var panes: List[FleetPane]

    def __eq__(self, other: Self) -> Bool:
        return (
            self.id == other.id
            and self.index == other.index
            and self.name == other.name
            and self.active_pane == other.active_pane
            and self.active == other.active
            and self.panes == other.panes
        )

    def program(self) -> Optional[Int]:
        for pane in self.panes:
            if pane.id == self.active_pane:
                return pane.program
        return None

    def has_pending_bell(self) -> Bool:
        for pane in self.panes:
            if pane.bell:
                return True
        return False


@fieldwise_init
struct FleetSession(Copyable, Equatable, Movable):
    var id: UInt64
    var name: String
    var label: String
    var label_index: Optional[String]
    var active: Bool
    var windows: List[FleetWindow]

    def __eq__(self, other: Self) -> Bool:
        return (
            self.id == other.id
            and self.name == other.name
            and self.label == other.label
            and self.label_index == other.label_index
            and self.active == other.active
            and self.windows == other.windows
        )

    def has_pending_bell(self) -> Bool:
        for window in self.windows:
            if window.has_pending_bell():
                return True
        return False


@fieldwise_init
struct FleetHost(Copyable, Equatable, Movable):
    var id: UInt32
    var name: String
    var state: HostState
    var platform: HostPlatform
    var loaded: Bool
    var sessions: List[FleetSession]

    def __eq__(self, other: Self) -> Bool:
        return (
            self.id == other.id
            and self.name == other.name
            and self.state == other.state
            and self.loaded == other.loaded
            and self.sessions == other.sessions
        )

    def connected(self) -> Bool:
        return self.state.connected()


struct FleetHostInput(Copyable, Movable):
    var id: UInt32
    var name: String
    var state: HostState
    var platform: HostPlatform
    var snapshot: Optional[MuxSnapshot]

    def __init__(
        out self,
        id: UInt32,
        var name: String,
        var state: HostState,
        platform: HostPlatform,
        var snapshot: Optional[MuxSnapshot],
    ):
        self.id = id
        self.name = name^
        self.state = state^
        self.platform = platform
        self.snapshot = snapshot^


struct _Budget:
    var nodes: Int
    var bytes: Int

    def __init__(out self):
        self.nodes = 0
        self.bytes = 0

    def take(mut self, nodes: Int, bytes: Int) raises:
        if nodes > NAV_TREE_NODES - self.nodes:
            raise Error("desktop fleet exceeds node limit")
        if bytes > NAV_TREE_BYTES - self.bytes - nodes * FLEET_NODE_BYTES:
            raise Error("desktop fleet exceeds byte limit")
        self.nodes += nodes
        self.bytes += bytes + nodes * FLEET_NODE_BYTES


def fleet_session_label(
    session: SessionSnapshot,
) raises -> Tuple[String, Optional[String]]:
    var name = palette_trim(session.name)
    var numbered = True
    for byte in name.as_bytes():
        numbered = numbered and byte >= 48 and byte <= 57
    if numbered:
        var place = palette_session_place(session)
        if place:
            var index: Optional[String] = None
            if name.byte_length() > 0:
                index = name.copy()
            return _last_component(place.value()), index^
    if name.byte_length() == 0:
        return String("session $", session.id), Optional[String](None)
    return session.name.copy(), Optional[String](None)


def fleet_pane_program(
    pane: PaneSnapshot, programs: PalettePrograms
) raises -> Optional[Int]:
    if pane.kind.kind == 1:
        return programs.resolve(pane.summary.process)
    if pane.kind.kind == 4:
        return programs.by_id("claude" if pane.kind.provider == 1 else "codex")
    return None


def fleet_pane_heading(
    pane: PaneSnapshot, programs: PalettePrograms
) raises -> String:
    if pane.kind.kind != 1:
        return palette_pane_label(pane)
    var program = programs.resolve(pane.summary.process)
    if program:
        return programs.values[program.value()].name.copy()
    if len(pane.summary.process) > 0:
        return _last_component(pane.summary.process[0])
    if pane.summary.shell.byte_length() > 0:
        return pane.summary.shell.copy()
    return palette_pane_label(pane)


def _active_for_session(
    previous: Optional[NavNode],
    host: UInt32,
    attached_host: UInt32,
    session: SessionSnapshot,
    attached: Optional[UInt64],
    focused: UInt64,
) -> Optional[NavNode]:
    if host != attached_host or attached != Optional[UInt64](session.id):
        return previous
    var result = previous
    var first: Optional[UInt64] = None
    for window in session.windows:
        if window.id != focused:
            continue
        if not first:
            first = window.id
        for pane in window.panes.values():
            if pane.id == window.active_pane:
                result = NavNode(host, 3, window.active_pane)
    if result:
        return result
    if first:
        return NavNode(host, 2, first.value())
    return NavNode(host, 1, session.id)


def _fleet_sessions(
    mut active_target: Optional[NavNode],
    host: UInt32,
    attached_host: UInt32,
    attached: Optional[UInt64],
    snapshot: MuxSnapshot,
    programs: PalettePrograms,
    mut budget: _Budget,
) raises -> List[FleetSession]:
    var result = List[FleetSession]()
    for session in snapshot.sessions:
        budget.take(1, session.name.byte_length())
        var active = host == attached_host and attached == Optional[UInt64](
            session.id
        )
        var focused = snapshot.focused_window_for(session)
        active_target = _active_for_session(
            active_target, host, attached_host, session, attached, focused
        )
        var windows = List[FleetWindow]()
        for window in session.windows:
            budget.take(1 + len(window.panes), window.name.byte_length())
            if len(window.pane_order) > NAV_TREE_NODES:
                raise Error("desktop fleet pane order exceeds node limit")
            var positions = Dict[UInt64, Int]()
            for position in range(len(window.pane_order)):
                if window.pane_order[position] not in positions:
                    positions[window.pane_order[position]] = position
            var ordered = palette_ordered_panes(window)
            var panes = List[FleetPane](capacity=len(ordered))
            for position in range(len(ordered)):
                var id = ordered[position]
                ref pane = window.panes[id]
                var label = fleet_pane_heading(pane, programs)
                budget.take(
                    0, label.byte_length() + pane.summary.cwd.byte_length()
                )
                var status: Optional[UInt8] = None
                if pane.status:
                    status = pane.status.value().state
                panes.append(
                    FleetPane(
                        pane.id,
                        _pane_index(
                            positions.get(id, position), window.pane_base_index
                        ),
                        label^,
                        pane.summary.cwd.copy(),
                        pane.kind.kind,
                        pane.kind.provider if pane.kind.kind == 4 else UInt8(0),
                        pane.bell,
                        status,
                        fleet_pane_program(pane, programs),
                    )
                )
            windows.append(
                FleetWindow(
                    window.id,
                    window.index,
                    window.name.copy(),
                    window.active_pane,
                    active and window.id == focused,
                    panes^,
                )
            )
        var label = fleet_session_label(session)
        budget.take(
            0,
            label[0].byte_length()
            + (label[1].value().byte_length() if label[1] else 0),
        )
        result.append(
            FleetSession(
                session.id,
                session.name.copy(),
                label[0].copy(),
                label[1].copy(),
                active,
                windows^,
            )
        )
    return result^


struct FleetModel(Copyable, Equatable, Movable):
    var hosts: List[FleetHost]
    var active_target: Optional[NavNode]

    def __init__(out self):
        self.hosts = List[FleetHost]()
        self.active_target = None

    def __init__(
        out self,
        attached_host: UInt32,
        attached: Optional[UInt64],
        hosts: Span[FleetHostInput, _],
        programs: PalettePrograms,
    ) raises:
        self.hosts = List[FleetHost]()
        self.active_target = None
        var budget = _Budget()
        for host in hosts:
            host.state.validate()
            if host.platform.kind > 3 or host.platform.distro > 4:
                raise Error("desktop host platform is invalid")
            budget.take(1, host.name.byte_length())
            var sessions = List[FleetSession]()
            if host.snapshot:
                sessions = _fleet_sessions(
                    self.active_target,
                    host.id,
                    attached_host,
                    attached,
                    host.snapshot.value(),
                    programs,
                    budget,
                )
            self.hosts.append(
                FleetHost(
                    host.id,
                    host.name.copy(),
                    host.state.copy(),
                    host.platform,
                    Bool(host.snapshot),
                    sessions^,
                )
            )

    def __eq__(self, other: Self) -> Bool:
        return (
            self.hosts == other.hosts
            and self.active_target == other.active_target
        )

    def host(self, id: UInt32) -> Optional[Int]:
        for index in range(len(self.hosts)):
            if self.hosts[index].id == id:
                return index
        return None

    def tree_hosts(self) -> List[NavTreeHost]:
        var result = List[NavTreeHost](capacity=len(self.hosts))
        for host in self.hosts:
            var sessions = List[NavTreeSession](capacity=len(host.sessions))
            for session in host.sessions:
                var windows = List[NavTreeWindow](capacity=len(session.windows))
                for window in session.windows:
                    var panes = List[UInt64](capacity=len(window.panes))
                    for pane in window.panes:
                        panes.append(pane.id)
                    windows.append(
                        NavTreeWindow(
                            window.id,
                            window.active,
                            window.active_pane,
                            window.name.copy(),
                            panes^,
                        )
                    )
                sessions.append(
                    NavTreeSession(
                        session.id,
                        session.active,
                        session.name.copy(),
                        windows^,
                    )
                )
            result.append(NavTreeHost(host.id, host.connected(), sessions^))
        return result^

    def expandable(self, node: NavNode) raises -> Bool:
        var index = self.host(node.host)
        if not index:
            return False
        if node.kind == 0:
            return True
        return fleet_expandable(
            Span(self.hosts[index.value()].sessions), _target(node)
        )

    def pending_bell(self, node: NavNode) raises -> Bool:
        var index = self.host(node.host)
        if not index:
            return False
        var target: Optional[NavTarget] = None
        if node.kind != 0:
            target = _target(node)
        return fleet_pending_bell(
            Span(self.hosts[index.value()].sessions), target
        )

    def activation(
        self,
        node: NavNode,
        attached_host: UInt32,
        attached_session: Optional[UInt64],
    ) raises -> Optional[NavActivation]:
        var index = self.host(node.host)
        if not index:
            return None
        ref host = self.hosts[index.value()]
        return fleet_activation(
            node,
            host.state,
            Span(host.sessions),
            attached_host,
            attached_session,
        )

    def rename_activation(
        self, node: NavNode, attached_host: UInt32
    ) raises -> Optional[Tuple[String, NavActivation]]:
        if node.kind == 0:
            return None
        var index = self.host(node.host)
        if not index:
            return None
        ref host = self.hosts[index.value()]
        return fleet_rename_activation(
            node.host,
            host.state,
            Span(host.sessions),
            _target(node),
            attached_host,
        )

    def shortcut_windows(self) -> List[Tuple[UInt32, UInt64]]:
        var result = List[Tuple[UInt32, UInt64]]()
        for host in self.hosts:
            if not host.connected():
                continue
            for session in host.sessions:
                for window in session.windows:
                    if len(result) == 9:
                        return result^
                    result.append((host.id, window.id))
        return result^

    def window_shortcut(
        self,
        position: Int,
        attached_host: UInt32,
        attached_session: Optional[UInt64],
    ) raises -> Optional[NavActivation]:
        var windows = self.shortcut_windows()
        if position < 0 or position >= len(windows):
            return None
        return self.activation(
            NavNode(windows[position][0], 2, windows[position][1]),
            attached_host,
            attached_session,
        )


def _target(node: NavNode) raises -> NavTarget:
    if node.kind == 0 or node.kind > 3:
        raise Error("desktop fleet node has no target")
    return NavTarget(node.kind - 1, node.id)


def fleet_target_owner(
    sessions: Span[FleetSession, _], target: NavTarget
) raises -> Optional[Tuple[UInt64, UInt64]]:
    if target.kind > 2:
        raise Error("desktop fleet target identity is invalid")
    if target.kind == 0:
        return None
    for session in sessions:
        for window in session.windows:
            var matched = target.kind == 1 and window.id == target.id
            if target.kind == 2:
                for pane in window.panes:
                    matched = matched or pane.id == target.id
            if matched:
                return (session.id, window.id)
    return None


def fleet_session_for_target(
    sessions: Span[FleetSession, _], target: NavTarget
) raises -> Optional[UInt64]:
    if target.kind == 0:
        return target.id
    var owner = fleet_target_owner(sessions, target)
    if owner:
        return owner.value()[0]
    return None


def fleet_renameable_name(
    sessions: Span[FleetSession, _], target: NavTarget
) raises -> Optional[String]:
    if target.kind > 2:
        raise Error("desktop fleet target identity is invalid")
    if target.kind == 2:
        return None
    for session in sessions:
        if target.kind == 0 and session.id == target.id:
            return session.name.copy()
        if target.kind == 1:
            for window in session.windows:
                if window.id == target.id:
                    return window.name.copy()
    return None


def fleet_expandable(
    sessions: Span[FleetSession, _], target: NavTarget
) raises -> Bool:
    if target.kind > 2:
        raise Error("desktop fleet target identity is invalid")
    if target.kind == 2:
        return False
    for session in sessions:
        if target.kind == 0 and session.id == target.id:
            return len(session.windows) != 0
        if target.kind == 1:
            for window in session.windows:
                if window.id == target.id:
                    return len(window.panes) != 0
    return False


def fleet_rename_target(
    sessions: Span[FleetSession, _], target: NavTarget
) raises -> Optional[NavTarget]:
    if target.kind < 2:
        return target
    var owner = fleet_target_owner(sessions, target)
    if owner:
        return NavTarget(1, owner.value()[1])
    return None


def fleet_rename_activation(
    host: UInt32,
    state: HostState,
    sessions: Span[FleetSession, _],
    target: NavTarget,
    attached_host: UInt32,
) raises -> Optional[Tuple[String, NavActivation]]:
    var renamed = fleet_rename_target(sessions, target)
    if not renamed or not state.connected():
        return None
    var name = fleet_renameable_name(sessions, renamed.value())
    if not name:
        return None
    var prompt = rename_prompt_command(renamed.value(), name.value())
    var commands = List[Invocation]()
    commands.append(prompt[1].copy())
    if host == attached_host:
        return (
            prompt[0].copy(),
            NavActivation(3, host, None, String(), commands^),
        )
    var session = fleet_session_for_target(sessions, renamed.value())
    if not session:
        return None
    return (
        prompt[0].copy(),
        NavActivation(4, host, session, String(), commands^),
    )


def fleet_activation(
    node: NavNode,
    state: HostState,
    sessions: Span[FleetSession, _],
    attached_host: UInt32,
    attached_session: Optional[UInt64],
) raises -> Optional[NavActivation]:
    if node.kind == 0:
        return host_activation(node.host, attached_host, state)
    var target = _target(node)
    var owner = fleet_target_owner(sessions, target)
    var window: Optional[UInt64] = None
    if owner:
        window = owner.value()[1]
    return activation_for_target(
        node.host,
        target,
        fleet_session_for_target(sessions, target),
        window,
        attached_host,
        attached_session,
        state.connected(),
    )


def fleet_pending_bell(
    sessions: Span[FleetSession, _], target: Optional[NavTarget]
) raises -> Bool:
    if not target:
        for session in sessions:
            if session.has_pending_bell():
                return True
        return False
    var value = target.value()
    if value.kind > 2:
        raise Error("desktop fleet target identity is invalid")
    for session in sessions:
        if value.kind == 0 and session.id == value.id:
            return session.has_pending_bell()
        for window in session.windows:
            if value.kind == 1 and window.id == value.id:
                return window.has_pending_bell()
            for pane in window.panes:
                if value.kind == 2 and pane.id == value.id:
                    return pane.bell
    return False
