from std.ffi import external_call
from std.sys import argv
from std.testing import assert_equal, assert_true, assert_false
from zzmojo.stream_transport import InteractiveConnection, connect_interactive
from zzmojo.terminal_model import TerminalViewport
from zzmojo.terminal_search import SearchSession, SEARCH_ACCEPT_AND_CLOSE
from zzmojo.terminal_search_protocol import SearchQuery, CopyAction, copy_mode_request, copy_request, search_request
from zzmojo.stream_protocol import input_text


def pause():
    _ = external_call["usleep", Int32](UInt32(20_000))


def contains(viewport: TerminalViewport, text: String) -> Bool:
    var bytes = text.as_bytes()
    for start in range(max(0, len(viewport.cells) - len(bytes) + 1)):
        var equal = True
        for offset in range(len(bytes)):
            if viewport.cells[start + offset].glyph != UInt32(bytes[offset]):
                equal = False
                break
        if equal:
            return True
    return False


def wait_search(mut stream: InteractiveConnection, total: UInt32, invalid: Bool = False) raises:
    for _ in range(200):
        stream.drain()
        if UInt64(0) in stream.model.terminals:
            var status = stream.model.terminals[0].metadata.search
            if status and not status.value().pending and status.value().total == total and status.value().invalid_pattern == invalid:
                return
        pause()
    raise Error("private daemon search did not converge")


def main() raises:
    var args = argv()
    var stream = connect_interactive(args[1], "search-proof")
    for _ in range(200):
        stream.drain()
        if UInt64(0) in stream.model.terminals and contains(stream.model.terminals[0], "SEARCH-READY"):
            break
        pause()
    assert_true(UInt64(0) in stream.model.terminals)
    assert_true(contains(stream.model.terminals[0], "SEARCH-READY"))
    var frame = search_request(0, 17, SearchQuery("needle", 0, 1, 0))
    stream.transport.write(Span(frame))
    wait_search(stream, 2)
    var first = stream.model.terminals[0].metadata.search.value().current
    frame = search_request(0, 19)
    stream.transport.write(Span(frame))
    for _ in range(200):
        stream.drain()
        var status = stream.model.terminals[0].metadata.search
        if status and not status.value().pending and status.value().current != first:
            break
        pause()
    assert_true(stream.model.terminals[0].metadata.search.value().current != first)
    frame = search_request(0, 20)
    stream.transport.write(Span(frame))
    for _ in range(200):
        stream.drain()
        if stream.model.terminals[0].metadata.search.value().current == first:
            break
        pause()
    assert_equal(stream.model.terminals[0].metadata.search.value().current, first)
    frame = search_request(0, 18, SearchQuery("[", 1, 1, 0))
    stream.transport.write(Span(frame))
    wait_search(stream, 0, True)
    frame = search_request(0, 21)
    stream.transport.write(Span(frame))
    for _ in range(200):
        stream.drain()
        if not stream.model.terminals[0].metadata.search:
            break
        pause()
    assert_false(Bool(stream.model.terminals[0].metadata.search))
    frame = copy_mode_request(0, False, True)
    stream.transport.write(Span(frame))
    for _ in range(200):
        stream.drain()
        var mode = stream.model.terminals[0].metadata.mode
        if mode.kind == 1 and mode.hide_position:
            break
        pause()
    assert_equal(stream.model.terminals[0].metadata.mode.kind, UInt8(1))
    assert_true(stream.model.terminals[0].metadata.mode.hide_position)
    var prompt = SearchSession()
    var effect = prompt.begin(0, 1, SEARCH_ACCEPT_AND_CLOSE)
    stream.transport.write(Span(effect.frame))
    effect = prompt.edit(0, "needle")
    stream.transport.write(Span(effect.frame))
    wait_search(stream, 3)
    effect = prompt.enter(0, False)
    assert_equal(len(effect.frame), 0)
    assert_false(Bool(prompt.query))
    stream.drain()
    assert_true(Bool(stream.model.terminals[0].metadata.search))
    frame = search_request(0, 21)
    stream.transport.write(Span(frame))
    frame = copy_request(0, CopyAction(38))
    stream.transport.write(Span(frame))
    for _ in range(200):
        stream.drain()
        if stream.model.terminals[0].metadata.mode.kind == 0:
            break
        pause()
    assert_equal(stream.model.terminals[0].metadata.mode.kind, UInt8(0))
    frame = input_text(0, "typed\r")
    stream.transport.write(Span(frame))
    for _ in range(200):
        stream.drain()
        if contains(stream.model.terminals[0], "INPUT:typed"):
            break
        pause()
    assert_true(contains(stream.model.terminals[0], "INPUT:typed"))
    assert_true(stream.model.bend_tree_updates > 0)
    assert_true(stream.model.bend_cell_updates > 0)
    print("private search passed: exact case count, next/previous, invalid regex, hidden copy position, accept without close, live return and PTY input isolation")
