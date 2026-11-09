from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_optional_id
from zzmojo.stream_protocol import decode_stream_frame, HostPlatform, attach_session
from zzmojo.tree_model import MuxSnapshot, SessionSnapshot, WindowSnapshot, PaneSnapshot
from zzmojo.palette_programs import PalettePrograms
from zzmojo.palette_protocol import palette_command_request
from zzmojo.desktop_nav import NavActivation, NavTarget
from zzmojo.desktop_nav_tree import NavNode, project_tree, NAV_TREE_NODES
from zzmojo.desktop_host import HostState, host_indicator
from zzmojo.desktop_fleet import (
    FleetModel,
    FleetHostInput,
    fleet_session_for_target,
    fleet_renameable_name,
)


def write_flag(mut output: List[UInt8], value: Bool):
    output.append(UInt8(1) if value else UInt8(0))


def write_text(mut output: List[UInt8], value: Optional[String]):
    write_flag(output, Bool(value))
    if value:
        append_text(output, value.value())


def write_uint(mut output: List[UInt8], value: Optional[UInt64]):
    write_flag(output, Bool(value))
    if value:
        append_uint(output, value.value())


def write_node(mut output: List[UInt8], node: NavNode):
    append_uint(output, UInt64(node.host))
    output.append(node.kind)
    append_uint(output, node.id)


def write_activation(mut output: List[UInt8], value: Optional[NavActivation]) raises:
    write_flag(output, Bool(value))
    if not value:
        return
    ref activation = value.value()
    output.append(activation.kind)
    append_uint(output, UInt64(activation.host))
    write_uint(output, activation.session)
    append_text(output, activation.target)
    append_uint(output, UInt64(len(activation.commands)))
    for command in activation.commands:
        var frame = palette_command_request(0, command)
        append_uint(output, UInt64(len(frame)))
        output.extend(frame^)


def program_id(programs: PalettePrograms, value: Optional[Int]) -> Optional[String]:
    if value:
        return programs.values[value.value()].id.copy()
    return None


def read_state(mut reader: Decoder) raises -> HostState:
    var kind = reader.byte()
    if kind == 2:
        return HostState.reconnecting(UInt32(reader.uint()))
    if kind == 4:
        return HostState.unreachable(reader.text())
    if kind == 5:
        var local = UInt16(reader.uint())
        var remote: Optional[UInt16] = None
        if reader.byte() != 0:
            remote = UInt16(reader.uint())
        return HostState.incompatible(local, remote)
    return HostState(kind)


def write_host(mut output: List[UInt8], model: FleetModel, index: Int, programs: PalettePrograms) raises:
    ref host = model.hosts[index]
    append_text(output, host.state.label())
    var offer = host.state.install_offer()
    write_flag(output, Bool(offer))
    if offer:
        output.append(offer.value())
    write_text(output, host.state.failure_detail())
    var indicator = host_indicator(host.state, host.loaded)
    if not indicator:
        output.append(0)
    elif not indicator.value().failed:
        output.append(1)
    else:
        output.append(2)
        write_text(output, indicator.value().detail)
    append_uint(output, UInt64(len(host.sessions)))
    for session in host.sessions:
        append_uint(output, session.id)
        append_text(output, session.name)
        append_text(output, session.label)
        write_text(output, session.label_index)
        write_flag(output, session.active)
        write_flag(output, session.has_pending_bell())
        append_uint(output, UInt64(len(session.windows)))
        for window in session.windows:
            append_uint(output, window.id)
            append_uint(output, UInt64(window.index))
            append_text(output, window.name)
            append_uint(output, window.active_pane)
            write_flag(output, window.active)
            write_text(output, program_id(programs, window.program()))
            append_uint(output, UInt64(len(window.panes)))
            for pane in window.panes:
                append_uint(output, pane.id)
                append_uint(output, UInt64(pane.index))
                append_text(output, pane.label)
                append_text(output, pane.cwd)
                output.append(pane.kind)
                output.append(pane.provider)
                write_flag(output, pane.bell)
                var status: Optional[UInt64] = None
                if pane.status:
                    status = UInt64(pane.status.value())
                write_uint(output, status)
                write_text(output, program_id(programs, pane.program))


def write_query(mut output: List[UInt8], model: FleetModel, node: NavNode, attached_host: UInt32, attached: Optional[UInt64]) raises:
    write_flag(output, model.expandable(node))
    write_flag(output, model.pending_bell(node))
    var index = model.host(node.host)
    var session: Optional[UInt64] = None
    var name: Optional[String] = None
    if index and node.kind != 0:
        var target = NavTarget(node.kind - 1, node.id)
        session = fleet_session_for_target(Span(model.hosts[index.value()].sessions), target)
        name = fleet_renameable_name(Span(model.hosts[index.value()].sessions), target)
    write_uint(output, session)
    write_text(output, name)
    write_activation(output, model.activation(node, attached_host, attached))
    var rename = model.rename_activation(node, attached_host)
    if rename:
        write_text(output, rename.value()[0].copy())
        write_activation(output, rename.value()[1].copy())
    else:
        write_text(output, None)
        write_activation(output, None)


