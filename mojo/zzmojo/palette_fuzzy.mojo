from .palette_unicode import lowercase_table, whitespace_table


comptime PALETTE_QUERY_BYTES = 1024 * 1024
comptime PALETTE_LABEL_BYTES = 4 * 1024 * 1024


@fieldwise_init
struct FuzzyRange(TrivialRegisterPassable):
    var start: Int
    var end: Int


@fieldwise_init
struct FuzzyMatch(Copyable, Movable):
    var score: Int
    var hits: List[FuzzyRange]


struct PaletteMatcher(Movable):
    var lowercase: List[UInt32]
    var whitespace: List[UInt32]

    def __init__(out self):
        self.lowercase = lowercase_table()
        self.whitespace = whitespace_table()

    def _lower_index(self, scalar: UInt32) -> Int:
        var lo = 0
        var hi = len(self.lowercase) // 4
        while lo < hi:
            var middle = lo + (hi - lo) // 2
            var value = self.lowercase[middle * 4]
            if value < scalar:
                lo = middle + 1
            elif value > scalar:
                hi = middle
            else:
                return middle * 4
        return -1

    def _space(self, scalar: UInt32) -> Bool:
        var lo = 0
        var hi = len(self.whitespace)
        while lo < hi:
            var middle = lo + (hi - lo) // 2
            if self.whitespace[middle] < scalar:
                lo = middle + 1
            elif self.whitespace[middle] > scalar:
                hi = middle
            else:
                return True
        return False

    def _fold(self, scalar: UInt32, mut output: List[UInt32]):
        var index = self._lower_index(scalar)
        if index < 0:
            output.append(scalar)
            return
        for offset in range(1, 4):
            var value = self.lowercase[index + offset]
            if value == 0:
                break
            output.append(value)

    def _matches(self, scalar: UInt32, needle: UInt32) -> Bool:
        var index = self._lower_index(scalar)
        if index < 0:
            return scalar == needle
        for offset in range(1, 4):
            var value = self.lowercase[index + offset]
            if value == 0:
                break
            if value == needle:
                return True
        return False

    def find(self, query: String, label: String) raises -> Optional[FuzzyMatch]:
        if query.byte_length() > PALETTE_QUERY_BYTES or label.byte_length() > PALETTE_LABEL_BYTES:
            raise Error("palette match exceeds its text limit")
        var needle = List[UInt32]()
        for character in query.codepoints():
            var scalar = UInt32(Int(character))
            if not self._space(scalar):
                self._fold(scalar, needle)
        var hits = List[FuzzyRange]()
        if len(needle) == 0:
            return FuzzyMatch(0, hits^)
        var first = -1
        var previous = 0
        var gaps = 0
        var position = 0
        var byte = 0
        var current = 0
        for character in label.codepoints():
            var scalar = UInt32(Int(character))
            var width = 1 if scalar < 128 else 2 if scalar < 2048 else 3 if scalar < 65536 else 4
            if self._matches(scalar, needle[current]):
                current += 1
                if first < 0:
                    first = position
                else:
                    gaps += position - previous - 1
                previous = position
                hits.append(FuzzyRange(byte, byte + width))
                if current == len(needle):
                    return FuzzyMatch(first + 2 * gaps, hits^)
            position += 1
            byte += width
        return None
