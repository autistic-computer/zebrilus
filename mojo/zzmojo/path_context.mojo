from .palette_places_model import PalettePlace
from .tree_model import MuxSnapshot, PaneSnapshot, _Order, _ordered
from .path_text import start_dir


@fieldwise_init
struct FilesPlace(Copyable, Movable):
    var host: UInt32
    var session: Optional[UInt64]
    var window: Optional[UInt64]
    var pane: Optional[UInt64]


def files_place(place: PalettePlace, written: UInt8, here: UInt32) -> FilesPlace:
    return FilesPlace(place.host.value() if place.host else here, place.session if written & 2 != 0 else None, place.window if written & 4 != 0 else None, place.pane if written & 8 != 0 else None)


def _place_pane(snapshot: MuxSnapshot, place: FilesPlace) -> Optional[UInt64]:
    if place.pane:
        return place.pane
    for session in snapshot.sessions:
        if place.session and session.id != place.session.value():
            continue
        for window in session.windows:
            if (window.id == place.window.value() if place.window else window.id == session.active_window):
                return window.active_pane
    return None


def start_folder(place: FilesPlace, here: Bool, snapshot: MuxSnapshot, insert: Optional[UInt64], home: Optional[String], available: Bool = True) raises -> String:
    var pane: Optional[UInt64] = None
    if place.session or place.window or place.pane:
        if available:
            pane = _place_pane(snapshot, place)
    elif here:
        pane = insert
    if pane and available:
        for session in snapshot.sessions:
            for window in session.windows:
                if pane.value() in window.panes:
                    ref retained = window.panes[pane.value()]
                    var cwd: Optional[String] = None
                    if retained.kind.kind == 1 and retained.summary.cwd != "":
                        cwd = retained.summary.cwd.copy()
                    return start_dir(cwd, home)
    return start_dir(None, home)


def _runs_remote(pane: PaneSnapshot) -> Bool:
    for command in pane.summary.process:
        var start = 0
        for index in range(command.byte_length()):
            if command.as_bytes()[index] == 47:
                start = index + 1
        var name = String(from_utf8_lossy=command.as_bytes()[start:])
        if name == "ssh" or name == "mosh" or name == "mosh-client" or name == "tmux" or name == "zz_cli":
            return True
    return False


def listing_pane(snapshot: MuxSnapshot, anchor: Optional[UInt64]) raises -> Optional[UInt64]:
    var first: Optional[UInt64] = None
    var local: Optional[UInt64] = None
    var anchored = False
    for session in snapshot.sessions:
        for window in session.windows:
            var keys = List[_Order](capacity=len(window.panes))
            for key in window.panes.keys():
                keys.append(_Order(key, 0, 0))
            keys = _ordered(keys^)
            for key in keys:
                ref pane = window.panes[key.major]
                if pane.kind.kind != 1 or pane.dead:
                    continue
                if anchor and pane.id == anchor.value():
                    anchored = True
                if not first:
                    first = pane.id
                if not local and not _runs_remote(pane):
                    local = pane.id
    return anchor if anchored else local if local else first


def path_inserts(snapshot: MuxSnapshot, pane: Optional[UInt64]) raises -> Bool:
    if not pane:
        return False
    for session in snapshot.sessions:
        for window in session.windows:
            if pane.value() in window.panes:
                ref retained = window.panes[pane.value()]
                return retained.kind.kind == 1 and not retained.dead
    return True