def write_rows(mut output: List[UInt8], model: FleetModel, expand_windows: Bool, programs: PalettePrograms) raises:
    var hosts = model.tree_hosts()
    var expanded = Dict[Tuple[UInt32, UInt8, UInt64], Bool]()
    for host in hosts:
        expanded[NavNode(host.id, 0, 0).key()] = True
        for session in host.sessions:
            expanded[NavNode(host.id, 1, session.id).key()] = True
            if expand_windows:
                for window in session.windows:
                    expanded[NavNode(host.id, 2, window.id).key()] = True
    var projection = project_tree(Span(hosts), expanded)
    append_uint(output, UInt64(len(projection.entries)))
    for entry in projection.entries:
        write_node(output, entry.node)
        output.append(entry.depth)
        write_flag(output, entry.expandable)
        write_flag(output, entry.expanded)
        write_flag(output, entry.on_active_path)
        ref host = model.hosts[entry.host_index]
        if entry.node.kind == 0:
            append_text(output, host.name)
            output.append(0)
            output.append(0)
            write_text(output, None)
        elif entry.node.kind == 1:
            append_text(output, host.sessions[entry.session_index].label)
            output.append(0)
            output.append(0)
            write_text(output, None)
        elif entry.node.kind == 2:
            ref window = host.sessions[entry.session_index].windows[entry.window_index]
            append_text(output, window.name)
            output.append(0)
            output.append(0)
            var program: Optional[Int] = None
            if not entry.expanded:
                program = window.program()
            write_text(output, program_id(programs, program))
        else:
            ref pane = host.sessions[entry.session_index].windows[entry.window_index].panes[entry.pane_index]
            append_text(output, pane.label)
            output.append(pane.kind)
            output.append(pane.provider)
            write_text(output, program_id(programs, pane.program))


def bounds(programs: PalettePrograms) raises:
    var hosts = List[FleetHostInput]()
    hosts.append(FleetHostInput(0, String("local"), HostState(8), HostPlatform(0, 0), None))
    with assert_raises(contains="host state is invalid"):
        _ = FleetModel(0, None, Span(hosts), programs)
    hosts[0].state = HostState(3)
    hosts[0].state.reason = String("stale")
    with assert_raises(contains="foreign fields"):
        _ = FleetModel(0, None, Span(hosts), programs)
    hosts[0].state = HostState(3)
    hosts[0].platform = HostPlatform(4, 0)
    with assert_raises(contains="platform is invalid"):
        _ = FleetModel(0, None, Span(hosts), programs)
    hosts[0].platform = HostPlatform(3, 3)
    var snapshot = MuxSnapshot()
    var session = SessionSnapshot()
    session.id = 1
    session.name = String("0")
    session.active_window = 2
    var window = WindowSnapshot()
    window.id = 2
    for id in range(NAV_TREE_NODES - 3):
        var pane = PaneSnapshot()
        pane.id = UInt64(id)
        window.panes[UInt64(id)] = pane^
    session.windows.append(window^)
    snapshot.sessions.append(session^)
    hosts[0].snapshot = snapshot.copy()
    var model = FleetModel(0, UInt64(1), Span(hosts), programs)
    assert_equal(len(model.hosts[0].sessions[0].windows[0].panes), NAV_TREE_NODES - 3)
    assert_equal(model.hosts[0].sessions[0].windows[0].panes[7].label, String("terminal"))
    assert_true(model.active_target == Optional[NavNode](NavNode(0, 3, 0)))
    hosts.append(FleetHostInput(1, String(), HostState(0), HostPlatform(0, 0), None))
    with assert_raises(contains="node limit"):
        _ = FleetModel(0, UInt64(1), Span(hosts), programs)
    _ = hosts.pop()
    var name = List[UInt8](capacity=8 * 1024 * 1024)
    for _ in range(8 * 1024 * 1024):
        name.append(120)
    hosts[0].snapshot.value().sessions[0].windows[0].name = String(from_utf8=Span(name))
    with assert_raises(contains="byte limit"):
        _ = FleetModel(0, UInt64(1), Span(hosts), programs)


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    assert_true(len(bytes) <= 64 * 1024 * 1024)
    var reader = Decoder(bytes^)
    reader.offset = 0
    var programs = PalettePrograms()
    var output = List[UInt8]()
    var cases = Int(reader.uint())
    for _ in range(cases):
        var attached_host = UInt32(reader.uint())
        var attached = read_optional_id(reader)
        var inputs = List[FleetHostInput]()
        for _ in range(Int(reader.uint())):
            var id = UInt32(reader.uint())
            var name = reader.text()
            var state = read_state(reader)
            var snapshot: Optional[MuxSnapshot] = None
            if reader.byte() != 0:
                var frame = decode_stream_frame(reader.bytes())
                snapshot = frame.snapshot.take()
            inputs.append(FleetHostInput(id, name^, state^, HostPlatform(0, 0), snapshot^))
        var model = FleetModel(attached_host, attached, Span(inputs), programs)
        for index in range(len(model.hosts)):
            write_host(output, model, index, programs)
        write_flag(output, Bool(model.active_target))
        if model.active_target:
            write_node(output, model.active_target.value())
        for _ in range(Int(reader.uint())):
            var host = UInt32(reader.uint())
            var kind = reader.byte()
            var node = NavNode(host, kind, reader.uint())
            write_query(output, model, node, attached_host, attached)
        var shortcuts = model.shortcut_windows()
        append_uint(output, UInt64(len(shortcuts)))
        for position in range(len(shortcuts)):
            append_uint(output, UInt64(shortcuts[position][0]))
            append_uint(output, shortcuts[position][1])
            write_activation(output, model.window_shortcut(position, attached_host, attached))
        write_rows(output, model, True, programs)
        write_rows(output, model, False, programs)
    reader.finish()
    var attach = attach_session(String("$", 7))
    append_uint(output, UInt64(len(attach)))
    output.extend(attach^)
    with open(args[2], "w") as file:
        file.write_bytes(Span(output))
    bounds(programs)
    print("desktop fleet presentation and actions passed", cases, len(output))
