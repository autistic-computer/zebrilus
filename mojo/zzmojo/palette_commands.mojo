from .protocol import Decoder
from .palette_command_data import command_bytes
from .palette_fuzzy import PaletteMatcher, FuzzyRange


@fieldwise_init
struct PaletteCommand(Copyable, Movable):
    var name: String
    var aliases: String
    var description: String
    var usage: String
    var target: Optional[UInt8]
    var needs_arguments: Bool
    var allowed: UInt8


@fieldwise_init
struct PaletteCommandMatch(Copyable, Movable):
    var command: Int
    var score: Int
    var hits: List[FuzzyRange]


struct PaletteCommands(Movable):
    var commands: List[PaletteCommand]
    var matcher: PaletteMatcher

    def __init__(out self) raises:
        self.commands = List[PaletteCommand]()
        self.matcher = PaletteMatcher()
        var data = command_bytes()
        if len(data) > 128 * 1024:
            raise Error("palette command catalog exceeds its byte limit")
        var reader = Decoder(data^)
        reader.offset = 0
        var count = reader.uint()
        if count > 512:
            raise Error("palette command catalog exceeds its item limit")
        for _ in range(Int(count)):
            var name = reader.text()
            var aliases = reader.text()
            var description = reader.text()
            var usage = reader.text()
            var kind = reader.uint()
            var target: Optional[UInt8] = None
            if kind > 0:
                if kind > 9:
                    raise Error("invalid palette target kind")
                target = UInt8(kind - 1)
            var needed = reader.uint()
            var allowed = reader.uint()
            if needed > 1 or allowed > 15:
                raise Error("invalid palette command policy")
            self.commands.append(PaletteCommand(name^, aliases^, description^, usage^, target, needed != 0, UInt8(allowed)))
        reader.finish()

    def rows(self, query: String, browser: Bool = True, editor: Bool = False) raises -> List[PaletteCommandMatch]:
        var result = List[PaletteCommandMatch]()
        var availability = UInt8(1) << UInt8(Int(browser) * 2 + Int(editor))
        for index in range(len(self.commands)):
            ref command = self.commands[index]
            if command.allowed & availability == 0:
                continue
            var found = self.matcher.find(query, command.name)
            var score: Int
            var hits = List[FuzzyRange]()
            if found:
                var found_label = found.take()
                score = found_label.score
                swap(hits, found_label.hits)
            elif query.byte_length() > 0 and command.aliases.byte_length() > 0:
                var alias = self.matcher.find(query, command.aliases)
                if not alias:
                    continue
                score = alias.value().score + 1000
            else:
                continue
            var position = len(result)
            for previous in range(len(result)):
                if score < result[previous].score:
                    position = previous
                    break
            result.insert(position, PaletteCommandMatch(index, score, hits^))
        return result^
