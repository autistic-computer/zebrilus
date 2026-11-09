from std.testing import assert_equal, assert_true, assert_false, assert_raises
from zzmojo.actions import ActionQueue
from zzmojo.palette_protocol import palette_command_request
from zzmojo.stream_protocol import HostPlatform, attach_session
from zzmojo.tree_model import MuxSnapshot, SessionSnapshot, WindowSnapshot, PaneSnapshot
from zzmojo.exec_protocol import Invocation
from zzmojo.desktop_nav import NavActivation, nav_command
from zzmojo.desktop_nav_tree import NavNode, NAV_TREE_NODES
from zzmojo.desktop_host import HostState
from zzmojo.desktop_fleet import FleetHostInput
from zzmojo.desktop_sidebar import (
    DesktopSidebar,
    DesktopNavSource,
    stage_navigation,
    commit_navigation,
)


def pane(id: UInt64, kind: UInt8, var title: String, var process: List[String], var shell: String, var cwd: String) -> PaneSnapshot:
    var result = PaneSnapshot()
    result.id = id
    result.title = title^
    result.kind.kind = kind
    result.kind.provider = 1
    result.summary.process = process^
    result.summary.shell = shell^
    result.summary.cwd = cwd^
    return result^


def window(id: UInt64, var name: String, active: UInt64, var panes: List[PaneSnapshot]) -> WindowSnapshot:
    var result = WindowSnapshot()
    result.id = id
    result.index = UInt32(id)
    result.name = name^
    result.active_pane = active
    for item in panes:
        result.panes[item.id] = item.copy()
    return result^


def session(id: UInt64, var name: String, active: UInt64, var windows: List[WindowSnapshot]) -> SessionSnapshot:
    var result = SessionSnapshot()
    result.id = id
    result.name = name^
    result.active_window = active
    result.windows = windows^
    return result^


def fleet() -> List[FleetHostInput]:
    var local = MuxSnapshot()
    local.sessions.append(session(1, "0", 10, [
        window(10, "zsh", 100, [
            pane(100, 1, "vim", ["/opt/homebrew/bin/nvim", "nvim"], "", "/home/me/dev/zz"),
            pane(101, 4, "agent", List[String](), "", ""),
        ]),
        window(11, "w11", 110, [pane(110, 1, "", List[String](), "fish", "")]),
    ]))
    var remote = MuxSnapshot()
    remote.sessions.append(session(3, "work", 30, [window(30, "build", 300, [pane(300, 1, "make", List[String](), "", "")])]))
    var hosts = List[FleetHostInput]()
    hosts.append(FleetHostInput(0, "local", HostState(3), HostPlatform(3, 3), local^))
    hosts.append(FleetHostInput(5, "box", HostState(3), HostPlatform(1, 0), remote^))
    hosts.append(FleetHostInput(6, "old", HostState.incompatible(120, UInt16(118)), HostPlatform(0, 0), None))
    return hosts^


def command(name: String, var arguments: List[String]) raises -> List[UInt8]:
    return palette_command_request(0, nav_command(name, arguments))


def frames(activation: NavActivation) raises -> List[List[UInt8]]:
    var result = List[List[UInt8]]()
    for item in activation.commands:
        result.append(palette_command_request(0, item))
    return result^


