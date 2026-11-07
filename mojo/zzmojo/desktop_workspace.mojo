from std.memory import ArcPointer
from .actions import Action, ActionQueue, ActionBatch
from .chrome_keymap import ChromeKeymap, chrome_key_identity
from .desktop_input import desktop_router_map
from .desktop_key_tables import RetainedKeyTables, DesktopKeyTables
from .desktop_router_delivery import (
    DesktopDelivery,
    DesktopDeliveryPlan,
    DesktopEnvelope,
    DesktopKeyTarget,
    DesktopUiEffect,
)
from .input_protocol import (
    KeyInput,
    InputFrame,
    InputRoutingState,
    encode_input_frame,
)
from .input_router_gate import (
    InputRouterSession,
    InputRouterGate,
    RouterPayloadArena,
    MAX_ROUTER_ARENA_ENTRIES,
)
from .verified_input_router import (
    RouterState,
    RouterKey,
    RouterPayload,
    RouterInput,
    RouterEvent,
    no_owner,
)


@fieldwise_init
struct DesktopPrepared(Movable):
    var generation: UInt64
    var payload: UInt64
    var prefix_revision: UInt64
    var source: Tuple[UInt64, UInt64]
    var tables: ArcPointer[DesktopKeyTables]
    var context: DesktopEnvelope
    var target: DesktopKeyTarget
    var action: UInt8
    var session: InputRouterSession
    var delivery: DesktopDelivery
    var plan: Optional[DesktopDeliveryPlan]


def _retain_key(mut keys: List[RouterKey], key: RouterKey) raises:
    for current in keys:
        if current == key:
            return
    if len(keys) == MAX_ROUTER_ARENA_ENTRIES:
        raise Error("desktop key identity retention exceeds capacity")
    keys.append(key)


def _fork_delivery(delivery: DesktopDelivery) raises -> DesktopDelivery:
    if delivery._receipt or len(delivery._ui) != 0:
        raise Error("desktop workspace requires acknowledged UI delivery")
    var result = DesktopDelivery()
    result._held = delivery._held.copy()
    result._generation = delivery._generation
    return result^


def _fork_session(
    session: InputRouterSession, var payloads: RouterPayloadArena
) raises -> InputRouterSession:
    if (
        len(session._state.effects) != 0
        or len(session._delivery) != 0
        or session.generation() == UInt64.MAX
    ):
        raise Error("desktop workspace requires a drained live session")
    var result = InputRouterSession(
        session.snapshot(), session._map.copy(), payloads^
    )
    result._generation = session.generation() + 1
    return result^


