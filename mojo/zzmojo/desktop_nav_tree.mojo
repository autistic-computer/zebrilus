from .desktop_nav import NavTarget

comptime NAV_TREE_NODES = 65536
comptime NAV_TREE_BYTES = 16 * 1024 * 1024


@fieldwise_init
struct NavNode(Equatable, TrivialRegisterPassable):
    var host: UInt32
    var kind: UInt8
    var id: UInt64

    def __eq__(self, other: Self) -> Bool:
        return (
            self.host == other.host
            and self.kind == other.kind
            and self.id == other.id
        )

    def key(self) -> Tuple[UInt32, UInt8, UInt64]:
        return self.host, self.kind, self.id


def validate_node(node: NavNode) raises:
    if node.kind > 3 or node.kind == 0 and node.id != 0:
        raise Error("desktop tree node identity is invalid")


@fieldwise_init
struct NavTreeWindow(Copyable, Movable):
    var id: UInt64
    var active: Bool
    var active_pane: UInt64
    var name: String
    var panes: List[UInt64]


@fieldwise_init
struct NavTreeSession(Copyable, Movable):
    var id: UInt64
    var active: Bool
    var name: String
    var windows: List[NavTreeWindow]


@fieldwise_init
struct NavTreeHost(Copyable, Movable):
    var id: UInt32
    var connected: Bool
    var sessions: List[NavTreeSession]


def validate_fleet(hosts: Span[NavTreeHost, _]) raises:
    var count = 0
    var size = 0
    for host in hosts:
        count += 1
        size += 128
        for session in host.sessions:
            count += 1
            size += 128 + session.name.byte_length()
            for window in session.windows:
                if len(window.panes) > NAV_TREE_NODES - count:
                    raise Error("desktop fleet exceeds node limit")
                count += 1 + len(window.panes)
                size += (
                    128 * (1 + len(window.panes)) + window.name.byte_length()
                )
                if count > NAV_TREE_NODES or size > NAV_TREE_BYTES:
                    raise Error("desktop fleet exceeds admission budget")
        if count > NAV_TREE_NODES or size > NAV_TREE_BYTES:
            raise Error("desktop fleet exceeds admission budget")


def target_owner(
    host: NavTreeHost, target: NavTarget
) raises -> Optional[Tuple[UInt64, UInt64]]:
    if target.kind > 2:
        raise Error("desktop tree target identity is invalid")
    if target.kind == 0:
        return None
    for session in host.sessions:
        for window in session.windows:
            var matched = target.kind == 1 and window.id == target.id
            if target.kind == 2:
                for pane in window.panes:
                    matched = matched or pane == target.id
            if matched:
                return (session.id, window.id)
    return None


def renameable_name(
    host: NavTreeHost, target: NavTarget
) raises -> Optional[String]:
    if target.kind > 2:
        raise Error("desktop tree target identity is invalid")
    if target.kind == 2:
        return None
    for session in host.sessions:
        if target.kind == 0 and session.id == target.id:
            return session.name.copy()
        if target.kind == 1:
            for window in session.windows:
                if window.id == target.id:
                    return window.name.copy()
    return None


def target_expandable(host: NavTreeHost, target: NavTarget) raises -> Bool:
    if target.kind > 2:
        raise Error("desktop tree target identity is invalid")
    if target.kind == 2:
        return False
    for session in host.sessions:
        if target.kind == 0 and session.id == target.id:
            return len(session.windows) != 0
        if target.kind == 1:
            for window in session.windows:
                if window.id == target.id:
                    return len(window.panes) != 0
    return False


def shortcut_windows(
    hosts: Span[NavTreeHost, _]
) -> List[Tuple[UInt32, UInt64]]:
    var result = List[Tuple[UInt32, UInt64]]()
    for host in hosts:
        if not host.connected:
            continue
        for session in host.sessions:
            for window in session.windows:
                if len(result) == 9:
                    return result^
                result.append((host.id, window.id))
    return result^


def active_for_session(
    previous: Optional[NavNode],
    host: UInt32,
    attached_host: UInt32,
    session: NavTreeSession,
    attached_session: Optional[UInt64],
    focused: UInt64,
) -> Optional[NavNode]:
    if host != attached_host or attached_session != Optional[UInt64](
        session.id
    ):
        return previous
    var result = previous
    var first: Optional[UInt64] = None
    for window in session.windows:
        if window.id != focused:
            continue
        if not first:
            first = window.id
        for pane in window.panes:
            if pane == window.active_pane:
                result = NavNode(host, 3, pane)
    if result:
        return result
    if first:
        return NavNode(host, 2, first.value())
    return NavNode(host, 1, session.id)


@fieldwise_init
struct NavVisibleEntry(Copyable, Movable):
    var node: NavNode
    var parent: Optional[NavNode]
    var host_index: Int
    var session_index: Int
    var window_index: Int
    var pane_index: Int
    var depth: UInt8
    var expandable: Bool
    var expanded: Bool
    var on_active_path: Bool


