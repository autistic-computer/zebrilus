from .palette_places_model import (
    PalettePlace,
    PaletteFleet,
    PLACE_HOST,
    PLACE_SESSION,
    PLACE_WINDOW,
    PLACE_PANE,
)
from .palette_places_parse import (
    parse_palette_line,
    read_written_place,
    command_place_target,
    has_place_target,
    with_place_target,
)
from .palette_target_catalog import target_traits
from .palette_text import palette_trim, palette_words
from .palette_verbs import (
    PaletteVerb,
    PaletteSlot,
    PaletteFilled,
    PaletteCompiled,
    compile_palette_verb,
)
from .exec_protocol import Invocation

comptime PALETTE_INPUT_BYTES = 64 * 1024

comptime PALETTE_NO_EFFECT: UInt8 = 0
comptime PALETTE_EXECUTE: UInt8 = 1
comptime PALETTE_ACTIVATE: UInt8 = 2
comptime PALETTE_CONNECT: UInt8 = 3
comptime PALETTE_NOTIFY: UInt8 = 4
comptime PALETTE_LEVEL: UInt8 = 5


@fieldwise_init
struct PaletteChip(Copyable, Movable):
    var level: UInt8
    var place: PalettePlace
    var label: String
    var automatic: Bool


@fieldwise_init
struct PaletteBack(Copyable, Movable):
    var handled: Bool
    var highlight: Optional[PalettePlace]


struct PaletteDraft(Copyable, Movable):
    var verb: PaletteVerb
    var filled: List[PaletteFilled]
    var labels: List[String]

    def __init__(out self, verb: PaletteVerb):
        self.verb = verb.copy()
        self.filled = List[PaletteFilled]()
        self.labels = List[String]()

    def next(self) -> Optional[PaletteSlot]:
        if len(self.filled) < len(self.verb.slots):
            return self.verb.slots[len(self.filled)].copy()
        return None


struct PaletteEffect(Copyable, Movable):
    var kind: UInt8
    var host: Optional[UInt32]
    var place: PalettePlace
    var command: Optional[Invocation]
    var line: String
    var display: String
    var message: String
    var connect: Bool
    var slot: Optional[PaletteSlot]
    var level_key: Optional[UInt8]

    def __init__(out self):
        self.kind = PALETTE_NO_EFFECT
        self.host = None
        self.place = PalettePlace()
        self.command = None
        self.line = String()
        self.display = String()
        self.message = String()
        self.connect = False
        self.slot = None
        self.level_key = None


def palette_place_label(
    fleet: PaletteFleet, place: PalettePlace
) -> Optional[String]:
    var level = place.level()
    if not level:
        return None
    if level.value() == PLACE_HOST:
        var index = fleet.host_index(place.host.value())
        if index:
            return fleet.hosts[index.value()].name.copy()
    elif level.value() == PLACE_SESSION:
        var index = fleet.session_index(place)
        if index:
            var host, session = index.value()
            return fleet.hosts[host].sessions[session].label.copy()
    elif level.value() == PLACE_WINDOW:
        var index = fleet.window_index(place)
        if index:
            var host, session, window = index.value()
            ref record = fleet.hosts[host].sessions[session].windows[window]
            return String(record.index) + " " + record.name
    else:
        var index = fleet.pane_index(place)
        if index:
            var host, session, window, pane = index.value()
            ref record = (
                fleet.hosts[host].sessions[session].windows[window].panes[pane]
            )
            return String(record.index) + " " + record.name()
    return None


def palette_chip(
    fleet: PaletteFleet, place: PalettePlace
) -> Optional[PaletteChip]:
    var label = palette_place_label(fleet, place)
    var level = place.level()
    if label and level:
        return PaletteChip(level.value(), place.copy(), label.take(), False)
    return None


def palette_chips_to(
    fleet: PaletteFleet, place: PalettePlace
) -> List[PaletteChip]:
    var chips = List[PaletteChip]()
    for level in range(4):
        var at = place.above(UInt8(level + 1)) if level < 3 else place.copy()
        if at.level() != Optional[UInt8](UInt8(level)):
            continue
        var chip = palette_chip(fleet, at)
        if chip:
            chips.append(chip.take())
    return chips^


