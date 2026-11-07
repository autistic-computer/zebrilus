from std.sys import argv
from std.memory import ArcPointer
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool
from zzmojo.palette_files import (
    PaletteFiles,
    FILES_INSERT,
    FILES_COPY_CLOSE,
    FILES_COPIED,
)
from zzmojo.path_listing import PathListing, PathChunk
from zzmojo.path_protocol import PathEntry, PathRoot
from zzmojo.path_rows import PathRow
from zzmojo.path_rank import PathHit
from zzmojo.path_text import FilesPath


def optional(mut output: List[UInt8], value: Optional[String]):
    output.append(UInt8(Bool(value)))
    if value:
        append_text(output, value.value())


def main() raises:
    var args = argv()
    var input: List[UInt8]
    with open(args[1], "r") as file:
        input = file.read_bytes()
    var reader = Decoder(input^)
    reader.offset = 0
    var count = Int(reader.uint())
    var output = List[UInt8]()
    for _ in range(count):
        var files = PaletteFiles()
        files.active = True
        files.host_name = "remote λ"
        files.path = FilesPath(reader.text(), reader.text(), reader.text())
        var listing = PathListing(files.host_name, reader.text())
        var entries = List[PathEntry]()
        var rows = Int(reader.uint())
        for _ in range(rows):
            var rel = reader.text()
            var name = reader.text()
            var kind = UInt8(1 if read_bool(reader) else 0)
            var symlink = read_bool(reader)
            var entry = PathEntry(rel^, kind, symlink)
            entries.append(entry.copy())
            files.ranking.rows.append(
                PathRow(entry^, name^, List[PathHit](), None)
            )
        if rows > 0:
            listing.chunks.append(ArcPointer(PathChunk(entries^)))
        files.ranking.selected = Int(reader.uint())
        if read_bool(reader):
            listing.root = PathRoot(reader.text(), reader.text(), "/root", 0)
        listing.done = True
        files.service.listing = listing^
        files.here = read_bool(reader)
        var inserts = read_bool(reader)
        files.service.insert_pane = Optional[UInt64](9) if inserts else None
        var key = UInt8(reader.uint())
        optional(output, files.open_directory())
        optional(output, files.absolute_directory())
        optional(output, files.picked_folder())
        optional(output, files.copy_text())
        append_text(output, files.empty_message())
        output.append(UInt8(inserts))
        var hints = files.hints(inserts)
        append_uint(output, UInt64(len(hints)))
        for hint in hints:
            append_text(output, hint.key)
            append_text(output, hint.label)
        var effect = files.key(key, inserts)
        append_uint(output, UInt64(effect.kind))
        append_text(
            output, String() if effect.kind == FILES_COPY_CLOSE else effect.text
        )
        append_uint(output, UInt64(files.ranking.selected))
        optional(output, files.ranking.pending)
        append_text(
            output,
            effect.text if effect.kind == FILES_COPY_CLOSE
            or effect.kind == FILES_COPIED else String(),
        )
        append_uint(output, UInt64(1 if effect.kind == FILES_INSERT else 0))
        if effect.kind == FILES_INSERT:
            append_uint(output, 2)
            append_uint(output, effect.pane.value())
            append_text(output, effect.root.value().root)
            append_text(output, effect.root.value().display_root)
            optional(output, effect.root.value().cwd)
            append_text(output, effect.entry.value().rel)
            output.append(UInt8(effect.entry.value().kind == 1))
            output.append(UInt8(effect.entry.value().symlink))
    reader.finish()
    with open(args[2], "w") as file:
        file.write_all(Span(output))
    print("native file-level action vectors passed")
