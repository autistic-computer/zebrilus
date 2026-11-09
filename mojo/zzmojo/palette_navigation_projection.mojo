from std.collections import Set
from .tree_model import (
    PaneSnapshot,
    WindowSnapshot,
    SessionSnapshot,
    _Order,
    _ordered,
)
from .palette_places_model import MAX_FLEET_NODES
from .palette_text import palette_trim


def palette_ordered_panes(window: WindowSnapshot) raises -> List[UInt64]:
    if (
        len(window.panes) > MAX_FLEET_NODES
        or len(window.layout.nodes) > MAX_FLEET_NODES
    ):
        raise Error("palette pane ordering exceeds its node limit")
    var seen = Set[UInt64]()
    var result = List[UInt64](capacity=len(window.panes))
    var layout = window.layout.panes()
    for id in layout:
        if id in seen:
            continue
        seen.add(id)
        if id in window.panes:
            result.append(id)
    var remaining = List[_Order]()
    for id in window.panes.keys():
        remaining.append(_Order(id, 0, 0))
    remaining = _ordered(remaining^)
    for item in remaining:
        if item.major not in seen:
            seen.add(item.major)
            result.append(item.major)
    return result^


def _pane_index(index: Int, base: UInt32) -> UInt32:
    if UInt64(index) > UInt64(UInt32.MAX - base):
        return UInt32.MAX
    return base + UInt32(index)


def palette_pane_index(window: WindowSnapshot, pane: UInt64) raises -> UInt32:
    if len(window.pane_order) > MAX_FLEET_NODES:
        raise Error("palette pane order exceeds its node limit")
    for index in range(len(window.pane_order)):
        if window.pane_order[index] == pane:
            return _pane_index(index, window.pane_base_index)
    var ordered = palette_ordered_panes(window)
    for index in range(len(ordered)):
        if ordered[index] == pane:
            return _pane_index(index, window.pane_base_index)
    return window.pane_base_index


def palette_pane_label(pane: PaneSnapshot) -> String:
    var title = palette_trim(pane.title)
    if title.byte_length() > 0 and (pane.kind.kind != 4 or title != "agent"):
        return title^
    if pane.kind.kind == 0:
        return String("new pane")
    if pane.kind.kind == 1:
        return String("terminal")
    if pane.kind.kind == 2:
        var url = String("about:blank")
        if len(pane.kind.tabs) > 0:
            var index = (
                Int(pane.kind.active_tab) if pane.kind.active_tab
                < UInt64(len(pane.kind.tabs)) else 0
            )
            url = pane.kind.tabs[index].copy()
        var trimmed = palette_trim(url)
        url = trimmed^
        return url^ if url.byte_length() > 0 else String("browser")
    if pane.kind.kind == 3:
        return String("editor")
    return String("Claude Code" if pane.kind.provider == 1 else "Codex")


def _pane_place(pane: PaneSnapshot) -> Optional[String]:
    if pane.summary.repo.byte_length() > 0:
        return pane.summary.repo.copy()
    if pane.summary.cwd.byte_length() > 0:
        return pane.summary.cwd.copy()
    return None


def _last_component(path: String) -> String:
    var first = 0
    var last = -1
    var start = 0
    for index in range(path.byte_length() + 1):
        if (
            index == path.byte_length()
            or path.as_bytes()[index] == 47
            or path.as_bytes()[index] == 92
        ):
            if index > start:
                first = start
                last = index
            start = index + 1
    return (
        String(from_utf8_lossy=path.as_bytes()[first:last]) if last
        >= 0 else path.copy()
    )


def palette_session_place(session: SessionSnapshot) raises -> Optional[String]:
    var active: Optional[String] = None
    for window in session.windows:
        if (
            window.id == session.active_window
            and window.active_pane in window.panes
        ):
            active = _pane_place(window.panes[window.active_pane])
            break
    var order = List[String]()
    var counts = Dict[String, Int]()
    var panes = 0
    for window in session.windows:
        panes += len(window.panes)
        if panes > MAX_FLEET_NODES:
            raise Error("palette session places exceed their node limit")
        var sorted = List[_Order]()
        for id in window.panes.keys():
            sorted.append(_Order(id, 0, 0))
        sorted = _ordered(sorted^)
        for id in sorted:
            var place = _pane_place(window.panes[id.major])
            if not place:
                continue
            var path = place.take()
            if path not in counts:
                order.append(path.copy())
                counts[path] = 1
            else:
                counts[path] += 1
    var best: Optional[String] = None
    var count = 0
    var favored = False
    for path in order:
        var is_active = active and active.value() == path
        if (
            counts[path] > count
            or counts[path] == count
            and is_active
            and not favored
        ):
            best = path.copy()
            count = counts[path]
            favored = is_active
    return best^


def palette_session_label(session: SessionSnapshot) raises -> String:
    var name = palette_trim(session.name)
    var numbered = True
    for byte in name.as_bytes():
        numbered = numbered and byte >= 48 and byte <= 57
    if numbered:
        var place = palette_session_place(session)
        if place:
            var label = _last_component(place.value())
            if name.byte_length() > 0:
                label += " " + name
            return label^
    if name.byte_length() == 0:
        return "session $" + String(session.id)
    return session.name.copy()
