from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true
from zzmojo.cli_backend import spawn_backend
from zzmojo.transport import connect


def main() raises:
    var args = argv()
    var previous = SIMD[.uint64, 16](0)
    var mask = SIMD[.uint64, 16](0)
    _ = external_call["sigemptyset", Int32](Pointer(to=mask))
    for signal in [Int32(2), 15, 17, 28]:
        _ = external_call["sigaddset", Int32](Pointer(to=mask), signal)
    assert_equal(
        external_call["pthread_sigmask", Int32](
            Int32(2), Pointer(to=mask), Pointer(to=previous)
        ),
        Int32(0),
    )
    var configs: List[List[UInt8]] = [List[UInt8](args[2].as_bytes())]
    var started = spawn_backend(args[1].as_bytes(), configs)
    assert_true(started.process > 0)
    var current = SIMD[.uint64, 16](0)
    var empty = SIMD[.uint64, 16](0)
    assert_equal(
        external_call["pthread_sigmask", Int32](
            Int32(0), Pointer(to=empty), Pointer(to=current)
        ),
        Int32(0),
    )
    for signal in [Int32(2), 15, 17, 28]:
        assert_equal(
            external_call["sigismember", Int32](Pointer(to=current), signal),
            Int32(1),
        )
    assert_equal(
        external_call["pthread_sigmask", Int32](
            Int32(2), Pointer(to=previous), Pointer(to=current)
        ),
        Int32(0),
    )
    var connection = connect(args[1])
    assert_equal(connection.server_id, started.server)
    print(started.server, started.process)
