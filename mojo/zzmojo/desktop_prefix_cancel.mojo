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
    var context: DesktopEnvelope
    var next: UInt64
    var dialog: DesktopDialogPrefix
    var request: Optional[UInt64]
    var queue: ActionBatch


struct DesktopPrefixCancel(Movable):
    var next: UInt64
    var dialog: DesktopDialogPrefix
    var generation: UInt64

    def __init__(out self):
        self.next = 0
        self.dialog = DesktopDialogPrefix(False, None)
        self.generation = 0

    def _prepare(
        self,
        queue: ActionQueue,
        context: DesktopEnvelope,
        dialog: DesktopDialogPrefix,
        wanted: Bool,
        connected: Bool,
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
            context.copy(),
            next,
            dialog.copy(),
            request,
            queue.stage(actions),
        )

    def prepare_cancel(
        self, queue: ActionQueue, context: DesktopEnvelope, connected: Bool
    ) raises -> DesktopPrefixPrepared:
        return self._prepare(queue, context, self.dialog, True, connected)

    def prepare_dialog(
        self,
        queue: ActionQueue,
        context: DesktopEnvelope,
        active: Bool,
        acknowledged: Optional[UInt64],
        connected: Bool,
    ) raises -> DesktopPrefixPrepared:
        var decision = desktop_dialog_prefix(self.dialog, active, acknowledged)
        var result = self._prepare(
            queue, context, decision.state, decision.request, connected
        )
        if result.request:
            result.dialog.sent = True
            result.dialog.pending = result.request
        return result^

    def commit(
        mut self,
        mut queue: ActionQueue,
        context: DesktopEnvelope,
        mut prepared: DesktopPrefixPrepared,
    ) raises:
        if (
            self.generation == UInt64.MAX
            or prepared.generation != self.generation
            or prepared.original_next != self.next
            or prepared.original_sent != self.dialog.sent
            or prepared.original_pending != self.dialog.pending
            or prepared.context != context
        ):
            raise Error("desktop prefix cancellation is stale")
        queue.validate_batch(prepared.queue)
        queue.commit_batch(prepared.queue)
        self.next = prepared.next
        self.dialog = prepared.dialog.copy()
        self.generation += 1
