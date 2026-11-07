from .verified_input_router import RouterOwner, RouterEvent, RouterOperation, no_owner
from .chrome_keymap import ChromeKeymap


def desktop_router_map(configured: ChromeKeymap, scope: UInt32) raises -> ChromeKeymap:
    var result = ChromeKeymap(0, scope)
    result._bindings = configured.bindings()
    var bindings = result.bindings()
    for binding in bindings:
        if binding.action.kind >= 10 and binding.action.kind <= 12:
            _ = result.unbind(binding.table, binding.key)
    return result^


def desktop_intercepts(window_matches: Bool, key: String) -> Bool:
    return window_matches and key != "shift" and key != "control" and key != "alt" and key != "platform" and key != "function"


def desktop_guard(key: String, overlay: Bool, cancel_pending: Bool, drag: UInt8, modifiers: UInt8, function: Bool) raises -> UInt8:
    if drag > 2 or modifiers > 15:
        raise Error("invalid desktop guard input")
    if not overlay and key == "escape" and drag == 1:
        return 1
    if not overlay and cancel_pending and modifiers & 8 == 0 and not function:
        return 2
    return 3


@fieldwise_init
struct DesktopKeyProjection(Copyable, Movable):
    var event: Optional[RouterEvent]
    var capture: Optional[UInt64]
    var armed: Bool
    var claimed: Bool

    def operations(self) -> List[RouterOperation]:
        var result = List[RouterOperation]()
        if self.event:
            var operation = RouterOperation(0)
            operation.event = self.event.value()
            result.append(operation^)
        var capture = RouterOperation(1)
        capture.capture = self.capture
        result.append(capture^)
        return result^


def desktop_key_projection(before: Optional[RouterOwner], active: Optional[RouterOwner], capture: Optional[UInt64], armed: Bool, claimed: Bool, overlay: Bool, modifiers: UInt8, function: Bool) raises -> DesktopKeyProjection:
    if modifiers > 15:
        raise Error("invalid desktop modifier bits")
    for owner in [before, active]:
        if owner and (owner.value().kind != 1 or owner.value().surface > 2):
            raise Error("desktop active surface is not a pane")
    var event: Optional[RouterEvent] = None
    if before != active:
        if active:
            event = RouterEvent(1, active.value().pane, active.value().surface, no_owner())
        else:
            event = RouterEvent(8, 0, 0, no_owner())
    return DesktopKeyProjection(event, None if overlay else capture, armed, claimed and not overlay and modifiers & 8 == 0 and not function)