struct NavProjection(Copyable, Movable):
    var entries: List[NavVisibleEntry]
    var visible_indices: Dict[Tuple[UInt32, UInt8, UInt64], Int]
    var expandable: Dict[Tuple[UInt32, UInt8, UInt64], Bool]

    def __init__(out self):
        self.entries = List[NavVisibleEntry]()
        self.visible_indices = Dict[Tuple[UInt32, UInt8, UInt64], Int]()
        self.expandable = Dict[Tuple[UInt32, UInt8, UInt64], Bool]()

    def push(mut self, entry: NavVisibleEntry):
        self.visible_indices[entry.node.key()] = len(self.entries)
        self.entries.append(entry.copy())


def project_tree(
    hosts: Span[NavTreeHost, _],
    expanded: Dict[Tuple[UInt32, UInt8, UInt64], Bool],
) raises -> NavProjection:
    validate_fleet(hosts)
    if len(expanded) > NAV_TREE_NODES:
        raise Error("desktop expanded-node set exceeds admission budget")
    for key in expanded:
        validate_node(NavNode(key[0], key[1], key[2]))
    var result = NavProjection()
    for host in hosts:
        result.expandable[NavNode(host.id, 0, 0).key()] = True
        for session in host.sessions:
            if len(session.windows) != 0:
                result.expandable[NavNode(host.id, 1, session.id).key()] = True
            for window in session.windows:
                if len(window.panes) != 0:
                    result.expandable[
                        NavNode(host.id, 2, window.id).key()
                    ] = True
    for hi in range(len(hosts)):
        var host_node = NavNode(hosts[hi].id, 0, 0)
        var host_expanded = host_node.key() in expanded
        result.push(
            NavVisibleEntry(
                host_node, None, hi, -1, -1, -1, 0, True, host_expanded, False
            )
        )
        if not host_expanded:
            continue
        for si in range(len(hosts[hi].sessions)):
            ref session = hosts[hi].sessions[si]
            var session_node = NavNode(host_node.host, 1, session.id)
            var can_expand = len(session.windows) != 0
            var is_expanded = can_expand and session_node.key() in expanded
            result.push(
                NavVisibleEntry(
                    session_node,
                    host_node,
                    hi,
                    si,
                    -1,
                    -1,
                    1,
                    can_expand,
                    is_expanded,
                    session.active,
                )
            )
            if not is_expanded:
                continue
            for wi in range(len(session.windows)):
                ref window = session.windows[wi]
                var window_node = NavNode(host_node.host, 2, window.id)
                can_expand = len(window.panes) != 0
                is_expanded = can_expand and window_node.key() in expanded
                result.push(
                    NavVisibleEntry(
                        window_node,
                        session_node,
                        hi,
                        si,
                        wi,
                        -1,
                        2,
                        can_expand,
                        is_expanded,
                        window.active,
                    )
                )
                if not is_expanded:
                    continue
                for pi in range(len(window.panes)):
                    var pane = window.panes[pi]
                    result.push(
                        NavVisibleEntry(
                            NavNode(host_node.host, 3, pane),
                            window_node,
                            hi,
                            si,
                            wi,
                            pi,
                            3,
                            False,
                            False,
                            window.active and pane == window.active_pane,
                        )
                    )
    return result^


def tree_navigation(
    projection: NavProjection, selected: Optional[Int], direction: UInt8
) raises -> Tuple[UInt8, Int]:
    if direction > 5:
        raise Error("desktop tree direction is invalid")
    if len(projection.entries) == 0 or (not selected and direction >= 4):
        return 0, 0
    var index = min(selected.or_else(0), len(projection.entries) - 1)
    if index < 0:
        raise Error("desktop tree selected index is invalid")
    var row = projection.entries[index].copy()
    if direction == 0:
        return 1, max(index - 1, 0)
    if direction == 1:
        return 1, min(index + 1, len(projection.entries) - 1)
    if direction == 2:
        return 1, 0
    if direction == 3:
        return 1, len(projection.entries) - 1
    if direction == 4:
        if row.expandable and row.expanded:
            return 2, index
        if (
            row.parent
            and row.parent.value().key() in projection.visible_indices
        ):
            return 1, projection.visible_indices[row.parent.value().key()]
    else:
        if row.expandable and not row.expanded:
            return 2, index
        if (
            index + 1 < len(projection.entries)
            and projection.entries[index + 1].depth == row.depth + 1
        ):
            return 1, index + 1
    return 0, 0


def expand_path(
    mut expanded: Dict[Tuple[UInt32, UInt8, UInt64], Bool],
    hosts: Span[NavTreeHost, _],
    node: NavNode,
) raises:
    validate_node(node)
    if node.kind == 0:
        return
    for host in hosts:
        if host.id != node.host:
            continue
        expanded[NavNode(host.id, 0, 0).key()] = True
        var owner = target_owner(host, NavTarget(node.kind - 1, node.id))
        if owner:
            expanded[NavNode(host.id, 1, owner.value()[0]).key()] = True
            if node.kind == 3:
                expanded[NavNode(host.id, 2, owner.value()[1]).key()] = True
        return


