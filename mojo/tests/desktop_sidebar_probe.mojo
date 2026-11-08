from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_optional_id
from zzmojo.desktop_nav_tree import (
    NavNode,
    NavTreeHost,
    NavTreeSession,
    NavTreeWindow,
    SidebarTreeState,
    active_for_session,
    NAV_TREE_NODES,
    NAV_TREE_BYTES,
)
from desktop_nav_tree_probe import read_hosts, read_node


@fieldwise_init
struct Model(Copyable, Movable):
    var hosts: List[NavTreeHost]


def optional_node(mut reader: Decoder) raises -> Optional[NavNode]:
    if reader.byte() != 0:
        return read_node(reader)
    return None


def check_node(value: Optional[NavNode], mut reader: Decoder) raises:
    var expected = optional_node(reader)
    assert_true(value == expected)


def bounds() raises:
    var state = SidebarTreeState()
    var before = state.selected
    with assert_raises():
        state.select_pointer(NavNode(0, 4, 0))
    assert_true(before == state.selected)
    with assert_raises():
        state.select_pointer(NavNode(0, 0, 7))
    assert_true(before == state.selected)
    var hosts = List[NavTreeHost]()
    var previous = List[UInt32]()
    with assert_raises():
        _ = state.reconcile(Span(previous), Span(hosts), NavNode(0, 4, 0))
    assert_true(before == state.selected)
    assert_equal(len(state.expanded), 0)
    with assert_raises():
        _ = state.navigate(Span(hosts), 6)
    assert_true(before == state.selected)
    var panes = List[UInt64](capacity=NAV_TREE_NODES - 3)
    for index in range(NAV_TREE_NODES - 3):
        panes.append(UInt64(index))
    var windows: List[NavTreeWindow] = [
        NavTreeWindow(2, True, 0, String(), panes^)
    ]
    var sessions: List[NavTreeSession] = [
        NavTreeSession(1, True, String(), windows^)
    ]
    hosts.append(NavTreeHost(0, True, sessions^))
    _ = state.reconcile(Span(previous), Span(hosts), NavNode(0, 3, 0))
    assert_equal(len(state.projection.entries), NAV_TREE_NODES)
    assert_equal(len(state.projection.visible_indices), NAV_TREE_NODES)
    assert_equal(
        state.projection.entries[len(state.projection.entries) - 1].node.id,
        UInt64(NAV_TREE_NODES - 4),
    )
    var count = len(state.projection.entries)
    var selection = state.selected
    hosts[0].sessions[0].windows[0].panes.append(UInt64.MAX)
    with assert_raises():
        _ = state.reconcile(Span(previous), Span(hosts), None)
    assert_equal(len(state.projection.entries), count)
    assert_true(state.selected == selection)
    assert_equal(len(state.expanded), 3)
    _ = hosts[0].sessions[0].windows[0].panes.pop()
    var name = List[UInt8](capacity=NAV_TREE_BYTES)
    for _ in range(NAV_TREE_BYTES):
        name.append(120)
    hosts[0].sessions[0].name = String(from_utf8=Span(name))
    with assert_raises():
        _ = state.reconcile(Span(previous), Span(hosts), None)
    assert_equal(len(state.projection.entries), count)
    assert_true(state.selected == selection)


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    assert_true(len(bytes) <= 16 * 1024 * 1024)
    var reader = Decoder(bytes^)
    reader.offset = 0
    var models = List[Model]()
    for _ in range(Int(reader.uint())):
        models.append(Model(read_hosts(reader)))
    var traces = Int(reader.uint())
    var transitions = 0
    for _ in range(traces):
        var state = SidebarTreeState()
        var current = List[NavTreeHost]()
        var count = Int(reader.uint())
        transitions += count
        for _ in range(count):
            var op = reader.byte()
            var scroll: Optional[Int] = None
            if op == 0:
                var index = Int(reader.uint())
                var attached = UInt32(reader.uint())
                var session = read_optional_id(reader)
                var focused = reader.uint()
                ref hosts = models[index].hosts
                var active: Optional[NavNode] = None
                for host in hosts:
                    for value in host.sessions:
                        active = active_for_session(
                            active, host.id, attached, value, session, focused
                        )
                var previous = List[UInt32]()
                for host in current:
                    previous.append(host.id)
                scroll = state.reconcile(Span(previous), Span(hosts), active)
                current = hosts.copy()
            elif op == 3:
                scroll = state.navigate(Span(current), UInt8(reader.uint()))
            elif op == 5:
                scroll = state.select_index(Int(reader.uint()))
            else:
                var node = read_node(reader)
                if op == 1:
                    state.select_pointer(node)
                elif op == 2:
                    state.select_or_toggle(Span(current), node)
                elif op == 4:
                    scroll = state.focus(Span(current))
                elif op == 6:
                    _ = state.toggle(Span(current), node)
                else:
                    raise Error("unknown desktop sidebar operation")
            check_node(state.selected, reader)
            check_node(state.active_target, reader)
            assert_equal(state.selection_from_pointer, reader.byte() != 0)
            assert_equal(len(state.expanded), Int(reader.uint()))
            for _ in range(len(state.expanded)):
                assert_true(read_node(reader).key() in state.expanded)
            var expected_scroll = read_optional_id(reader)
            assert_equal(Bool(scroll), Bool(expected_scroll))
            if scroll:
                assert_equal(scroll.value(), Int(expected_scroll.value()))
            assert_equal(len(state.projection.entries), Int(reader.uint()))
            for row in state.projection.entries:
                assert_true(row.node == read_node(reader))
    reader.finish()
    bounds()
    print("desktop sidebar Source helper lifecycle passed", traces, transitions)
