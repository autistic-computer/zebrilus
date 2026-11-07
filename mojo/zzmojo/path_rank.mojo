from .path_case import PathCase
from .path_pattern import PathPattern
from .path_score import PathScorer
from .path_rank_rules import path_prior


@fieldwise_init
struct PathHit(TrivialRegisterPassable):
    var start: Int
    var end: Int


trait PathLabels:
    def count(self) -> Int:
        ...

    def label(self, index: Int) -> String:
        ...

    def changed(self, index: Int) -> Bool:
        ...


@fieldwise_init
struct PathRankItem(TrivialRegisterPassable):
    var index: Int
    var score: UInt16
    var prior: UInt32


def _less[
    L: PathLabels
](source: L, left: PathRankItem, right: PathRankItem) -> Bool:
    if left.score != right.score:
        return left.score > right.score
    if left.prior != right.prior:
        return left.prior > right.prior
    return source.label(left.index) < source.label(right.index)


def _swap(mut items: List[PathRankItem], left: Int, right: Int):
    var value = items[left]
    items[left] = items[right]
    items[right] = value


def _median[
    L: PathLabels
](
    source: L, items: List[PathRankItem], left: Int, middle: Int, right: Int
) -> Int:
    var x = _less(source, items[left], items[middle])
    var y = _less(source, items[left], items[right])
    if x == y:
        return (
            right if _less(source, items[middle], items[right]) != x else middle
        )
    return left


@inline(.never)
def _median_recursive[
    L: PathLabels
](
    source: L,
    items: List[PathRankItem],
    var left: Int,
    var middle: Int,
    var right: Int,
    length: Int,
) -> Int:
    if length * 8 >= 64:
        var eighth = length // 8
        left = _median_recursive(
            source, items, left, left + eighth * 4, left + eighth * 7, eighth
        )
        middle = _median_recursive(
            source,
            items,
            middle,
            middle + eighth * 4,
            middle + eighth * 7,
            eighth,
        )
        right = _median_recursive(
            source, items, right, right + eighth * 4, right + eighth * 7, eighth
        )
    return _median(source, items, left, middle, right)


@inline(.never)
def _partition[
    L: PathLabels
](
    source: L,
    mut items: List[PathRankItem],
    begin: Int,
    end: Int,
    pivot_index: Int,
    equal: Bool,
) -> Int:
    _swap(items, begin, pivot_index)
    var pivot = items[begin]
    var first = begin + 1
    if first == end:
        return 0
    var saved = items[first]
    var gap = first
    var count = 0
    for right in range(first + 1, end + 1):
        var value = items[right] if right < end else saved
        var less = not _less(source, pivot, value) if equal else _less(
            source, value, pivot
        )
        var left = first + count
        items[gap] = items[left]
        items[left] = value
        gap = right
        count += Int(less)
    _swap(items, begin, begin + count)
    return count


@inline(.never)
def _insertion[
    L: PathLabels
](source: L, mut items: List[PathRankItem], begin: Int, end: Int):
    for index in range(begin + 1, end):
        var value = items[index]
        var place = index
        while place > begin and _less(source, value, items[place - 1]):
            items[place] = items[place - 1]
            place -= 1
        items[place] = value