def main() raises:
    var sidebar = DesktopSidebar("desk")
    var hosts = fleet()
    assert_equal(sidebar.update(Span(hosts), 0, UInt64(1)).value(), 3)
    assert_equal(sidebar.row_count(), 8)
    var rows = sidebar.rows(0, 100, True)
    assert_equal(rows[0].label, String("desk"))
    assert_equal(rows[0].platform.distro, UInt8(3))
    assert_equal(rows[1].label, String("zz"))
    assert_equal(rows[2].label, String("zsh"))
    assert_false(Bool(rows[2].program))
    assert_equal(rows[3].label, String("Neovim"))
    assert_equal(sidebar.programs.values[rows[3].program.value()].id, String("neovim"))
    assert_true(rows[3].active and rows[3].selected and rows[3].on_active_path)
    assert_equal(rows[4].label, String("Claude Code"))
    assert_equal(rows[5].label, String("w11"))
    assert_equal(rows[5].rename.value(), String("Rename Window…"))
    assert_equal(rows[6].label, String("box"))
    assert_true(rows[6].removable and not rows[6].expanded)
    assert_equal(rows[7].indicator.value().detail.value(), String("This zz speaks protocol v120; that machine speaks v118.\nUpgrade whichever side is older, then reconnect."))
    assert_equal(rows[7].install_offer.value(), UInt8(1))
    assert_equal(len(sidebar.rows(6, 9, True)), 2)
    assert_equal(len(sidebar.rows(9, 9, True)), 0)

    var revision = sidebar.revision
    hosts[0].state = HostState(9)
    with assert_raises(contains="host state is invalid"):
        _ = sidebar.update(Span(hosts), 0, UInt64(1))
    assert_equal(sidebar.revision, revision)
    assert_equal(sidebar.row_count(), 8)
    hosts[0].state = HostState(3)

    var intent = sidebar.click(NavNode(0, 2, 11))
    assert_true(intent.release_focus)
    assert_equal(intent.activation.value().kind, UInt8(3))
    assert_equal(frames(intent.activation.value())[0], command("select-window", [String("-t"), String("@11")]))
    assert_true(sidebar.tree.selection_from_pointer)
    assert_false(Bool(sidebar.update(Span(hosts), 0, UInt64(1))))
    assert_true(sidebar.tree.selected == Optional[NavNode](NavNode(0, 2, 11)))
    assert_false(sidebar.rows(5, 1, True)[0].selected)

    var renamed = sidebar.rename()
    assert_equal(frames(renamed.activation.value())[0], command("command-prompt", [String("-p"), String("rename-window: "), String("-I"), String("w11"), String("rename-window -t '@11' -- '%%'")]))
    assert_equal(sidebar.confirm().activation.value().kind, UInt8(3))

    _ = sidebar.click(NavNode(5, 0, 0))
    assert_equal(sidebar.row_count(), 8)
    assert_true(sidebar.toggle(NavNode(5, 0, 0)))
    assert_equal(sidebar.row_count(), 9)
    var host_click = sidebar.click(NavNode(5, 0, 0))
    assert_equal(host_click.activation.value().kind, UInt8(0))
    var old = sidebar.click(NavNode(6, 0, 0))
    assert_equal(old.activation.value().kind, UInt8(5))

    var first = sidebar.window_shortcut(0)
    assert_true(first.focus_pane)
    assert_equal(frames(first.activation.value())[0], command("select-window", [String("-t"), String("@10")]))
    var remote = sidebar.window_shortcut(2)
    assert_equal(remote.activation.value().kind, UInt8(2))
    assert_equal(remote.activation.value().target, String("$3:@30"))
    assert_false(Bool(sidebar.window_shortcut(3).activation))
    with assert_raises(contains="shortcut position"):
        _ = sidebar.window_shortcut(9)
    sidebar.show_window_shortcuts(True)
    assert_equal(sidebar.rows(2, 1, True)[0].shortcut.value(), UInt8(0))

    var created = sidebar.new_session(List[String](), String("/tmp"))
    assert_equal(frames(created.activation.value())[0], command("new-session", List[String]()))
    assert_equal(frames(created.activation.value())[1], command("attach-session", List[String]()))
    assert_equal(frames(sidebar.new_window().activation.value())[0], command("new-window", [String("-t"), String("$1")]))
    assert_equal(frames(sidebar.split(0).activation.value())[0], command("split-window", [String("--kind"), String("picker"), String("-h"), String("-t"), String("%100")]))
    assert_equal(frames(sidebar.detach().activation.value())[0], command("detach-client", List[String]()))

    var queue = ActionQueue()
    var source = DesktopNavSource(1, 2, 3, 4)
    var staged = stage_navigation(queue, sidebar.new_session(List[String](), None).activation.value(), 0, source)
    var stale = stage_navigation(queue, sidebar.detach().activation.value(), 0, source)
    var retired = DesktopNavSource(1, 2, 3, 5)
    with assert_raises(contains="retired source"):
        commit_navigation(queue, staged^, retired)
    assert_equal(len(queue), 0)
    staged = stage_navigation(queue, sidebar.new_session(List[String](), None).activation.value(), 0, source)
    commit_navigation(queue, staged^, source)
    assert_equal(len(queue), 2)
    assert_equal(queue.items[0].kind, 16)
    assert_equal(queue.items[1].frame, command("attach-session", List[String]()))
    with assert_raises(contains="stale"):
        commit_navigation(queue, stale^, source)
    assert_equal(len(queue), 2)

    var attach = stage_navigation(queue, NavActivation(1, 0, UInt64(1), String(), List[Invocation]()), 0, source)
    var jump = stage_navigation(queue, remote.activation.value(), 0, source)
    assert_false(Bool(jump.batch))
    assert_equal(jump.host_request.value().host, UInt32(5))
    commit_navigation(queue, attach^, source)
    assert_equal(len(queue), 3)
    assert_equal(queue.items[2].kind, 10)
    assert_equal(queue.items[2].frame, attach_session("$1"))
    var at = stage_navigation(queue, NavActivation(2, 0, None, String("$1:@11"), List[Invocation]()), 0, source)
    commit_navigation(queue, at^, source)
    assert_equal(queue.items[3].frame, command("attach-session", [String("-t"), String("$1:@11")]))

    var large = MuxSnapshot()
    var panes = List[PaneSnapshot](capacity=NAV_TREE_NODES - 3)
    for id in range(NAV_TREE_NODES - 3):
        panes.append(pane(UInt64(id), 1, "", List[String](), "", ""))
    large.sessions.append(session(1, "", 2, [window(2, "big", 0, panes^)]))
    var full = List[FleetHostInput]()
    full.append(FleetHostInput(0, "local", HostState(3), HostPlatform(3, 3), large^))
    var over = full.copy()
    over.append(FleetHostInput(5, "box", HostState(0), HostPlatform(0, 0), None))
    revision = sidebar.revision
    with assert_raises(contains="node limit"):
        _ = sidebar.update(Span(over), 0, UInt64(1))
    assert_equal(sidebar.revision, revision)
    assert_equal(sidebar.row_count(), 9)
    assert_equal(sidebar.update(Span(full), 0, UInt64(1)).value(), 3)
    assert_equal(sidebar.row_count(), NAV_TREE_NODES)
    var last = sidebar.rows(NAV_TREE_NODES - 1, 4, True)
    assert_equal(len(last), 1)
    assert_true(last[0].node == NavNode(0, 3, UInt64(NAV_TREE_NODES - 4)))
    assert_equal(last[0].label, String("terminal"))
    revision = sidebar.revision
    var name = List[UInt8](capacity=16 * 1024 * 1024)
    for _ in range(16 * 1024 * 1024):
        name.append(120)
    full[0].name = String(from_utf8=Span(name))
    with assert_raises(contains="byte limit"):
        _ = sidebar.update(Span(full), 0, UInt64(1))
    assert_equal(sidebar.revision, revision)
    assert_equal(sidebar.row_count(), NAV_TREE_NODES)
    assert_equal(sidebar.rows(0, 1, True)[0].label, String("desk"))
    print("desktop sidebar controller passed", sidebar.revision, len(queue))
