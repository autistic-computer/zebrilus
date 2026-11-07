from .verified_input_router import (
    RouterKey, RouterPayload, RouterAction, RouterOwner, RouterInput,
    RouterEffect, RouterHeld, RouterState, RouterEvent, RouterOperation,
    RouterAnswer, RouterOutcome, MAX_ROUTER_BYTES, MAX_ROUTER_CLAIMS,
    MAX_ROUTER_EFFECTS, NATIVE_ROUTER_BYTES, NATIVE_ROUTER_CLAIMS,
    NATIVE_ROUTER_EFFECTS, no_owner, pane_owner,
)

comptime ROUTER_MAGIC = 1230131266


def _owner_valid(value: RouterOwner) -> Bool:
    return value.kind <= 4 and ((value.kind == 1 and value.surface <= 2) or (value.kind != 1 and value.pane == 0 and value.surface == 0))


def _input_valid(value: RouterInput) -> Bool:
    return value.action <= 2


def _effect_words(value: RouterEffect) raises -> Int:
    if value.kind == 0 and _input_valid(value.input):
        return 9 + (2 if value.input.unshifted else 0)
    if value.kind == 1:
        return 3
    if value.kind == 2 and _owner_valid(value.owner):
        return 2 + (3 if value.owner.kind == 1 else 0)
    raise Error("invalid router effect")


def state_words(state: RouterState) raises -> Int:
    if len(state.held) > MAX_ROUTER_CLAIMS or len(state.local) > MAX_ROUTER_CLAIMS or len(state.effects) > MAX_ROUTER_EFFECTS:
        raise Error("router retained state exceeds outer count limit")
    var words = 8 + len(state.held) * 4 + len(state.local) * 2
    for owner in [state.owner, state.observed, state.restore]:
        if not _owner_valid(owner):
            raise Error("invalid router focus owner")
        words += 1 + (3 if owner.kind == 1 else 0)
    if state.pending:
        if not _owner_valid(state.pending.value()):
            raise Error("invalid pending router focus")
        words += 1 + (3 if state.pending.value().kind == 1 else 0)
    if state.active:
        if state.active.value().kind != 1 or not _owner_valid(state.active.value()):
            raise Error("invalid active router pane")
        words += 3
    if state.capture:
        words += 2
    for effect in state.effects:
        words += _effect_words(effect)
    return words


def operation_words(operation: RouterOperation) raises -> Int:
    if operation.kind == 0:
        var event = operation.event
        if event.kind > 9 or event.surface > 2 or not _owner_valid(event.owner):
            raise Error("invalid router event")
        return 2 + (3 if event.kind <= 1 else 2 if event.kind == 7 else (1 + (3 if event.owner.kind == 1 else 0)) if event.kind == 6 else 0)
    if operation.kind == 1:
        return 2 + (2 if operation.capture else 0)
    if operation.kind == 2:
        return 3
    if operation.kind == 3:
        if not _input_valid(operation.input) or len(operation.lookups) != 4:
            raise Error("invalid router key operation")
        var count = 13 + (2 if operation.input.unshifted else 0)
        for action in operation.lookups:
            if action:
                count += 2
        return count
    if operation.kind == 4:
        return 1
    if operation.kind == 5:
        return 4
    raise Error("invalid router operation")


def admit_router(state: RouterState, operation: RouterOperation) raises:
    if (2 + state_words(state) + operation_words(operation)) * 4 > MAX_ROUTER_BYTES:
        raise Error("router request exceeds outer byte limit")
    if len(state.effects) >= MAX_ROUTER_EFFECTS and (operation.kind == 0 or operation.kind == 3):
        raise Error("router transition has no effect queue capacity")
    if len(state.held) >= MAX_ROUTER_CLAIMS and operation.kind == 3 and operation.input.action != 2:
        raise Error("router transition has no held claim capacity")
    if len(state.local) >= MAX_ROUTER_CLAIMS and (operation.kind == 2 or operation.kind == 3):
        raise Error("router transition has no local claim capacity")


def native_eligible(state: RouterState, operation: RouterOperation) raises -> Bool:
    admit_router(state, operation)
    return (2 + state_words(state) + operation_words(operation)) * 4 <= NATIVE_ROUTER_BYTES and len(state.held) + 1 <= NATIVE_ROUTER_CLAIMS and len(state.local) + 1 <= NATIVE_ROUTER_CLAIMS and len(state.effects) + 1 <= NATIVE_ROUTER_EFFECTS


def _id(mut words: List[UInt32], value: UInt64):
    words.append(UInt32(value >> 32))
    words.append(UInt32(value & 0xFFFFFFFF))


