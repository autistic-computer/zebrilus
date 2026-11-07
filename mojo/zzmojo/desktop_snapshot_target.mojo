from .tree_model import MuxSnapshot
from .verified_input_router import RouterOwner, pane_owner


@fieldwise_init
struct DesktopSnapshotTarget(Copyable, Movable):
    var active: Optional[RouterOwner]
    var capture: Optional[UInt64]


def desktop_active_pane(snapshot: MuxSnapshot, attached: Optional[UInt64]) -> Optional[UInt64]:
    if not attached:
        return None
    for session in snapshot.sessions:
        if session.id == attached.value():
            var focused = snapshot.focused_window_for(session)
            for window in session.windows:
                if window.id == focused:
                    return window.active_pane
            return None
    return None


def desktop_modal_capture(snapshot: MuxSnapshot, attached: Optional[UInt64]) -> Optional[UInt64]:
    if not attached:
        return None
    for session in snapshot.sessions:
        if session.id == attached.value():
            var focused = snapshot.focused_window_for(session)
            for window in session.windows:
                if window.id == focused:
                    if window.modal and window.modal.value().capture_keys:
                        return window.modal.value().pane
                    return None
            return None
    return None


def desktop_snapshot_target(snapshot: MuxSnapshot, attached: Optional[UInt64]) raises -> DesktopSnapshotTarget:
    var active_id = desktop_active_pane(snapshot, attached)
    var capture = desktop_modal_capture(snapshot, attached)
    if active_id and attached:
        for session in snapshot.sessions:
            if session.id == attached.value():
                for window in session.windows:
                    if active_id.value() in window.panes:
                        var kind = window.panes[active_id.value()].kind.kind
                        return DesktopSnapshotTarget(
                            pane_owner(active_id.value(), UInt8(0 if kind == 1 else 1 if kind == 2 else 2)), capture
                        )
    return DesktopSnapshotTarget(None, capture)
