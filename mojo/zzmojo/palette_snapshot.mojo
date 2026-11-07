from .tree_model import MuxSnapshot
from .palette_places_model import (
    PaletteFleet,
    PaletteFleetHost,
    PaletteFleetSession,
    PaletteFleetWindow,
    PaletteFleetPane,
    PalettePlace,
    MAX_FLEET_NODES,
    MAX_FLEET_BYTES,
)
from .palette_places_search import place_key
from .palette_presentation import (
    PaletteDecorations,
    PaletteDecoration,
    palette_count,
)
from .palette_navigation_projection import (
    palette_ordered_panes,
    palette_pane_index,
    palette_pane_label,
    palette_session_label,
)
from .palette_programs import PalettePrograms
from .palette_text import palette_trim


@fieldwise_init
struct PaletteProjection(Copyable, Movable):
    var fleet: PaletteFleet
    var decorations: PaletteDecorations


@fieldwise_init
struct _ProjectionBudget(TrivialRegisterPassable):
    var nodes: Int
    var bytes: Int

    def add(mut self, nodes: Int, bytes: Int) raises:
        if (
            nodes > MAX_FLEET_NODES - self.nodes
            or bytes > MAX_FLEET_BYTES - self.bytes
        ):
            raise Error("palette snapshot exceeds its node or text limit")
        self.nodes += nodes
        self.bytes += bytes


def palette_hostname(value: Optional[String]) -> String:
    if value:
        var name = palette_trim(value.value())
        if name.byte_length() > 0:
            return name^
    return String("localhost")


def project_palette_snapshot(
    snapshot: MuxSnapshot,
    attached: Optional[UInt64],
    hostname: String,
    programs: PalettePrograms,
    attention: Dict[UInt64, UInt8] = Dict[UInt64, UInt8](),
    online: Bool = True,
    apple: Bool = False,
    host_id: UInt32 = 0,
) raises -> PaletteProjection:
    var fleet = PaletteFleet()
    fleet.here = host_id
    var decorations = PaletteDecorations()
    var budget = _ProjectionBudget(0, 0)
    var host_name = palette_hostname(hostname)
    var host_decoration = PaletteDecoration()
    host_decoration.detail = "This Mac" if apple else "This computer"
    host_decoration.right = palette_count(
        len(snapshot.sessions), "session"
    ) if online else String("offline · ↵ to connect")
    host_decoration.status = UInt8(0 if online else 3)
    budget.add(
        1,
        host_name.byte_length()
        + host_decoration.detail.byte_length()
        + host_decoration.right.byte_length(),
    )
    decorations.hosts[host_id] = host_decoration^
    var sessions = List[PaletteFleetSession]()
    var current: Optional[UInt64] = None
    for source_session in snapshot.sessions:
        var label = palette_session_label(source_session)
        budget.add(1, source_session.name.byte_length() + label.byte_length())
        if attached and source_session.id == attached.value():
            current = source_session.id
        var focused = snapshot.focused_window_for(source_session)
        var windows = List[PaletteFleetWindow]()
        for source_window in source_session.windows:
            budget.add(1, source_window.name.byte_length())
            var panes = List[PaletteFleetPane]()
            var ids = palette_ordered_panes(source_window)
            for id in ids:
                ref source = source_window.panes[id]
                var name = palette_pane_label(source)
                var program: Optional[Int] = None
                if source.kind.kind == 1:
                    program = programs.resolve(source.summary.process)
                elif source.kind.kind == 4:
                    program = programs.by_id(
                        "codex" if source.kind.provider == 0 else "claude"
                    )
                var program_name: Optional[String] = None
                if program:
                    program_name = programs.values[program.value()].name.copy()
                var value = PaletteDecoration()
                var phase = attention.get(id, UInt8(255))
                if phase > 3 and phase != 255:
                    raise Error("unknown palette agent attention state")
                value.running = phase == 1
                if source.kind.kind == 0:
                    value.detail = "New pane"
                    value.icon = "Plus"
                elif source.kind.kind == 1:
                    value.icon = "SquareTerminal"
                    value.detail = (
                        "Agent · running" if phase
                        == 1 else "Agent · waiting for input" if phase
                        == 2 else "Agent · idle" if phase
                        == 0 else "Agent · failed" if phase
                        == 3 else "Terminal"
                    )
                elif source.kind.kind == 2:
                    value.detail = "Browser"
                    value.icon = "Globe"
                elif source.kind.kind == 3:
                    value.detail = "Editor"
                    value.icon = "File"
                else:
                    value.detail = "Agent"
                    value.icon = (
                        'Asset("icons/openai.svg")' if source.kind.provider
                        == 0 else 'Asset("icons/claude.svg")'
                    )
                var size = (
                    name.byte_length()
                    + source.summary.cwd.byte_length()
                    + value.detail.byte_length()
                    + value.icon.byte_length()
                )
                if program_name:
                    size += program_name.value().byte_length()
                var place = PalettePlace(
                    host_id, source_session.id, source_window.id, id
                )
                var key = place_key(place)
                budget.add(1, size + key.byte_length())
                var needs_you = source.status and (
                    source.status.value().state == 3
                    or source.status.value().state == 4
                )
                panes.append(
                    PaletteFleetPane(
                        id,
                        palette_pane_index(source_window, id),
                        name^,
                        program_name^,
                        source.summary.cwd,
                        source.kind.kind == 1,
                        needs_you,
                    )
                )
                decorations.panes[key] = value^
            windows.append(
                PaletteFleetWindow(
                    source_window.id,
                    source_window.index,
                    source_window.name,
                    attached == Optional[UInt64](source_session.id)
                    and source_window.id == focused,
                    source_window.active_pane,
                    panes^,
                )
            )
        sessions.append(
            PaletteFleetSession(
                source_session.id, source_session.name, label^, windows^
            )
        )
    fleet.hosts.append(
        PaletteFleetHost(host_id, host_name^, online, current, sessions^)
    )
    return PaletteProjection(fleet^, decorations^)
