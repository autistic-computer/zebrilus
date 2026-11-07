comptime MAX_ROUTER_BYTES = 1024 * 1024
comptime MAX_ROUTER_CLAIMS = 4096
comptime MAX_ROUTER_EFFECTS = 4096
comptime NATIVE_ROUTER_BYTES = 65536
comptime NATIVE_ROUTER_CLAIMS = 256
comptime NATIVE_ROUTER_EFFECTS = 512

@fieldwise_init
struct RouterKey(TrivialRegisterPassable, Equatable):
    var id: UInt64

    def __eq__(self, other: Self) -> Bool:
        return self.id == other.id


@fieldwise_init
struct RouterPayload(TrivialRegisterPassable, Equatable):
    var id: UInt64

    def __eq__(self, other: Self) -> Bool:
        return self.id == other.id


@fieldwise_init
struct RouterAction(TrivialRegisterPassable, Equatable):
    var id: UInt64

    def __eq__(self, other: Self) -> Bool:
        return self.id == other.id


@fieldwise_init
struct RouterOwner(TrivialRegisterPassable, Equatable):
    var kind: UInt8
    var pane: UInt64
    var surface: UInt8

    def __eq__(self, other: Self) -> Bool:
        return self.kind == other.kind and self.pane == other.pane and self.surface == other.surface


def no_owner() -> RouterOwner:
    return RouterOwner(0, 0, 0)


def pane_owner(pane: UInt64, surface: UInt8) -> RouterOwner:
    return RouterOwner(1, pane, surface)


@fieldwise_init
struct RouterInput(Copyable, Movable, Equatable):
    var key: RouterKey
    var unshifted: Optional[RouterKey]
    var action: UInt8
    var payload: RouterPayload

    def __eq__(self, other: Self) -> Bool:
        return self.key == other.key and self.unshifted == other.unshifted and self.action == other.action and self.payload == other.payload

    def identity(self) -> RouterKey:
        return self.unshifted.value() if self.unshifted else self.key


def empty_input() -> RouterInput:
    return RouterInput(RouterKey(0), None, 0, RouterPayload(0))


struct RouterEffect(Copyable, Movable):
    var kind: UInt8
    var pane: UInt64
    var input: RouterInput
    var action: RouterAction
    var owner: RouterOwner

    def __init__(out self, kind: UInt8):
        self.kind = kind
        self.pane = 0
        self.input = empty_input()
        self.action = RouterAction(0)
        self.owner = no_owner()


@fieldwise_init
struct RouterHeld(TrivialRegisterPassable):
    var key: RouterKey
    var pane: UInt64


struct RouterState(Copyable, Movable):
    var keymap: UInt64
    var owner: RouterOwner
    var pending: Optional[RouterOwner]
    var observed: RouterOwner
    var restore: RouterOwner
    var active: Optional[RouterOwner]
    var capture: Optional[UInt64]
    var held: List[RouterHeld]
    var local: List[RouterKey]
    var effects: List[RouterEffect]

    def __init__(out self, keymap: UInt64 = 0):
        self.keymap = keymap
        self.owner = no_owner()
        self.pending = None
        self.observed = no_owner()
        self.restore = no_owner()
        self.active = None
        self.capture = None
        self.held = List[RouterHeld]()
        self.local = List[RouterKey]()
        self.effects = List[RouterEffect]()

    def effective(self) -> RouterOwner:
        return self.pending.value() if self.pending else self.owner


@fieldwise_init
struct RouterEvent(TrivialRegisterPassable):
    var kind: UInt8
    var pane: UInt64
    var surface: UInt8
    var owner: RouterOwner


struct RouterOperation(Copyable, Movable):
    var kind: UInt8
    var event: RouterEvent
    var capture: Optional[UInt64]
    var key: RouterKey
    var input: RouterInput
    var armed: Bool
    var claimed: Bool
    var lookups: List[Optional[RouterAction]]
    var releases: Bool
    var ui_binds: Bool
    var terminal_binds: Bool

    def __init__(out self, kind: UInt8):
        self.kind = kind
        self.event = RouterEvent(0, 0, 0, no_owner())
        self.capture = None
        self.key = RouterKey(0)
        self.input = empty_input()
        self.armed = False
        self.claimed = False
        self.lookups = List[Optional[RouterAction]](length=4, fill=None)
        self.releases = False
        self.ui_binds = False
        self.terminal_binds = False


struct RouterAnswer(Copyable, Movable):
    var kind: UInt8
    var value: Bool
    var effects: List[RouterEffect]

    def __init__(out self, kind: UInt8 = 0, value: Bool = False):
        self.kind = kind
        self.value = value
        self.effects = List[RouterEffect]()


@fieldwise_init
struct RouterOutcome(Copyable, Movable):
    var state: RouterState
    var answer: RouterAnswer


def _consume_held(mut state: RouterState, key: RouterKey) -> Optional[UInt64]:
    for index in range(len(state.held)):
        if state.held[index].key == key:
            var pane = state.held[index].pane
            state.held[index] = state.held[len(state.held) - 1]
            _ = state.held.pop()
            return pane
    return None


def _clear_local(mut state: RouterState, key: RouterKey):
    var remaining = List[RouterKey]()
    for held in state.local:
        if held != key:
            remaining.append(held)
    state.local = remaining^


def _suppress(mut state: RouterState, key: RouterKey):
    for held in state.local:
        if held == key:
            return
    state.local.append(key)


