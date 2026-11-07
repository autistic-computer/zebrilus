from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool, read_uint, read_text, read_optional_id
from zzmojo.palette_places_model import (
    PaletteFleet,
    PaletteFleetHost,
    PaletteFleetSession,
    PaletteFleetWindow,
    PaletteFleetPane,
    PalettePlace,
)
from zzmojo.palette_places_parse import (
    parse_palette_line,
    read_written_place,
    written_place,
    has_place_target,
    with_place_target,
    command_place_target,
)
from zzmojo.palette_places_search import (
    palette_candidates,
    palette_children,
    search_places,
    place_key,
)
from zzmojo.palette_fuzzy import PaletteMatcher
from zzmojo.palette_target_catalog import target_traits


from palette_fleet_fixture import (
    read_fleet,
    read_place,
    write_place,
    write_text,
)


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var fleets = List[PaletteFleet]()
    var count = Int(read_uint(reader, 64))
    for _ in range(count):
        fleets.append(read_fleet(reader))
    var matcher = PaletteMatcher()
    var bytes = List[UInt8]()
    var cases = Int(read_uint(reader, 65536))
    for _ in range(cases):
        var tag = read_uint(reader, 9)
        var fleet_index = Int(read_uint(reader, UInt64(len(fleets) - 1)))
        ref fleet = fleets[fleet_index]
        var scope = read_place(reader)
        var level = UInt8(read_uint(reader, 3))
        var query = read_text(reader, 1024 * 1024)
        var target = read_text(reader)
        var command = read_text(reader)
        var boosts = Dict[String, Int]()
        var boost_count = Int(read_uint(reader, 65536))
        for _ in range(boost_count):
            var key = place_key(read_place(reader))
            var value = reader.uint()
            boosts[key] = (
                Int(value // 2) if value & 1 == 0 else -Int(value // 2) - 1
            )
        if tag == 0:
            var line = parse_palette_line(query)
            write_text(bytes, line.command)
            write_text(bytes, line.search)
        elif tag == 1:
            var written = read_written_place(query, fleet)
            append_uint(bytes, UInt64(Bool(written)))
            if written:
                write_place(bytes, written.value().place)
                write_text(bytes, written.value().path)
                append_uint(bytes, UInt64(Bool(written.value().line)))
                if written.value().line:
                    append_uint(bytes, UInt64(written.value().line.value()))
        elif tag == 2:
            write_text(bytes, written_place(fleet, scope))
        elif tag >= 3 and tag <= 5:
            var rows = (
                palette_candidates(
                    fleet, scope, level, query, matcher, boosts
                ) if tag
                == 3 else palette_children(fleet, scope, matcher, boosts) if tag
                == 4 else search_places(fleet, scope, query, matcher, boosts)
            )
            append_uint(bytes, UInt64(len(rows)))
            for row in rows:
                write_place(bytes, row.place)
                append_uint(bytes, UInt64(row.level))
                append_text(bytes, row.label)
                append_uint(bytes, UInt64(len(row.hits)))
                for hit in row.hits:
                    append_uint(bytes, UInt64(hit.start))
                    append_uint(bytes, UInt64(hit.end))
                append_uint(
                    bytes,
                    UInt64(
                        row.score * 2 if row.score >= 0 else -row.score * 2 - 1
                    ),
                )
        elif tag == 6:
            write_place(bytes, fleet.current())
        elif tag == 7:
            var pane = fleet.landing(scope)
            append_uint(bytes, UInt64(Bool(pane)))
            if pane:
                var host, session, window, at = pane.value()
                append_uint(
                    bytes,
                    fleet.hosts[host]
                    .sessions[session]
                    .windows[window]
                    .panes[at]
                    .id,
                )
        elif tag == 8:
            append_uint(bytes, UInt64(has_place_target(query)))
            append_text(bytes, with_place_target(query, target))
        else:
            var traits = target_traits(command)
            var result: Optional[String] = None
            if traits:
                result = command_place_target(
                    traits.value().name,
                    traits.value().supported,
                    traits.value().kind,
                    scope,
                )
            write_text(bytes, result)
    reader.finish()
    with open(args[2], "w") as target:
        target.write_all(Span(bytes))
