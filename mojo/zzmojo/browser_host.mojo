from std.memory import ArcPointer

comptime MAX_BROWSER_SESSIONS = 4096


@fieldwise_init
struct BrowserSession(Copyable, Movable):
    var pane: UInt64
    var tab: UInt64
    var session: UInt64


@fieldwise_init
struct BrowserSource(Copyable, Movable):
    var pane: UInt64
    var tab: UInt64
    var session: UInt64
    var generation: UInt64
    var actor: ArcPointer[UInt8]


struct BrowserHost(Copyable, Movable):
    var sessions: List[BrowserSession]
    var generation: UInt64
    var actor: ArcPointer[UInt8]

    def __init__(out self):
        self.sessions = List[BrowserSession]()
        self.generation = 0
        self.actor = ArcPointer(UInt8(0))

    def bind(mut self, pane: UInt64, tab: UInt64, session: UInt64) raises:
        if self.generation == UInt64.MAX:
            raise Error("browser host generation exhausted")
        var index = -1
        for i in range(len(self.sessions)):
            var same_key = (
                self.sessions[i].pane == pane and self.sessions[i].tab == tab
            )
            if self.sessions[i].session == session and not same_key:
                raise Error("browser session already belongs to another tab")
            if same_key:
                index = i
        if index < 0 and len(self.sessions) >= MAX_BROWSER_SESSIONS:
            raise Error("browser host session capacity exhausted")
        var value = BrowserSession(pane, tab, session)
        if index < 0:
            self.sessions.append(value^)
        else:
            self.sessions[index] = value^
        self.generation += 1

    def source(self, session: UInt64) -> Optional[BrowserSource]:
        for entry in self.sessions:
            if entry.session == session:
                return BrowserSource(
                    entry.pane,
                    entry.tab,
                    session,
                    self.generation,
                    self.actor.copy(),
                )
        return None

    def validate(self, source: BrowserSource) raises:
        if source.generation != self.generation:
            raise Error("browser source receipt is stale")
        self.validate_owner(source)

    def same_actor(self, actor: ArcPointer[UInt8]) -> Bool:
        return Int(Pointer(to=self.actor[])) == Int(Pointer(to=actor[]))

    def validate_owner(self, source: BrowserSource) raises:
        if not self.same_actor(source.actor):
            raise Error("browser source belongs to another host")
        var current = self.source(source.session)
        if (
            not current
            or current.value().pane != source.pane
            or current.value().tab != source.tab
        ):
            raise Error("browser event belongs to a retired source")

    def retire(mut self, source: BrowserSource) raises:
        self.validate(source)
        if self.generation == UInt64.MAX:
            raise Error("browser host generation exhausted")
        var sessions = List[BrowserSession]()
        for entry in self.sessions:
            if entry.session != source.session:
                sessions.append(entry.copy())
        self.sessions = sessions^
        self.generation += 1
