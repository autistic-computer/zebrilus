from .protocol import Decoder
from .tree_wire import read_uint, read_text
from .palette_program_data import program_bytes
from .palette_text import palette_trim


@fieldwise_init
struct PaletteProgram(Copyable, Movable):
    var id: String
    var name: String


@fieldwise_init
struct _ProgramKey(Copyable, Movable):
    var key: String
    var program: Int


def _component(text: String) -> String:
    var start = 0
    for index in range(text.byte_length()):
        if text.as_bytes()[index] == 47:
            start = index + 1
    return String(from_utf8_lossy=text.as_bytes()[start:])


struct PalettePrograms(Movable):
    var values: List[PaletteProgram]
    var keys: Dict[String, List[_ProgramKey]]

    def __init__(out self) raises:
        self.values = List[PaletteProgram]()
        self.keys = Dict[String, List[_ProgramKey]]()
        var data = program_bytes()
        if len(data) > 128 * 1024:
            raise Error("palette program catalog exceeds its byte limit")
        var reader = Decoder(data^)
        reader.offset = 0
        var count = Int(read_uint(reader, 512))
        for index in range(count):
            var id = read_text(reader, 4096)
            var name = read_text(reader, 4096)
            self.values.append(PaletteProgram(id^, name^))
            var keys = Int(read_uint(reader, 128))
            for _ in range(keys):
                var key = read_text(reader, 4096)
                var component = _component(key)
                if component not in self.keys:
                    self.keys[component] = List[_ProgramKey]()
                self.keys[component].append(_ProgramKey(key^, index))
        reader.finish()

    def by_id(self, id: String) -> Optional[Int]:
        for index in range(len(self.values)):
            if self.values[index].id == id:
                return index
        return None

    def _lookup(self, bytes: Span[UInt8, _]) raises -> Optional[Int]:
        var start = 0
        for index in range(len(bytes)):
            if bytes[index] == 47:
                start = index + 1
        var name: String
        try:
            name = String(from_utf8=bytes[start:])
        except:
            return None
        if name not in self.keys:
            return None
        var best: Optional[Int] = None
        var length = 0
        for candidate in self.keys[name]:
            var count = candidate.key.byte_length()
            if count > len(bytes) or count < length:
                continue
            var matches = (
                count == len(bytes) or bytes[len(bytes) - count - 1] == 47
            )
            for index in range(count):
                matches = (
                    matches
                    and bytes[len(bytes) - count + index]
                    == candidate.key.as_bytes()[index]
                )
            if matches:
                best = candidate.program
                length = count
        return best

    def resolve(self, candidates: List[String]) raises -> Optional[Int]:
        if len(candidates) > 4096:
            raise Error("palette process candidates exceed their item limit")
        for candidate in candidates:
            if candidate.byte_length() > 1024 * 1024:
                raise Error("palette process candidate exceeds its text limit")
            var trimmed = palette_trim(candidate)
            var start = max(trimmed.byte_length() - 128, 0)
            var normalized = List[UInt8](capacity=128)
            for byte in trimmed.as_bytes()[start:]:
                normalized.append(
                    UInt8(47) if byte == 92 else byte + 32 if byte >= 65
                    and byte <= 90 else byte
                )
            if len(normalized) >= 4:
                var end = len(normalized)
                if (
                    normalized[end - 4] == 46
                    and normalized[end - 3] == 101
                    and normalized[end - 2] == 120
                    and normalized[end - 1] == 101
                ):
                    normalized.resize(end - 4, UInt8(0))
            var found = self._lookup(Span(normalized))
            if found:
                return found
            var dot = -1
            var component = 0
            for index in range(len(normalized)):
                if normalized[index] == 47:
                    component = index + 1
                    dot = -1
                elif normalized[index] == 46 and index > component:
                    dot = index
            if dot >= 0:
                found = self._lookup(Span(normalized)[:dot])
                if found:
                    return found
        return None
