from .path_pattern import PathPattern, PathWindow
from .palette_text import scalar_width
from .path_cpu import path_vector_bytes


comptime PathLanes = SIMD[.int32, 64]


@fieldwise_init
struct PathMatch(Movable):
    var accepted: Bool
    var score: UInt16
    var exact: Bool
    var indices: List[UInt32]


def _upper(value: UInt8) -> Bool:
    return value >= 65 and value <= 90


def _lower(value: UInt8) -> Bool:
    return value >= 97 and value <= 122


def _delimiter(value: UInt8) -> Bool:
    return value < 128 and not (
        _upper(value) or _lower(value) or value >= 48 and value <= 57
    )


def _flipped(value: UInt8, sensitive: Bool) -> UInt8:
    return (
        value ^ 32 if not sensitive
        and (_upper(value) or _lower(value)) else value
    )


@inline(.never)
def _shift(
    values: PathLanes, adjacent: PathLanes, shift: Int, lanes: Int
) -> PathLanes:
    var result = PathLanes(0)
    for index in range(lanes):
        result[index] = (
            values[index - shift] if index
            >= shift else adjacent[lanes + index - shift]
        )
    return result


@inline(.never)
def _ascii_gaps(
    var row: PathLanes,
    adjacent: PathLanes,
    mask: PathLanes,
    adjacent_mask: PathLanes,
    lanes: Int,
) -> PathLanes:
    var shift = 1
    while shift < lanes:
        var shifted = _shift(row, adjacent, shift, lanes)
        var matched = _shift(mask, adjacent_mask, shift, lanes)
        row = max(
            row,
            max(
                shifted - (PathLanes(Int32(shift)) + (matched & PathLanes(4))),
                PathLanes(0),
            ),
        )
        shift *= 2
    return row


@inline(.never)
def _unicode_gaps(
    var row: PathLanes,
    adjacent: PathLanes,
    var pending: PathLanes,
    adjacent_pending: PathLanes,
    var continuation: PathLanes,
    var adjacent_continuation: PathLanes,
    var scalar: PathLanes,
    var adjacent_scalar: PathLanes,
    lanes: Int,
    maximum: Int32,
) -> Tuple[PathLanes, PathLanes]:
    var shift = 1
    while shift < lanes:
        var shifted = _shift(row, adjacent, shift, lanes)
        var opened = _shift(pending, adjacent_pending, shift, lanes)
        var penalty = max(
            PathLanes(Int32(shift)) - continuation, PathLanes(0)
        ) + (opened & scalar & PathLanes(4))
        row = max(row, max(shifted - penalty, PathLanes(0)))
        pending = max(pending, max(opened - scalar, PathLanes(0)))
        if shift * 2 < lanes:
            continuation = min(
                continuation
                + _shift(continuation, adjacent_continuation, shift, lanes),
                PathLanes(maximum),
            )
            adjacent_continuation = min(
                adjacent_continuation
                + _shift(adjacent_continuation, PathLanes(0), shift, lanes),
                PathLanes(maximum),
            )
            scalar = max(scalar, _shift(scalar, adjacent_scalar, shift, lanes))
            adjacent_scalar = max(
                adjacent_scalar,
                _shift(adjacent_scalar, PathLanes(0), shift, lanes),
            )
        shift *= 2
    return (row, pending)


@inline(.never)
def _greedy(
    pattern: PathPattern, bytes: Span[UInt8, _], origin: Int
) -> PathMatch:
    var indices = List[UInt32](capacity=len(pattern.bytes))
    var score = 0
    var position = 0
    var delimiter_enabled = False
    var previous_lower = False
    var previous_delimiter = False
    for needle in range(len(pattern.bytes)):
        var first = position
        var found = False
        while position <= len(bytes) - len(pattern.bytes) + needle:
            var value = bytes[position]
            var is_delimiter = _delimiter(value)
            if not is_delimiter:
                delimiter_enabled = True
            if value != pattern.bytes[needle] and value != _flipped(
                pattern.bytes[needle], pattern.sensitive
            ):
                previous_delimiter = delimiter_enabled and is_delimiter
                previous_lower = _lower(value)
                position += 1
                continue
            score = min(score + 12, 65535)
            if position != first and needle != 0:
                score = max(
                    score
                    - min(5 + min(max(position - first - 1, 0), 65535), 65535),
                    0,
                )
            score = min(
                score + (4 if pattern.bytes[needle] == value else 0), 65535
            )
            score = min(
                score + (4 if _upper(value) and previous_lower else 0), 65535
            )
            score = min(
                score + (12 if origin == 0 and position == 0 else 0), 65535
            )
            score = min(
                score + (4 if previous_delimiter and not is_delimiter else 0),
                65535,
            )
            previous_delimiter = delimiter_enabled and is_delimiter
            previous_lower = _lower(value)
            indices.append(UInt32(position + origin))
            position += 1
            found = True
            break
        if not found:
            return PathMatch(True, 0, False, List[UInt32]())
    indices.reverse()
    return PathMatch(True, UInt16(score), False, indices^)