@inline(.never)
def _heap[
    L: PathLabels
](source: L, mut items: List[PathRankItem], begin: Int, end: Int):
    var length = end - begin
    for offset in range(length + length // 2):
        var index = length + length // 2 - offset - 1
        var node = index - length if index >= length else 0
        if index < length:
            _swap(items, begin, begin + index)
        var size = min(index, length)
        while True:
            var child = 2 * node + 1
            if child >= size:
                break
            if child + 1 < size and _less(
                source, items[begin + child], items[begin + child + 1]
            ):
                child += 1
            if not _less(source, items[begin + node], items[begin + child]):
                break
            _swap(items, begin + node, begin + child)
            node = child


@inline(.never)
def _quicksort[
    L: PathLabels
](
    source: L,
    mut items: List[PathRankItem],
    var begin: Int,
    end: Int,
    var ancestor: Optional[PathRankItem],
    var limit: Int,
):
    while True:
        var length = end - begin
        if length <= 32:
            _insertion(source, items, begin, end)
            return
        if limit == 0:
            _heap(source, items, begin, end)
            return
        limit -= 1
        var eighth = length // 8
        var pivot = _median(
            source, items, begin, begin + eighth * 4, begin + eighth * 7
        ) if length < 64 else _median_recursive(
            source, items, begin, begin + eighth * 4, begin + eighth * 7, eighth
        )
        if ancestor and not _less(source, ancestor.value(), items[pivot]):
            begin += _partition(source, items, begin, end, pivot, True) + 1
            ancestor = None
            continue
        var count = _partition(source, items, begin, end, pivot, False)
        var position = begin + count
        var value = items[position]
        _quicksort(source, items, begin, position, ancestor, limit)
        begin = position + 1
        ancestor = value


@inline(.never)
def path_sort[L: PathLabels](source: L, mut items: List[PathRankItem]):
    var length = len(items)
    if length <= 20:
        _insertion(source, items, 0, length)
        return
    var reversed = _less(source, items[1], items[0])
    var run = 2
    while run < length:
        if _less(source, items[run], items[run - 1]) != reversed:
            break
        run += 1
    if run == length:
        if reversed:
            items.reverse()
        return
    var limit = 0
    var remaining = length | 1
    while remaining > 1:
        limit += 2
        remaining >>= 1
    _quicksort(source, items, 0, length, None, limit)


@inline(.never)
def path_byte_ranges(label: String, var indices: List[UInt32]) -> List[PathHit]:
    sort(Span(indices))
    var ranges = List[PathHit]()
    for value in indices:
        var start = Int(value)
        if start >= label.byte_length():
            continue
        while start > 0 and label.as_bytes()[start] & 0xC0 == 0x80:
            start -= 1
        var end = start + 1
        while (
            end < label.byte_length() and label.as_bytes()[end] & 0xC0 == 0x80
        ):
            end += 1
        if len(ranges) > 0 and ranges[len(ranges) - 1].end >= start:
            ranges[len(ranges) - 1].end = max(ranges[len(ranges) - 1].end, end)
        else:
            ranges.append(PathHit(start, end))
    return ranges^


struct PathRanker(Movable):
    var characters: PathCase
    var scorer: PathScorer

    def __init__(out self, vector_bytes: Int = 0) raises:
        self.characters = PathCase()
        self.scorer = PathScorer(vector_bytes)

    @inline(.never)
    def rank[
        L: PathLabels
    ](mut self, query: String, source: L, limit: Int = 200) raises -> List[
        PathRankItem
    ]:
        if source.count() > 262144:
            raise Error("path ranking exceeds its candidate limit")
        var pattern = PathPattern(query, self.characters)
        var ranked = List[PathRankItem](capacity=source.count())
        for index in range(source.count()):
            var label = source.label(index)
            var window = pattern.filter(label)
            if window.accepted:
                var matched = self.scorer.score(pattern, label, window)
                ranked.append(
                    PathRankItem(
                        index,
                        matched.score,
                        path_prior(
                            query, label, source.changed(index), self.characters
                        ),
                    )
                )
        path_sort(source, ranked)
        if len(ranked) > max(limit, 0):
            ranked.resize(max(limit, 0), PathRankItem(0, 0, 0))
        return ranked^

    def highlights(
        mut self, query: String, label: String
    ) raises -> List[PathHit]:
        var pattern = PathPattern(query, self.characters)
        var window = pattern.filter(label)
        var matched = self.scorer.score(pattern, label, window)
        var indices = List[UInt32]()
        swap(indices, matched.indices)
        return path_byte_ranges(label, indices^)
