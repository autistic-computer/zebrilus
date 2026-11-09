from .agent_text import text_find, text_slice, text_trim

comptime FLIP_TO_CHAT: UInt8 = 0
comptime FLIP_TO_CLI: UInt8 = 1

comptime FLIP_ASK: Int = 0
comptime FLIP_NOW: Int = 1
comptime FLIP_WHEN_IDLE: Int = 2
comptime FLIP_CANCEL: Int = 3
comptime FLIP_IGNORED: Int = -1
comptime FLIP_DISMISSED: Int = -2

comptime FLIP_IDLE: UInt8 = 0
comptime FLIP_SENDING: UInt8 = 1
comptime FLIP_BUSY: UInt8 = 2
comptime FLIP_ARMED: UInt8 = 3

comptime FLIP_BUSY_TITLE = "The agent is busy"
comptime FLIP_WHEN_IDLE_LABEL = "Switch when idle"
comptime FLIP_NOW_LABEL = "Switch now"
comptime FLIP_CANCEL_LABEL = "Cancel"


def flip_label(to: UInt8) -> String:
    return "Switch to chat" if to == FLIP_TO_CHAT else "Switch to the CLI"


def flip_busy_detail(reason: String) -> String:
    return String("Switching now stops ", reason, ".")


def flip_armed_label(to: UInt8) -> String:
    return (
        "Switches to chat when idle"
        if to == FLIP_TO_CHAT
        else "Switches to the CLI when idle"
    )


def flip_tooltip(to: UInt8, armed: Bool) -> String:
    return "Cancel the switch" if armed else flip_label(to)


def flip_command(pane: UInt64, mode: Int) -> List[String]:
    var args = List[String]()
    args.append("-t")
    args.append(String("%", pane))
    if mode == FLIP_NOW:
        args.append("--now")
    elif mode == FLIP_WHEN_IDLE:
        args.append("--when-idle")
    elif mode == FLIP_CANCEL:
        args.append("--cancel")
    return args^


def busy_reason(error: String) -> Optional[String]:
    var at = text_find(error, "busy:")
    if at < 0:
        return None
    var before = text_slice(error, 0, at)
    if before.byte_length() > 0 and not before.endswith(": "):
        return None
    var reason = text_trim(text_slice(error, at + 5, error.byte_length()))
    if reason.byte_length() == 0:
        return Optional[String]("something is running")
    return Optional[String](reason^)


@fieldwise_init
struct Flip(Copyable, Movable):
    var kind: UInt8
    var mode: Int
    var reason: String

    def pending(self) -> Bool:
        return self.kind == FLIP_SENDING

    def armed(self) -> Bool:
        return (
            self.kind == FLIP_ARMED
            or self.kind == FLIP_SENDING
            and self.mode == FLIP_CANCEL
        )

    def busy(self) -> Bool:
        return self.kind == FLIP_BUSY


@fieldwise_init
struct PaneFlip(Copyable, Movable):
    var to: UInt8
    var flip: Flip


@fieldwise_init
struct FlipRequest(Copyable, Movable):
    var pane: UInt64
    var mode: Int


@fieldwise_init
struct FlipSide(Copyable, Movable):
    var pane: UInt64
    var to: UInt8
    var armed: Bool


struct AgentFlips(Movable):
    var panes: Dict[UInt64, PaneFlip]
    var armed: Dict[UInt64, UInt8]
    var requests: Dict[UInt64, FlipRequest]

    def __init__(out self):
        self.panes = Dict[UInt64, PaneFlip]()
        self.armed = Dict[UInt64, UInt8]()
        self.requests = Dict[UInt64, FlipRequest]()

    def state(self, pane: UInt64) raises -> Flip:
        if pane in self.panes:
            return self.panes[pane].flip.copy()
        if pane in self.armed:
            return Flip(FLIP_ARMED, 0, "")
        return Flip(FLIP_IDLE, 0, "")

    def side(self, pane: UInt64) raises -> Optional[UInt8]:
        if pane in self.panes:
            return Optional[UInt8](self.panes[pane].to)
        if pane in self.armed:
            return Optional[UInt8](self.armed[pane])
        return None

    def press(mut self, pane: UInt64) raises -> Int:
        var state = self.state(pane)
        if state.kind == FLIP_IDLE:
            return FLIP_ASK
        if state.kind == FLIP_BUSY:
            self.dismiss(pane)
            return FLIP_DISMISSED
        if state.kind == FLIP_ARMED:
            return FLIP_CANCEL
        return FLIP_IGNORED

    def choose(self, pane: UInt64, mode: Int) raises -> Bool:
        if not self.side(pane):
            return False
        var state = self.state(pane)
        if state.kind == FLIP_BUSY:
            return mode == FLIP_NOW or mode == FLIP_WHEN_IDLE
        return state.kind == FLIP_ARMED and mode == FLIP_CANCEL

    def dismiss(mut self, pane: UInt64) raises:
        if pane in self.panes and self.panes[pane].flip.kind == FLIP_BUSY:
            _ = self.panes.pop(pane)

    def sent(
        mut self, pane: UInt64, to: UInt8, mode: Int, request: Optional[UInt64]
    ) -> Bool:
        if not request:
            return False
        self.requests[request.value()] = FlipRequest(pane, mode)
        self.panes[pane] = PaneFlip(to, Flip(FLIP_SENDING, mode, ""))
        return True

    def owns(self, request: UInt64) -> Bool:
        return request in self.requests

    def settle(
        mut self, request: UInt64, failure: Optional[String]
    ) raises -> Optional[String]:
        if request not in self.requests:
            return None
        var sent = self.requests.pop(request)
        if sent.pane not in self.panes:
            return None
        var entry = self.panes[sent.pane].copy()
        if entry.flip.kind != FLIP_SENDING or entry.flip.mode != sent.mode:
            return failure.copy()
        var next = Flip(FLIP_IDLE, 0, "")
        var message = Optional[String]()
        if not failure:
            if sent.mode == FLIP_WHEN_IDLE:
                next = Flip(FLIP_ARMED, 0, "")
        else:
            var reason = busy_reason(failure.value())
            if sent.mode == FLIP_ASK and reason:
                next = Flip(FLIP_BUSY, 0, reason.value())
            elif sent.mode == FLIP_CANCEL:
                next = Flip(FLIP_ARMED, 0, "")
                message = failure.copy()
            else:
                message = failure.copy()
        if next.kind == FLIP_IDLE:
            _ = self.panes.pop(sent.pane)
        else:
            self.panes[sent.pane] = PaneFlip(entry.to, next^)
        return message^

    def observe_panes(mut self, sides: List[FlipSide]) raises:
        var known = Dict[UInt64, UInt8]()
        self.armed = Dict[UInt64, UInt8]()
        for side in sides:
            known[side.pane] = side.to
            if side.armed:
                self.armed[side.pane] = side.to
        var dropped = List[UInt64]()
        for pane in self.panes.keys():
            if (
                pane not in known
                or known[pane] != self.panes[pane].to
                or self.panes[pane].flip.kind == FLIP_ARMED
            ):
                dropped.append(pane)
        for pane in dropped:
            _ = self.panes.pop(pane)
        var stale = List[UInt64]()
        for request in self.requests.keys():
            if self.requests[request].pane not in self.panes:
                stale.append(request)
        for request in stale:
            _ = self.requests.pop(request)
