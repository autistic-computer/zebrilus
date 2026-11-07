from std.ffi import external_call
from .verified_input_router import (
    RouterKey, RouterPayload, RouterAction, RouterInput, RouterEffect,
    RouterState, RouterOperation, RouterOutcome, RouterAnswer, reduce_router,
    MAX_ROUTER_EFFECTS, NATIVE_ROUTER_BYTES,
)
from .input_router_codec import (
    admit_router, native_eligible, encode_request, decode_outcome,
    encode_outcome, state_words,
)
from .input_router_storage import RouterWordBuffer
from .bend_gate import BendGate

comptime MAX_ROUTER_ARENA_ENTRIES = 4096
comptime MAX_ROUTER_PAYLOAD_BYTES = 4 * 1024 * 1024
comptime MAX_ROUTER_ACTION_BYTES = 256 * 1024


struct RouterMapArena(Copyable, Movable):
    var _version: UInt64
    var _actions: List[RouterAction]
    var _bodies: List[List[UInt8]]

    def __init__(out self, version: UInt64, var actions: List[RouterAction], var bodies: List[List[UInt8]]) raises:
        if len(actions) > MAX_ROUTER_ARENA_ENTRIES or len(actions) != len(bodies):
            raise Error("router map arena exceeds count limit")
        var total = 0
        for index in range(len(actions)):
            if len(bodies[index]) > MAX_ROUTER_ACTION_BYTES - total:
                raise Error("router action arena exceeds byte limit")
            total += len(bodies[index])
            for previous in range(index):
                if actions[previous] == actions[index]:
                    raise Error("router map arena repeats an action reference")
        self._version = version
        self._actions = actions^
        self._bodies = bodies^

    def version(self) -> UInt64:
        return self._version

    def validate(self, action: RouterAction) raises:
        for value in self._actions:
            if value == action:
                return
        raise Error("router returned an action outside its map version")

    def body(self, action: RouterAction) raises -> List[UInt8]:
        for index in range(len(self._actions)):
            if self._actions[index] == action:
                return self._bodies[index].copy()
        raise Error("router action reference is outside its arena")


struct RouterPayloadArena(Copyable, Movable):
    var _scope: UInt64
    var _keys: List[RouterKey]
    var _inputs: List[RouterInput]
    var _bodies: List[List[UInt8]]

    def __init__(out self, scope: UInt64, var keys: List[RouterKey], var inputs: List[RouterInput], var bodies: List[List[UInt8]]) raises:
        if len(keys) > MAX_ROUTER_ARENA_ENTRIES or len(inputs) > MAX_ROUTER_ARENA_ENTRIES or len(inputs) != len(bodies):
            raise Error("router payload arena exceeds count limit")
        for index in range(len(keys)):
            for previous in range(index):
                if keys[index] == keys[previous]:
                    raise Error("router key arena repeats an identity")
        var total = 0
        for index in range(len(inputs)):
            if len(bodies[index]) > MAX_ROUTER_PAYLOAD_BYTES - total:
                raise Error("router payload arena exceeds byte limit")
            total += len(bodies[index])
            if inputs[index].action > 2:
                raise Error("router payload has an invalid key action")
            for previous in range(index):
                if inputs[index].payload == inputs[previous].payload:
                    raise Error("router payload arena repeats a reference")
        self._scope = scope
        self._keys = keys^
        self._inputs = inputs^
        self._bodies = bodies^
        for input in self._inputs:
            self.validate_key(input.key)
            if input.unshifted:
                self.validate_key(input.unshifted.value())

    def scope(self) -> UInt64:
        return self._scope

    def validate_key(self, key: RouterKey) raises:
        for value in self._keys:
            if value == key:
                return
        raise Error("router returned a key outside its identity arena")

    def validate(self, input: RouterInput) raises:
        for value in self._inputs:
            if value.payload == input.payload:
                if value != input:
                    raise Error("router input does not match its immutable payload")
                return
        raise Error("router returned an input outside its payload arena")

    def body(self, payload: RouterPayload) raises -> List[UInt8]:
        for index in range(len(self._inputs)):
            if self._inputs[index].payload == payload:
                return self._bodies[index].copy()
        raise Error("router payload reference is outside its arena")


