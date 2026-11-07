from .local_scroll import (
    LocalScrollState,
    LocalScrollStep,
    ScrollContext,
    LOCAL_SCROLL_DEBOUNCE_NS,
    LOCAL_SCROLL_TIMEOUT_NS,
)
from .state import AppModel


@fieldwise_init
struct PaneScrollEffect(TrivialRegisterPassable):
    var pane: UInt64
    var kind: UInt8
    var value: UInt32


@fieldwise_init
struct ScrollTimer(TrivialRegisterPassable):
    var generation: UInt64
    var deadline: UInt64
    var expire: Bool


def scroll_context(model: AppModel, pane: UInt64) raises -> ScrollContext:
    ref viewport = model.terminals[pane]
    var invalidations = model.history_invalidations.get(pane, UInt64(0))
    if pane in model.histories:
        ref history = model.histories[pane][]
        if (
            history.generation == viewport.generation
            and history.view_generation == viewport.view_generation
            and history.columns == viewport.columns
            and history.rows == viewport.rows
        ):
            return ScrollContext.from_viewport(
                viewport,
                UInt64(len(history.history)),
                UInt64(len(history.below)),
                history.history_invalidations,
            )
    return ScrollContext.from_viewport(viewport, 0, 0, invalidations)


struct TerminalScrollController(Movable):
    var states: Dict[UInt64, LocalScrollState]
    var timers: Dict[UInt64, ScrollTimer]
    var server: UInt64
    var client: UInt64

    def __init__(out self):
        self.states = Dict[UInt64, LocalScrollState]()
        self.timers = Dict[UInt64, ScrollTimer]()
        self.server = 0
        self.client = 0

    def _ensure(mut self, pane: UInt64):
        if pane not in self.states:
            self.states[pane] = LocalScrollState()

    def _step(
        mut self, pane: UInt64, step: LocalScrollStep, now: UInt64
    ) -> List[PaneScrollEffect]:
        var result = List[PaneScrollEffect]()
        for effect in step.effects:
            if effect.kind == 2:
                self.timers[pane] = ScrollTimer(
                    effect.value, now + LOCAL_SCROLL_DEBOUNCE_NS, False
                )
            else:
                result.append(
                    PaneScrollEffect(pane, effect.kind, UInt32(effect.value))
                )
        return result^

    def pixels(
        mut self,
        model: AppModel,
        pane: UInt64,
        delta: Float32,
        line_height: Float32,
        now: UInt64,
    ) raises -> Optional[List[PaneScrollEffect]]:
        if pane not in model.terminals:
            return None
        var context = scroll_context(model, pane)
        if not context.available():
            return None
        self._ensure(pane)
        var step = self.states[pane].scroll_by_pixels(
            delta, line_height, context, now
        )
        if not step:
            return None
        return self._step(pane, step.value(), now)

    def rows(
        mut self, model: AppModel, pane: UInt64, delta: Int64, now: UInt64
    ) raises -> Optional[List[PaneScrollEffect]]:
        if pane not in model.terminals:
            return None
        var context = scroll_context(model, pane)
        if not context.available():
            return None
        self._ensure(pane)
        return self._step(
            pane, self.states[pane].scroll_by_rows(delta, context, now), now
        )

    def observe(
        mut self, model: AppModel, now: UInt64
    ) raises -> List[PaneScrollEffect]:
        var result = List[PaneScrollEffect]()
        if (
            self.server != model.source_server
            or self.client != model.source_client
        ):
            self.states.clear()
            self.timers.clear()
            self.server = model.source_server
            self.client = model.source_client
        var retired = List[UInt64]()
        for pane in self.states:
            if pane not in model.terminals:
                retired.append(pane)
                continue
            var context = scroll_context(model, pane)
            var observation = self.states[pane].observe(context, False, now)
            if observation.restore:
                result.append(
                    PaneScrollEffect(pane, 0, observation.restore.value())
                )
        for pane in retired:
            _ = self.states.pop(pane)
            if pane in self.timers:
                _ = self.timers.pop(pane)
        var due = List[UInt64]()
        for entry in self.timers.items():
            if entry.value.deadline <= now:
                due.append(entry.key)
        for pane in due:
            var timer = self.timers.pop(pane)
            if pane not in self.states or pane not in model.terminals:
                continue
            if timer.expire:
                _ = self.states[pane].expire(timer.generation, now)
            else:
                var sync = self.states[pane].sync(
                    timer.generation, scroll_context(model, pane)
                )
                if sync and sync.value().kind == 1:
                    result.append(
                        PaneScrollEffect(pane, 0, sync.value().target)
                    )
                    self.timers[pane] = ScrollTimer(
                        timer.generation,
                        now
                        + LOCAL_SCROLL_TIMEOUT_NS
                        - LOCAL_SCROLL_DEBOUNCE_NS,
                        True,
                    )
        return result^

    def next_wake(self) -> Optional[UInt64]:
        var result: Optional[UInt64] = None
        for entry in self.timers.items():
            if not result or entry.value.deadline < result.value():
                result = entry.value.deadline
        return result

    def cancel(mut self, pane: UInt64) raises -> Bool:
        if pane in self.timers:
            _ = self.timers.pop(pane)
        return self.states[pane].cancel() if pane in self.states else False

    def clear_fraction(mut self, pane: UInt64) raises -> Bool:
        return (
            self.states[pane].clear_sub_row() if pane in self.states else False
        )

    def flush(mut self, pane: UInt64) raises -> Optional[UInt32]:
        return self.states[pane].flush() if pane in self.states else Optional[
            UInt32
        ](None)

    def release(mut self, pane: UInt64) raises -> Optional[UInt32]:
        return self.states[pane].release() if pane in self.states else Optional[
            UInt32
        ](None)

    def target(self, pane: UInt64) raises -> Optional[UInt32]:
        return self.states[pane].target() if pane in self.states else Optional[
            UInt32
        ](None)

    def sub_row(self, pane: UInt64) raises -> Float32:
        return self.states[pane].sub_row if pane in self.states else Float32(0)
