from std.math import ceil, isnan
from .terminal_model import TerminalViewport, TerminalScrollbar
from .terminal_projection import _add_u32

comptime LOCAL_SCROLL_DEBOUNCE_NS: UInt64 = 120_000_000
comptime LOCAL_SCROLL_TIMEOUT_NS: UInt64 = 2_000_000_000


def _sub_u32(left: UInt32, right: UInt32) -> UInt32:
    return left - right if left >= right else UInt32(0)


def _elapsed(now: UInt64, started: UInt64) -> UInt64:
    return now - started if now >= started else UInt64(0)


def _maximum(left: Float64, right: Float64) -> Float64:
    if isnan(left):
        return right
    if isnan(right):
        return left
    return max(left, right)


def _minimum(left: Float64, right: Float64) -> Float64:
    if isnan(left):
        return right
    if isnan(right):
        return left
    return min(left, right)


def _float_u32(value: Float64) -> UInt32:
    if isnan(value) or value <= 0:
        return 0
    if value >= Float64(UInt32.MAX):
        return UInt32.MAX
    return UInt32(value)


@fieldwise_init
struct ScrollContext(TrivialRegisterPassable):
    var scrollbar: TerminalScrollbar
    var mode: UInt8
    var mouse_tracking: Bool
    var history_rows: UInt64
    var below_rows: UInt64
    var invalidations: UInt64
    var columns: UInt16
    var rows: UInt16

    @staticmethod
    def from_viewport(
        viewport: TerminalViewport,
        history_rows: UInt64,
        below_rows: UInt64,
        invalidations: UInt64,
    ) -> Self:
        return Self(
            viewport.metadata.scrollbar,
            viewport.metadata.mode.kind,
            viewport.metadata.mouse_tracking,
            history_rows,
            below_rows,
            invalidations,
            viewport.columns,
            viewport.rows,
        )

    def live_scrollback(self) -> Bool:
        return (
            self.mode == 0
            and not self.mouse_tracking
            and self.scrollbar.total > self.scrollbar.length
        )

    def available(self) -> Bool:
        return self.live_scrollback() and self.history_rows != 0


@fieldwise_init
struct PixelScrollPosition(TrivialRegisterPassable):
    var target: UInt32
    var sub_row: Float32


def pixel_scroll_position(
    target: UInt32,
    sub_row: Float32,
    delta: Float32,
    line_height: Float32,
    oldest: UInt32,
    newest: UInt32,
) -> PixelScrollPosition:
    var height = Float64(line_height)
    var lower = (Float64(oldest) - Float64(target)) * height
    var upper = (Float64(newest) - Float64(target)) * height
    var relative = _minimum(
        _maximum(-Float64(sub_row) - Float64(delta), lower), upper
    )
    var rows = ceil(relative / height)
    var next_sub = rows * height - relative
    if next_sub >= height:
        rows -= 1.0
        next_sub -= height
    if next_sub < 1e-3:
        next_sub = 0.0
    return PixelScrollPosition(
        _float_u32(
            _minimum(_maximum(Float64(target) + rows, 0.0), Float64(UInt32.MAX))
        ),
        Float32(next_sub),
    )


