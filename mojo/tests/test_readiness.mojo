from std.ffi import external_call
from std.memory import OwnedPointer
from std.testing import assert_equal, assert_true
from mjui.sys.dispatch import MainQueue
from zzmojo.readiness import Readiness, stream_ready


def notify(context: MutOpaquePointer[MutUntrackedOrigin]) abi("C"):
    pass


def main() raises:
    var descriptors = List[Int32](length=2, fill=-1)
    assert_equal(external_call["pipe", Int32](descriptors.unsafe_ptr()), Int32(0))
    var source = OwnedPointer(Readiness())
    var queue = MainQueue(notify, source.ptr().unsafe_bitcast[NoneType]())
    source[].handle = queue.handle
    source[].token = queue.watch_fd(descriptors[0], stream_ready, source.ptr().unsafe_bitcast[NoneType]())
    queue.start()
    var byte: UInt8 = 7
    assert_equal(external_call["write", Int](Int(descriptors[1]), Pointer(to=byte), Int(1)), 1)
    for _ in range(1000):
        _ = queue.drain()
        if source[].ready:
            break
        _ = external_call["usleep", Int32](UInt32(1000))
    assert_true(source[].ready)
    assert_equal(source[].token, UInt64(0))
    source[].ready = False
    for _ in range(10):
        _ = external_call["usleep", Int32](UInt32(1000))
        _ = queue.drain()
    assert_true(not source[].ready)
    assert_equal(external_call["read", Int](Int(descriptors[0]), Pointer(to=byte), Int(1)), 1)
    assert_equal(byte, UInt8(7))
    queue.close()
    _ = external_call["close", Int32](descriptors[0])
    _ = external_call["close", Int32](descriptors[1])
    print("daemon readiness unregisters before worker reads and does not spin")
