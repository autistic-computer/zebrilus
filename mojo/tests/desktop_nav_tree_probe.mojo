from std.sys import argv
from std.testing import assert_equal, assert_true, assert_raises
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_optional_id
from zzmojo.desktop_nav import NavTarget
from zzmojo.desktop_nav_tree import (
    NavNode,
    NavTreeHost,
    NavTreeSession,
    NavTreeWindow,
    project_tree,
    shortcut_windows,
    target_owner,
    renameable_name,
    target_expandable,
    tree_navigation,
    validate_fleet,
    NAV_TREE_NODES,
)


def read_node(mut reader: Decoder) raises -> NavNode:
    var host = UInt32(reader.uint())
    var kind = reader.byte()
    var id = reader.uint()
    return NavNode(host, kind, id)


def location(mut reader: Decoder) raises -> Int:
    if reader.byte() != 0:
        return Int(reader.uint())
    return -1


def bounds() raises:
    var hosts = List[NavTreeHost]()
    var sessions = List[NavTreeSession]()
    var windows = List[NavTreeWindow]()
    var panes = List[UInt64](capacity=NAV_TREE_NODES)
    for _ in range(NAV_TREE_NODES):
        panes.append(0)
    windows.append(NavTreeWindow(0, False, 0, String(), panes^))
    sessions.append(NavTreeSession(0, False, String(), windows^))
    hosts.append(NavTreeHost(0, True, sessions^))
    with assert_raises():
        validate_fleet(Span(hosts))


def read_hosts(mut reader: Decoder) raises -> List[NavTreeHost]:
    var hosts = List[NavTreeHost]()
    for _ in range(Int(reader.uint())):
        var host = UInt32(reader.uint())
        var connected = reader.byte() != 0
        var sessions = List[NavTreeSession]()
        for _ in range(Int(reader.uint())):
            var session = reader.uint()
            var active = reader.byte() != 0
            var name = reader.text()
            var windows = List[NavTreeWindow]()
            for _ in range(Int(reader.uint())):
                var window = reader.uint()
                var window_active = reader.byte() != 0
                var pane = reader.uint()
                var window_name = reader.text()
                var panes = List[UInt64]()
                for _ in range(Int(reader.uint())):
                    panes.append(reader.uint())
                windows.append(
                    NavTreeWindow(
                        window, window_active, pane, window_name^, panes^
                    )
                )
            sessions.append(NavTreeSession(session, active, name^, windows^))
        hosts.append(NavTreeHost(host, connected, sessions^))
    return hosts^


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    assert_true(len(bytes) <= 16 * 1024 * 1024)
    var reader = Decoder(bytes^)
    reader.offset = 0
    var models = Int(reader.uint())
    var rows = 0
    var navigation = 0
    for _ in range(models):
        var hosts = read_hosts(reader)
        validate_fleet(Span(hosts))
        for _ in range(Int(reader.uint())):
            var hi = Int(reader.uint())
            var kind = reader.byte()
            var id = reader.uint()
            var target = NavTarget(kind, id)
            var owner = target_owner(hosts[hi], target)
            var present = reader.byte() != 0
            assert_equal(Bool(owner), present)
            if present:
                var session = reader.uint()
                var window = reader.uint()
                assert_equal(owner.value()[0], session)
                assert_equal(owner.value()[1], window)
            var name = renameable_name(hosts[hi], target)
            present = reader.byte() != 0
            assert_equal(Bool(name), present)
            if present:
                assert_equal(name.value(), reader.text())
            assert_equal(
                target_expandable(hosts[hi], target), reader.byte() != 0
            )
        var shortcuts = shortcut_windows(Span(hosts))
        assert_equal(len(shortcuts), Int(reader.uint()))
        for value in shortcuts:
            assert_equal(value[0], UInt32(reader.uint()))
            assert_equal(value[1], reader.uint())
        for _ in range(Int(reader.uint())):
            var expanded = Dict[Tuple[UInt32, UInt8, UInt64], Bool]()
            for _ in range(Int(reader.uint())):
                expanded[read_node(reader).key()] = True
            var projection = project_tree(Span(hosts), expanded)
            assert_equal(len(projection.entries), Int(reader.uint()))
            rows += len(projection.entries)
            for entry in projection.entries:
                assert_true(entry.node == read_node(reader))
                var present = reader.byte() != 0
                assert_equal(Bool(entry.parent), present)
                if present:
                    assert_true(entry.parent.value() == read_node(reader))
                assert_equal(entry.host_index, location(reader))
                assert_equal(entry.session_index, location(reader))
                assert_equal(entry.window_index, location(reader))
                assert_equal(entry.pane_index, location(reader))
                assert_equal(entry.depth, reader.byte())
                assert_equal(entry.expandable, reader.byte() != 0)
                assert_equal(entry.expanded, reader.byte() != 0)
                assert_equal(entry.on_active_path, reader.byte() != 0)
            assert_equal(len(projection.visible_indices), Int(reader.uint()))
            for _ in range(len(projection.visible_indices)):
                var key = read_node(reader).key()
                var index = Int(reader.uint())
                assert_equal(projection.visible_indices[key], index)
            assert_equal(len(projection.expandable), Int(reader.uint()))
            for _ in range(len(projection.expandable)):
                assert_true(read_node(reader).key() in projection.expandable)
            var count = Int(reader.uint())
            navigation += count
            for _ in range(count):
                var selected: Optional[Int] = None
                var selected_id = read_optional_id(reader)
                if selected_id:
                    selected = Int(selected_id.value())
                var direction = reader.byte()
                var value = tree_navigation(projection, selected, direction)
                assert_equal(value[0], reader.byte())
                assert_equal(value[1], Int(reader.uint()))
    reader.finish()
    bounds()
    print(
        "actual desktop tree projection/navigation passed",
        models,
        rows,
        navigation,
    )
