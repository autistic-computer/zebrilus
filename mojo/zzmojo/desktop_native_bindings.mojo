from .chrome_actions import ChromeAction, ChromeActions


@fieldwise_init
struct DesktopNativeBinding(TrivialRegisterPassable):
    var kind: UInt8
    var index: UInt8


def desktop_native_binding(context: UInt8, action: ChromeAction) raises -> DesktopNativeBinding:
    if context > 2 or not ChromeActions().decode(action.code()):
        raise Error("native chrome context or action identity is invalid")
    var kind = action.kind
    if context == 0:
        if kind == 23 or kind == 24:
            return DesktopNativeBinding(1, 0)
        if kind == 26:
            return DesktopNativeBinding(2, 0)
        if kind == 29:
            return DesktopNativeBinding(3, 0)
        if kind == 27:
            return DesktopNativeBinding(4, 0)
        if kind == 25:
            return DesktopNativeBinding(5, 0)
        if kind == 28:
            return DesktopNativeBinding(6, 0)
    elif context == 1:
        if kind >= 13 and kind <= 22:
            return DesktopNativeBinding(kind - 12, 0)
    else:
        if kind >= 30 and kind <= 50:
            return DesktopNativeBinding(kind - 29, action.index if kind == 37 else UInt8(0))
        if kind == 7:
            return DesktopNativeBinding(22, 0)
    return DesktopNativeBinding(0, 0)
