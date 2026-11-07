from std.math import floor, isfinite
from .which_key_sheet import WhichKeySheet


@fieldwise_init
struct WhichKeyBlock(Copyable, Movable):
    var group: UInt8
    var title: Bool
    var titled: Bool
    var rows: List[Int]

    def height(self) -> Float32:
        return Float32(len(self.rows)) * 22 + (Float32(24) if self.titled else Float32(0))


@fieldwise_init
struct WhichKeyStack(Copyable, Movable):
    var blocks: List[WhichKeyBlock]
    var height: Float32


@fieldwise_init
struct _Group(Copyable, Movable):
    var group: UInt8
    var rows: List[Int]


@fieldwise_init
struct WhichKeyFit(Copyable, Movable):
    var per_column: Int
    var per_line: Int
    var width: Float32
    var height: Float32
    var stacks: List[WhichKeyStack]


def _groups(sheet: WhichKeySheet) -> List[_Group]:
    var groups = List[_Group]()
    for index in range(len(sheet.rows)):
        var group = sheet.rows[index].group
        var found = -1
        for slot in range(len(groups)):
            if groups[slot].group == group:
                found = slot
                break
        if found >= 0:
            groups[found].rows.append(index)
        else:
            var indices: List[Int] = [index]
            groups.append(_Group(group, indices^))
    return groups^


@inline(.never)
def _stacks(groups: List[_Group], per_column: Int) -> List[WhichKeyStack]:
    var stacks = List[WhichKeyStack]()
    var open = Optional[Float32]()
    var per = max(per_column, 1)
    for group in groups:
        var titled = group.group != 0
        var whole = len(group.rows) <= per
        var extra = Float32(24) if titled else Float32(0)
        var height = Float32(len(group.rows)) * 22 + extra
        if whole and open and open.value() + 16 + height <= Float32(per) * 22 + extra and len(stacks) > 0:
            ref stack = stacks[len(stacks) - 1]
            stack.blocks.append(WhichKeyBlock(group.group, titled, titled, group.rows.copy()))
            stack.height = open.value() + 16 + height
            open = stack.height
            continue
        for start in range(0, len(group.rows), per):
            var rows = List[Int]()
            for index in range(start, min(start + per, len(group.rows))):
                rows.append(group.rows[index])
            var block = WhichKeyBlock(group.group, titled and start == 0, titled, rows^)
            var blocks: List[WhichKeyBlock] = [block.copy()]
            stacks.append(WhichKeyStack(blocks^, block.height()))
        open = Optional[Float32](height) if whole else None
    return stacks^


def _layout_height(stacks: List[WhichKeyStack], per_line: Int) -> Float32:
    var result: Float32 = 0
    for first in range(0, len(stacks), max(per_line, 1)):
        if first > 0:
            result += 16
        var maximum: Float32 = 0
        for index in range(first, min(first + max(per_line, 1), len(stacks))):
            maximum = max(maximum, stacks[index].height)
        result += maximum
    return result


@inline(.never)
def which_key_fit(sheet: WhichKeySheet, column_width: Float32, available_width: Float32, available_height: Float32) raises -> WhichKeyFit:
    if not isfinite(column_width) or column_width <= 0 or not isfinite(available_width) or not isfinite(available_height):
        raise Error("which-key fit geometry is not finite")
    var groups = _groups(sheet)
    var tallest = 1
    for group in groups:
        tallest = max(tallest, len(group.rows))
    var columns = floor((available_width - 24 - 4 + 16) / (column_width + 16))
    var per_line = max(Int(columns), 1) if isfinite(columns) and columns >= 1 else 1
    var budget = max(available_height - 24 - 22 - 8, Float32(0))
    var best_height = Float32.MAX
    var best_per = tallest
    var best_stacks = List[WhichKeyStack]()
    for per in range(tallest, 0, -1):
        var stacks = _stacks(groups, per)
        var height = _layout_height(stacks, per_line)
        if height <= budget or height < best_height:
            best_height = height
            best_per = per
            best_stacks = stacks^
            if height <= budget:
                break
    var count = max(min(len(best_stacks), per_line), 1)
    var width = Float32(count) * column_width + Float32(count - 1) * 16 + 24 + 4
    return WhichKeyFit(best_per, per_line, width, best_height, best_stacks^)
