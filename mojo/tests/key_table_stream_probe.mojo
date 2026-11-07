from std.sys import argv
from std.ffi import external_call
from std.time import perf_counter_ns
from std.testing import assert_equal, assert_true
from zzmojo.stream_transport import connect_interactive, InteractiveConnection
from zzmojo.state import AppModel
from zzmojo.input_protocol import KeyInput
from zzmojo.protocol import append_uint, append_text, envelope, Decoder


def wait_query(mut connection: InteractiveConnection, input: KeyInput, all_keys: Bool) raises:
    var started = perf_counter_ns()
    while perf_counter_ns() - started < 3_000_000_000:
        connection.drain()
        if connection.model.key_tables.eligible and connection.model.key_tables.snapshot.value()[].prefix.query(input).all_keys == all_keys:
            return
        _ = external_call["usleep", Int32](UInt32(10000))
    raise Error("private daemon key notification did not converge")


def wait_ready(mut connection: InteractiveConnection) raises:
    var started = perf_counter_ns()
    while perf_counter_ns() - started < 3_000_000_000:
        connection.drain()
        if connection.model.key_tables.eligible:
            return
        _ = external_call["usleep", Int32](UInt32(10000))
    raise Error("private daemon key table resync did not finish")


def main() raises:
    var arguments = argv()
    var connection = connect_interactive(arguments[1], "keys-proof", native_keys=True)
    var response = connection.command("show-options", [String("-gqv"), String("prefix")])
    assert_equal(response.exit_code, UInt8(0))
    wait_ready(connection)
    assert_true(connection.model.key_tables.eligible)
    assert_true(Bool(connection.model.key_tables.snapshot))
    var old = connection.model.key_tables.copy()
    var raw = KeyInput()
    raw.key = 0
    raw.codepoint = 120
    raw.modifiers = 2
    var previous = old.snapshot.value()[].prefix.query(raw)
    response = connection.command("bind-key", [String("-T"), String("prefix"), String("C-x"), String("list-keys"), String("-N")])
    assert_equal(response.exit_code, UInt8(0))
    wait_query(connection, raw, True)
    assert_true(connection.model.key_tables.eligible)
    assert_true(connection.model.key_tables.snapshot.value()[].prefix.query(raw).all_keys)
    assert_equal(old.snapshot.value()[].prefix.query(raw).all_keys, previous.all_keys)
    var model = AppModel()
    model.refresh(connection)
    var address = Int(model.key_tables.snapshot.value()[].tables.unsafe_ptr())
    for _ in range(1000):
        var copied = model.copy()
        assert_equal(Int(copied.key_tables.snapshot.value()[].tables.unsafe_ptr()), address)
    response = connection.command("bind-key", [String("-T"), String("root"), String("C-WheelUpPane"), String("list-keys"), String("-N")])
    assert_equal(response.exit_code, UInt8(0))
    assert_true(len(connection.model.key_tables.snapshot.value()[].root_mouse) > 0)
    response = connection.command("unbind-key", [String("-T"), String("prefix"), String("C-x")])
    assert_equal(response.exit_code, UInt8(0))
    wait_query(connection, raw, False)
    assert_equal(connection.model.key_tables.snapshot.value()[].prefix.query(raw).all_keys, False)
    var mismatch: List[UInt8] = [10, 0, 59]
    append_uint(mismatch, connection.model.key_tables.snapshot.value()[].hash ^ 1)
    mismatch.extend([0, 0])
    connection.consume(envelope(Span(mismatch)))
    assert_equal(connection.model.key_tables.eligible, False)
    assert_true(connection.model.key_tables.pending)
    var revision = connection.model.key_tables.revision
    connection.request_key_tables()
    assert_equal(connection.model.key_tables.revision, revision)
    response = connection.command("show-options", [String("-gqv"), String("prefix")])
    assert_equal(response.exit_code, UInt8(0))
    wait_ready(connection)
    assert_true(connection.model.key_tables.eligible)
    assert_equal(connection.model.key_tables.pending, False)
    var oversized: List[UInt8] = [10, 0, 25]
    append_uint(oversized, 257)
    for index in range(257):
        append_text(oversized, String("too-many-", index))
        oversized.append(0)
    connection.consume(envelope(Span(oversized)))
    assert_equal(connection.model.key_tables.eligible, False)
    assert_true(connection.model.key_tables.pending)
    assert_true(connection.model.key_tables.error.byte_length() > 0)
    response = connection.command("show-options", [String("-gqv"), String("prefix")])
    assert_equal(response.exit_code, UInt8(0))
    wait_ready(connection)
    assert_true(connection.model.key_tables.eligible)
    var unhandled = connection.model.take_events()
    for frame in unhandled:
        var reader = Decoder(frame.copy())
        if reader.uint() == 10:
            _ = reader.uint()
            var kind = reader.uint()
            assert_true(kind != 25 and kind != 56 and kind != 59 and kind != 54)
    print("private key table retention, dynamic binds, shared snapshots, bounded refusal and resync passed")
