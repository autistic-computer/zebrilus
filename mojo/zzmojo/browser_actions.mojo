from mjui.web_zoom import (
    browser_zoom_factor,
    browser_zoom_percent,
    browser_zoom_step,
)
from .browser_controller import (
    BrowserPlan,
    accept_browser_address,
    bind_browser_effects,
)
from .browser_host import BrowserHost
from .browser_state import (
    BrowserModel,
    BrowserEffect,
    BROWSER_BACK,
    BROWSER_CANCEL_PICK,
    BROWSER_DEVTOOLS,
    BROWSER_EDIT,
    BROWSER_FOCUS_ADDRESS,
    BROWSER_FOCUS_PAGE,
    BROWSER_FORWARD,
    BROWSER_NOTIFY,
    BROWSER_RECREATE,
    BROWSER_RELOAD,
    BROWSER_START_PICK,
    BROWSER_ZOOM,
)
from .browser_wire import BrowserCommand
from .desktop_native_bindings import DesktopNativeBinding

comptime BROWSER_ACTION_ZOOM_IN: UInt8 = 1
comptime BROWSER_ACTION_ZOOM_OUT: UInt8 = 2
comptime BROWSER_ACTION_ZOOM_RESET: UInt8 = 3
comptime BROWSER_ACTION_DEVTOOLS: UInt8 = 4
comptime BROWSER_ACTION_NEW_TAB: UInt8 = 5
comptime BROWSER_ACTION_NEXT_TAB: UInt8 = 6
comptime BROWSER_ACTION_PREVIOUS_TAB: UInt8 = 7
comptime BROWSER_ACTION_SELECT_TAB: UInt8 = 8
comptime BROWSER_ACTION_SELECT_LAST_TAB: UInt8 = 9
comptime BROWSER_ACTION_BACK: UInt8 = 10
comptime BROWSER_ACTION_FORWARD: UInt8 = 11
comptime BROWSER_ACTION_RELOAD: UInt8 = 12
comptime BROWSER_ACTION_FOCUS_ADDRESS: UInt8 = 13
comptime BROWSER_ACTION_ELEMENT_SELECTOR: UInt8 = 14
comptime BROWSER_ACTION_UNDO: UInt8 = 15
comptime BROWSER_ACTION_SELECT_ALL: UInt8 = 21
comptime BROWSER_ACTION_CLOSE_PANE: UInt8 = 22

comptime BROWSER_RUNTIME_READY: UInt8 = 0
comptime BROWSER_RUNTIME_FAILED: UInt8 = 1
comptime BROWSER_PANE_FAILED: UInt8 = 2

comptime BROWSER_PICKER_UNAVAILABLE = "Element picker is unavailable"


def browser_edit_name(action: UInt8) raises -> String:
    if action == 15:
        return String("undo")
    if action == 16:
        return String("redo")
    if action == 17:
        return String("cut")
    if action == 18:
        return String("copy")
    if action == 19:
        return String("paste")
    if action == 20:
        return String("paste-and-match-style")
    if action == 21:
        return String("select-all")
    raise Error("browser action is not an edit command")


@fieldwise_init
struct BrowserActionPlan(Movable):
    var handled: Bool
    var plan: BrowserPlan

    def commit(
        deinit self, mut current: BrowserModel, host: BrowserHost
    ) raises -> Bool:
        var handled = self.handled
        self.plan^.commit(current, host)
        return handled


def _live(pane: UInt64, model: BrowserModel, host: BrowserHost) -> Bool:
    for entry in host.sessions:
        if entry.pane == pane and entry.tab == model.active_tab:
            return True
    return False


def _cancel_pick(mut model: BrowserModel):
    if model.element_pick_active:
        model.effects.append(
            BrowserEffect(BROWSER_CANCEL_PICK, model.active_tab, String(), 0)
        )
        model.element_pick_active = False


def _plan(
    pane: UInt64,
    model: BrowserModel,
    host: BrowserHost,
    var next: BrowserModel,
) raises -> BrowserPlan:
    bind_browser_effects(pane, len(model.effects), next, host)
    next.validate()
    return BrowserPlan(
        model.generation,
        model.identity(),
        None,
        host.generation,
        host.actor.copy(),
        next^,
    )


def _check(pane: UInt64, model: BrowserModel) raises:
    model.validate()
    if not model.pane or model.pane.value() != pane:
        raise Error("browser state belongs to another pane")


