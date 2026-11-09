from .actions import Action, ActionQueue, ActionBatch, MAX_PENDING_ACTIONS
from .verified_input_router import RouterKey, RouterState, RouterInput, RouterOwner, RouterEvent, RouterEffect, RouterOperation, MAX_ROUTER_EFFECTS
from .input_router_gate import InputRouterSession, InputRouterGate, RouterBatchPrepared, RouterMapArena, RouterPayloadArena
from .input_protocol import KeyInput, InputRoutingState, canonical_key, decode_input_frame, encode_input_frame
from .chrome_keymap import ChromeKeymap, chrome_key_identity
from .chrome_actions import ChromeAction
from .prefix_bindings import PrefixBindings, PrefixBinding, MAX_PREFIX_TEXT
from .desktop_input import desktop_key_projection
from .protocol import append_uint, append_text


@fieldwise_init
struct DesktopEnvelope(Copyable, Movable, Equatable):
    var scope: UInt64
    var server: UInt64
    var client: UInt64
    var epoch: UInt64
    var output_owner: Optional[UInt64]

    def __eq__(self, other: Self) -> Bool:
        return self.scope == other.scope and self.server == other.server and self.client == other.client and self.epoch == other.epoch and self.output_owner == other.output_owner


@fieldwise_init
struct DesktopKeyTarget(Copyable, Movable):
    var active: Optional[RouterOwner]
    var capture: Optional[UInt64]
    var overlay: Bool
    var function: Bool
    var connected: Bool


@fieldwise_init
struct DesktopHeld(Copyable, Movable):
    var key: RouterKey
    var pane: UInt64
    var envelope: DesktopEnvelope


@fieldwise_init
struct DesktopEffect(Copyable, Movable):
    var effect: RouterEffect
    var envelope: DesktopEnvelope


@fieldwise_init
struct DesktopUiEffect(Copyable, Movable):
    var kind: UInt8
    var action: Optional[ChromeAction]
    var envelope: DesktopEnvelope
    var pane: Optional[UInt64]
    var bindings: List[PrefixBinding]
    var canonical_prefix: Optional[String]


@fieldwise_init
struct DesktopDeliveryPlan(Movable):
    var generation: UInt64
    var prefix_generation: UInt64
    var configured_version: UInt64
    var context: DesktopEnvelope
    var router: RouterBatchPrepared
    var queue: ActionBatch
    var held: List[DesktopHeld]
    var ui: List[DesktopUiEffect]
    var native: Optional[Bool]
    var routing: Optional[List[UInt8]]


def _validate_held(state: RouterState, held: List[DesktopHeld], scope: UInt64) raises:
    if len(state.held) != len(held):
        raise Error("desktop held provenance does not cover router claims")
    for index in range(len(held)):
        if state.held[index].key != held[index].key or state.held[index].pane != held[index].pane or held[index].envelope.scope != scope:
            raise Error("desktop held provenance differs from router state")
        for previous in range(index):
            if held[previous].key == held[index].key:
                raise Error("desktop provenance requires source-unique held identities")


def _held_owner(held: List[DesktopHeld], key: RouterKey, pane: UInt64) raises -> DesktopEnvelope:
    for value in held:
        if value.key == key and value.pane == pane:
            return value.envelope.copy()
    raise Error("router release has no retained desktop provenance")


def _retained_held(after: RouterState, held: List[DesktopHeld], fresh: List[DesktopEffect], current: DesktopEnvelope) raises -> List[DesktopHeld]:
    var result = List[DesktopHeld]()
    for value in after.held:
        var found: Optional[DesktopEnvelope] = None
        for effect in fresh:
            if effect.effect.kind == 0 and effect.effect.input.action != 2 and effect.effect.input.identity() == value.key and effect.effect.pane == value.pane:
                found = current.copy()
        if not found:
            found = _held_owner(held, value.key, value.pane)
        result.append(DesktopHeld(value.key, value.pane, found.value().copy()))
    _validate_held(after, result, current.scope)
    return result^