struct PathScorer(Movable):
    var scores: List[UInt16]
    var masks: List[UInt8]
    var vector_bytes: Int

    def __init__(out self, var vector_bytes: Int = 0) raises:
        if vector_bytes == 0:
            vector_bytes = path_vector_bytes()
        if vector_bytes != 16 and vector_bytes != 32 and vector_bytes != 64:
            raise Error("unsupported path score vector size")
        self.scores = List[UInt16]()
        self.masks = List[UInt8]()
        self.vector_bytes = vector_bytes

    def retained_bytes(self) -> Int:
        return self.scores.capacity() * 2 + self.masks.capacity()

    def _load(
        self, row: Int, column: Int, stride: Int, lanes: Int
    ) -> PathLanes:
        var result = PathLanes(0)
        var offset = row * stride + column
        for lane in range(lanes):
            result[lane] = Int32(self.scores[offset + lane])
        return result

    def _mask(
        self, row: Int, column: Int, stride: Int, lanes: Int, maximum: Int32
    ) -> PathLanes:
        var result = PathLanes(0)
        var offset = row * stride + column
        for lane in range(lanes):
            result[lane] = maximum if self.masks[offset + lane] != 0 else 0
        return result

    @inline(.never)
    def _trace(
        self,
        pattern: PathPattern,
        bytes: Span[UInt8, _],
        origin: Int,
        count: Int,
        stride: Int,
        lanes: Int,
        var score: Int,
    ) -> List[UInt32]:
        var column = lanes
        while (
            column < stride
            and Int(self.scores[count * stride + column]) != score
        ):
            column += 1
        var row = count
        var typos = 0
        var previous = -1
        var indices = List[UInt32](capacity=count)
        while (
            row > 0
            and typos <= pattern.typos
            and column >= lanes
            and score != 0
        ):
            var position = column - lanes
            if (
                pattern.unicode
                and position < len(bytes)
                and bytes[position] & 0xC0 == 0x80
            ):
                column -= 1
                score = Int(self.scores[row * stride + column])
                continue
            if self.masks[row * stride + column] != 0:
                if not pattern.unicode:
                    indices.append(UInt32(position + origin))
                elif previous != position:
                    var width = scalar_width(pattern.units[row - 1])
                    for offset in range(width):
                        indices.append(
                            UInt32(position + origin + width - offset - 1)
                        )
                    previous = position
                row -= 1
                column -= 1
                score = Int(self.scores[row * stride + column])
                continue
            var diagonal = Int(self.scores[(row - 1) * stride + column - 1])
            var left = Int(self.scores[row * stride + column - 1])
            var up = Int(self.scores[(row - 1) * stride + column])
            if diagonal >= left and diagonal >= up:
                row -= 1
                column -= 1
                typos += 1
                score = diagonal
            elif left >= up:
                column -= 1
                score = left
            else:
                row -= 1
                typos += 1
                score = up
        return indices^

    @inline(.never)
    def score(
        mut self, pattern: PathPattern, label: String, window: PathWindow
    ) raises -> PathMatch:
        if not window.accepted:
            return PathMatch(False, 0, False, List[UInt32]())
        if (
            window.start < 0
            or window.end < window.start
            or window.end > label.byte_length()
        ):
            raise Error("path score window exceeds its label")
        if len(pattern.units) == 0:
            return PathMatch(True, 0, False, List[UInt32]())
        var origin = max(window.start - 1, 0)
        var bytes = label.as_bytes()[origin : window.end]
        if len(bytes) > 1024:
            var result = _greedy(pattern, bytes, origin)
            result.exact = (
                origin == 0
                and window.end == label.byte_length()
                and pattern.bytes == List[UInt8](bytes)
            )
            if result.exact:
                result.score = UInt16(min(Int(result.score) + 8, 65535))
            return result^
        var count = len(pattern.units)
        if count > len(bytes) + 6:
            raise Error("path score window cannot contain its accepted pattern")
        var small = len(pattern.bytes) * 18 + 20 <= 255
        var lanes = self.vector_bytes if small else self.vector_bytes // 2
        var maximum: Int32 = 255 if small else 65535
        var stride = ((len(bytes) + lanes - 1) // lanes + 1) * lanes
        var size = (count + 1) * stride
        if self.scores.capacity() < size:
            self.scores = List[UInt16](capacity=size)
        if self.masks.capacity() < size:
            self.masks = List[UInt8](capacity=size)
        self.scores.clear()
        self.scores.resize(size, 0)
        self.masks.clear()
        self.masks.resize(size, 0)
        var scalar_values = List[UInt32](capacity=label.byte_length())
        if pattern.unicode:
            scalar_values.resize(label.byte_length(), 0)
            var offset = 0
            for character in label.codepoints():
                var scalar = UInt32(Int(character))
                scalar_values[offset] = scalar
                offset += scalar_width(scalar)
        var pending = List[PathLanes]()
        pending.resize(count + 1, PathLanes(0))
        var previous_continuation = PathLanes(0)
        var previous_scalar = PathLanes(0)
        var best = 0
        for column in range(lanes, stride, lanes):
            var start = column - lanes
            var bonuses = PathLanes(18)
            var continuation = PathLanes(0)
            var scalar = PathLanes(0)
            for lane in range(lanes):
                var position = start + lane
                var value = bytes[position] if position < len(bytes) else UInt8(
                    0
                )
                var prior = bytes[
                    position - 1
                ] if position > 0 and position <= len(bytes) else UInt8(0)
                var is_delimiter = _delimiter(value)
                var prior_delimiter = position > 0 and _delimiter(prior)
                var capital = _upper(value) and position > 0 and _lower(prior)
                bonuses[lane] += Int32(
                    (4 if prior_delimiter and not is_delimiter else 0)
                    + (4 if capital else 0)
                    + (12 if origin == 0 and position == 0 else 0)
                )
                if position < len(bytes):
                    continuation[lane] = 1 if value & 0xC0 == 0x80 else 0
                    scalar[lane] = maximum if value & 0xC0 != 0x80 else 0
            var previous_row = PathLanes(0)
            var up_mask = PathLanes(0)
            for row in range(1, count + 1):
                var mask = PathLanes(0)
                var exact = PathLanes(0)
                for lane in range(lanes):
                    var position = start + lane
                    var value = bytes[position] if position < len(
                        bytes
                    ) else UInt8(0)
                    if pattern.unicode:
                        if scalar[lane] != 0:
                            var character = scalar_values[origin + position]
                            exact[lane] = (
                                maximum if character
                                == pattern.units[row - 1] else 0
                            )
                            mask[lane] = maximum if pattern.matches(
                                row - 1, character
                            ) else 0
                    else:
                        exact[lane] = (
                            maximum if value == pattern.bytes[row - 1] else 0
                        )
                        mask[lane] = (
                            maximum if value == pattern.bytes[row - 1]
                            or value
                            == _flipped(
                                pattern.bytes[row - 1], pattern.sensitive
                            ) else 0
                        )
                var adjacent = self._load(row, column - lanes, stride, lanes)
                var diagonal = _shift(
                    previous_row,
                    self._load(row - 1, column - lanes, stride, lanes),
                    1,
                    lanes,
                )
                diagonal = min(diagonal + (mask & bonuses), PathLanes(maximum))
                diagonal = max(diagonal - PathLanes(6), PathLanes(0))
                diagonal = min(
                    diagonal + (exact & PathLanes(4)), PathLanes(maximum)
                )
                var up = max(
                    max(previous_row - PathLanes(1), PathLanes(0))
                    - (up_mask & PathLanes(4)),
                    PathLanes(0),
                )
                if pattern.unicode:
                    diagonal &= scalar
                    up &= scalar
                var scores = max(diagonal, up)
                if pattern.unicode:
                    var propagated = _unicode_gaps(
                        scores,
                        adjacent,
                        mask,
                        pending[row],
                        continuation,
                        previous_continuation,
                        scalar,
                        previous_scalar,
                        lanes,
                        maximum,
                    )
                    scores = propagated[0]
                    pending[row] = propagated[1]
                else:
                    scores = _ascii_gaps(
                        scores,
                        adjacent,
                        mask,
                        self._mask(row, column - lanes, stride, lanes, maximum),
                        lanes,
                    )
                var offset = row * stride + column
                for lane in range(lanes):
                    self.scores[offset + lane] = UInt16(scores[lane])
                    self.masks[offset + lane] = UInt8(mask[lane] != 0)
                    if row == count:
                        best = max(best, Int(scores[lane]))
                previous_row = scores
                up_mask = mask
            previous_continuation = continuation
            previous_scalar = scalar
        var indices = (
            self._trace(
                pattern, bytes, origin, count, stride, lanes, best
            ) if best
            != 0 else List[UInt32]()
        )
        var exact = (
            origin == 0
            and window.end == label.byte_length()
            and pattern.bytes == List[UInt8](bytes)
        )
        return PathMatch(
            True,
            UInt16(min(best + (8 if exact else 0), 65535)),
            exact,
            indices^,
        )
