from .desktop_nav import NavActivation
from .exec_protocol import Invocation

comptime HOST_DISCONNECTED = 0
comptime HOST_CONNECTING = 1
comptime HOST_RECONNECTING = 2
comptime HOST_CONNECTED = 3
comptime HOST_UNREACHABLE = 4
comptime HOST_INCOMPATIBLE = 5
comptime HOST_MISSING = 6
comptime HOST_INSTALLING = 7
comptime HOST_REASON_BYTES = 64 * 1024


struct HostState(Copyable, Equatable, Movable):
    var kind: UInt8
    var attempt: UInt32
    var reason: String
    var local: UInt16
    var remote: Optional[UInt16]

    def __init__(out self, kind: UInt8):
        self.kind = kind
        self.attempt = 0
        self.reason = String()
        self.local = 0
        self.remote = None

    @staticmethod
    def reconnecting(attempt: UInt32) -> Self:
        var state = Self(HOST_RECONNECTING)
        state.attempt = attempt
        return state^

    @staticmethod
    def unreachable(var reason: String) -> Self:
        var state = Self(HOST_UNREACHABLE)
        state.reason = reason^
        return state^

    @staticmethod
    def incompatible(local: UInt16, remote: Optional[UInt16]) -> Self:
        var state = Self(HOST_INCOMPATIBLE)
        state.local = local
        state.remote = remote
        return state^

    def __eq__(self, other: Self) -> Bool:
        return (
            self.kind == other.kind
            and self.attempt == other.attempt
            and self.reason == other.reason
            and self.local == other.local
            and self.remote == other.remote
        )

    def validate(self) raises:
        if self.kind > HOST_INSTALLING:
            raise Error("desktop host state is invalid")
        if self.reason.byte_length() > HOST_REASON_BYTES:
            raise Error("desktop host failure reason exceeds its byte limit")
        if (
            self.kind != HOST_RECONNECTING
            and self.attempt != 0
            or self.kind != HOST_UNREACHABLE
            and self.reason.byte_length() != 0
            or self.kind != HOST_INCOMPATIBLE
            and (self.local != 0 or Bool(self.remote))
        ):
            raise Error("desktop host state carries foreign fields")

    def connected(self) -> Bool:
        return self.kind == HOST_CONNECTED

    def label(self) -> String:
        if self.kind == HOST_DISCONNECTED:
            return "disconnected"
        if self.kind == HOST_CONNECTING:
            return "connecting"
        if self.kind == HOST_RECONNECTING:
            return "reconnecting"
        if self.kind == HOST_CONNECTED:
            return "connected"
        if self.kind == HOST_UNREACHABLE:
            return "unreachable"
        if self.kind == HOST_INCOMPATIBLE:
            return "incompatible"
        if self.kind == HOST_MISSING:
            return "not installed"
        return "installing"

    def install_offer(self) -> Optional[UInt8]:
        if self.kind == HOST_MISSING:
            return UInt8(0)
        if self.kind == HOST_INCOMPATIBLE and (
            not self.remote or self.local >= self.remote.value()
        ):
            return UInt8(1)
        return None

    def failure_detail(self) -> Optional[String]:
        if self.kind == HOST_UNREACHABLE:
            return self.reason.copy()
        if self.kind == HOST_INCOMPATIBLE:
            if not self.remote:
                return String(
                    "That machine runs a zz too old to report its"
                    " protocol.\nUpdate zz there, then reconnect."
                )
            var remote = self.remote.value()
            if self.local == remote:
                return String(
                    "That machine runs an older build of protocol v",
                    remote,
                    ".\nUpgrade zz there, or restart its daemon, then reconnect.",
                )
            return String(
                "This zz speaks protocol v",
                self.local,
                "; that machine speaks v",
                remote,
                ".\nUpgrade whichever side is older, then reconnect.",
            )
        if self.kind == HOST_MISSING:
            return String(
                "zz is not installed on that machine.\nRight-click it and"
                " choose Install zz."
            )
        return None


@fieldwise_init
struct HostIndicator(Copyable, Movable):
    var failed: Bool
    var detail: Optional[String]


def host_indicator(state: HostState, loaded: Bool) -> Optional[HostIndicator]:
    if state.kind == HOST_CONNECTED and loaded:
        return None
    if (
        state.kind == HOST_CONNECTED
        or state.kind == HOST_CONNECTING
        or state.kind == HOST_RECONNECTING
        or state.kind == HOST_INSTALLING
    ):
        return HostIndicator(False, None)
    return HostIndicator(True, state.failure_detail())


def host_activation(
    host: UInt32, attached: UInt32, state: HostState
) -> Optional[NavActivation]:
    if state.kind == HOST_CONNECTED and host != attached:
        return NavActivation(0, host, None, String(), List[Invocation]())
    if (
        state.kind == HOST_DISCONNECTED
        or state.kind == HOST_RECONNECTING
        or state.kind == HOST_UNREACHABLE
        or state.kind == HOST_INCOMPATIBLE
        or state.kind == HOST_MISSING
    ):
        return NavActivation(5, host, None, String(), List[Invocation]())
    return None