def _raw_input(session: InputRouterSession, input: RouterInput) raises -> KeyInput:
    var body = session.payload_body(input.payload)
    var frame = decode_input_frame(body.copy())
    if frame.kind != 1 or not frame.key:
        raise Error("desktop payload is not an immutable native KeyInput frame")
    var raw = frame.key.value().copy()
    var unshifted: Optional[RouterKey] = None
    if raw.unshifted:
        unshifted = RouterKey(UInt64(raw.unshifted.value()))
    if chrome_key_identity(raw) != input.key or unshifted != input.unshifted or raw.action != input.action:
        raise Error("desktop payload metadata differs from the verified input")
    var encoded = encode_input_frame(frame)
    if len(encoded) != len(body):
        raise Error("desktop payload is not a canonical complete key frame")
    for index in range(len(body)):
        if body[index] != encoded[index]:
            raise Error("desktop payload is not a canonical complete key frame")
    return raw^


def _routing_bytes(routing: InputRoutingState) raises -> List[UInt8]:
    if len(routing.options) > 21:
        raise Error("desktop routing options exceed count limit")
    var result: List[UInt8] = [UInt8(routing.armed), UInt8(Bool(routing.table)), UInt8(routing.repeat)]
    if routing.table:
        if routing.table.value().byte_length() > MAX_PREFIX_TEXT:
            raise Error("desktop routing table exceeds byte limit")
        append_text(result, routing.table.value())
    append_uint(result, routing.cancelled_request)
    for key in routing.options:
        if key > 20:
            raise Error("desktop routing option identity is invalid")
    for key in range(21):
        var id = UInt8(key)
        result.append(UInt8(id in routing.options))
        if id in routing.options:
            ref option = routing.options[id]
            if option.value.byte_length() > MAX_PREFIX_TEXT or option.source > 3:
                raise Error("desktop routing option exceeds field limits")
            append_text(result, option.value)
            result.append(option.source)
    return result^


