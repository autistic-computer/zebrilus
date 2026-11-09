from mjui.ui import Ui
from mjui.input import KeyEvent, TextInput, TEXT_COMMAND, CMD_CANCEL
from mjui.keys import Key
from .view import WorkspaceView, TERMINAL_KEY
from .keyboard_controller import KeyboardController, RoutedKey
from .clipboard_controller import ClipboardController, ClipboardPlan
from .terminal_search_controller import SearchPlan
from .palette_runtime import PalettePlan
from .input_protocol import mjui_key_input
from .desktop_input import desktop_intercepts
from .which_key_live import WhichKeyIntent


struct RawWorkspaceKey(Copyable, Movable):
    var key: KeyEvent
    var cancel: Bool
    var chooser_scope: Optional[UInt64]

    def __init__(
        out self,
        key: KeyEvent,
        cancel: Bool,
        chooser_scope: Optional[UInt64] = None,
    ):
        self.key = key
        self.cancel = cancel
        self.chooser_scope = chooser_scope


struct WorkspaceKeyRoute(Movable):
    var claimed: Bool
    var unmark: Bool
    var key: Optional[RoutedKey]
    var clipboard: Optional[ClipboardPlan]
    var search: Optional[SearchPlan]
    var palette: Optional[PalettePlan]
    var before: UInt64
    var after: UInt64

    def __init__(out self, focus: UInt64):
        self.claimed = False
        self.unmark = False
        self.key = None
        self.clipboard = None
        self.search = None
        self.palette = None
        self.before = focus
        self.after = focus


def raw_workspace_key(
    key: KeyEvent, input: TextInput, chooser_scope: Optional[UInt64] = None
) -> RawWorkspaceKey:
    var cancel = (
        key.text_event >= 0
        and Int(key.text_event) < len(input.events)
        and input.events[Int(key.text_event)].kind == TEXT_COMMAND
        and input.events[Int(key.text_event)].cmd == CMD_CANCEL
    )
    return RawWorkspaceKey(key, cancel, chooser_scope)


def _modifier_name(key: UInt16) -> String:
    if key == Key.SHIFT_L or key == Key.SHIFT_R:
        return "shift"
    if key == Key.CTRL_L or key == Key.CTRL_R:
        return "control"
    if key == Key.ALT_L or key == Key.ALT_R:
        return "alt"
    if key == Key.CMD_L or key == Key.CMD_R:
        return "platform"
    if key == Key.FN:
        return "function"
    return String()


def route_workspace_key(
    mut view: WorkspaceView,
    mut ui: Ui,
    mut keyboard: KeyboardController,
    mut clipboard: ClipboardController,
    raw: RawWorkspaceKey,
    input: TextInput,
) raises -> WorkspaceKeyRoute:
    var key = raw.key
    var result = WorkspaceKeyRoute(ui.focus)
    if not key.release:
        view.which_key.apply(
            WhichKeyIntent.input(
                desktop_intercepts(True, _modifier_name(key.key)),
                key.repeat,
                view.model.routing.armed,
            )
        )
    if (
        not key.release
        and raw.chooser_scope
        and (
            not view.chooser.opened()
            or raw.chooser_scope.value() != view.chooser.session.scope
        )
    ):
        keyboard.claim_text(key, Span(input.bytes))
        keyboard.claim_text_command(key)
        result.claimed = True
        return result^
    if view.chooser.opened() and not view.palette.opened():
        result.claimed = True
        if key.release:
            result.key = keyboard.route(
                view.model,
                False,
                key,
                Span(input.bytes),
                view.focus_epoch,
                view.input_pane(),
            )
            return result^
        if not raw.chooser_scope or view.chooser_unmark:
            keyboard.claim_text(key, Span(input.bytes))
            keyboard.claim_text_command(key)
            return result^
        if not view.active or key.consumed or not view.chooser.input_scope(ui):
            return result^
        var normalized = mjui_key_input(
            key.key, key.ch, key.unshifted, key.mods, key.action()
        )
        var choice = view.chooser.session.key(normalized, Bool(normalized.text))
        if choice:
            keyboard.claim_text(key, Span(input.bytes))
            keyboard.claim_text_command(key)
            view.dispatch_chooser(ui, choice.value())
        result.after = ui.focus
        return result^
    var palette = view.plan_palette(
        ui, keyboard, key, Span(input.bytes), raw.cancel
    )
    if palette:
        if palette.value().kind != 1:
            result.palette = palette.take()
        result.unmark = result.palette and result.palette.value().kind == 0
        result.claimed = True
        result.after = ui.focus
        return result^
    if view.palette.opened():
        if key.release:
            result.key = keyboard.route(
                view.model,
                False,
                key,
                Span(input.bytes),
                view.focus_epoch,
                view.input_pane(),
            )
            if result.key:
                result.claimed = True
                return result^
        result.claimed = ui.key(key)
        result.after = ui.focus
        return result^
    var search = view.plan_search(ui, keyboard, key, Span(input.bytes))
    if search:
        if search.value().kind != 2:
            result.search = search.take()
        result.unmark = result.search and result.search.value().kind == 0
        result.claimed = True
        result.after = ui.focus
        return result^
    result.clipboard = clipboard.plan(
        view,
        ui,
        keyboard,
        key,
        Span(input.bytes),
        composed=len(input.marked) > 0,
    )
    if result.clipboard:
        result.claimed = True
        ui.request_wake(ui.time)
        return result^
    result.key = keyboard.route(
        view.model,
        ui.focus == TERMINAL_KEY and view.active,
        key,
        Span(input.bytes),
        view.focus_epoch,
        view.input_pane(),
    )
    if result.key:
        ui.request_wake(ui.time)
    result.claimed = keyboard.claimed
    if not result.claimed:
        result.claimed = ui.key(key)
    result.after = ui.focus
    return result^
