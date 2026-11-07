from .path_case_data import path_case_table


struct PathCase(Movable):
    var values: List[UInt32]

    def __init__(out self):
        self.values = path_case_table()

    def _index(self, scalar: UInt32) -> Int:
        var left = 0
        var right = len(self.values) // 4
        while left < right:
            var middle = left + (right - left) // 2
            var value = self.values[middle * 4]
            if value < scalar:
                left = middle + 1
            elif value > scalar:
                right = middle
            else:
                return middle * 4
        return -1

    def upper(self, scalar: UInt32) -> Bool:
        if scalar < 128:
            return scalar >= 65 and scalar <= 90
        var index = self._index(scalar)
        return index >= 0 and self.values[index + 1] != 0

    def flipped(self, scalar: UInt32) -> UInt32:
        if scalar < 128:
            return (
                scalar ^ 32 if scalar >= 65
                and scalar <= 90
                or scalar >= 97
                and scalar <= 122 else scalar
            )
        var index = self._index(scalar)
        return self.values[index + 2] if index >= 0 else scalar

    def lower_first(self, scalar: UInt32) -> UInt32:
        if scalar < 128:
            return scalar + 32 if scalar >= 65 and scalar <= 90 else scalar
        var index = self._index(scalar)
        return self.values[index + 3] if index >= 0 else scalar


def path_respects_case(query: String, characters: PathCase) -> Bool:
    for character in query.codepoints():
        if characters.upper(UInt32(Int(character))):
            return True
    return False
