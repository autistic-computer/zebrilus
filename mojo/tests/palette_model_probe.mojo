from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_uint, read_bool, read_text
from zzmojo.palette_places_model import PaletteFleet, PalettePlace
from zzmojo.palette_places_search import place_key
from zzmojo.palette_controller import PaletteDraft, palette_chip
from zzmojo.palette_controller_model import PaletteModel
from zzmojo.palette_presentation import (
    PaletteRow,
    PaletteDecoration,
    PaletteDecorations,
    palette_place_pills,
)
from zzmojo.palette_verbs import PaletteFilled
from zzmojo.palette_navigation import PaletteNavRow, initial_selection
from palette_fleet_fixture import (
    read_fleet,
    read_place,
    write_place,
    write_optional,
)


def decorations(fleet: PaletteFleet) -> PaletteDecorations:
    var result = PaletteDecorations()
    for host in fleet.hosts:
        var value = PaletteDecoration()
        value.right = String(len(host.sessions)) + " sessions"
        value.status = UInt8(0 if host.online else 3)
        result.hosts[host.id] = value^
        for session in host.sessions:
            for window in session.windows:
                for pane in window.panes:
                    var value = PaletteDecoration()
                    value.detail = "pane:" + pane.title
                    value.icon = "SquareTerminal" if pane.terminal else "Plus"
                    value.running = (
                        pane.program and pane.program.value() == "running"
                    )
                    result.panes[
                        place_key(
                            PalettePlace(
                                host.id, session.id, window.id, pane.id
                            )
                        )
                    ] = (value^)
    return result^


def write_row(mut bytes: List[UInt8], row: PaletteRow):
    append_text(bytes, row.label)
    append_text(bytes, row.detail)
    append_text(bytes, row.right)
    append_uint(bytes, UInt64(len(row.hits)))
    for hit in row.hits:
        append_uint(bytes, UInt64(hit.start))
        append_uint(bytes, UInt64(hit.end))
    append_uint(bytes, UInt64(row.muted_prefix))
    append_uint(bytes, UInt64(Bool(row.status)))
    if row.status:
        append_uint(bytes, UInt64(row.status.value()))
    append_uint(bytes, UInt64(row.running))
    append_text(bytes, row.icon)
    append_uint(bytes, 0)
    append_uint(bytes, UInt64(Bool(row.expanded)))
    if row.expanded:
        append_uint(bytes, UInt64(row.expanded.value()))
    append_uint(bytes, UInt64(Bool(row.shortcut)))
    append_uint(bytes, UInt64(row.action))
    write_place(bytes, row.place)
    append_uint(bytes, UInt64(row.index))
    append_text(bytes, row.command)


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var fleets = List[PaletteFleet]()
    var displays = List[PaletteDecorations]()
    var fleet_count = Int(read_uint(reader, 64))
    for _ in range(fleet_count):
        var fleet = read_fleet(reader)
        displays.append(decorations(fleet))
        fleets.append(fleet^)
    var statuses = Int(read_uint(reader, 65536))
    for _ in range(statuses):
        var index = Int(read_uint(reader, UInt64(len(fleets) - 1)))
        var host = UInt32(read_uint(reader, 4294967295))
        var status = UInt8(read_uint(reader, 3))
        ref display = displays[index]
        ref value = display.hosts[host]
        value.status = status
    var bytes = List[UInt8]()
    var cases = Int(read_uint(reader, 65536))
    for _ in range(cases):
        var fleet_index = Int(read_uint(reader, UInt64(len(fleets) - 1)))
        ref fleet = fleets[fleet_index]
        var browser = read_bool(reader)
        var editor = read_bool(reader)
        var insert = read_bool(reader)
        var aliases = read_text(reader, 1024 * 1024)
        var model = PaletteModel(browser, editor, aliases, insert)
        var chips = Int(read_uint(reader, 4))
        for _ in range(chips):
            var at = read_place(reader)
            var automatic = read_bool(reader)
            var chip = palette_chip(fleet, at)
            if chip:
                var value = chip.take()
                value.automatic = automatic
                model.controller.chips.append(value^)
        if read_bool(reader):
            var index = Int(read_uint(reader, 65536))
            var filled = List[PaletteFilled]()
            var labels = List[String]()
            var count = Int(read_uint(reader, 4096))
            for _ in range(count):
                var kind = UInt8(read_uint(reader, 1))
                var place = read_place(reader)
                var text = read_text(reader, 1024 * 1024)
                var label = read_text(reader, 1024 * 1024)
                filled.append(PaletteFilled(kind, place^, text^))
                labels.append(label^)
            if index < len(model.verbs):
                var draft = PaletteDraft(model.verbs[index])
                draft.filled = filled^
                draft.labels = labels^
                model.controller.draft = draft^
        model.controller.set_query(read_text(reader, 64 * 1024))
        var history_count = Int(read_uint(reader, 20))
        for _ in range(history_count):
            model.controller.history.append(read_text(reader, 64 * 1024))
        model.rebuild(fleet, displays[fleet_index])
        append_uint(bytes, UInt64(len(model.rows)))
        var nav = List[PaletteNavRow]()
        for row in model.rows:
            write_row(bytes, row)
            nav.append(
                PaletteNavRow(
                    UInt8(0 if row.action == 0 else 3),
                    row.right == "current",
                    row.indent,
                    row.expanded,
                    0,
                )
            )
        var pills = palette_place_pills(model.controller, displays[fleet_index])
        append_uint(bytes, UInt64(len(pills)))
        for pill in pills:
            append_uint(bytes, UInt64(pill.kind))
            append_text(bytes, pill.prefix)
            append_text(bytes, pill.label)
            append_uint(bytes, UInt64(Bool(pill.status)))
            if pill.status:
                append_uint(bytes, UInt64(pill.status.value()))
        append_text(bytes, model.controller.placeholder())
        var initial = initial_selection(Span(nav))
        append_uint(bytes, UInt64(Bool(initial)))
        if initial:
            append_uint(bytes, UInt64(initial.value()))
    reader.finish()
    with open(args[2], "w") as target:
        target.write_all(Span(bytes))
