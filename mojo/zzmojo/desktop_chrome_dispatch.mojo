from .chrome_actions import ChromeAction, ChromeActions


@fieldwise_init
struct DesktopChromeDispatch(Equatable, TrivialRegisterPassable):
    var kind: UInt8
    var index: UInt8

    def __eq__(self, other: Self) -> Bool:
        return self.kind == other.kind and self.index == other.index


def desktop_chrome_dispatch(
    action: ChromeAction,
) raises -> DesktopChromeDispatch:
    if not ChromeActions().decode(action.code()):
        raise Error("desktop chrome action identity is invalid")
    if action.kind <= 6:
        return DesktopChromeDispatch(
            action.kind + 1, action.index if action.kind == 2 else UInt8(0)
        )
    if action.kind == 8 or action.kind == 9:
        return DesktopChromeDispatch(action.kind, 0)
    return DesktopChromeDispatch(0, 0)
