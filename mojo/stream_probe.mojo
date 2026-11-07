from std.ffi import external_call
from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.stream_transport import connect_interactive
from zzmojo.stream_protocol import input_text
from zzmojo.protocol import Decoder
from zzmojo.terminal_model import TerminalViewport


def has_ascii(viewport: TerminalViewport, text: String) -> Bool:
    var bytes = text.as_bytes()
    if len(bytes) > len(viewport.cells):
        return False
    for start in range(len(viewport.cells) - len(bytes) + 1):
        var matches = True
        for offset in range(len(bytes)):
            if viewport.cells[start + offset].glyph != UInt32(bytes[offset]):
                matches = False
                break
        if matches:
            return True
    return False


def pause():
    _ = external_call["usleep", Int32](UInt32(20000))


def main() raises:
    var args = argv()
    if len(args) != 2:
        raise Error("usage: stream-probe SOCKET")
    var stream = connect_interactive(args[1], "stream-proof")
    for _ in range(100):
        stream.drain()
        if UInt64(0) in stream.model.terminals:
            break
        pause()
    assert_true(UInt64(0) in stream.model.terminals)
    assert_true(has_ascii(stream.model.terminals[0], "REDMARK"))
    var styled = False
    ref viewport = stream.model.terminals[0]
    for cell in viewport.cells:
        if cell.glyph == 82:
            styled = (
                viewport.styles[Int(cell.style)].foreground & 0xFFFFFF
            ) != (viewport.metadata.foreground & 0xFFFFFF)
            break
    assert_true(styled)
    var generation = viewport.generation
    var text = input_text(0, "NATIVE_INPUT\n")
    stream.transport.write(Span(text))
    for _ in range(100):
        stream.drain()
        if has_ascii(stream.model.terminals[0], "ACK:NATIVE_INPUT"):
            break
        pause()
    if not has_ascii(stream.model.terminals[0], "ACK:NATIVE_INPUT"):
        var capture: List[String] = ["-p", "-t", "%0"]
        var diagnostic = stream.command("capture-pane", Span(capture))
        print(
            "daemon capture after native input:",
            String(from_utf8_lossy=Span(diagnostic.output)),
        )
        print(
            "viewport generations:",
            generation,
            stream.model.terminals[0].generation,
        )
        print(
            "tree pending:",
            stream.tree_pending,
            "full pending:",
            len(stream.full_pending),
        )
        print(
            "client view:",
            stream.model.view.session.or_else(UInt64.MAX),
            stream.model.view.focused_window.or_else(UInt64.MAX),
            stream.model.view.read_only,
            stream.model.view.client_flags,
        )
        for raw in stream.model.unhandled:
            var reader = Decoder(raw.copy())
            var kind = reader.uint()
            if kind == 10:
                _ = reader.uint()
                print("unhandled event:", reader.uint(), "bytes:", len(raw))
    assert_true(has_ascii(stream.model.terminals[0], "ACK:NATIVE_INPUT"))
    assert_true(stream.model.terminals[0].generation >= generation)
    assert_true(stream.model.bend_cell_updates > 0)
    var split: List[String] = ["-h", "-t", "%0", "sleep 30"]
    var result = stream.command("split-window", Span(split))
    assert_equal(result.exit_code, UInt8(0))
    var found = False
    for _ in range(100):
        stream.drain()
        for session in stream.model.snapshot.sessions:
            for window in session.windows:
                if len(window.panes) == 2:
                    found = True
        if found:
            break
        pause()
    assert_true(found)
    assert_true(stream.model.bend_tree_updates > 0)
    assert_true(Bool(stream.model.appearance))
    assert_true(stream.model.appearance_revision > 0)
    assert_equal(len(stream.model.appearance_provenance), 30)
    print(
        "native interactive stream passed: colored viewport, direct input,"
        " Bend-mutated cells and split tree"
    )
