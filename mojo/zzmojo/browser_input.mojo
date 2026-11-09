from std.math import trunc
from mjui.input import MOD_CTRL, MOD_ALT, MOD_CMD, MOD_SHIFT
from mjui.keys import Key

comptime BROWSER_POINTER_DOWN: UInt8 = 0
comptime BROWSER_POINTER_UP: UInt8 = 1
comptime BROWSER_POINTER_MOVE: UInt8 = 2
comptime BROWSER_POINTER_LEAVE: UInt8 = 3
comptime BROWSER_POINTER_UP_OUTSIDE: UInt8 = 4

comptime BROWSER_BUTTON_LEFT: UInt8 = 1
comptime BROWSER_BUTTON_RIGHT: UInt8 = 2
comptime BROWSER_BUTTON_MIDDLE: UInt8 = 4

comptime BROWSER_KEY_PASS: UInt8 = 0
comptime BROWSER_KEY_PAGE: UInt8 = 1
comptime BROWSER_KEY_CANCEL_PICK: UInt8 = 2
comptime BROWSER_KEY_RESTORE_QUERY: UInt8 = 3
comptime BROWSER_KEY_CLEAR_SUGGESTIONS: UInt8 = 4
comptime BROWSER_KEY_LEAVE_ADDRESS: UInt8 = 5
comptime BROWSER_KEY_OMNIBOX_NEXT: UInt8 = 6
comptime BROWSER_KEY_OMNIBOX_PREVIOUS: UInt8 = 7
comptime BROWSER_KEY_OMNIBOX_DELETE: UInt8 = 8
comptime BROWSER_KEY_ADDRESS: UInt8 = 9


@fieldwise_init
struct BrowserPointerRoute(TrivialRegisterPassable):
    var forward: Bool
    var x: Int32
    var y: Int32
    var focus_page: Bool


@fieldwise_init
struct BrowserKeyRoute(TrivialRegisterPassable):
    var kind: UInt8
    var text_follows: Bool


def browser_round(value: Float32) -> Int32:
    if value != value:
        return 0
    if value >= Float32(2147483520):
        return Int32.MAX
    if value <= Float32(-2147483648):
        return Int32.MIN
    var whole = trunc(value)
    var rest = value - whole
    if rest >= 0.5:
        whole += 1
    elif rest <= -0.5:
        whole -= 1
    return Int32(whole)


def browser_text_follows(key: UInt16, mods: UInt32) -> Bool:
    if mods & (MOD_CTRL | MOD_ALT | MOD_CMD) != 0:
        return False
    if key >= Key.ENTER and key <= Key.DOWN or key == Key.KP_ENTER:
        return False
    return not (key >= Key.F1 and key <= Key.F24)


struct BrowserInput(Copyable, Movable):
    var buttons: UInt8
    var address_editing: Bool
    var page_focused: Bool
    var omnibox_selected: Int
    var omnibox_suggestions: Int

    def __init__(out self):
        self.buttons = 0
        self.address_editing = False
        self.page_focused = False
        self.omnibox_selected = -1
        self.omnibox_suggestions = 0

    def focus_page(mut self):
        self.address_editing = False
        self.page_focused = True

    def focus_address(mut self):
        self.address_editing = True
        self.page_focused = False

    def pointer(
        mut self,
        phase: UInt8,
        button: UInt8,
        pressed: UInt8,
        left: Float32,
        top: Float32,
        width: Float32,
        height: Float32,
        x: Float32,
        y: Float32,
    ) -> BrowserPointerRoute:
        var focus = False
        var clamp: Bool
        if phase == BROWSER_POINTER_DOWN:
            focus = True
            self.focus_page()
            self.buttons |= button
            clamp = False
        elif phase == BROWSER_POINTER_UP or phase == BROWSER_POINTER_UP_OUTSIDE:
            if phase == BROWSER_POINTER_UP_OUTSIDE and self.buttons == 0:
                return BrowserPointerRoute(False, 0, 0, False)
            self.buttons &= ~button
            clamp = True
        elif phase == BROWSER_POINTER_MOVE:
            if pressed != 0 and self.buttons & pressed == 0:
                return BrowserPointerRoute(False, 0, 0, False)
            clamp = pressed != 0
        else:
            clamp = False
        var right = left + width
        var bottom = top + height
        if not clamp and not (x >= left and x < right and y >= top and y < bottom):
            return BrowserPointerRoute(False, 0, 0, focus)
        return BrowserPointerRoute(
            True,
            browser_round(min(max(x, left), right) - left),
            browser_round(min(max(y, top), bottom) - top),
            focus,
        )

    def key(
        mut self,
        key: UInt16,
        mods: UInt32,
        release: Bool,
        picking: Bool,
    ) -> BrowserKeyRoute:
        if not release and key == Key.ESCAPE:
            if picking:
                return BrowserKeyRoute(BROWSER_KEY_CANCEL_PICK, False)
            if self.address_editing:
                if self.omnibox_selected >= 0:
                    self.omnibox_selected = -1
                    return BrowserKeyRoute(BROWSER_KEY_RESTORE_QUERY, False)
                if self.omnibox_suggestions > 0:
                    self.omnibox_suggestions = 0
                    return BrowserKeyRoute(BROWSER_KEY_CLEAR_SUGGESTIONS, False)
                self.focus_page()
                return BrowserKeyRoute(BROWSER_KEY_LEAVE_ADDRESS, False)
        if self.address_editing:
            if not release and mods & (MOD_CTRL | MOD_ALT | MOD_CMD) == 0:
                if key == Key.DOWN and mods & MOD_SHIFT == 0:
                    return BrowserKeyRoute(BROWSER_KEY_OMNIBOX_NEXT, False)
                if key == Key.UP and mods & MOD_SHIFT == 0:
                    return BrowserKeyRoute(BROWSER_KEY_OMNIBOX_PREVIOUS, False)
                if key == Key.DELETE and mods & MOD_SHIFT != 0:
                    return BrowserKeyRoute(BROWSER_KEY_OMNIBOX_DELETE, False)
            return BrowserKeyRoute(BROWSER_KEY_ADDRESS, False)
        if not self.page_focused:
            return BrowserKeyRoute(BROWSER_KEY_PASS, False)
        return BrowserKeyRoute(
            BROWSER_KEY_PAGE, not release and browser_text_follows(key, mods)
        )