@inline(.never)
def prepare_browser_action(
    pane: UInt64,
    model: BrowserModel,
    host: BrowserHost,
    binding: DesktopNativeBinding,
) raises -> BrowserActionPlan:
    _check(pane, model)
    var next = model.copy()
    var handled = True
    var kind = binding.kind
    var tab = next.active_tab
    if kind >= BROWSER_ACTION_ZOOM_IN and kind <= BROWSER_ACTION_ZOOM_RESET:
        if _live(pane, next, host):
            var factor = browser_zoom_factor(
                Float64(1) if kind
                == BROWSER_ACTION_ZOOM_RESET else browser_zoom_step(
                    next.current.page_zoom_factor,
                    Int8(1) if kind == BROWSER_ACTION_ZOOM_IN else Int8(-1),
                )
            )
            next.current.page_zoom_factor = factor
            next.current.page_zoom_percent = browser_zoom_percent(factor)
            next.effects.append(BrowserEffect(BROWSER_ZOOM, tab, String(), factor))
            if next.element_pick_active:
                next.effects.append(
                    BrowserEffect(BROWSER_START_PICK, tab, String(), factor)
                )
    elif kind == BROWSER_ACTION_DEVTOOLS:
        if _live(pane, next, host):
            next.effects.append(BrowserEffect(BROWSER_DEVTOOLS, tab, String(), 0))
    elif kind == BROWSER_ACTION_NEW_TAB:
        _ = next.open_tab(None, True)
    elif kind == BROWSER_ACTION_NEXT_TAB or kind == BROWSER_ACTION_PREVIOUS_TAB:
        var target = next.cycle(
            Int64(1) if kind == BROWSER_ACTION_NEXT_TAB else Int64(-1)
        )
        if target:
            next.activate(target.value())
    elif kind == BROWSER_ACTION_SELECT_TAB:
        if Int(binding.index) < len(next.tabs):
            next.activate(next.tabs[Int(binding.index)].id)
    elif kind == BROWSER_ACTION_SELECT_LAST_TAB:
        next.activate(next.tabs[len(next.tabs) - 1].id)
    elif kind == BROWSER_ACTION_BACK or kind == BROWSER_ACTION_FORWARD:
        if (
            next.current.can_go_back if kind
            == BROWSER_ACTION_BACK else next.current.can_go_forward
        ):
            _cancel_pick(next)
            next.drop_history(tab)
            next.effects.append(
                BrowserEffect(
                    BROWSER_BACK if kind
                    == BROWSER_ACTION_BACK else BROWSER_FORWARD,
                    tab,
                    String(),
                    0,
                )
            )
    elif kind == BROWSER_ACTION_RELOAD:
        _cancel_pick(next)
        next.drop_history(tab)
        next.current.error = None
        next.effects.append(BrowserEffect(BROWSER_RELOAD, tab, String(), 0))
    elif kind == BROWSER_ACTION_FOCUS_ADDRESS:
        next.effects.append(BrowserEffect(BROWSER_FOCUS_ADDRESS, tab, String(), 0))
    elif kind == BROWSER_ACTION_ELEMENT_SELECTOR:
        if next.element_pick_active:
            _cancel_pick(next)
        else:
            next.effects.append(
                BrowserEffect(
                    BROWSER_START_PICK,
                    tab,
                    String(),
                    next.current.page_zoom_factor,
                    flag=True,
                )
            )
    elif kind >= BROWSER_ACTION_UNDO and kind <= BROWSER_ACTION_SELECT_ALL:
        next.effects.append(
            BrowserEffect(BROWSER_EDIT, tab, browser_edit_name(kind), 0)
        )
    elif kind == BROWSER_ACTION_CLOSE_PANE:
        if len(next.tabs) > 1:
            next.close_tab(tab)
        else:
            handled = False
    else:
        handled = False
    return BrowserActionPlan(handled, _plan(pane, model, host, next^))


@inline(.never)
def prepare_browser_pick_result(
    pane: UInt64,
    model: BrowserModel,
    host: BrowserHost,
    started: Bool,
    toggled: Bool,
) raises -> BrowserPlan:
    _check(pane, model)
    var next = model.copy()
    if started and toggled:
        next.element_pick_active = True
        next.effects.append(
            BrowserEffect(BROWSER_FOCUS_PAGE, next.active_tab, String(), 0)
        )
    elif not started and (toggled or next.element_pick_active):
        next.element_pick_active = False
        next.effects.append(
            BrowserEffect(
                BROWSER_NOTIFY,
                next.active_tab,
                String(BROWSER_PICKER_UNAVAILABLE),
                0,
                flag=True,
            )
        )
    return _plan(pane, model, host, next^)


@inline(.never)
def prepare_browser_failure(
    pane: UInt64,
    model: BrowserModel,
    host: BrowserHost,
    kind: UInt8,
    message: String = String(),
    running: Bool = False,
) raises -> BrowserPlan:
    _check(pane, model)
    if message.byte_length() > 65536:
        raise Error("browser failure message exceeds retained input admission")
    var next = model.copy()
    if kind == BROWSER_RUNTIME_READY:
        next.recoverable = True
        next.current.error = None
    elif kind == BROWSER_RUNTIME_FAILED or kind == BROWSER_PANE_FAILED:
        next.element_pick_active = False
        next.pending_history.clear()
        next.current.error = message
        next.recoverable = running if kind == BROWSER_RUNTIME_FAILED else True
    else:
        raise Error("unknown browser runtime event")
    return _plan(pane, model, host, next^)


@inline(.never)
def prepare_browser_retry(
    pane: UInt64, model: BrowserModel, host: BrowserHost
) raises -> BrowserPlan:
    _check(pane, model)
    var next = model.copy()
    if next.recoverable:
        _cancel_pick(next)
        next.drop_history(next.active_tab)
        next.current.error = None
        next.effects.append(
            BrowserEffect(
                BROWSER_RECREATE,
                next.active_tab,
                next.current.url,
                next.current.page_zoom_factor,
                next.profile,
            )
        )
    return _plan(pane, model, host, next^)


@inline(.never)
def prepare_browser_command(
    pane: UInt64,
    model: BrowserModel,
    host: BrowserHost,
    command: BrowserCommand,
    provider: UInt32 = 0,
) raises -> BrowserPlan:
    _check(pane, model)
    var next = model.copy()
    var tab = next.active_tab
    if command.kind == 0:
        accept_browser_address(next, command.text, provider)
    elif command.kind >= 1 and command.kind <= 3:
        _cancel_pick(next)
        next.drop_history(tab)
        next.effects.append(
            BrowserEffect(
                BROWSER_RELOAD if command.kind
                == 1 else BROWSER_BACK if command.kind
                == 2 else BROWSER_FORWARD,
                tab,
                String(),
                0,
            )
        )
    else:
        raise Error("browser command is delivered to the page, not the model")
    return _plan(pane, model, host, next^)
