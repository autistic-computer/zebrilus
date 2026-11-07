from std.ffi import external_call
from std.sys import argv
from std.time import perf_counter_ns
from std.testing import assert_equal, assert_true, assert_false
from zzmojo.stream_transport import (
    InteractiveConnection,
    connect_interactive,
    readable,
)
from zzmojo.terminal_model import TerminalViewport
from zzmojo.terminal_wire import decode_terminal_frame
from zzmojo.protocol import validate_envelope
from zzmojo.terminal_history import TerminalHistoryState
from zzmojo.history_pacer import HistoryPacer, HistoryRequestContext
from zzmojo.terminal_history_protocol import history_request
from zzmojo.local_scroll import LocalScrollState
from zzmojo.input_protocol import input_view


def pause():
    _ = external_call["usleep", Int32](UInt32(20_000))


def has_ascii(viewport: TerminalViewport, text: String) -> Bool:
    var bytes = text.as_bytes()
    for start in range(max(0, len(viewport.cells) - len(bytes) + 1)):
        var matches = True
        for offset in range(len(bytes)):
            if viewport.cells[start + offset].glyph != UInt32(bytes[offset]):
                matches = False
                break
        if matches:
            return True
    return False


def pump(
    mut stream: InteractiveConnection,
    mut history: TerminalHistoryState,
    mut pacer: HistoryPacer,
    mut counter: UInt64,
) raises:
    for _ in range(256):
        if not readable(stream.transport.fd):
            break
        var frame = stream.transport.read_frame()
        if validate_envelope(Span(frame)) != 1:
            stream.consume(frame^)
            continue
        var update = decode_terminal_frame(frame.copy())
        if update.pane != 0:
            stream.consume(frame^)
        elif update.kind == 1:
            var prepared = history.prepare_patch(
                stream.model.terminals[0], update.patch.value(), counter
            )
            stream.consume(frame^)
            history.commit_patch(
                stream.model.terminals[0],
                update.patch.value(),
                prepared^,
                counter,
            )
        elif update.kind == 0:
            stream.consume(frame^)
            history.replace(stream.model.terminals[0], counter)
        elif update.kind == 3:
            var arrival = pacer.chunk_arrived(0, history.history_mutations)
            stream.consume(frame^)
            assert_true(arrival.apply)
            assert_true(
                history.apply_history(
                    stream.model.terminals[0], update.history.take(), counter
                )
            )
            if UInt64(0) in stream.model.histories:
                _ = stream.model.histories.pop(0)
        else:
            stream.consume(frame^)
    stream.materialize()


def main() raises:
    var args = argv()
    if len(args) != 2:
        raise Error("usage: terminal-history-smoke SOCKET")
    var stream = connect_interactive(args[1], "history-proof")
    for _ in range(150):
        stream.drain()
        if UInt64(0) in stream.model.terminals and has_ascii(
            stream.model.terminals[0], "LIVE-END"
        ):
            break
        pause()
    assert_true(UInt64(0) in stream.model.terminals)
    assert_true(has_ascii(stream.model.terminals[0], "LIVE-END"))
    for _ in range(150):
        stream.drain()
        if (
            stream.model.history_updates > 0
            and len(stream.history_cache.pacer.pending_requests) == 0
        ):
            break
        pause()
    assert_true(stream.model.history_updates > 0)
    assert_true(UInt64(0) in stream.model.retained_histories)
    var counter: UInt64 = 1
    var retained = TerminalHistoryState(stream.model.terminals[0], counter)
    var pacer = HistoryPacer()
    var requested = pacer.request(
        0,
        None,
        2000,
        HistoryRequestContext.from_state(retained, stream.model.terminals[0]),
        UInt64(perf_counter_ns()),
    )
    assert_true(Bool(requested))
    assert_equal(requested.value().count, UInt32(512))
    var frame = history_request(
        0, requested.value().start, requested.value().count
    )
    stream.transport.write(Span(frame))
    for _ in range(150):
        pump(stream, retained, pacer, counter)
        if retained.history.count == 512:
            break
        pause()
    assert_equal(retained.history.count, 512)
    assert_true(retained.retained_bytes() <= 64 * 1024 * 1024)
    var first_revision = retained.history.row(0).revision
    assert_true(first_revision != retained.history.row(1).revision)
    var contains_marker = False
    for index in range(retained.history.count):
        ref cells = retained.history.row(index).cells[].values
        if (
            len(cells) >= 7
            and cells[0].glyph == 72
            and cells[1].glyph == 73
            and cells[2].glyph == 83
        ):
            contains_marker = True
    assert_true(contains_marker)
    var local = LocalScrollState()
    var step = local.scroll_by_pixels(
        45,
        20,
        retained.context(stream.model.terminals[0]),
        UInt64(perf_counter_ns()),
    )
    assert_true(Bool(step))
    assert_equal(local.sub_row, Float32(5))
    var target = local.target().value()
    var synced = local.sync(
        local.generation, retained.context(stream.model.terminals[0])
    )
    assert_true(Bool(synced))
    assert_equal(synced.value().kind, UInt8(1))
    frame = input_view(0, 25, synced.value().target)
    stream.transport.write(Span(frame))
    for _ in range(150):
        pump(stream, retained, pacer, counter)
        if stream.model.terminals[0].metadata.scrollbar.offset == target:
            break
        pause()
    assert_equal(stream.model.terminals[0].metadata.scrollbar.offset, target)
    var observation = local.observe(
        retained.context(stream.model.terminals[0]),
        False,
        UInt64(perf_counter_ns()),
    )
    assert_false(Bool(observation.restore))
    assert_false(Bool(local.target()))
    assert_equal(local.sub_row, Float32(5))
    assert_true(retained.below.count > 0)
    var generation = local.generation
    _ = local.cancel()
    assert_false(
        Bool(
            local.sync(generation, retained.context(stream.model.terminals[0]))
        )
    )
    print(
        (
            "private daemon history passed: 512 source rows, stable identities,"
            " fractional scroll and server convergence"
        ),
        retained.owned_bytes(),
    )