def _validate_effects(effects: List[RouterEffect], map: RouterMapArena, payloads: RouterPayloadArena) raises:
    for effect in effects:
        if effect.kind == 0:
            payloads.validate(effect.input)
        elif effect.kind == 1:
            map.validate(effect.action)


def validate_router_state(state: RouterState, map: RouterMapArena, payloads: RouterPayloadArena) raises:
    _ = state_words(state)
    if state.keymap != map.version():
        raise Error("router state belongs to another map version")
    for value in state.held:
        payloads.validate_key(value.key)
    for value in state.local:
        payloads.validate_key(value)
    _validate_effects(state.effects, map, payloads)


def validate_router_request(state: RouterState, operation: RouterOperation, map: RouterMapArena, payloads: RouterPayloadArena) raises:
    admit_router(state, operation)
    validate_router_state(state, map, payloads)
    if operation.kind == 2:
        payloads.validate_key(operation.key)
    elif operation.kind == 3:
        payloads.validate(operation.input)
        for action in operation.lookups:
            if action:
                map.validate(action.value())


def validate_router_outcome(outcome: RouterOutcome, operation: RouterOperation, map: RouterMapArena, payloads: RouterPayloadArena) raises:
    validate_router_state(outcome.state, map, payloads)
    _validate_effects(outcome.answer.effects, map, payloads)
    var expected = UInt8(1 if operation.kind == 3 else 2 if operation.kind == 4 else 3 if operation.kind == 5 else 0)
    if outcome.answer.kind != expected:
        raise Error("router answer does not match its operation")
    _ = encode_outcome(outcome)


@fieldwise_init
struct RouterPrepared(Copyable, Movable):
    var generation: UInt64
    var scope: UInt64
    var map_version: UInt64
    var operation: RouterOperation
    var outcome: RouterOutcome
    var native: Bool
    var original: List[UInt32]


def _matching_words(actual: List[UInt32], expected: List[UInt32]) raises:
    if len(actual) != len(expected):
        raise Error("router normalized values differ")
    for index in range(len(actual)):
        if actual[index] != expected[index]:
            raise Error("router normalized values differ")


struct InputRouterGate(Movable):
    var _worker: Optional[BendGate]
    var _request: RouterWordBuffer
    var _result: RouterWordBuffer
    var native_count: UInt64
    var fallback_count: UInt64

    def __init__(out self, path: Optional[String] = None) raises:
        self._worker = None
        self._request = RouterWordBuffer()
        self._result = RouterWordBuffer()
        self.native_count = 0
        self.fallback_count = 0
        if path:
            try:
                self._worker = BendGate(path.value())
            except:
                self._worker = None

    def prepare(self, state: RouterState, operation: RouterOperation, map: RouterMapArena, payloads: RouterPayloadArena, generation: UInt64 = 0) raises -> RouterPrepared:
        validate_router_request(state, operation, map, payloads)
        var outcome = reduce_router(state, operation)
        validate_router_outcome(outcome, operation, map, payloads)
        return RouterPrepared(generation, payloads.scope(), map.version(), operation.copy(), outcome^, False, encode_request(state, operation))

    def execute(mut self, state: RouterState, operation: RouterOperation, map: RouterMapArena, payloads: RouterPayloadArena, generation: UInt64 = 0) raises -> RouterPrepared:
        var checked = self.prepare(state, operation, map, payloads, generation)
        if self._worker and native_eligible(state, operation):
            try:
                self._request.store(encode_request(state, operation))
                self._result.clear()
                var line = String("input ", self._request.path, " ", self._result.path, "\n")
                if self._worker.value()._verdict(line, 0, 2000) != 0:
                    raise Error("Bend input worker rejected request")
                var outcome = decode_outcome(self._result.load(NATIVE_ROUTER_BYTES), native=True)
                validate_router_outcome(outcome, operation, map, payloads)
                var actual = encode_outcome(outcome)
                var expected = encode_outcome(checked.outcome)
                _matching_words(actual, expected)
                self.native_count += 1
                return RouterPrepared(generation, payloads.scope(), map.version(), operation.copy(), outcome^, True, checked.original.copy())
            except:
                if self._worker and self._worker.value().process > 0:
                    _ = external_call["kill", Int32](self._worker.value().process, Int32(9))
                self._worker = None
        self.fallback_count += 1
        return checked^