struct PaletteController(Copyable, Movable):
    var opened: Bool
    var finishing: Bool
    var query: String
    var cursor: Int
    var chips: List[PaletteChip]
    var draft: Optional[PaletteDraft]
    var selected: Optional[Int]
    var navigation_engaged: Bool
    var host: Optional[UInt32]
    var history: List[String]

    def __init__(out self):
        self.opened = False
        self.finishing = False
        self.query = String()
        self.cursor = 0
        self.chips = List[PaletteChip]()
        self.draft = None
        self.selected = None
        self.navigation_engaged = False
        self.host = None
        self.history = List[String]()

    def scope(self) -> PalettePlace:
        return (
            self.chips[len(self.chips) - 1].place.copy() if len(self.chips)
            > 0 else PalettePlace()
        )

    def set_query(mut self, query: String) raises:
        if query.byte_length() > PALETTE_INPUT_BYTES:
            raise Error("palette query exceeds its text limit")
        self.query = query
        self.cursor = query.byte_length()
        self.selected = None
        self.navigation_engaged = False

    def start(mut self, fleet: PaletteFleet, command: Bool = False) raises:
        fleet.validate()
        self.opened = True
        self.finishing = False
        self.host = None
        self.draft = None
        self.chips.clear()
        var chip = palette_chip(fleet, PalettePlace(fleet.here))
        if chip:
            var current = chip.take()
            current.automatic = True
            self.chips.append(current^)
        self.set_query(":" if command else "")

    def close(mut self):
        self.opened = False

    def raw_command(self) raises -> Optional[String]:
        if self.draft:
            return None
        var parsed = parse_palette_line(self.query)
        return parsed.command.take() if parsed.command else Optional[String](
            None
        )

    def next_slot(self) -> Optional[PaletteSlot]:
        return self.draft.value().next() if self.draft else Optional[
            PaletteSlot
        ](None)

    def placeholder(self) -> String:
        var slot = self.next_slot()
        if slot:
            if slot.value().kind == 1:
                var text = slot.value().label.copy()
                if (
                    text.byte_length() > 0
                    and text.as_bytes()[0] >= 97
                    and text.as_bytes()[0] <= 122
                ):
                    return String(chr(Int(text.as_bytes()[0] - 32))) + String(
                        from_utf8_lossy=text.as_bytes()[1:]
                    )
                return text^
            if slot.value().kind == 0:
                return String(
                    "Pick a host" if slot.value().level
                    == 0 else "Pick a session" if slot.value().level
                    == 1 else "Pick a window" if slot.value().level
                    == 2 else "Pick a pane"
                )
            return String(
                "Pick a value" if slot.value().kind == 2 else "A folder or file"
            )
        return String("Type a place or an action")

    def commit_typed(
        mut self, text: String, fleet: PaletteFleet
    ) raises -> Bool:
        if self.query.byte_length() != 0 or self.draft:
            return False
        var count = 0
        for _ in text.codepoints():
            count += 1
        if count < 2:
            return False
        var written = read_written_place(text, fleet)
        if not written or written.value().path:
            return False
        self.chips = palette_chips_to(fleet, written.value().place)
        self.set_query("")
        return True

    def enter_place(
        mut self, place: PalettePlace, fleet: PaletteFleet
    ) raises -> PaletteEffect:
        var effect = PaletteEffect()
        var chips = palette_chips_to(fleet, place)
        if len(chips) == 0:
            return effect^
        if chips[len(chips) - 1].level == PLACE_HOST and place.host:
            var host = fleet.host_index(place.host.value())
            if not host or not fleet.hosts[host.value()].online:
                effect.kind = PALETTE_CONNECT
                effect.host = place.host
        for index in range(len(chips)):
            for existing in self.chips:
                if (
                    existing.level == chips[index].level
                    and existing.place == chips[index].place
                ):
                    chips[index] = existing.copy()
                    break
        self.chips = chips^
        self.set_query("")
        return effect^

    def step_back(mut self) raises -> PaletteBack:
        var popped: Optional[PalettePlace] = None
        if self.draft:
            ref draft = self.draft.value()
            if len(draft.filled) == 0:
                self.draft = None
            else:
                _ = draft.filled.pop()
                var label = draft.labels.pop()
                if label.byte_length() == 0:
                    self.draft = None
        elif len(self.chips) > 0:
            var chip = self.chips.pop()
            popped = chip.place.copy()
        else:
            return PaletteBack(False, None)
        self.set_query("")
        return PaletteBack(True, popped^)

    def backspace(mut self) raises -> Bool:
        if self.query.byte_length() != 0:
            return False
        if self.draft:
            var popped = self.step_back()
            return True
        if len(self.chips) == 0:
            return False
        _ = self.chips.pop()
        self.set_query("")
        return True

    def _execute(
        mut self, compiled: PaletteCompiled, fleet: PaletteFleet
    ) raises -> PaletteEffect:
        var effect = PaletteEffect()
        var scope = self.scope()
        effect.host = compiled.host if compiled.host else scope.host if scope.host else Optional[
            UInt32
        ](
            fleet.here
        )
        self.host = effect.host
        var host = fleet.host_index(effect.host.value())
        if not host or not fleet.hosts[host.value()].online:
            effect.kind = PALETTE_NOTIFY
            effect.message = "Connect to this host before running a command"
            return effect^
        effect.kind = PALETTE_EXECUTE
        effect.line = compiled.line
        effect.display = compiled.display
        var words = palette_words(compiled.line)
        effect.message = "Ran " + (
            words[0] if len(words) > 0 else compiled.line
        )
        if compiled.invocation:
            effect.command = compiled.invocation.copy()
        else:
            var command = Invocation()
            command.name = "if-shell"
            var args: List[String] = ["-F", "1", compiled.line]
            for arg in args:
                command.args.append(List[UInt8](arg.as_bytes()))
            effect.command = command^
        var history: List[String] = [compiled.line]
        for line in self.history:
            if line != compiled.line and len(history) < 20:
                history.append(line.copy())
        self.history = history^
        self.finishing = True
        return effect^

    def run_line(
        mut self,
        input: String,
        var command: Optional[Invocation],
        fleet: PaletteFleet,
    ) raises -> PaletteEffect:
        var words = palette_words(input)
        var name = words[0].copy() if len(words) > 0 else String()
        var traits = target_traits(name)
        var target: Optional[String] = None
        if traits and not has_place_target(input):
            target = command_place_target(
                traits.value().name,
                traits.value().supported,
                traits.value().kind,
                self.scope(),
            )
        var line = input.copy()
        if target:
            line = with_place_target(input, target.value())
            if command and len(command.value().args) == 0:
                var structured = Invocation()
                structured.name = name
                structured.args.append(List[UInt8](String("-t").as_bytes()))
                structured.args.append(List[UInt8](target.value().as_bytes()))
                command = structured^
            else:
                command = None
        return self._execute(
            PaletteCompiled(None, line^, input, command^), fleet
        )

    def advance(mut self, fleet: PaletteFleet) raises -> PaletteEffect:
        if self.draft and not self.draft.value().next():
            var compiled = compile_palette_verb(
                self.draft.value().verb, self.draft.value().filled, fleet
            )
            if compiled:
                return self._execute(compiled.take(), fleet)
            return PaletteEffect()
        self.set_query("")
        var effect = PaletteEffect()
        var slot = self.next_slot()
        if slot and slot.value().kind >= 2:
            effect.kind = PALETTE_LEVEL
            effect.slot = slot^
        return effect^

    def start_verb(
        mut self, verb: PaletteVerb, fleet: PaletteFleet
    ) raises -> PaletteEffect:
        if verb.raw and verb.run == 0:
            self.set_query(":" + verb.command + " ")
            return PaletteEffect()
        var scope = self.scope()
        var draft = PaletteDraft(verb)
        var next = draft.next()
        if (
            next
            and next.value().kind == 0
            and scope.level()
            and scope.level().value() > PLACE_HOST
        ):
            draft.filled.append(PaletteFilled(0, scope.copy(), ""))
            draft.labels.append(String())
        self.draft = draft^
        return self.advance(fleet)

    def fill(
        mut self, value: PaletteFilled, label: String, fleet: PaletteFleet
    ) raises -> PaletteEffect:
        if self.draft:
            self.draft.value().filled.append(value.copy())
            self.draft.value().labels.append(label.copy())
        return self.advance(fleet)

    def activate_place(
        self, place: PalettePlace, fleet: PaletteFleet
    ) -> PaletteEffect:
        var effect = PaletteEffect()
        if not place.host:
            return effect^
        effect.kind = PALETTE_ACTIVATE
        effect.host = place.host
        effect.place = place.copy()
        if not place.session and not place.window and not place.pane:
            var index = fleet.host_index(place.host.value())
            effect.connect = not index or not fleet.hosts[index.value()].online
        return effect^

    def activated(
        mut self, place: PalettePlace, success: Bool, fleet: PaletteFleet
    ) raises -> PaletteEffect:
        if success:
            self.close()
            var effect = PaletteEffect()
            effect.kind = PALETTE_NOTIFY
            var label = palette_place_label(fleet, place)
            effect.message = "Switched to " + (
                label.value() if label else String()
            )
            return effect^
        elif not place.session and not place.window and not place.pane:
            return self.enter_place(place, fleet)
        return PaletteEffect()
