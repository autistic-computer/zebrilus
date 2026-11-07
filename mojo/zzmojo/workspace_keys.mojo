from mjui.ui import Ui
from mjui.input import KeyEvent, TextInput, TEXT_COMMAND, CMD_CANCEL
from .view import WorkspaceView, TERMINAL_KEY
from .keyboard_controller import KeyboardController, RoutedKey
from .clipboard_controller import ClipboardController, ClipboardPlan
from .terminal_search_controller import SearchPlan
from .palette_runtime import PalettePlan


@fieldwise_init
struct RawWorkspaceKey(TrivialRegisterPassable):
    var key: KeyEvent
    var cancel: Bool


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


def raw_workspace_key(key: KeyEvent, input: TextInput) -> RawWorkspaceKey:
    var cancel = (
        key.text_event >= 0
        and Int(key.text_event) < len(input.events)
        and input.events[Int(key.text_event)].kind == TEXT_COMMAND
        and input.events[Int(key.text_event)].cmd == CMD_CANCEL
    )
    return RawWorkspaceKey(key, cancel)


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
