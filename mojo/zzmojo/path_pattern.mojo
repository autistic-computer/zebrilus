from .path_case import PathCase, path_respects_case
from .palette_text import scalar_width


comptime MAX_PATH_QUERY: Int = 64 * 1024
comptime MAX_PATH_LABEL: Int = 4096


@fieldwise_init
struct PathWindow(TrivialRegisterPassable):
    var accepted: Bool
    var start: Int
    var end: Int


struct PathPattern(Movable):
    var bytes: List[UInt8]
    var units: List[UInt32]
    var flipped: List[UInt32]
    var sensitive: Bool
    var unicode: Bool
    var typos: Int

    def __init__(out self, query: String, characters: PathCase) raises:
        if query.byte_length() > MAX_PATH_QUERY:
            raise Error("path query exceeds its byte limit")
        self.bytes = List[UInt8](query.as_bytes())
        self.units = List[UInt32]()
        self.flipped = List[UInt32]()
        self.sensitive = path_respects_case(query, characters)
        self.unicode = False
        for character in query.codepoints():
            var scalar = UInt32(Int(character))
            self.unicode = self.unicode or scalar >= 128
            self.units.append(scalar)
            self.flipped.append(
                scalar if self.sensitive else characters.flipped(scalar)
            )
        self.typos = min(len(self.units) // 4, 6)

    def matches(self, index: Int, scalar: UInt32) -> Bool:
        return self.units[index] == scalar or self.flipped[index] == scalar

    def _end(
        self, values: Span[UInt32, _], ends: Span[Int, _], size: Int
    ) -> Int:
        var first = max(len(self.units) - 1 - self.typos, 0)
        for offset in range(len(values)):
            var index = len(values) - offset - 1
            for needle in range(first, len(self.units)):
                if self.matches(needle, values[index]):
                    return ends[index]
        return size

    def filter(self, label: String) raises -> PathWindow:
        var size = label.byte_length()
        if size > MAX_PATH_LABEL:
            raise Error("path label exceeds its byte limit")
        var count = len(self.units)
        if count == 0:
            return PathWindow(True, 0, size)
        if size < max(count - self.typos, 0) or size == 0:
            return PathWindow(False, 0, size)
        var values = List[UInt32](capacity=size)
        var starts = List[Int](capacity=size)
        var ends = List[Int](capacity=size)
        if self.unicode:
            var position = 0
            for character in label.codepoints():
                var scalar = UInt32(Int(character))
                values.append(scalar)
                starts.append(position)
                position += scalar_width(scalar)
                ends.append(position)
        else:
            for index in range(size):
                values.append(UInt32(label.as_bytes()[index]))
                starts.append(index)
                ends.append(index + 1)
        var states = SIMD[.int32, 8](0)
        for path in range(1, self.typos + 1):
            states[path] = Int32(path)
        var earliest = -1
        for index in range(len(values)):
            for path in range(self.typos + 1):
                if self.matches(Int(states[path]), values[index]):
                    if earliest < 0:
                        earliest = starts[index]
                    states[path] += 1
                    if Int(states[path]) == count:
                        return PathWindow(
                            True,
                            earliest,
                            self._end(Span(values), Span(ends), size),
                        )
            for path in range(1, self.typos + 1):
                states[path] = max(states[path], states[path - 1] + 1)
                if Int(states[path]) == count:
                    return PathWindow(
                        True,
                        earliest,
                        self._end(Span(values), Span(ends), size),
                    )
        return PathWindow(False, max(earliest, 0), size)
