from .actions import Action, ActionQueue, ActionBatch
from .exec_protocol import Invocation
from .palette_programs import PalettePrograms
from .palette_protocol import palette_command_request
from .stream_protocol import HostPlatform, attach_session
from .desktop_snapshot_target import desktop_active_pane
from .desktop_chrome_dispatch import DesktopChromeDispatch
from .desktop_host import HostIndicator, host_indicator
from .desktop_nav import (
    NavActivation,
    nav_command,
    new_session_commands,
    new_window_command,
    picker_split_command,
    detach_command,
)
from .desktop_nav_tree import NavNode, NavTreeHost, SidebarTreeState
from .desktop_fleet import FleetModel, FleetHostInput

comptime NAV_PASSIVE = 10
comptime NAV_COMMAND = 16
comptime LOCAL_HOST: UInt32 = 0


@fieldwise_init
struct DesktopSidebarRow(Copyable, Movable):
    var node: NavNode
    var depth: UInt8
    var label: String
    var pane_kind: UInt8
    var provider: UInt8
    var program: Optional[Int]
    var expandable: Bool
    var expanded: Bool
    var on_active_path: Bool
    var active: Bool
    var selected: Bool
    var connected: Bool
    var bell: Bool
    var shortcut: Optional[UInt8]
    var indicator: Optional[HostIndicator]
    var platform: HostPlatform
    var rename: Optional[String]
    var install_offer: Optional[UInt8]
    var removable: Bool


@fieldwise_init
struct DesktopNavIntent(Copyable, Movable):
    var activation: Optional[NavActivation]
    var release_focus: Bool
    var focus_pane: Bool
    var scroll: Optional[Int]

    @staticmethod
    def none() -> Self:
        return Self(None, False, False, None)


@fieldwise_init
struct DesktopNavSource(Equatable, TrivialRegisterPassable):
    var scope: UInt64
    var server: UInt64
    var client: UInt64
    var epoch: UInt64

    def __eq__(self, other: Self) -> Bool:
        return (
            self.scope == other.scope
            and self.server == other.server
            and self.client == other.client
            and self.epoch == other.epoch
        )


struct DesktopNavAdmission(Movable):
    var source: DesktopNavSource
    var batch: Optional[ActionBatch]
    var host_request: Optional[NavActivation]

    def __init__(
        out self,
        source: DesktopNavSource,
        var batch: Optional[ActionBatch],
        var host_request: Optional[NavActivation],
    ):
        self.source = source
        self.batch = batch^
        self.host_request = host_request^


def stage_navigation(
    queue: ActionQueue,
    activation: NavActivation,
    attached_host: UInt32,
    source: DesktopNavSource,
) raises -> DesktopNavAdmission:
    if activation.kind > 5:
        raise Error("desktop navigation activation kind is invalid")
    if (
        activation.host != attached_host
        or activation.kind == 0
        or activation.kind >= 4
    ):
        return DesktopNavAdmission(source, None, activation.copy())
    var actions = List[Action]()
    var owner = Optional[Tuple[UInt64, UInt64]]((source.server, source.client))
    if activation.kind == 1:
        if not activation.session:
            raise Error("desktop session activation has no session")
        actions.append(
            Action(
                NAV_PASSIVE,
                String(),
                None,
                source.epoch,
                None,
                attach_session(String("$", activation.session.value())),
                owner,
                None,
            )
        )
    else:
        var commands = List[Invocation]()
        if activation.kind == 2:
            commands.append(
                nav_command(
                    "attach-session", [String("-t"), activation.target.copy()]
                )
            )
        else:
            commands = activation.commands.copy()
        for command in commands:
            actions.append(
                Action(
                    NAV_COMMAND,
                    String(),
                    None,
                    source.epoch,
                    None,
                    palette_command_request(0, command),
                    owner,
                    None,
                )
            )
    return DesktopNavAdmission(source, queue.stage(actions^), None)


def commit_navigation(
    mut queue: ActionQueue,
    var admission: DesktopNavAdmission,
    current: DesktopNavSource,
) raises:
    if current != admission.source:
        raise Error("desktop navigation admission belongs to a retired source")
    if admission.batch:
        var batch = admission.batch.take()
        queue.commit_batch(batch)