def _optional_id(mut words: List[UInt32], value: Optional[UInt64]):
    words.append(UInt32(Bool(value)))
    if value:
        _id(words, value.value())


def _owner(mut words: List[UInt32], value: RouterOwner):
    words.append(UInt32(value.kind))
    if value.kind == 1:
        _id(words, value.pane)
        words.append(UInt32(value.surface))


def _optional_owner(mut words: List[UInt32], value: Optional[RouterOwner]):
    words.append(UInt32(Bool(value)))
    if value:
        _owner(words, value.value())


def _input(mut words: List[UInt32], value: RouterInput):
    _id(words, value.key.id)
    words.append(UInt32(Bool(value.unshifted)))
    if value.unshifted:
        _id(words, value.unshifted.value().id)
    words.append(UInt32(value.action))
    _id(words, value.payload.id)


def _effect(mut words: List[UInt32], value: RouterEffect):
    words.append(UInt32(value.kind))
    if value.kind == 0:
        _id(words, value.pane)
        _input(words, value.input)
    elif value.kind == 1:
        _id(words, value.action.id)
    else:
        _owner(words, value.owner)


def _effects(mut words: List[UInt32], effects: List[RouterEffect]):
    words.append(UInt32(len(effects)))
    for value in effects:
        _effect(words, value)


def encode_state(mut words: List[UInt32], state: RouterState):
    _id(words, state.keymap)
    _owner(words, state.owner)
    _optional_owner(words, state.pending)
    _owner(words, state.observed)
    _owner(words, state.restore)
    words.append(UInt32(Bool(state.active)))
    if state.active:
        _id(words, state.active.value().pane)
        words.append(UInt32(state.active.value().surface))
    _optional_id(words, state.capture)
    words.append(UInt32(len(state.held)))
    for value in state.held:
        _id(words, value.key.id)
        _id(words, value.pane)
    words.append(UInt32(len(state.local)))
    for value in state.local:
        _id(words, value.id)
    _effects(words, state.effects)


def encode_request(state: RouterState, operation: RouterOperation) raises -> List[UInt32]:
    admit_router(state, operation)
    var words: List[UInt32] = [ROUTER_MAGIC, 1]
    encode_state(words, state)
    words.append(UInt32(operation.kind))
    if operation.kind == 0:
        var value = operation.event
        words.append(UInt32(value.kind))
        if value.kind <= 1 or value.kind == 7:
            _id(words, value.pane)
            if value.kind <= 1:
                words.append(UInt32(value.surface))
        elif value.kind == 6:
            _owner(words, value.owner)
    elif operation.kind == 1:
        _optional_id(words, operation.capture)
    elif operation.kind == 2:
        _id(words, operation.key.id)
    elif operation.kind == 3:
        _input(words, operation.input)
        words.append(UInt32(operation.armed))
        words.append(UInt32(operation.claimed))
        for action in operation.lookups:
            words.append(UInt32(Bool(action)))
            if action:
                _id(words, action.value().id)
    elif operation.kind == 5:
        words.append(UInt32(operation.releases))
        words.append(UInt32(operation.ui_binds))
        words.append(UInt32(operation.terminal_binds))
    return words^


def encode_outcome(outcome: RouterOutcome) raises -> List[UInt32]:
    _ = state_words(outcome.state)
    var words: List[UInt32] = [ROUTER_MAGIC, 1]
    encode_state(words, outcome.state)
    words.append(UInt32(outcome.answer.kind))
    if outcome.answer.kind == 1 or outcome.answer.kind == 3:
        words.append(UInt32(outcome.answer.value))
    elif outcome.answer.kind == 2:
        if len(outcome.answer.effects) > MAX_ROUTER_EFFECTS:
            raise Error("router drained effects exceed outer limit")
        _effects(words, outcome.answer.effects)
    elif outcome.answer.kind != 0:
        raise Error("invalid router answer")
    if len(words) * 4 > MAX_ROUTER_BYTES:
        raise Error("router outcome exceeds outer limit")
    return words^