def _forward(mut state: RouterState, input: RouterInput, pane: UInt64, repeat: Bool):
    if repeat:
        return
    var key = input.identity()
    _clear_local(state, key)
    _ = _consume_held(state, key)
    state.held.append(RouterHeld(key, pane))
    var effect = RouterEffect(0)
    effect.pane = pane
    effect.input = input.copy()
    state.effects.append(effect^)


def _chrome(mut state: RouterState, key: RouterKey, action: RouterAction):
    _suppress(state, key)
    var effect = RouterEffect(1)
    effect.action = action
    state.effects.append(effect^)


def _focus_effect(mut state: RouterState, owner: RouterOwner):
    var effect = RouterEffect(2)
    effect.owner = owner
    state.effects.append(effect^)


def _remove_pane(mut state: RouterState, pane: UInt64):
    if state.owner.kind == 1 and state.owner.pane == pane:
        state.owner = no_owner()
    if state.pending and state.pending.value().kind == 1 and state.pending.value().pane == pane:
        state.pending = None
        state.owner = no_owner()
    if state.observed.kind == 1 and state.observed.pane == pane:
        state.observed = no_owner()
    if state.restore.kind == 1 and state.restore.pane == pane:
        state.restore = no_owner()
    if state.active and state.active.value().pane == pane:
        state.active = None
    var local = List[RouterKey]()
    for key in state.local:
        var removed = False
        for held in state.held:
            removed = removed or (held.key == key and held.pane == pane)
        if not removed:
            local.append(key)
    var remaining = List[RouterHeld]()
    for held in state.held:
        if held.pane != pane:
            remaining.append(held)
    state.held = remaining^
    state.local = local^


def _event(mut state: RouterState, event: RouterEvent):
    if event.kind <= 1:
        var target = pane_owner(event.pane, event.surface)
        state.active = target
        if event.kind == 0:
            if state.effective().kind == 3:
                state.restore = target
            else:
                state.pending = target
                _focus_effect(state, target)
    elif event.kind == 2 or event.kind == 3:
        var target = RouterOwner(UInt8(2 if event.kind == 2 else 4), 0, 0)
        var previous = state.effective()
        if previous.kind == 3:
            state.restore = target
        else:
            if previous.kind == 1:
                state.restore = previous
            state.pending = None
            state.owner = target
            _focus_effect(state, target)
    elif event.kind == 4:
        var previous = state.effective()
        if previous.kind != 3:
            state.restore = previous
            state.pending = None
            state.owner = RouterOwner(3, 0, 0)
    elif event.kind == 5:
        if state.effective().kind == 3:
            state.owner = state.restore
            state.pending = None
            _focus_effect(state, state.owner)
    elif event.kind == 6:
        state.observed = event.owner
        if not state.pending or state.pending.value() == event.owner:
            state.owner = event.owner
            state.pending = None
    elif event.kind == 7:
        _remove_pane(state, event.pane)
    elif event.kind == 8:
        state.owner = no_owner()
        state.pending = None
        state.observed = no_owner()
        state.restore = no_owner()
        state.active = None
        state.held.clear()
        state.local.clear()
    else:
        state.held.clear()
        state.local.clear()


def _key(mut state: RouterState, operation: RouterOperation) -> Bool:
    ref input = operation.input
    var key = input.identity()
    if input.action == 2:
        for index in range(len(state.local)):
            if state.local[index] == key:
                state.local[index] = state.local[len(state.local) - 1]
                _ = state.local.pop()
                _ = _consume_held(state, key)
                return False
        var pane = _consume_held(state, key)
        if pane:
            var effect = RouterEffect(0)
            effect.pane = pane.value()
            effect.input = input.copy()
            state.effects.append(effect^)
            return False
        return True
    _clear_local(state, key)
    var owner = state.effective()
    if owner.kind == 3:
        return True
    if state.capture:
        _forward(state, input, state.capture.value(), False)
        return False
    if operation.lookups[0]:
        _chrome(state, key, operation.lookups[0].value())
        return False
    if operation.claimed and state.active:
        _forward(state, input, state.active.value().pane, input.action == 1 and operation.armed)
        return False
    var table = 0
    if owner.kind == 2:
        table = 1
    elif owner.kind == 1 and owner.surface == 0:
        table = 2
    elif owner.kind == 1 and owner.surface == 1:
        table = 3
    if table != 0 and operation.lookups[table]:
        _chrome(state, key, operation.lookups[table].value())
        return False
    return True


def reduce_router(state: RouterState, operation: RouterOperation) -> RouterOutcome:
    var next = state.copy()
    var answer = RouterAnswer()
    if operation.kind == 0:
        _event(next, operation.event)
    elif operation.kind == 1:
        next.capture = operation.capture.copy()
    elif operation.kind == 2:
        _suppress(next, operation.key)
    elif operation.kind == 3:
        answer = RouterAnswer(1, _key(next, operation))
    elif operation.kind == 4:
        answer = RouterAnswer(2)
        swap(answer.effects, next.effects)
    else:
        var owner = next.effective()
        var terminal = owner.kind == 1 and owner.surface == 0
        answer = RouterAnswer(3, (owner.kind == 0 or terminal) and not next.capture and len(next.effects) == 0 and (not operation.releases or len(next.local) == 0) and not operation.ui_binds and (not terminal or not operation.terminal_binds))
    return RouterOutcome(next^, answer^)