def node_expandable(hosts: Span[NavTreeHost, _], node: NavNode) raises -> Bool:
    validate_node(node)
    for host in hosts:
        if host.id == node.host:
            return node.kind == 0 or target_expandable(
                host, NavTarget(node.kind - 1, node.id)
            )
    return False


struct SidebarTreeState(Copyable, Movable):
    var expanded: Dict[Tuple[UInt32, UInt8, UInt64], Bool]
    var selected: Optional[NavNode]
    var selection_from_pointer: Bool
    var active_target: Optional[NavNode]
    var projection: NavProjection

    def __init__(out self):
        self.expanded = Dict[Tuple[UInt32, UInt8, UInt64], Bool]()
        self.selected = NavNode(0, 0, 0)
        self.selection_from_pointer = False
        self.active_target = None
        self.projection = NavProjection()

    def _rebuild(mut self, hosts: Span[NavTreeHost, _]) raises:
        var projection = project_tree(hosts, self.expanded)
        var retained = Dict[Tuple[UInt32, UInt8, UInt64], Bool]()
        for key in self.expanded:
            if key in projection.expandable:
                retained[key] = True
        if (
            self.selected
            and self.selected.value().key() not in projection.visible_indices
        ):
            self.selected = None
            if len(hosts) != 0:
                self.selected = NavNode(hosts[0].id, 0, 0)
        self.expanded = retained^
        self.projection = projection^

    def reconcile(
        mut self,
        previous: Span[UInt32, _],
        hosts: Span[NavTreeHost, _],
        active: Optional[NavNode],
    ) raises -> Optional[Int]:
        validate_fleet(hosts)
        if active:
            validate_node(active.value())
        if len(previous) > NAV_TREE_NODES:
            raise Error("desktop previous-host set exceeds admission budget")
        var next = self.copy()
        var had_local = False
        for host in previous:
            had_local = had_local or host == 0
        if not had_local:
            for host in hosts:
                if host.id == 0:
                    next.expanded[NavNode(0, 0, 0).key()] = True
        next.active_target = active
        if self.active_target != active and active:
            expand_path(next.expanded, hosts, active.value())
            next.selected = active
        next._rebuild(hosts)
        var scroll: Optional[Int] = None
        if (
            self.active_target != active
            and active
            and active.value().key() in next.projection.visible_indices
        ):
            scroll = next.projection.visible_indices[active.value().key()]
        self = next^
        return scroll

    def toggle(
        mut self, hosts: Span[NavTreeHost, _], node: NavNode
    ) raises -> Bool:
        validate_fleet(hosts)
        if not node_expandable(hosts, node):
            return False
        var next = self.copy()
        if node.key() in next.expanded:
            _ = next.expanded.pop(node.key())
        else:
            next.expanded[node.key()] = True
        next.selected = node
        next._rebuild(hosts)
        self = next^
        return True

    def select_pointer(mut self, node: NavNode) raises:
        validate_node(node)
        self.selection_from_pointer = True
        self.selected = node

    def select_or_toggle(
        mut self, hosts: Span[NavTreeHost, _], node: NavNode
    ) raises:
        validate_fleet(hosts)
        var next = self.copy()
        next.selection_from_pointer = True
        if not next.toggle(hosts, node):
            next.selected = node
        self = next^

    def select_index(mut self, index: Int) -> Optional[Int]:
        if index < 0 or index >= len(self.projection.entries):
            return None
        self.selection_from_pointer = False
        self.selected = self.projection.entries[index].node
        return index

    def navigate(
        mut self, hosts: Span[NavTreeHost, _], direction: UInt8
    ) raises -> Optional[Int]:
        validate_fleet(hosts)
        var selected: Optional[Int] = None
        if (
            self.selected
            and self.selected.value().key() in self.projection.visible_indices
        ):
            selected = self.projection.visible_indices[
                self.selected.value().key()
            ]
        var decision = tree_navigation(self.projection, selected, direction)
        if decision[0] == 1:
            return self.select_index(decision[1])
        if decision[0] == 2:
            var next = self.copy()
            next.selection_from_pointer = False
            _ = next.toggle(hosts, next.projection.entries[decision[1]].node)
            self = next^
        return None

    def focus(mut self, hosts: Span[NavTreeHost, _]) raises -> Optional[Int]:
        validate_fleet(hosts)
        if not self.active_target:
            return None
        var next = self.copy()
        var node = self.active_target.value()
        expand_path(next.expanded, hosts, node)
        next.selection_from_pointer = False
        next.selected = node
        next._rebuild(hosts)
        var scroll: Optional[Int] = None
        if node.key() in next.projection.visible_indices:
            scroll = next.projection.visible_indices[node.key()]
        self = next^
        return scroll