struct RouterReader(Movable):
    var words: List[UInt32]
    var position: Int
    var claim_limit: Int
    var effect_limit: Int

    def __init__(out self, var words: List[UInt32], native: Bool = False) raises:
        if len(words) * 4 > (NATIVE_ROUTER_BYTES if native else MAX_ROUTER_BYTES):
            raise Error("router packet exceeds byte limit")
        self.words = words^
        self.position = 0
        self.claim_limit = NATIVE_ROUTER_CLAIMS if native else MAX_ROUTER_CLAIMS
        self.effect_limit = NATIVE_ROUTER_EFFECTS if native else MAX_ROUTER_EFFECTS
        if self.word() != ROUTER_MAGIC or self.word() != 1:
            raise Error("invalid router packet header")

    def word(mut self) raises -> UInt32:
        if self.position >= len(self.words):
            raise Error("truncated router packet")
        var value = self.words[self.position]
        self.position += 1
        return value

    def tag(mut self, maximum: UInt32) raises -> UInt8:
        var value = self.word()
        if value > maximum:
            raise Error("invalid router discriminant")
        return UInt8(value)

    def boolean(mut self) raises -> Bool:
        return self.tag(1) != 0

    def identifier(mut self) raises -> UInt64:
        var high = self.word()
        var low = self.word()
        return (UInt64(high) << 32) | UInt64(low)

    def optional_id(mut self) raises -> Optional[UInt64]:
        if self.boolean():
            return self.identifier()
        return None

    def owner(mut self) raises -> RouterOwner:
        var kind = self.tag(4)
        if kind == 1:
            var pane = self.identifier()
            return RouterOwner(kind, pane, self.tag(2))
        return RouterOwner(kind, 0, 0)

    def optional_owner(mut self) raises -> Optional[RouterOwner]:
        if self.boolean():
            return self.owner()
        return None

    def input(mut self) raises -> RouterInput:
        var key = RouterKey(self.identifier())
        var unshifted: Optional[RouterKey] = None
        if self.boolean():
            unshifted = RouterKey(self.identifier())
        var action = self.tag(2)
        return RouterInput(key, unshifted, action, RouterPayload(self.identifier()))

    def effect(mut self) raises -> RouterEffect:
        var value = RouterEffect(self.tag(2))
        if value.kind == 0:
            value.pane = self.identifier()
            value.input = self.input()
        elif value.kind == 1:
            value.action = RouterAction(self.identifier())
        else:
            value.owner = self.owner()
        return value^

    def count(mut self, maximum: Int, minimum: Int) raises -> Int:
        var value = Int(self.word())
        if value > maximum or value > (len(self.words) - self.position) // minimum:
            raise Error("router vector exceeds count or remaining words")
        return value

    def effects(mut self) raises -> List[RouterEffect]:
        var count = self.count(self.effect_limit, 2)
        var result = List[RouterEffect]()
        for _ in range(count):
            result.append(self.effect())
        return result^

    def state(mut self) raises -> RouterState:
        var state = RouterState(self.identifier())
        state.owner = self.owner()
        state.pending = self.optional_owner()
        state.observed = self.owner()
        state.restore = self.owner()
        if self.boolean():
            var pane = self.identifier()
            state.active = pane_owner(pane, self.tag(2))
        state.capture = self.optional_id()
        var held = self.count(self.claim_limit, 4)
        for _ in range(held):
            var key = RouterKey(self.identifier())
            state.held.append(RouterHeld(key, self.identifier()))
        var local = self.count(self.claim_limit, 2)
        for _ in range(local):
            state.local.append(RouterKey(self.identifier()))
        state.effects = self.effects()
        return state^

    def operation(mut self) raises -> RouterOperation:
        var operation = RouterOperation(self.tag(5))
        if operation.kind == 0:
            var kind = self.tag(9)
            var event = RouterEvent(kind, 0, 0, no_owner())
            if kind <= 1 or kind == 7:
                event.pane = self.identifier()
                if kind <= 1:
                    event.surface = self.tag(2)
            elif kind == 6:
                event.owner = self.owner()
            operation.event = event
        elif operation.kind == 1:
            operation.capture = self.optional_id()
        elif operation.kind == 2:
            operation.key = RouterKey(self.identifier())
        elif operation.kind == 3:
            operation.input = self.input()
            operation.armed = self.boolean()
            operation.claimed = self.boolean()
            for index in range(4):
                if self.boolean():
                    operation.lookups[index] = RouterAction(self.identifier())
        elif operation.kind == 5:
            operation.releases = self.boolean()
            operation.ui_binds = self.boolean()
            operation.terminal_binds = self.boolean()
        return operation^

    def answer(mut self) raises -> RouterAnswer:
        var answer = RouterAnswer(self.tag(3))
        if answer.kind == 1 or answer.kind == 3:
            answer.value = self.boolean()
        elif answer.kind == 2:
            answer.effects = self.effects()
        return answer^

    def complete(self) raises:
        if self.position != len(self.words):
            raise Error("router packet has trailing words")


def decode_outcome(var words: List[UInt32], native: Bool = False) raises -> RouterOutcome:
    var reader = RouterReader(words^, native)
    var state = reader.state()
    var answer = reader.answer()
    reader.complete()
    return RouterOutcome(state^, answer^)
