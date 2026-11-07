from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_uint, read_bool, read_text
from zzmojo.palette_places_model import PaletteFleet
from zzmojo.palette_commands import PaletteCommands
from zzmojo.palette_verbs import (
    PaletteVerb,
    PaletteGuiVerb,
    PaletteFilled,
    PaletteCompiled,
    palette_verbs,
    match_palette_verbs,
    compile_palette_verb,
    palette_quote,
    palette_command_words,
    palette_target_name,
    palette_go_to,
)
from zzmojo.palette_fuzzy import PaletteMatcher
from zzmojo.palette_gui_catalog import gui_verb
from palette_fleet_fixture import (
    read_fleet,
    read_place,
    write_text,
    write_optional,
)


def write_verb(mut bytes: List[UInt8], verb: PaletteVerb):
    append_text(bytes, verb.name)
    append_text(bytes, verb.keywords)
    append_text(bytes, verb.description)
    append_uint(bytes, UInt64(verb.kind))
    append_uint(bytes, UInt64(verb.run))
    append_text(bytes, verb.command)
    append_text(bytes, verb.body)
    append_uint(bytes, UInt64(len(verb.args)))
    for arg in verb.args:
        append_text(bytes, arg)
    append_uint(bytes, UInt64(len(verb.slots)))
    for slot in verb.slots:
        append_uint(bytes, UInt64(slot.kind))
        append_uint(bytes, UInt64(slot.level))
        append_text(bytes, slot.label)
    append_uint(bytes, UInt64(verb.raw))
    append_text(bytes, verb.spelled())
    append_uint(bytes, UInt64(Bool(verb.target())))
    if verb.target():
        append_uint(bytes, UInt64(verb.target().value()))


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var fleets = List[PaletteFleet]()
    var fleet_count = Int(read_uint(reader, 64))
    for _ in range(fleet_count):
        fleets.append(read_fleet(reader))
    var commands = PaletteCommands()
    var matcher = PaletteMatcher()
    var bytes = List[UInt8]()
    var cases = Int(read_uint(reader, 65536))
    for _ in range(cases):
        var tag = read_uint(reader, 5)
        var fleet_index = Int(read_uint(reader, UInt64(len(fleets) - 1)))
        var browser = read_bool(reader)
        var editor = read_bool(reader)
        var insert = read_bool(reader)
        var aliases = read_text(reader, 1024 * 1024)
        var gui = List[PaletteGuiVerb]()
        var gui_count = Int(read_uint(reader, 64))
        for _ in range(gui_count):
            var offer = gui_verb(read_text(reader))
            if offer:
                gui.append(offer.take())
        var query = read_text(reader, 1024 * 1024)
        var level: Optional[UInt8] = None
        if read_bool(reader):
            level = UInt8(read_uint(reader, 3))
        var index = Int(read_uint(reader, 65536))
        var filled = List[PaletteFilled]()
        var filled_count = Int(read_uint(reader, 4096))
        for _ in range(filled_count):
            var kind = UInt8(read_uint(reader, 1))
            var place = read_place(reader)
            var text = read_text(reader, 1024 * 1024)
            filled.append(PaletteFilled(kind, place^, text^))
        var place = read_place(reader)
        var verbs = palette_verbs(
            commands, browser, editor, insert, aliases, gui
        )
        if tag == 0:
            append_uint(bytes, UInt64(len(verbs)))
            for verb in verbs:
                write_verb(bytes, verb)
        elif tag == 1:
            var rows = match_palette_verbs(verbs, query, matcher, level)
            append_uint(bytes, UInt64(len(rows)))
            for row in rows:
                append_uint(bytes, UInt64(row.index))
                append_uint(
                    bytes,
                    UInt64(
                        row.score * 2 if row.score >= 0 else -row.score * 2 - 1
                    ),
                )
                append_uint(bytes, UInt64(len(row.hits)))
                for hit in row.hits:
                    append_uint(bytes, UInt64(hit.start))
                    append_uint(bytes, UInt64(hit.end))
        elif tag == 2:
            var compiled = compile_palette_verb(
                verbs[index], filled, fleets[fleet_index]
            ) if index < len(verbs) else Optional[PaletteCompiled](None)
            append_uint(bytes, UInt64(Bool(compiled)))
            if compiled:
                var result = compiled.take()
                append_uint(bytes, UInt64(Bool(result.host)))
                if result.host:
                    append_uint(bytes, UInt64(result.host.value()))
                append_text(bytes, result.line)
                append_text(bytes, result.display)
                append_uint(bytes, UInt64(Bool(result.invocation)))
                if result.invocation:
                    ref command = result.invocation.value()
                    append_text(bytes, command.name)
                    append_uint(bytes, UInt64(len(command.args)))
                    for arg in command.args:
                        append_text(bytes, String(from_utf8=Span(arg)))
        elif tag == 3:
            append_text(bytes, palette_quote(query))
        elif tag == 4:
            var words = palette_command_words(query)
            append_uint(bytes, UInt64(len(words)))
            for word in words:
                append_text(bytes, word)
        else:
            write_text(bytes, palette_target_name(fleets[fleet_index], place))
            write_text(bytes, palette_go_to(fleets[fleet_index], place))
    reader.finish()
    with open(args[2], "w") as target:
        target.write_all(Span(bytes))
