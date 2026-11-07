from std.math import floor
from .palette_files import PaletteFiles
from .palette_presentation import PaletteRow, PaletteHint
from .palette_fuzzy import FuzzyRange
from .path_listing import path_mark_letter


def palette_file_rows(files: PaletteFiles) raises -> List[PaletteRow]:
    var values = List[PaletteRow](capacity=len(files.ranking.rows))
    for index in range(len(files.ranking.rows)):
        ref source = files.ranking.rows[index]
        var row = PaletteRow()
        row.action = 7
        row.index = index
        row.label = source.name.copy()
        var kind = String("dir ›" if source.entry.kind == 1 else "file")
        row.right = (
            String(
                path_mark_letter(source.mark.value()), " ", kind
            ) if source.mark else kind
            ^
        )
        row.icon = String("Folder" if source.entry.kind == 1 else "File")
        for hit in source.matches:
            row.hits.append(FuzzyRange(hit.start, hit.end))
        values.append(row^)
    return values^


def palette_file_hints(files: PaletteFiles, inserts: Bool) -> List[PaletteHint]:
    var values = List[PaletteHint]()
    for hint in files.hints(inserts):
        values.append(PaletteHint(hint.key, hint.label))
    return values^


def palette_file_height(height: Float32, rows: Int) -> Float32:
    var room = (height * 0.88 - 140).clamp(40, 440)
    var visible = max(1, Int(floor(room / 28)))
    return Float32(min(rows, visible)) * 28
