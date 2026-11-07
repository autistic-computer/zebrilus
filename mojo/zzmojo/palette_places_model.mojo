from .palette_text import palette_trim

comptime PLACE_HOST: UInt8 = 0
comptime PLACE_SESSION: UInt8 = 1
comptime PLACE_WINDOW: UInt8 = 2
comptime PLACE_PANE: UInt8 = 3
comptime MAX_FLEET_NODES = 65536
comptime MAX_FLEET_BYTES = 16 * 1024 * 1024


struct PalettePlace(Copyable, Movable):
    var host: Optional[UInt32]
    var session: Optional[UInt64]
    var window: Optional[UInt64]
    var pane: Optional[UInt64]

    def __init__(
        out self,
        host: Optional[UInt32] = None,
        session: Optional[UInt64] = None,
        window: Optional[UInt64] = None,
        pane: Optional[UInt64] = None,
    ):
        self.host = host
        self.session = session
        self.window = window
        self.pane = pane

    def level(self) -> Optional[UInt8]:
        if self.pane:
            return UInt8(PLACE_PANE)
        if self.window:
            return UInt8(PLACE_WINDOW)
        if self.session:
            return UInt8(PLACE_SESSION)
        if self.host:
            return UInt8(PLACE_HOST)
        return None

    def above(self, level: UInt8) -> Self:
        return Self(
            self.host if level > PLACE_HOST else None,
            self.session if level > PLACE_SESSION else None,
            self.window if level > PLACE_WINDOW else None,
            self.pane if level > PLACE_PANE else None,
        )

    def __eq__(self, other: Self) -> Bool:
        return (
            self.host == other.host
            and self.session == other.session
            and self.window == other.window
            and self.pane == other.pane
        )


@fieldwise_init
struct PaletteFleetPane(Copyable, Movable):
    var id: UInt64
    var index: UInt32
    var title: String
    var program: Optional[String]
    var cwd: String
    var terminal: Bool
    var needs_you: Bool

    def name(self) -> String:
        var title = palette_trim(self.title)
        if title.byte_length() > 0:
            return title^
        if self.program:
            return self.program.value().copy()
        return String("pane")


@fieldwise_init
struct PaletteFleetWindow(Copyable, Movable):
    var id: UInt64
    var index: UInt32
    var name: String
    var active: Bool
    var active_pane: UInt64
    var panes: List[PaletteFleetPane]


@fieldwise_init
struct PaletteFleetSession(Copyable, Movable):
    var id: UInt64
    var name: String
    var label: String
    var windows: List[PaletteFleetWindow]

    def active_window(self) -> Optional[Int]:
        for index in range(len(self.windows)):
            if self.windows[index].active:
                return index
        if len(self.windows) > 0:
            return 0
        return None


@fieldwise_init
struct PaletteFleetHost(Copyable, Movable):
    var id: UInt32
    var name: String
    var online: Bool
    var current: Optional[UInt64]
    var sessions: List[PaletteFleetSession]


struct PaletteFleet(Copyable, Movable):
    var hosts: List[PaletteFleetHost]
    var here: UInt32

    def __init__(out self):
        self.hosts = List[PaletteFleetHost]()
        self.here = 0

    def validate(self) raises:
        var count = len(self.hosts)
        var size = 0
        for host in self.hosts:
            size += host.name.byte_length()
            count += len(host.sessions)
            for session in host.sessions:
                size += session.name.byte_length() + session.label.byte_length()
                count += len(session.windows)
                for window in session.windows:
                    size += window.name.byte_length()
                    count += len(window.panes)
                    for pane in window.panes:
                        size += (
                            pane.title.byte_length()
                            + pane.cwd.byte_length()
                            + (
                                pane.program.value().byte_length() if pane.program else 0
                            )
                        )
            if count > MAX_FLEET_NODES or size > MAX_FLEET_BYTES:
                raise Error("palette fleet exceeds its node or text limit")

    def host_index(self, id: UInt32) -> Optional[Int]:
        for index in range(len(self.hosts)):
            if self.hosts[index].id == id:
                return index
        return None

    def named_host(self, name: String) -> Optional[Int]:
        for index in range(len(self.hosts)):
            if self.hosts[index].name == name:
                return index
        return None

    def session_index(self, place: PalettePlace) -> Optional[Tuple[Int, Int]]:
        if not place.host or not place.session:
            return None
        var host = self.host_index(place.host.value())
        if not host:
            return None
        for session in range(len(self.hosts[host.value()].sessions)):
            if (
                self.hosts[host.value()].sessions[session].id
                == place.session.value()
            ):
                return (host.value(), session)
        return None

    def window_index(
        self, place: PalettePlace
    ) -> Optional[Tuple[Int, Int, Int]]:
        var session = self.session_index(place)
        if not session or not place.window:
            return None
        var host, record = session.value()
        for window in range(len(self.hosts[host].sessions[record].windows)):
            if (
                self.hosts[host].sessions[record].windows[window].id
                == place.window.value()
            ):
                return (host, record, window)
        return None

    def pane_index(
        self, place: PalettePlace
    ) -> Optional[Tuple[Int, Int, Int, Int]]:
        var window = self.window_index(place)
        if not window or not place.pane:
            return None
        var host, session, record = window.value()
        for pane in range(
            len(self.hosts[host].sessions[session].windows[record].panes)
        ):
            if (
                self.hosts[host]
                .sessions[session]
                .windows[record]
                .panes[pane]
                .id
                == place.pane.value()
            ):
                return (host, session, record, pane)
        return None

    def current(self) -> PalettePlace:
        var index = self.host_index(self.here)
        if not index:
            return PalettePlace()
        ref host = self.hosts[index.value()]
        var place = PalettePlace(host.id)
        if not host.current:
            return place^
        var session = self.session_index(PalettePlace(host.id, host.current))
        if not session:
            return place^
        place.session = host.current
        ref record = host.sessions[session.value()[1]]
        var window = record.active_window()
        if window:
            place.window = record.windows[window.value()].id
            place.pane = record.windows[window.value()].active_pane
        return place^

    def landing(
        self, place: PalettePlace
    ) -> Optional[Tuple[Int, Int, Int, Int]]:
        if place.pane:
            return self.pane_index(place)
        var target = place.copy()
        var window = self.window_index(target)
        if not window:
            var session = self.session_index(target)
            if not session:
                if not target.host:
                    return None
                var host = self.host_index(target.host.value())
                if not host or not self.hosts[host.value()].current:
                    return None
                target.session = self.hosts[host.value()].current
                session = self.session_index(target)
            if not session:
                return None
            var host, record = session.value()
            var active = self.hosts[host].sessions[record].active_window()
            if not active:
                return None
            target.window = (
                self.hosts[host].sessions[record].windows[active.value()].id
            )
            window = (host, record, active.value())
        var host, session, record = window.value()
        target.pane = (
            self.hosts[host].sessions[session].windows[record].active_pane
        )
        return self.pane_index(target)