@fieldwise_init
struct RouterBatchPrepared(Movable):
    var generation: UInt64
    var scope: UInt64
    var map_version: UInt64
    var original: List[UInt32]
    var state: RouterState
    var delivery: List[RouterEffect]
    var operation_count: Int
    var answers: List[RouterAnswer]


def _session_words(state: RouterState, delivery: List[RouterEffect]) raises -> List[UInt32]:
    var answer = RouterAnswer(2)
    answer.effects = delivery.copy()
    return encode_outcome(RouterOutcome(state.copy(), answer^))


struct InputRouterSession(Movable):
    var _state: RouterState
    var _generation: UInt64
    var _scope: UInt64
    var _delivery: List[RouterEffect]
    var _map: RouterMapArena
    var _payloads: RouterPayloadArena

    def __init__(out self, var state: RouterState, var map: RouterMapArena, var payloads: RouterPayloadArena) raises:
        validate_router_state(state, map, payloads)
        self._state = state^
        self._generation = 0
        self._scope = payloads.scope()
        self._delivery = List[RouterEffect]()
        self._map = map^
        self._payloads = payloads^

    def snapshot(self) -> RouterState:
        return self._state.copy()

    def generation(self) -> UInt64:
        return self._generation

    def prepare(self, mut gate: InputRouterGate, operation: RouterOperation) raises -> RouterPrepared:
        if operation.kind == 4 and len(self._state.effects) > MAX_ROUTER_EFFECTS - len(self._delivery):
            raise Error("router delivery queue has no capacity")
        if self._generation == UInt64.MAX:
            raise Error("router transaction generation exhausted")
        return gate.execute(self._state, operation, self._map, self._payloads, self._generation)

    def commit(mut self, var prepared: RouterPrepared) raises:
        if prepared.generation != self._generation or prepared.scope != self._scope or prepared.scope != self._payloads.scope() or prepared.map_version != self._state.keymap or prepared.map_version != self._map.version():
            raise Error("router prepared transition is stale or belongs to another source")
        _matching_words(encode_request(self._state, prepared.operation), prepared.original)
        validate_router_outcome(prepared.outcome, prepared.operation, self._map, self._payloads)
        if self._generation == UInt64.MAX:
            raise Error("router transaction generation exhausted")
        if len(prepared.outcome.answer.effects) > MAX_ROUTER_EFFECTS - len(self._delivery):
            raise Error("router delivery queue has no capacity")
        var delivery = self._delivery.copy()
        for effect in prepared.outcome.answer.effects:
            delivery.append(effect.copy())
        self._state = prepared.outcome.state.copy()
        self._delivery = delivery^
        self._generation += 1

    def prepare_batch(self, mut gate: InputRouterGate, operations: List[RouterOperation]) raises -> RouterBatchPrepared:
        if len(operations) > 32 or self._generation == UInt64.MAX:
            raise Error("router batch exceeds operation or generation limit")
        var result = RouterBatchPrepared(self._generation, self._scope, self._map.version(), _session_words(self._state, self._delivery), self._state.copy(), self._delivery.copy(), 0, List[RouterAnswer]())
        for operation in operations:
            _ = self.extend_batch(gate, result, operation)
        return result^

    def extend_batch(self, mut gate: InputRouterGate, mut batch: RouterBatchPrepared, operation: RouterOperation) raises -> RouterAnswer:
        self.validate_batch(batch)
        if batch.operation_count >= 32:
            raise Error("router batch exceeds operation limit")
        var prepared = gate.execute(batch.state, operation, self._map, self._payloads, self._generation)
        if len(prepared.outcome.answer.effects) > MAX_ROUTER_EFFECTS - len(batch.delivery):
            raise Error("router batch delivery exceeds effect limit")
        var delivery = batch.delivery.copy()
        for effect in prepared.outcome.answer.effects:
            delivery.append(effect.copy())
        swap(batch.state, prepared.outcome.state)
        batch.delivery = delivery^
        batch.operation_count += 1
        batch.answers.append(prepared.outcome.answer.copy())
        self.validate_batch(batch)
        return prepared.outcome.answer.copy()

    def validate_batch(self, batch: RouterBatchPrepared) raises:
        if batch.generation != self._generation or batch.scope != self._scope or batch.scope != self._payloads.scope() or batch.map_version != self._state.keymap or batch.map_version != self._map.version() or self._generation == UInt64.MAX:
            raise Error("router batch is stale or belongs to another source")
        if batch.operation_count < 0 or batch.operation_count > 32 or len(batch.answers) != batch.operation_count or len(batch.delivery) > MAX_ROUTER_EFFECTS:
            raise Error("router batch exceeds count limit")
        _matching_words(_session_words(self._state, self._delivery), batch.original)
        validate_router_state(batch.state, self._map, self._payloads)
        _validate_effects(batch.delivery, self._map, self._payloads)
        var drained = 0
        for answer in batch.answers:
            if answer.kind > 3 or answer.kind != 2 and len(answer.effects) != 0 or len(answer.effects) > MAX_ROUTER_EFFECTS - drained:
                raise Error("router batch answers exceed type or effect limits")
            drained += len(answer.effects)
            _validate_effects(answer.effects, self._map, self._payloads)
        if state_words(batch.state) * 4 > 1024 * 1024:
            raise Error("router batch final state exceeds byte limit")

    def commit_batch(mut self, mut batch: RouterBatchPrepared) raises:
        self.validate_batch(batch)
        swap(self._state, batch.state)
        swap(self._delivery, batch.delivery)
        self._generation += 1

    def take_effects(mut self) -> List[RouterEffect]:
        var effects = List[RouterEffect]()
        swap(effects, self._delivery)
        return effects^

    def payload_body(self, payload: RouterPayload) raises -> List[UInt8]:
        return self._payloads.body(payload)

    def action_body(self, action: RouterAction) raises -> List[UInt8]:
        return self._map.body(action)

    def install_map(mut self, var replacement: RouterMapArena) raises:
        if len(self._state.effects) != 0 or len(self._delivery) != 0:
            raise Error("router map installation requires drained effects and delivery")
        if self._generation == UInt64.MAX:
            raise Error("router transaction generation exhausted")
        var state = self._state.copy()
        state.keymap = replacement.version()
        validate_router_state(state, replacement, self._payloads)
        self._map = replacement^
        self._state = state^
        self._generation += 1

    def replace_payloads(mut self, var replacement: RouterPayloadArena) raises:
        if replacement.scope() != self._scope:
            raise Error("router payload replacement belongs to another source scope")
        if len(self._delivery) != 0:
            raise Error("router payload replacement requires consumed delivery")
        if self._generation == UInt64.MAX:
            raise Error("router transaction generation exhausted")
        validate_router_state(self._state, self._map, replacement)
        for old_index in range(len(self._payloads._inputs)):
            var payload = self._payloads._inputs[old_index].payload
            var required = False
            for effect in self._state.effects:
                required = required or effect.kind == 0 and effect.input.payload == payload
            if required:
                for new_index in range(len(replacement._inputs)):
                    if replacement._inputs[new_index].payload == payload:
                        _matching_bytes(self._payloads._bodies[old_index], replacement._bodies[new_index])
        self._payloads = replacement^
        self._generation += 1


def _matching_bytes(actual: List[UInt8], expected: List[UInt8]) raises:
    if len(actual) != len(expected):
        raise Error("router payload replacement changed retained input bytes")
    for index in range(len(actual)):
        if actual[index] != expected[index]:
            raise Error("router payload replacement changed retained input bytes")