struct DesktopWorkspace(Movable):
    var configured: ChromeKeymap
    var map: ChromeKeymap
    var session: InputRouterSession
    var delivery: DesktopDelivery
    var gate: InputRouterGate
    var source: Optional[Tuple[UInt64, UInt64]]
    var _generation: UInt64
    var _next_payload: UInt64
    var _next_map: UInt32
    var _scope: UInt64

    def __init__(
        out self, profile: UInt8, worker: Optional[String] = None
    ) raises:
        self.configured = ChromeKeymap(profile, 1)
        self.map = desktop_router_map(self.configured, 2)
        var payloads = RouterPayloadArena(
            1, List[RouterKey](), List[RouterInput](), List[List[UInt8]]()
        )
        self.session = InputRouterSession(
            RouterState(self.map.version()), self.map.action_arena(), payloads^
        )
        self.delivery = DesktopDelivery()
        self.gate = InputRouterGate(worker)
        self.source = None
        self._generation = 0
        self._next_payload = 1
        self._next_map = 3
        self._scope = 1

    def generation(self) -> UInt64:
        return self._generation

    def bind_source(mut self, server: UInt64, client: UInt64) raises:
        var source = Tuple(server, client)
        if self.source == Optional[Tuple[UInt64, UInt64]](source):
            return
        if (
            self._generation == UInt64.MAX
            or self._scope == UInt64.MAX
            or self.session.generation() == UInt64.MAX
        ):
            raise Error("desktop workspace source generation exhausted")
        var delivery = _fork_delivery(self.delivery)
        var payloads = RouterPayloadArena(
            self._scope, self.session._payloads._keys.copy(), List[RouterInput](), List[List[UInt8]]()
        )
        var session = _fork_session(self.session, payloads^)
        var prefix = DesktopKeyTables()
        var queue = ActionQueue()
        var context = DesktopEnvelope(self._scope, server, client, 0, None)
        var events: List[RouterEvent] = [RouterEvent(8, 0, 0, no_owner())]
        var plan = delivery.prepare_events(
            session,
            self.gate,
            queue,
            self.map,
            self.configured,
            prefix.prefix,
            events,
            context,
        )
        var receipt = delivery.commit(
            session, queue, self.configured, prefix.prefix, context, plan^
        )
        if len(delivery.acknowledge(receipt, context)) != 0 or len(queue) != 0:
            raise Error("desktop source reset unexpectedly produced delivery")
        var scope = self._scope + 1
        session._payloads = RouterPayloadArena(
            scope, List[RouterKey](), List[RouterInput](), List[List[UInt8]]()
        )
        session._scope = scope
        swap(self.session, session)
        swap(self.delivery, delivery)
        self._scope = scope
        self.source = source
        self._generation += 1

    def observe(mut self, events: List[RouterEvent], epoch: UInt64 = 0) raises:
        if not self.source:
            return
        if (
            self._generation == UInt64.MAX
            or self.session.generation() > UInt64.MAX - 2
        ):
            raise Error("desktop observer generation exhausted")
        var delivery = _fork_delivery(self.delivery)
        var payloads = RouterPayloadArena(
            self._scope, self.session._payloads._keys.copy(), List[RouterInput](), List[List[UInt8]]()
        )
        var session = _fork_session(self.session, payloads^)
        var prefix = DesktopKeyTables()
        var queue = ActionQueue()
        var current = self.source.value()
        var context = DesktopEnvelope(
            self._scope, current[0], current[1], epoch, None
        )
        var plan = delivery.prepare_events(
            session,
            self.gate,
            queue,
            self.map,
            self.configured,
            prefix.prefix,
            events,
            context,
        )
        var receipt = delivery.commit(
            session, queue, self.configured, prefix.prefix, context, plan^
        )
        if len(delivery.acknowledge(receipt, context)) != 0 or len(queue) != 0:
            raise Error("desktop observer unexpectedly produced delivery")
        swap(self.session, session)
        swap(self.delivery, delivery)
        self._generation += 1

    def install_configured(mut self, configured: ChromeKeymap) raises:
        if self._next_map > UInt32.MAX - 2 or self._generation == UInt64.MAX:
            raise Error("desktop workspace map generation exhausted")
        var replacement = ChromeKeymap(0, self._next_map)
        replacement._bindings = configured.bindings()
        var map = desktop_router_map(replacement, self._next_map + 1)
        self.delivery.install_map(self.session, map.action_arena())
        swap(self.configured, replacement)
        swap(self.map, map)
        self._next_map += 2
        self._generation += 1

    def prepare(
        mut self,
        queue: ActionQueue,
        tables: RetainedKeyTables,
        routing: InputRoutingState,
        raw: KeyInput,
        target: DesktopKeyTarget,
        epoch: UInt64,
        output_owner: Optional[UInt64] = None,
        which_key: Bool = False,
        picker_sidebar: Bool = False,
    ) raises -> DesktopPrepared:
        if (
            not self.source
            or not tables.snapshot
            or raw.action != 2
            and not tables.eligible
        ):
            raise Error(
                "desktop workspace has no complete current key snapshot"
            )
        if (
            self._generation == UInt64.MAX
            or self._next_payload == UInt64.MAX
            or self.session.generation() > UInt64.MAX - 2
        ):
            raise Error("desktop workspace transaction generation exhausted")
        ref snapshot = tables.snapshot.value()[]
        var keys = List[RouterKey]()
        var state = self.session.snapshot()
        for held in state.held:
            _retain_key(keys, held.key)
        for key in state.local:
            _retain_key(keys, key)
        _retain_key(keys, chrome_key_identity(raw))
        if raw.unshifted:
            _retain_key(keys, RouterKey(UInt64(raw.unshifted.value())))
        var frame = InputFrame()
        frame.key = raw.copy()
        var operation = self.map.key_operation(
            state, raw, RouterPayload(self._next_payload), False, False
        )
        var inputs: List[RouterInput] = [operation.input.copy()]
        var bodies: List[List[UInt8]] = [encode_input_frame(frame)]
        var payloads = RouterPayloadArena(self._scope, keys^, inputs^, bodies^)
        var candidate = _fork_session(self.session, payloads^)
        var delivery = _fork_delivery(self.delivery)
        var current = self.source.value()
        var context = DesktopEnvelope(
            self._scope, current[0], current[1], epoch, output_owner
        )
        var plan = delivery.prepare_key(
            candidate,
            self.gate,
            queue,
            self.map,
            self.configured,
            snapshot.prefix,
            routing,
            operation.input,
            target,
            context,
            which_key,
            picker_sidebar,
        )
        return DesktopPrepared(
            self._generation,
            self._next_payload,
            tables.revision,
            current,
            tables.snapshot.value().copy(),
            context^,
            target.copy(),
            raw.action,
            candidate^,
            delivery^,
            plan^,
        )

    def stage_actions(
        self,
        queue: ActionQueue,
        mut prepared: DesktopPrepared,
        actions: List[Action],
    ) raises:
        if (
            prepared.generation != self._generation
            or prepared.payload != self._next_payload
        ):
            raise Error("desktop workspace action staging is stale")
        queue.validate_batch(prepared.plan.value().queue)
        var staged = ActionQueue()
        staged.items = prepared.plan.value().queue.items.copy()
        staged.pending_bytes = prepared.plan.value().queue.pending_bytes
        for action in actions:
            staged.append(
                action.kind,
                action.text,
                action.pane,
                action.selection_epoch,
                action.geometry,
                action.frame,
                action.source,
                action.output_owner,
            )
        var pending = staged.pending_bytes
        prepared.plan.value().queue = ActionBatch(
            queue.generation(), staged.take(), pending
        )
        queue.validate_batch(prepared.plan.value().queue)

    def commit(
        mut self,
        mut queue: ActionQueue,
        tables: RetainedKeyTables,
        routing: InputRoutingState,
        target: DesktopKeyTarget,
        epoch: UInt64,
        output_owner: Optional[UInt64],
        var prepared: DesktopPrepared,
    ) raises -> UInt64:
        if (
            prepared.generation != self._generation
            or prepared.payload != self._next_payload
            or not self.source
            or self.source.value() != prepared.source
            or prepared.context.scope != self._scope
            or prepared.prefix_revision != tables.revision
            or not tables.snapshot
        ):
            raise Error("desktop workspace prepared key is stale")
        var current = self.source.value()
        if prepared.context != DesktopEnvelope(
            self._scope, current[0], current[1], epoch, output_owner
        ):
            raise Error("desktop workspace delivery context changed")
        if prepared.action != 2 and (
            not tables.eligible
            or prepared.target.active != target.active
            or prepared.target.capture != target.capture
            or prepared.target.overlay != target.overlay
            or prepared.target.function != target.function
            or prepared.target.connected != target.connected
        ):
            raise Error("desktop workspace target projection changed")
        if tables.snapshot.value()[].generation != prepared.tables[].generation:
            raise Error("desktop workspace prefix snapshot changed")
        var plan = prepared.plan.take()
        var receipt = prepared.delivery.commit_key(
            prepared.session,
            queue,
            self.configured,
            tables.snapshot.value()[].prefix,
            routing,
            prepared.context,
            plan^,
        )
        swap(self.session, prepared.session)
        swap(self.delivery, prepared.delivery)
        self._next_payload += 1
        self._generation += 1
        return receipt

    def acknowledge(
        mut self,
        receipt: UInt64,
        epoch: UInt64,
        output_owner: Optional[UInt64] = None,
    ) raises -> List[DesktopUiEffect]:
        if not self.source:
            raise Error("desktop workspace has no delivery source")
        var current = self.source.value()
        return self.delivery.acknowledge(
            receipt,
            DesktopEnvelope(
                self._scope, current[0], current[1], epoch, output_owner
            ),
        )