struct DesktopDelivery(Movable):
    var _held: List[DesktopHeld]
    var _generation: UInt64
    var _ui: List[DesktopUiEffect]
    var _receipt: Optional[UInt64]
    var _context: Optional[DesktopEnvelope]

    def __init__(out self):
        self._held = List[DesktopHeld]()
        self._generation = 0
        self._ui = List[DesktopUiEffect]()
        self._receipt = None
        self._context = None

    def held(self) -> List[DesktopHeld]:
        return self._held.copy()

    def pending_ui(self) -> Int:
        return len(self._ui)

    def _prepare(self, session: InputRouterSession, mut gate: InputRouterGate, queue: ActionQueue, map: ChromeKeymap, configured: ChromeKeymap, prefix: PrefixBindings, operations: List[RouterOperation], context: DesktopEnvelope, armed: Bool, which_key: Bool, canonical_prefix: Optional[String] = None) raises -> DesktopDeliveryPlan:
        if self._receipt or self._generation == UInt64.MAX or context.scope != session._scope or map.version() != session._map.version():
            raise Error("desktop delivery requires an acknowledged matching source")
        if canonical_prefix and canonical_prefix.value().byte_length() > MAX_PREFIX_TEXT:
            raise Error("desktop prefix spelling exceeds byte limit")
        var before = session.snapshot()
        if before.owner.kind != 0 or before.pending or before.observed.kind != 0 or before.restore.kind != 0:
            raise Error("desktop global router has an unused focus owner")
        if len(before.effects) != 0 or len(session._delivery) != 0:
            raise Error("desktop transaction requires a drained router boundary")
        if len(operations) > 32:
            raise Error("desktop transaction exceeds operation limit")
        var keys = 0
        var release = False
        var projection = False
        for operation in operations:
            if operation.kind == 0:
                var kind = operation.event.kind
                if kind != 1 and kind != 7 and kind != 8 and kind != 9:
                    raise Error("desktop caller does not emit that focus event")
                projection = True
            elif operation.kind == 1:
                projection = True
            elif operation.kind == 3:
                keys += 1
                release = operation.input.action == 2
            elif operation.kind != 4:
                raise Error("operation is outside the desktop caller projection")
        if keys > 1 or release and (projection or armed):
            raise Error("desktop transaction must preserve one event and release-first ownership")
        _validate_held(before, self._held, context.scope)
        var router = session.prepare_batch(gate, List[RouterOperation]())
        var held = self._held.copy()
        var pending = List[DesktopEffect]()
        var effects = List[DesktopEffect]()
        var native: Optional[Bool] = None
        for operation in operations:
            var prior = router.state.copy()
            if operation.kind == 3:
                var raw = _raw_input(session, operation.input)
                if operation.armed != armed or operation.input.action == 2 and operation.claimed:
                    raise Error("desktop key prefix view differs from the caller projection")
                var expected = map.key_operation(prior, raw, operation.input.payload, operation.armed, operation.claimed)
                if expected.input != operation.input or len(operation.lookups) != len(expected.lookups):
                    raise Error("desktop key metadata or lookup count differs from the resolver")
                for index in range(len(expected.lookups)):
                    if operation.lookups[index] != expected.lookups[index]:
                        raise Error("desktop key lookup differs from the complete resolver")
            var answer = session.extend_batch(gate, router, operation)
            if operation.kind == 3:
                native = answer.value
            var fresh = List[DesktopEffect]()
            if operation.kind == 4:
                if len(pending) != len(prior.effects):
                    raise Error("desktop effect provenance is incomplete")
                for value in pending:
                    effects.append(value.copy())
                pending.clear()
            else:
                if len(router.state.effects) < len(prior.effects):
                    raise Error("desktop effect queue changed without a drain")
                for index in range(len(prior.effects), len(router.state.effects)):
                    var effect = router.state.effects[index].copy()
                    var owner = context.copy()
                    if effect.kind == 0 and effect.input.action == 2:
                        owner = _held_owner(held, effect.input.identity(), effect.pane)
                    var annotated = DesktopEffect(effect^, owner^)
                    pending.append(annotated.copy())
                    fresh.append(annotated^)
            held = _retained_held(router.state, held, fresh, context)
        if len(router.state.effects) != 0 or len(pending) != 0 or len(effects) != len(router.delivery):
            raise Error("desktop transaction must drain every effect exactly once")
        if len(effects) > 1:
            raise Error("desktop event unexpectedly produced multiple effects")
        var actions = List[Action]()
        var ui = List[DesktopUiEffect]()
        for delivered in effects:
            ref effect = delivered.effect
            if effect.kind == 0:
                var raw = _raw_input(session, effect.input)
                var query = prefix.query(raw)
                var route: UInt8 = 0
                var action: Optional[ChromeAction] = None
                if armed and which_key and query.all_keys:
                    route = 3
                elif armed and prefix.chrome_enabled(raw):
                    action = configured.resolve("prefix", raw)
                    if action:
                        route = 5
                if route != 0:
                    var suppress = RouterOperation(2)
                    suppress.key = effect.input.identity()
                    _ = session.extend_batch(gate, router, suppress)
                    var bindings = List[PrefixBinding]()
                    var spelling: Optional[String] = None
                    if route == 3:
                        bindings = prefix.snapshot()
                        spelling = canonical_prefix.copy()
                    ui.append(DesktopUiEffect(route, action, delivered.envelope.copy(), effect.pane, bindings^, spelling^))
                else:
                    var frame = decode_input_frame(session.payload_body(effect.input.payload))
                    frame.pane = effect.pane
                    frame.text_follows = False
                    var source: Optional[Tuple[UInt64, UInt64]] = Tuple(delivered.envelope.server, delivered.envelope.client)
                    actions.append(Action(10, "", effect.pane, delivered.envelope.epoch, None, encode_input_frame(frame), source, delivered.envelope.output_owner))
            elif effect.kind == 1:
                var action = map.action_from_reference(effect.action)
                var kind: UInt8 = 1 if action.kind <= 6 or action.kind == 8 or action.kind == 9 else 2
                ui.append(DesktopUiEffect(kind, action, delivered.envelope.copy(), None, List[PrefixBinding](), None))
            else:
                ui.append(DesktopUiEffect(2, None, delivered.envelope.copy(), None, List[PrefixBinding](), None))
        if len(ui) > MAX_ROUTER_EFFECTS or len(actions) > MAX_PENDING_ACTIONS:
            raise Error("desktop delivery exceeds effect capacity")
        _validate_held(router.state, held, context.scope)
        return DesktopDeliveryPlan(self._generation, prefix.generation, configured.version(), context.copy(), router^, queue.stage(actions), held^, ui^, native, None)

    def prepare_key(self, session: InputRouterSession, mut gate: InputRouterGate, queue: ActionQueue, map: ChromeKeymap, configured: ChromeKeymap, prefix: PrefixBindings, routing: InputRoutingState, input: RouterInput, target: DesktopKeyTarget, context: DesktopEnvelope, which_key: Bool) raises -> DesktopDeliveryPlan:
        var fingerprint = _routing_bytes(routing)
        if prefix.active_table != routing.table or prefix.repeat != routing.repeat:
            raise Error("desktop prefix table and routing snapshot differ")
        var raw = _raw_input(session, input)
        var operations = List[RouterOperation]()
        var armed = False
        var claimed = False
        if raw.action != 2:
            var projection = desktop_key_projection(session.snapshot().active, target.active, target.capture, routing.armed, target.connected and routing.claims_prefix_input(raw), target.overlay, raw.modifiers, target.function)
            operations = projection.operations()
            armed = projection.armed
            claimed = projection.claimed
        operations.append(map.key_operation(session.snapshot(), raw, input.payload, armed, claimed))
        operations.append(RouterOperation(4))
        var spelling: Optional[String] = None
        if target.connected and UInt8(0) in routing.options:
            spelling = canonical_key(routing.options[0].value)
        var result = self._prepare(session, gate, queue, map, configured, prefix, operations, context, armed, which_key, spelling)
        result.routing = fingerprint^
        return result^

    def prepare_events(self, session: InputRouterSession, mut gate: InputRouterGate, queue: ActionQueue, map: ChromeKeymap, configured: ChromeKeymap, prefix: PrefixBindings, events: List[RouterEvent], context: DesktopEnvelope) raises -> DesktopDeliveryPlan:
        if len(events) > 31:
            raise Error("desktop observation batch exceeds operation limit")
        var operations = List[RouterOperation]()
        for event in events:
            if event.kind != 7 and event.kind != 8 and event.kind != 9:
                raise Error("event is outside the desktop observer contract")
            var operation = RouterOperation(0)
            operation.event = event
            operations.append(operation^)
        operations.append(RouterOperation(4))
        return self._prepare(session, gate, queue, map, configured, prefix, operations, context, False, False)

    def commit_key(mut self, mut session: InputRouterSession, mut queue: ActionQueue, configured: ChromeKeymap, prefix: PrefixBindings, routing: InputRoutingState, context: DesktopEnvelope, var plan: DesktopDeliveryPlan) raises -> UInt64:
        if not plan.routing:
            raise Error("desktop key plan has no captured routing snapshot")
        var current = _routing_bytes(routing)
        if len(current) != len(plan.routing.value()):
            raise Error("desktop routing snapshot changed before commit")
        for index in range(len(current)):
            if current[index] != plan.routing.value()[index]:
                raise Error("desktop routing snapshot changed before commit")
        return self.commit(session, queue, configured, prefix, context, plan^)

    def commit(mut self, mut session: InputRouterSession, mut queue: ActionQueue, configured: ChromeKeymap, prefix: PrefixBindings, context: DesktopEnvelope, var plan: DesktopDeliveryPlan) raises -> UInt64:
        if self._receipt or self._generation == UInt64.MAX or plan.generation != self._generation or plan.prefix_generation != prefix.generation or plan.configured_version != configured.version() or plan.context != context or context.scope != session._scope:
            raise Error("desktop delivery plan is stale or belongs to another source")
        session.validate_batch(plan.router)
        queue.validate_batch(plan.queue)
        _validate_held(plan.router.state, plan.held, context.scope)
        if len(plan.ui) > MAX_ROUTER_EFFECTS:
            raise Error("desktop UI receipt exceeds effect capacity")
        session.commit_batch(plan.router)
        queue.commit_batch(plan.queue)
        swap(self._held, plan.held)
        swap(self._ui, plan.ui)
        self._generation += 1
        self._receipt = self._generation
        self._context = context.copy()
        _ = session.take_effects()
        return self._generation

    def acknowledge(mut self, receipt: UInt64, context: DesktopEnvelope) raises -> List[DesktopUiEffect]:
        if self._receipt != receipt or not self._context or self._context.value() != context:
            raise Error("desktop receipt is stale or belongs to another source")
        var result = List[DesktopUiEffect]()
        swap(result, self._ui)
        self._receipt = None
        self._context = None
        return result^

    def install_map(self, mut session: InputRouterSession, var replacement: RouterMapArena) raises:
        if self._receipt:
            raise Error("desktop map installation requires acknowledged delivery")
        _validate_held(session.snapshot(), self._held, session._scope)
        session.install_map(replacement^)

    def replace_payloads(self, mut session: InputRouterSession, var replacement: RouterPayloadArena) raises:
        if self._receipt:
            raise Error("desktop payload replacement requires acknowledged delivery")
        _validate_held(session.snapshot(), self._held, session._scope)
        session.replace_payloads(replacement^)
