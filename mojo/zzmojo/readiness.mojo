from std.ffi import external_call
from mjui.sys.dispatch import DispatchRef


struct Readiness(Movable):
    var handle: Optional[DispatchRef]
    var token: UInt64
    var ready: Bool

    def __init__(out self):
        self.handle = None
        self.token = 0
        self.ready = False

    def cancel(mut self) raises:
        if self.token != 0 and self.handle:
            var token = self.token
            self.token = 0
            if (
                external_call["mj_unwatch", Int32](self.handle.value(), token)
                != 0
            ):
                raise Error("cannot unregister the daemon readiness source")


def stream_ready(
    context: MutOpaquePointer[MutUntrackedOrigin], fd: Int32
) abi("C"):
    ref source = context.unsafe_bitcast[Readiness]()[]
    source.ready = True
    try:
        source.cancel()
    except:
        pass
