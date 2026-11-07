from std.memory import ArcPointer
from .terminal_model import TerminalViewport
from .terminal_history import (
    TerminalHistoryState,
    HistorySnapshot,
    HISTORY_BYTE_BUDGET,
)
from .history_pacer import HistoryPacer, HistoryRequestContext
from .local_scroll import ScrollContext


struct TerminalHistoryCache(Movable):
    var states: Dict[UInt64, TerminalHistoryState]
    var invalidations: Dict[UInt64, UInt64]
    var last_used: Dict[UInt64, UInt64]
    var next_row_revision: UInt64
    var clock: UInt64
    var byte_budget: Int
    var pacer: HistoryPacer

    def __init__(out self, byte_budget: Int = HISTORY_BYTE_BUDGET):
        self.states = Dict[UInt64, TerminalHistoryState]()
        self.invalidations = Dict[UInt64, UInt64]()
        self.last_used = Dict[UInt64, UInt64]()
        self.next_row_revision = 1
        self.clock = 0
        self.byte_budget = min(max(byte_budget, 0), HISTORY_BYTE_BUDGET)
        self.pacer = HistoryPacer()

    def touch(mut self, pane: UInt64):
        self.clock += 1
        self.last_used[pane] = self.clock

    def ensure(
        mut self, pane: UInt64, viewport: TerminalViewport
    ) raises -> Bool:
        if pane not in self.states:
            var state = TerminalHistoryState(viewport, self.next_row_revision)
            if pane in self.invalidations:
                state.history_invalidations = self.invalidations[pane]
            self.states[pane] = state^
        self.touch(pane)
        self.enforce()
        return pane in self.states

    def context(
        self, pane: UInt64, viewport: TerminalViewport
    ) raises -> ScrollContext:
        if pane in self.states:
            return self.states[pane].context(viewport)
        var invalidations = self.invalidations.get(pane, UInt64(0))
        return ScrollContext.from_viewport(viewport, 0, 0, invalidations)

    def request_context(
        self, pane: UInt64, viewport: TerminalViewport
    ) raises -> Optional[HistoryRequestContext]:
        if pane in self.states:
            return HistoryRequestContext.from_state(self.states[pane], viewport)
        return None

    def mutations(self, pane: UInt64) raises -> Optional[UInt64]:
        if pane in self.states:
            return self.states[pane].history_mutations
        return None

    def owned_bytes(self) raises -> Int:
        var result = len(self.invalidations) * 96 + len(self.last_used) * 96
        for pane in self.last_used.keys():
            result += self.states[pane].owned_bytes() + 256
        return result

    def enforce(mut self) raises:
        for pane in self.last_used.keys():
            self.invalidations[pane] = self.states[pane].history_invalidations
        var ordered = List[UInt64](capacity=len(self.states))
        for pane in self.last_used.keys():
            ordered.append(pane)
        for index in range(1, len(ordered)):
            var pane = ordered[index]
            var cursor = index
            while (
                cursor > 0
                and self.last_used[ordered[cursor - 1]] > self.last_used[pane]
            ):
                ordered[cursor] = ordered[cursor - 1]
                cursor -= 1
            ordered[cursor] = pane
        for pane in ordered:
            var excess = self.owned_bytes() - self.byte_budget
            if excess <= 0:
                break
            ref state = self.states[pane]
            var before = state.budget_evictions
            var payload = state.retained_bytes()
            if payload > 0:
                state.trim(max(0, payload - excess), self.next_row_revision)
            self.invalidations[pane] = state.history_invalidations
            if state.budget_evictions != before:
                self.pacer.forget(pane)
            if self.owned_bytes() > self.byte_budget:
                self.invalidations[pane] = state.history_invalidations + 1
                _ = self.states.pop(pane)
                _ = self.last_used.pop(pane)
                self.pacer.forget(pane)

    def publish(
        mut self, viewports: Dict[UInt64, TerminalViewport]
    ) raises -> Dict[UInt64, ArcPointer[HistorySnapshot]]:
        var result = Dict[UInt64, ArcPointer[HistorySnapshot]]()
        for pane in self.last_used.keys():
            if pane in viewports:
                result[pane] = self.states[pane].snapshot(viewports[pane])
        return result^

    def forget(mut self, pane: UInt64) raises:
        if pane in self.states:
            _ = self.states.pop(pane)
        if pane in self.last_used:
            _ = self.last_used.pop(pane)
        if pane in self.invalidations:
            _ = self.invalidations.pop(pane)
        self.pacer.forget(pane)

    def invalidate(mut self, pane: UInt64) raises:
        var stamp = self.invalidations.get(pane, UInt64(0))
        if pane in self.states:
            stamp = self.states[pane].history_invalidations
            _ = self.states.pop(pane)
        self.invalidations[pane] = stamp + 1
        if pane in self.last_used:
            _ = self.last_used.pop(pane)
        self.pacer.forget(pane)
