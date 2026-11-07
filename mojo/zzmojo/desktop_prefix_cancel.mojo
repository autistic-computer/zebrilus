from .actions import Action, ActionQueue, ActionBatch
from .desktop_router_delivery import DesktopEnvelope
from .palette_protocol import palette_cancel_prefix


@fieldwise_init
struct DesktopDialogPrefix(Copyable, Movable):
    var sent: Bool
    var pending: Optional[UInt64]


@fieldwise_init
struct DesktopPrefixDecision(Copyable, Movable):
    var state: DesktopDialogPrefix
    var request: Bool


def next_prefix_cancel_request(previous: UInt64) -> UInt64:
    return previous if previous == UInt64.MAX else previous + 1


def desktop_prefix_acknowledged(
    previous: Optional[UInt64], request_id: UInt64
) -> Optional[UInt64]:
    return (
        request_id if not previous
        or request_id > previous.value() else previous
    )


def desktop_prefix_reset(
    previous: Optional[UInt64], next_request: UInt64
) -> Optional[UInt64]:
    return previous if next_request == 0 else desktop_prefix_acknowledged(
        previous, next_request
    )


def desktop_dialog_prefix(
    state: DesktopDialogPrefix, active: Bool, acknowledged: Optional[UInt64]
) -> DesktopPrefixDecision:
    var result = state.copy()
    if (
        result.pending
        and acknowledged
        and acknowledged.value() >= result.pending.value()
    ):
        result.pending = None
    if not active:
        result.sent = False
    var request = active and not result.sent
    return DesktopPrefixDecision(result^, request)


@fieldwise_init
struct DesktopPrefixPrepared(Movable):
    var generation: UInt64
    var original_next: UInt64
    var original_sent: Bool
    var original_pending: Optional[UInt64]
    var original_acknowledged: Optional[UInt64]
    var context: DesktopEnvelope
    var next: UInt64
    var dialog: DesktopDialogPrefix
    var request: Optional[UInt64]
    var acknowledged: Optional[UInt64]
    var queue: Optional[ActionBatch]


struct DesktopPrefixCancel(Movable):
    var next: UInt64
    var dialog: DesktopDialogPrefix
    var generation: UInt64
    var acknowledged: Optional[UInt64]

    def __init__(out self):
        self.next = 0
        self.dialog = DesktopDialogPrefix(False, None)
        self.generation = 0
        self.acknowledged = None

    def fork(self) -> Self:
        var result = Self()
        result.next = self.next
        result.dialog = self.dialog.copy()
        result.generation = self.generation
        result.acknowledged = self.acknowledged
        return result^

    def reset_session(mut self) raises:
        if self.generation == UInt64.MAX:
            raise Error("desktop prefix cancellation generation exhausted")
        self.acknowledged = desktop_prefix_reset(self.acknowledged, self.next)
        self.generation += 1

    def _prepare(
        self,
        queue: ActionQueue,
        context: DesktopEnvelope,
        dialog: DesktopDialogPrefix,
        wanted: Bool,
        connected: Bool,
        acknowledged: Optional[UInt64],
    ) raises -> DesktopPrefixPrepared:
        if self.generation == UInt64.MAX:
            raise Error("desktop prefix cancellation generation exhausted")
        var next = self.next
        var request: Optional[UInt64] = None
        var actions = List[Action]()
        if wanted and connected:
            next = next_prefix_cancel_request(self.next)
            request = next
            actions.append(
                Action(
                    10,
                    "",
                    None,
                    context.epoch,
                    None,
                    palette_cancel_prefix(next),
                    (context.server, context.client),
                    None,
                )
            )
        return DesktopPrefixPrepared(
            self.generation,
            self.next,
            self.dialog.sent,
            self.dialog.pending,
            self.acknowledged,
            context.copy(),
            next,
            dialog.copy(),
            request,
            acknowledged,
            queue.stage(actions),
        )

    def prepare_cancel(
        self, queue: ActionQueue, context: DesktopEnvelope, connected: Bool
    ) raises -> DesktopPrefixPrepared:
        return self._prepare(
            queue, context, self.dialog, True, connected, self.acknowledged
        )

    def prepare_dialog(
        self,
        queue: ActionQueue,
        context: DesktopEnvelope,
        active: Bool,
        acknowledged: Optional[UInt64],
        connected: Bool,
    ) raises -> DesktopPrefixPrepared:
        var merged = self.acknowledged
        if acknowledged:
            merged = desktop_prefix_acknowledged(merged, acknowledged.value())
        var decision = desktop_dialog_prefix(self.dialog, active, merged)
        var result = self._prepare(
            queue, context, decision.state, decision.request, connected, merged
        )
        if result.request:
            result.dialog.sent = True
            result.dialog.pending = result.request
        return result^

    def validate(
        self, prepared: DesktopPrefixPrepared, context: DesktopEnvelope
    ) raises:
        if (
            self.generation == UInt64.MAX
            or prepared.generation != self.generation
            or prepared.original_next != self.next
            or prepared.original_sent != self.dialog.sent
            or prepared.original_pending != self.dialog.pending
            or prepared.original_acknowledged != self.acknowledged
            or prepared.context != context
        ):
            raise Error("desktop prefix cancellation is stale")

    def _publish(mut self, prepared: DesktopPrefixPrepared):
        self.next = prepared.next
        self.dialog.sent = prepared.dialog.sent
        self.dialog.pending = prepared.dialog.pending
        self.acknowledged = prepared.acknowledged
        self.generation += 1

    def commit(
        mut self,
        mut queue: ActionQueue,
        context: DesktopEnvelope,
        mut prepared: DesktopPrefixPrepared,
    ) raises:
        self.validate(prepared, context)
        if not prepared.queue:
            raise Error("desktop prefix action batch was consumed")
        queue.validate_batch(prepared.queue.value())
        queue.commit_batch(prepared.queue.value())
        self._publish(prepared)
        prepared.queue = None
