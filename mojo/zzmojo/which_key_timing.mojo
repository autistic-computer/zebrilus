comptime TIMER_STALE: UInt8 = 0
comptime TIMER_DROP: UInt8 = 1
comptime TIMER_SHOW: UInt8 = 2


def hidden_generation(generation: UInt64) -> UInt64:
    return UInt64(0) if generation == UInt64.MAX else generation + 1


def hide_for_input(held: Bool, armed: Bool) -> Bool:
    return not (held and armed)


def arm_table(table: Optional[Tuple[String, Bool]]) -> Optional[String]:
    if table and not table.value()[1]:
        return table.value()[0].copy()
    return None


def timer_decision(generation: UInt64, fired_generation: UInt64, table_matches: Bool, blocked: Bool) -> UInt8:
    if generation != fired_generation:
        return TIMER_STALE
    return TIMER_SHOW if table_matches and not blocked else TIMER_DROP


@fieldwise_init
struct WhichKeyChange(TrivialRegisterPassable):
    var state: WhichKeyTiming
    var notify: Bool
    var build: Bool


@fieldwise_init
struct WhichKeyTiming(TrivialRegisterPassable):
    var generation: UInt64
    var timer: Bool
    var visible: Bool

    def hide(self) -> WhichKeyChange:
        return WhichKeyChange(WhichKeyTiming(hidden_generation(self.generation), False, False), self.visible, False)

    def input(self, eligible: Bool, held: Bool, armed: Bool) -> WhichKeyChange:
        return self.hide() if eligible and hide_for_input(held, armed) else WhichKeyChange(self, False, False)

    def table_changed(self, eligible_table: Bool, delay_enabled: Bool) -> WhichKeyChange:
        var next = self.hide()
        next.state.timer = eligible_table and delay_enabled
        return next

    def fire(self, fired_generation: UInt64, table_matches: Bool, blocked: Bool, has_rows: Bool) -> WhichKeyChange:
        var decision = timer_decision(self.generation, fired_generation, table_matches, blocked)
        if decision == TIMER_STALE:
            return WhichKeyChange(self, False, False)
        var state = self
        state.timer = False
        var show = decision == TIMER_SHOW
        if show:
            state.visible = has_rows
        return WhichKeyChange(state, show, show)

    def render(self, blocked: Bool) -> WhichKeyChange:
        var state = self
        if blocked:
            state.timer = False
            state.visible = False
        return WhichKeyChange(state, False, False)

    def show_all(self, has_rows: Bool) -> WhichKeyChange:
        var state = self
        state.visible = has_rows
        return WhichKeyChange(state, True, True)