struct DesktopSidebar(Movable):
    var programs: PalettePrograms
    var model: FleetModel
    var hosts: List[NavTreeHost]
    var tree: SidebarTreeState
    var attached_host: UInt32
    var attached_session: Optional[UInt64]
    var active_pane: Optional[UInt64]
    var shortcuts_shown: Bool
    var local_hostname: String
    var revision: UInt64

    def __init__(out self, var local_hostname: String) raises:
        self.programs = PalettePrograms()
        self.model = FleetModel()
        self.hosts = List[NavTreeHost]()
        self.tree = SidebarTreeState()
        self.attached_host = LOCAL_HOST
        self.attached_session = None
        self.active_pane = None
        self.shortcuts_shown = False
        self.local_hostname = local_hostname^
        self.revision = 0

    def _publish(mut self) raises:
        if self.revision == UInt64.MAX:
            raise Error("desktop sidebar revision exhausted")
        self.revision += 1

    def update(
        mut self,
        hosts: Span[FleetHostInput, _],
        attached_host: UInt32,
        attached_session: Optional[UInt64],
    ) raises -> Optional[Int]:
        var model = FleetModel(
            attached_host, attached_session, hosts, self.programs
        )
        var active_pane: Optional[UInt64] = None
        for host in hosts:
            if host.id == attached_host:
                if host.snapshot:
                    active_pane = desktop_active_pane(
                        host.snapshot.value(), attached_session
                    )
                break
        var changed = model != self.model
        var tree = self.tree.copy()
        var scroll: Optional[Int] = None
        var tree_hosts = List[NavTreeHost]()
        if changed:
            var previous = List[UInt32](capacity=len(self.model.hosts))
            for host in self.model.hosts:
                previous.append(host.id)
            tree_hosts = model.tree_hosts()
            scroll = tree.reconcile(
                Span(previous), Span(tree_hosts), model.active_target
            )
        self._publish()
        if changed:
            self.tree = tree^
            self.hosts = tree_hosts^
        self.model = model^
        self.attached_host = attached_host
        self.attached_session = attached_session
        self.active_pane = active_pane
        return scroll

    def show_window_shortcuts(mut self, shown: Bool) raises:
        if self.shortcuts_shown != shown:
            self._publish()
            self.shortcuts_shown = shown

    def row_count(self) -> Int:
        return len(self.tree.projection.entries)

    def rows(
        self, first: Int, count: Int, focused: Bool
    ) raises -> List[DesktopSidebarRow]:
        if first < 0 or count < 0:
            raise Error("desktop sidebar row range is invalid")
        var end = first + min(count, max(self.row_count() - first, 0))
        var shortcuts = Dict[Tuple[UInt32, UInt8, UInt64], UInt8]()
        if self.shortcuts_shown:
            var windows = self.model.shortcut_windows()
            for position in range(len(windows)):
                var key = NavNode(windows[position][0], 2, windows[position][1]).key()
                if key not in shortcuts:
                    shortcuts[key] = UInt8(position)
        var result = List[DesktopSidebarRow](capacity=max(end - first, 0))
        for index in range(first, end):
            ref entry = self.tree.projection.entries[index]
            ref host = self.model.hosts[entry.host_index]
            var node = entry.node
            var label: String
            var pane_kind = UInt8(0)
            var provider = UInt8(0)
            var program: Optional[Int] = None
            var indicator: Optional[HostIndicator] = None
            var install_offer: Optional[UInt8] = None
            if node.kind == 0:
                label = (
                    self.local_hostname.copy() if node.host
                    == LOCAL_HOST else host.name.copy()
                )
                indicator = host_indicator(host.state, host.loaded)
                if node.host != LOCAL_HOST:
                    install_offer = host.state.install_offer()
            elif node.kind == 1:
                label = host.sessions[entry.session_index].label.copy()
            elif node.kind == 2:
                ref window = host.sessions[entry.session_index].windows[
                    entry.window_index
                ]
                label = window.name.copy()
                if not entry.expanded:
                    program = window.program()
            else:
                ref pane = host.sessions[entry.session_index].windows[
                    entry.window_index
                ].panes[entry.pane_index]
                label = pane.label.copy()
                pane_kind = pane.kind
                provider = pane.provider
                program = pane.program
            var rename: Optional[String] = None
            var renamed = self.model.rename_activation(node, self.attached_host)
            if renamed:
                rename = renamed.value()[0].copy()
            var selected = (
                focused
                and not self.tree.selection_from_pointer
                and self.tree.selected == Optional[NavNode](node)
            )
            var shortcut: Optional[UInt8] = None
            if node.key() in shortcuts:
                shortcut = shortcuts[node.key()]
            var connected = False
            var owner = self.model.host(node.host)
            if owner:
                connected = self.model.hosts[owner.value()].connected()
            result.append(
                DesktopSidebarRow(
                    node,
                    entry.depth,
                    label^,
                    pane_kind,
                    provider,
                    program,
                    entry.expandable,
                    entry.expanded,
                    entry.on_active_path,
                    self.tree.active_target == Optional[NavNode](node),
                    selected,
                    connected,
                    self.model.pending_bell(node),
                    shortcut,
                    indicator^,
                    host.platform if node.kind == 0 else HostPlatform(0, 0),
                    rename^,
                    install_offer,
                    node.kind == 0 and node.host != LOCAL_HOST,
                )
            )
        return result^

    def toggle(mut self, node: NavNode) raises -> Bool:
        var changed = self.tree.toggle(Span(self.hosts), node)
        if changed:
            self._publish()
        return changed

    def navigate(mut self, direction: UInt8) raises -> Optional[Int]:
        var scroll = self.tree.navigate(Span(self.hosts), direction)
        self._publish()
        return scroll

    def focus(mut self) raises -> Optional[Int]:
        var scroll = self.tree.focus(Span(self.hosts))
        self._publish()
        return scroll

    def click(mut self, node: NavNode) raises -> DesktopNavIntent:
        var activation = self.model.activation(
            node, self.attached_host, self.attached_session
        )
        var tree = self.tree.copy()
        if activation:
            tree.select_pointer(node)
        else:
            tree.select_or_toggle(Span(self.hosts), node)
        self._publish()
        self.tree = tree^
        var release = Bool(activation)
        return DesktopNavIntent(activation^, release, False, None)

    def confirm(mut self) raises -> DesktopNavIntent:
        if not self.tree.selected:
            return DesktopNavIntent.none()
        var node = self.tree.selected.value()
        var activation = self.model.activation(
            node, self.attached_host, self.attached_session
        )
        if activation:
            return DesktopNavIntent(activation^, True, False, None)
        if self.model.expandable(node):
            var tree = self.tree.copy()
            tree.selection_from_pointer = False
            _ = tree.toggle(Span(self.hosts), node)
            self._publish()
            self.tree = tree^
        return DesktopNavIntent.none()

    def rename(self) raises -> DesktopNavIntent:
        if not self.tree.selected:
            return DesktopNavIntent.none()
        var renamed = self.model.rename_activation(
            self.tree.selected.value(), self.attached_host
        )
        if not renamed:
            return DesktopNavIntent.none()
        return DesktopNavIntent(renamed.value()[1].copy(), False, False, None)

    def window_shortcut(self, position: UInt8) raises -> DesktopNavIntent:
        if position > 8:
            raise Error("desktop window shortcut position is invalid")
        var activation = self.model.window_shortcut(
            Int(position), self.attached_host, self.attached_session
        )
        var focus = Bool(activation)
        return DesktopNavIntent(activation^, False, focus, None)

    def _execute(self, var commands: List[Invocation]) -> DesktopNavIntent:
        return DesktopNavIntent(
            NavActivation(3, self.attached_host, None, String(), commands^),
            False,
            False,
            None,
        )

    def new_session(
        self,
        capabilities: List[String],
        working_directory: Optional[String],
    ) raises -> DesktopNavIntent:
        var index = self.model.host(self.attached_host)
        if not index:
            return DesktopNavIntent.none()
        ref host = self.model.hosts[index.value()]
        var directory: Optional[String] = None
        if (
            self.attached_host == LOCAL_HOST
            and host.loaded
            and len(host.sessions) == 0
        ):
            directory = working_directory.copy()
        return self._execute(
            new_session_commands(
                self.attached_host == LOCAL_HOST, capabilities, directory
            )
        )

    def new_window(self) raises -> DesktopNavIntent:
        if not self.attached_session:
            return DesktopNavIntent.none()
        var commands = List[Invocation]()
        commands.append(new_window_command(self.attached_session.value()))
        return self._execute(commands^)

    def split(self, axis: UInt8) raises -> DesktopNavIntent:
        if axis > 1:
            raise Error("desktop split axis is invalid")
        if not self.active_pane:
            return DesktopNavIntent.none()
        var commands = List[Invocation]()
        commands.append(picker_split_command(self.active_pane.value(), axis))
        return self._execute(commands^)

    def detach(self) raises -> DesktopNavIntent:
        var commands = List[Invocation]()
        commands.append(detach_command())
        return self._execute(commands^)

    def chrome_intent(
        self,
        dispatch: DesktopChromeDispatch,
        capabilities: List[String],
        working_directory: Optional[String],
    ) raises -> Optional[DesktopNavIntent]:
        if dispatch.kind == 1:
            return self.new_session(capabilities, working_directory)
        if dispatch.kind == 2:
            return self.new_window()
        if dispatch.kind == 3:
            return self.window_shortcut(dispatch.index)
        if dispatch.kind == 4 or dispatch.kind == 5:
            return self.split(dispatch.kind - 4)
        if dispatch.kind == 6:
            return self.detach()
        return None