def daemon_follow_offset(
    target: UInt32,
    reference: UInt32,
    rows: UInt16,
    maximum: UInt32,
    toward_newer: Bool,
) -> Optional[UInt32]:
    var count = max(UInt32(rows), UInt32(1))
    var step = max(count // 2, UInt32(1))
    var desired = min(
        max(min(_add_u32(target, step), maximum), _sub_u32(reference, step)),
        _add_u32(reference, step),
    )
    var lead = _sub_u32(reference, target)
    var needed = (
        reference < target or lead < max(count // 4, UInt32(1))
    ) if toward_newer else lead > max(count * 3 // 4, UInt32(1))
    if needed and reference != desired:
        return desired
    return None


def scroll_fraction_offset(fraction: UInt32, maximum: UInt32) -> UInt32:
    return UInt32(UInt64(maximum) * UInt64(fraction) // UInt64(UInt32.MAX))


def local_scroll_needs_prefetch(target: UInt32, context: ScrollContext) -> Bool:
    if context.history_rows > UInt64(UInt32.MAX):
        return False
    var front = _sub_u32(context.scrollbar.offset, UInt32(context.history_rows))
    var length = UInt32(
        min(UInt64(context.scrollbar.length) * 2, UInt64(UInt32.MAX))
    )
    return target < _add_u32(front, length)


struct LocalScroll(Copyable, Movable):
    var target_offset: UInt32
    var requested_offset: Optional[UInt32]
    var request_floor: Optional[UInt32]
    var started: UInt64

    def __init__(out self, target: UInt32, started: UInt64):
        self.target_offset = target
        self.requested_offset = None
        self.request_floor = None
        self.started = started

    def settled_at(self, server_offset: UInt32) -> Bool:
        return server_offset == self.target_offset and (
            not self.requested_offset
            or self.requested_offset.value() == self.target_offset
        )

    def record_request(mut self, offset: UInt32):
        self.requested_offset = offset
        self.request_floor = min(
            self.request_floor.value(), offset
        ) if self.request_floor else offset

    def observe_server(mut self, offset: UInt32):
        if (self.request_floor and self.request_floor.value() == offset) or (
            self.requested_offset and self.requested_offset.value() == offset
        ):
            self.request_floor = None

    def reach(self, server_offset: UInt32) -> UInt32:
        return min(
            self.request_floor.value(), server_offset
        ) if self.request_floor else server_offset


def local_scroll_should_retire(
    local: LocalScroll,
    server_offset: UInt32,
    expected_invalidations: UInt64,
    invalidations: UInt64,
    now: UInt64,
) -> Bool:
    return (
        local.settled_at(server_offset)
        or expected_invalidations != invalidations
        or _elapsed(now, local.started) >= LOCAL_SCROLL_TIMEOUT_NS
    )


@fieldwise_init
struct LocalScrollEffect(TrivialRegisterPassable):
    var kind: UInt8
    var value: UInt64


struct LocalScrollStep(Copyable, Movable):
    var redraw: Bool
    var effects: List[LocalScrollEffect]

    def __init__(out self):
        self.redraw = False
        self.effects = List[LocalScrollEffect]()


@fieldwise_init
struct LocalScrollSync(TrivialRegisterPassable):
    var kind: UInt8
    var target: UInt32


struct LocalScrollObservation(Copyable, Movable):
    var changed: Bool
    var restore: Optional[UInt32]

    def __init__(out self):
        self.changed = False
        self.restore = None


def _push_prefetch(
    target: UInt32, context: ScrollContext, mut step: LocalScrollStep
):
    if local_scroll_needs_prefetch(target, context):
        step.effects.append(LocalScrollEffect(1, UInt64(target)))


struct LocalScrollState(Copyable, Movable):
    var local: Optional[LocalScroll]
    var generation: UInt64
    var sub_row: Float32
    var observed_invalidations: UInt64
    var observed_columns: UInt16

    def __init__(out self, observed_invalidations: UInt64 = 0):
        self.local = None
        self.generation = 0
        self.sub_row = 0
        self.observed_invalidations = observed_invalidations
        self.observed_columns = 0

    def target(self) -> Optional[UInt32]:
        if self.local:
            return self.local.value().target_offset
        return None

    def clear(mut self) -> Bool:
        var cleared = Bool(self.local)
        self.local = None
        if cleared:
            self.generation += 1
        return cleared

    def cancel(mut self) -> Bool:
        self.generation += 1
        var sub_row = self.clear_sub_row()
        var local = Bool(self.local)
        self.local = None
        return local or sub_row

    def clear_sub_row(mut self) -> Bool:
        var previous = self.sub_row
        self.sub_row = 0
        return previous > 0

    def flush(mut self) -> Optional[UInt32]:
        if not self.local:
            return None
        ref local = self.local.value()
        if (
            local.requested_offset
            and local.requested_offset.value() == local.target_offset
        ):
            return None
        var target = local.target_offset
        local.record_request(target)
        return target

    def release(mut self) -> Optional[UInt32]:
        var target = self.target()
        if target:
            _ = self.clear()
        return target

    def scroll_by_pixels(
        mut self,
        delta: Float32,
        line_height: Float32,
        context: ScrollContext,
        now: UInt64,
    ) -> Optional[LocalScrollStep]:
        if line_height <= 0:
            return None
        var step = LocalScrollStep()
        if delta == 0:
            return step^
        var server = context.scrollbar.offset
        var maximum = _sub_u32(
            context.scrollbar.total, context.scrollbar.length
        )
        var coverage = _sub_u32(
            server, UInt32(min(context.history_rows, UInt64(UInt32.MAX)))
        )
        if self.local:
            self.local.value().observe_server(server)
        var target = server
        var reach = server
        var reference = server
        if self.local:
            target = self.local.value().target_offset
            reach = self.local.value().reach(server)
            reference = self.local.value().requested_offset.or_else(server)
        var position = pixel_scroll_position(
            target,
            self.sub_row,
            delta,
            line_height,
            coverage,
            min(
                _add_u32(
                    reach, UInt32(min(context.below_rows, UInt64(UInt32.MAX)))
                ),
                maximum,
            ),
        )
        var follow = daemon_follow_offset(
            position.target, reference, context.rows, maximum, delta < 0
        )
        var crept = self.sub_row != position.sub_row
        if crept:
            self.sub_row = position.sub_row
            step.redraw = True
        self._pixel_scroll_to(
            position.target, follow, crept, context, now, step
        )
        return step^

    def _pixel_scroll_to(
        mut self,
        target: UInt32,
        follow: Optional[UInt32],
        crept: Bool,
        context: ScrollContext,
        now: UInt64,
        mut step: LocalScrollStep,
    ):
        var moved = not self.local or self.local.value().target_offset != target
        var next = LocalScroll(target, now)
        if self.local:
            next = self.local.value().copy()
            if moved or crept:
                next.target_offset = target
                next.started = now
        if follow:
            next.record_request(follow.value())
            step.effects.append(LocalScrollEffect(0, UInt64(follow.value())))
        if next.settled_at(context.scrollbar.offset):
            step.redraw = self.clear() or step.redraw
            return
        if not moved and not crept and not follow:
            return
        self.observed_invalidations = context.invalidations
        self.observed_columns = context.columns
        self.generation += 1
        self.local = next^
        step.redraw = True
        if moved:
            _push_prefetch(target, context, step)
        step.effects.append(LocalScrollEffect(2, self.generation))

    def scroll_by_rows(
        mut self, delta: Int64, context: ScrollContext, now: UInt64
    ) -> LocalScrollStep:
        var base = UInt64(self.target().or_else(context.scrollbar.offset))
        var target: UInt64
        if delta >= 0:
            target = base + UInt64(delta)
        else:
            var magnitude = UInt64(-(delta + 1)) + 1
            target = base - magnitude if base >= magnitude else UInt64(0)
        return self.scroll_to(
            UInt32(
                min(
                    target,
                    UInt64(
                        _sub_u32(
                            context.scrollbar.total, context.scrollbar.length
                        )
                    ),
                )
            ),
            context,
            now,
        )

    def scroll_to(
        mut self, target: UInt32, context: ScrollContext, now: UInt64
    ) -> LocalScrollStep:
        var step = LocalScrollStep()
        var scrollbar = context.scrollbar
        var coverage = _sub_u32(
            scrollbar.offset,
            UInt32(min(context.history_rows, UInt64(UInt32.MAX))),
        )
        var next_target = min(
            target, _sub_u32(scrollbar.total, scrollbar.length)
        )
        var server = scrollbar.offset
        var at_tail = _add_u32(server, scrollbar.length) >= scrollbar.total
        self.observed_invalidations = context.invalidations
        self.observed_columns = context.columns
        if next_target < coverage:
            _push_prefetch(next_target, context, step)
            step.redraw = self.clear()
            step.effects.append(LocalScrollEffect(0, UInt64(next_target)))
            return step^
        if next_target > _add_u32(
            server, UInt32(min(context.below_rows, UInt64(UInt32.MAX)))
        ):
            step.redraw = self.clear()
            step.effects.append(LocalScrollEffect(0, UInt64(next_target)))
            return step^
        if next_target == server:
            step.redraw = self.clear()
            if at_tail:
                step.effects.append(LocalScrollEffect(0, UInt64(next_target)))
            return step^
        var next = LocalScroll(next_target, now)
        if self.local:
            next = self.local.value().copy()
            next.target_offset = next_target
            next.started = now
        self.generation += 1
        self.local = next^
        step.redraw = True
        _push_prefetch(next_target, context, step)
        step.effects.append(LocalScrollEffect(2, self.generation))
        return step^

    def sync(
        mut self, generation: UInt64, context: ScrollContext
    ) -> Optional[LocalScrollSync]:
        if self.generation != generation or not self.local:
            return None
        self.local.value().observe_server(context.scrollbar.offset)
        if (
            self.local.value().settled_at(context.scrollbar.offset)
            or not context.available()
        ):
            _ = self.clear()
            return LocalScrollSync(0, 0)
        var target = self.local.value().target_offset
        self.local.value().record_request(target)
        return LocalScrollSync(1, target)

    def expire(mut self, generation: UInt64, now: UInt64) -> Bool:
        if self.generation != generation or not self.local:
            return False
        return (
            _elapsed(now, self.local.value().started) >= LOCAL_SCROLL_TIMEOUT_NS
            and self.clear()
        )

    def observe(
        mut self, context: ScrollContext, replaced: Bool, now: UInt64
    ) -> LocalScrollObservation:
        var available = context.available()
        var dropped = (
            replaced or context.invalidations != self.observed_invalidations
        )
        if self.local:
            self.local.value().observe_server(context.scrollbar.offset)
        var observation = LocalScrollObservation()
        if self.local:
            var local = self.local.value().copy()
            if (
                replaced
                or not available
                or local_scroll_should_retire(
                    local,
                    context.scrollbar.offset,
                    self.observed_invalidations,
                    context.invalidations,
                    now,
                )
            ):
                if (
                    dropped
                    and local.target_offset != context.scrollbar.offset
                    and context.columns == self.observed_columns
                    and context.live_scrollback()
                ):
                    observation.restore = local.target_offset
                _ = self.clear()
                observation.changed = True
        if self.sub_row > 0 and (not available or dropped):
            self.sub_row = 0
            observation.changed = True
        self.observed_invalidations = context.invalidations
        self.observed_columns = context.columns
        return observation^

    def overscroll_room(
        self, context: ScrollContext, line_height: Float32
    ) -> Tuple[Float32, Float32]:
        var target = self.target().or_else(context.scrollbar.offset)
        var maximum = _sub_u32(
            context.scrollbar.total, context.scrollbar.length
        )
        var height = Float64(line_height)
        var sub_row = Float64(self.sub_row)
        return (
            Float32(Float64(_sub_u32(maximum, target)) * height + sub_row),
            Float32(_maximum(Float64(target) * height - sub_row, 0)),
        )
