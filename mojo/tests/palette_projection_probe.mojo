from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_uint, read_text
from zzmojo.stream_protocol import decode_stream_frame
from zzmojo.palette_navigation_projection import (
    palette_ordered_panes,
    palette_pane_index,
    palette_pane_label,
    palette_session_label,
)
from zzmojo.palette_programs import PalettePrograms
from zzmojo.tree_model import PaneSnapshot, _Order, _ordered
from zzmojo.palette_snapshot import project_palette_snapshot, palette_hostname
from zzmojo.palette_places_model import PalettePlace
from zzmojo.palette_places_search import place_key


def write_text(mut bytes: List[UInt8], value: Optional[String]):
    append_uint(bytes, UInt64(Bool(value)))
    if value:
        append_text(bytes, value.value())


def pane_program(
    pane: PaneSnapshot, programs: PalettePrograms
) raises -> Optional[Int]:
    if pane.kind.kind == 1:
        return programs.resolve(pane.summary.process)
    if pane.kind.kind == 4:
        return programs.by_id("codex" if pane.kind.provider == 0 else "claude")
    return None


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var count = Int(read_uint(reader, 65536))
    var bytes = List[UInt8]()
    var programs = PalettePrograms()
    for _ in range(count):
        var request = reader.bytes()
        var input = Decoder(request^)
        input.offset = 0
        var kind = read_uint(input, 1)
        if kind == 0:
            var frame = decode_stream_frame(input.bytes())
            var snapshot = frame.snapshot.take()
            var attention = Dict[UInt64, UInt8]()
            for id in range(100, 106):
                attention[UInt64(id)] = UInt8((id - 100) % 4)
            var projected = project_palette_snapshot(
                snapshot, snapshot.sessions[0].id, " host ", programs, attention
            )
            assert_equal(projected.fleet.here, UInt32(0))
            assert_equal(projected.fleet.hosts[0].name, "host")
            assert_equal(
                projected.fleet.hosts[0].current,
                Optional[UInt64](snapshot.sessions[0].id),
            )
            append_uint(bytes, UInt64(len(snapshot.sessions)))
            for session_index in range(len(snapshot.sessions)):
                ref session = snapshot.sessions[session_index]
                ref projected_session = projected.fleet.hosts[0].sessions[
                    session_index
                ]
                append_text(bytes, projected_session.label)
                append_uint(bytes, UInt64(len(session.windows)))
                for window_index in range(len(session.windows)):
                    ref window = session.windows[window_index]
                    ref projected_window = projected_session.windows[
                        window_index
                    ]
                    append_uint(bytes, UInt64(len(projected_window.panes)))
                    for pane in projected_window.panes:
                        append_uint(bytes, pane.id)
                    assert_equal(
                        projected_window.active,
                        window.id == snapshot.focused_window_for(session),
                    )
                    var queries: List[UInt64] = [
                        100,
                        101,
                        103,
                        105,
                        110,
                        111,
                        113,
                        115,
                        99999,
                        0,
                        UInt64.MAX,
                    ]
                    for id in queries:
                        append_uint(
                            bytes, UInt64(palette_pane_index(window, id))
                        )
                    var sorted = List[_Order]()
                    for id in window.panes.keys():
                        sorted.append(_Order(id, 0, 0))
                    sorted = _ordered(sorted^)
                    append_uint(bytes, UInt64(len(sorted)))
                    for item in sorted:
                        ref pane = window.panes[item.major]
                        var place = PalettePlace(
                            UInt32(0), session.id, window.id, pane.id
                        )
                        var found = projected.fleet.pane_index(place)
                        assert_true(Bool(found))
                        ref record = projected_window.panes[found.value()[3]]
                        append_uint(bytes, record.id)
                        append_uint(bytes, UInt64(record.index))
                        append_text(bytes, record.title)
                        write_text(bytes, record.program)
                        ref decoration = projected.decorations.panes[
                            place_key(place)
                        ]
                        var phase = attention.get(pane.id, UInt8(255))
                        assert_equal(decoration.running, phase == 1)
                        if pane.kind.kind == 4:
                            assert_equal(decoration.detail, "Agent")
                            assert_equal(
                                decoration.icon,
                                'Asset("icons/openai.svg")' if pane.kind.provider
                                == 0 else 'Asset("icons/claude.svg")',
                            )
        else:
            var candidates = List[String]()
            var size = Int(read_uint(input, 4096))
            for _ in range(size):
                candidates.append(read_text(input, 1024 * 1024))
            var found = programs.resolve(candidates)
            write_text(
                bytes,
                programs.values[found.value()].id.copy() if found else Optional[
                    String
                ](None),
            )
            write_text(
                bytes,
                programs.values[
                    found.value()
                ].name.copy() if found else Optional[String](None),
            )
        input.finish()
    reader.finish()
    with open(args[2], "w") as target:
        target.write_all(Span(bytes))
