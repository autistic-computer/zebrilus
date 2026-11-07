from std.memory import ArcPointer
from .path_protocol import PathEntry
from .path_listing import PathChunk, PathListing
from .path_rank import PathLabels, PathRanker, PathRankItem, PathHit
from .path_rank_rules import path_browse_order


@fieldwise_init
struct PathRow(Copyable, Movable):
    var entry: PathEntry
    var name: String
    var matches: List[PathHit]
    var mark: Optional[UInt8]


@fieldwise_init
struct _PathLocator(TrivialRegisterPassable):
    var chunk: UInt32
    var index: UInt32
    var mark: Int8


struct _PathRowLabels(Movable, PathLabels):
    var chunks: List[ArcPointer[PathChunk]]
    var locators: List[_PathLocator]
    var base: Int

    def __init__(
        out self, listing: PathListing, base: String, browse: Bool
    ) raises:
        self.chunks = listing.chunks.copy()
        var count = 0
        for chunk in self.chunks:
            count += len(chunk[].entries)
            if count > 262144:
                raise Error("path rows exceed their candidate limit")
        self.locators = List[_PathLocator](capacity=count)
        self.base = base.byte_length()
        for chunk in range(len(self.chunks)):
            for index in range(len(self.chunks[chunk][].entries)):
                ref entry = self.chunks[chunk][].entries[index]
                if (
                    entry.rel.byte_length() <= self.base
                    or not entry.rel.startswith(base)
                ):
                    continue
                if browse:
                    var label = String(
                        from_utf8_lossy=entry.rel.as_bytes()[self.base :]
                    ).rstrip("/")
                    if "/" in label:
                        continue
                if len(self.locators) == 262144:
                    raise Error("path rows exceed their candidate limit")
                var mark = listing.marks.get(entry.rel)
                self.locators.append(
                    _PathLocator(
                        UInt32(chunk),
                        UInt32(index),
                        Int8(mark.value()) if mark else -1,
                    )
                )

    def count(self) -> Int:
        return len(self.locators)

    def label(self, index: Int) -> String:
        var locator = self.locators[index]
        ref entry = self.chunks[Int(locator.chunk)][].entries[
            Int(locator.index)
        ]
        var text = String(from_utf8_lossy=entry.rel.as_bytes()[self.base :])
        return String(text.rstrip("/"))

    def changed(self, index: Int) -> Bool:
        return self.locators[index].mark >= 0

    def browse_order(self, left: Int, right: Int) -> Int:
        var a = self.locators[left]
        var b = self.locators[right]
        ref left_entry = self.chunks[Int(a.chunk)][].entries[Int(a.index)]
        ref right_entry = self.chunks[Int(b.chunk)][].entries[Int(b.index)]
        return path_browse_order(
            left_entry.rel,
            left_entry.kind == 1,
            right_entry.rel,
            right_entry.kind == 1,
        )

    def row(self, index: Int, var hits: List[PathHit]) -> PathRow:
        var locator = self.locators[index]
        var entry = (
            self.chunks[Int(locator.chunk)][].entries[Int(locator.index)].copy()
        )
        var mark: Optional[UInt8] = (
            UInt8(locator.mark) if locator.mark >= 0 else None
        )
        return PathRow(entry^, self.label(index), hits^, mark)


@inline(.never)
def path_rows(
    mut ranker: PathRanker, listing: PathListing, base: String, filter: String
) raises -> List[PathRow]:
    var source = _PathRowLabels(listing, base, filter.byte_length() == 0)
    var kept = List[PathRankItem]()
    if filter.byte_length() == 0:
        for index in range(source.count()):
            var low = 0
            var high = len(kept)
            while low < high:
                var middle = low + (high - low) // 2
                if source.browse_order(index, kept[middle].index) < 0:
                    high = middle
                else:
                    low = middle + 1
            if low < 200:
                kept.insert(low, PathRankItem(index, 0, 0))
                if len(kept) > 200:
                    _ = kept.pop()
    else:
        kept = ranker.rank(filter, source)
    var rows = List[PathRow](capacity=len(kept))
    for item in kept:
        var hits = (
            ranker.highlights(
                filter, source.label(item.index)
            ) if filter.byte_length()
            > 0 else List[PathHit]()
        )
        rows.append(source.row(item.index, hits^))
    return rows^
