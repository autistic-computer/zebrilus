from .terminal_model import TerminalViewport, MAX_GRID_CELLS
from .terminal_history import (
    TerminalHistoryState,
    MAX_HISTORY_ROWS,
    MAX_HISTORY_CHUNK_ROWS,
)
from .local_scroll import _sub_u32, _elapsed

comptime HISTORY_BACKFILL_QUIET_NS: UInt64 = 100_000_000
comptime HISTORY_REQUEST_RETRY_NS: UInt64 = 3_000_000_000


@fieldwise_init
struct HistoryRequestContext(TrivialRegisterPassable):
    var offset: UInt32
    var visible_rows: UInt32
    var retained_rows: UInt64
    var limit: UInt64
    var mutations: UInt64

    @staticmethod
    def from_state(
        state: TerminalHistoryState, viewport: TerminalViewport
    ) -> Self:
        return Self(
            state.history_scrollbar.offset,
            viewport.metadata.scrollbar.length,
            UInt64(state.history.count),
            UInt64(state.history.limit),
            state.history_mutations,
        )


@fieldwise_init
struct HistoryRange(TrivialRegisterPassable):
    var start: UInt32
    var count: UInt32


def bounded_history_range(
    requested: HistoryRange, columns: UInt16
) -> HistoryRange:
    var count = min(
        requested.count, UInt32(MAX_GRID_CELLS // max(Int(columns), 1))
    )
    return HistoryRange(requested.start + requested.count - count, count)


def history_request_range(
    context: HistoryRequestContext, budget: UInt64, prefetch: Optional[UInt32]
) -> Optional[HistoryRange]:
    var available = min(budget, context.limit)
    if context.retained_rows >= available or context.retained_rows > UInt64(
        context.offset
    ):
        return None
    var front = context.offset - UInt32(context.retained_rows)
    var desired = front
    if prefetch:
        desired = _sub_u32(
            front, _sub_u32(prefetch.value(), context.visible_rows)
        )
    var count = UInt32(
        min(
            UInt64(min(desired, UInt32(MAX_HISTORY_CHUNK_ROWS))),
            available - context.retained_rows,
        )
    )
    if count != 0:
        return HistoryRange(front - count, count)
    return None


@fieldwise_init
struct PendingHistoryRequest(Copyable, Movable):
    var mutations: UInt64
    var prefetch: Optional[UInt32]
    var sent: UInt64


@fieldwise_init
struct HistoryFollowUp(TrivialRegisterPassable):
    var kind: UInt8
    var value: UInt64


@fieldwise_init
struct HistoryArrival(TrivialRegisterPassable):
    var apply: Bool
    var follow_up: HistoryFollowUp


struct HistoryPacer(Movable):
    var pending_requests: Dict[UInt64, PendingHistoryRequest]
    var deferred_requests: Dict[UInt64, UInt64]

    def __init__(out self):
        self.pending_requests = Dict[UInt64, PendingHistoryRequest]()
        self.deferred_requests = Dict[UInt64, UInt64]()

    def request(
        mut self,
        pane: UInt64,
        prefetch: Optional[UInt32],
        trickle_budget: UInt64,
        retained: Optional[HistoryRequestContext],
        now: UInt64,
    ) raises -> Optional[HistoryRange]:
        if not prefetch and pane in self.deferred_requests:
            return None
        var target = prefetch
        if pane in self.pending_requests:
            ref pending = self.pending_requests[pane]
            if target and pending.prefetch:
                target = min(target.value(), pending.prefetch.value())
            elif not target:
                target = pending.prefetch
            if _elapsed(now, pending.sent) < HISTORY_REQUEST_RETRY_NS:
                pending.prefetch = target
                return None
            _ = self.pending_requests.pop(pane)
        if not retained:
            return None
        var budget = UInt64(MAX_HISTORY_ROWS) if target else trickle_budget
        var requested = history_request_range(retained.value(), budget, target)
        if not requested:
            return None
        self.pending_requests[pane] = PendingHistoryRequest(
            retained.value().mutations, target, now
        )
        return requested

    def chunk_arrived(
        mut self, pane: UInt64, mutations: Optional[UInt64]
    ) raises -> HistoryArrival:
        var pending: Optional[PendingHistoryRequest] = None
        if pane in self.pending_requests:
            pending = self.pending_requests.pop(pane)
        var apply = (
            Bool(mutations)
            and Bool(pending)
            and pending.value().mutations == mutations.value()
        )
        if pending and pending.value().prefetch:
            return HistoryArrival(
                apply,
                HistoryFollowUp(0, UInt64(pending.value().prefetch.value())),
            )
        if pending and mutations and not apply:
            return HistoryArrival(False, HistoryFollowUp(1, mutations.value()))
        return HistoryArrival(apply, HistoryFollowUp(2, 0))

    def defer(mut self, pane: UInt64, mutations: UInt64) -> Bool:
        var fresh = pane not in self.deferred_requests
        self.deferred_requests[pane] = mutations
        return fresh

    def resume_deferred(
        mut self, pane: UInt64, mutations: Optional[UInt64]
    ) raises -> UInt8:
        if pane not in self.deferred_requests or not mutations:
            return 2
        if self.deferred_requests[pane] == mutations.value():
            _ = self.deferred_requests.pop(pane)
            return 0
        self.deferred_requests[pane] = mutations.value()
        return 1

    def forget(mut self, pane: UInt64) raises:
        if pane in self.pending_requests:
            _ = self.pending_requests.pop(pane)
        if pane in self.deferred_requests:
            _ = self.deferred_requests.pop(pane)

    def clear_deferred(mut self):
        self.deferred_requests.clear()

    def clear(mut self):
        self.pending_requests.clear()
        self.deferred_requests.clear()
