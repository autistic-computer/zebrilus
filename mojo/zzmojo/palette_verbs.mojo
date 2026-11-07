from .palette_commands import PaletteCommands
from .palette_target_catalog import PaletteTargetTraits, target_traits
from .palette_places_model import (
    PaletteFleet,
    PalettePlace,
    PLACE_HOST,
    PLACE_SESSION,
    PLACE_WINDOW,
    PLACE_PANE,
)
from .palette_places_parse import command_place_target, has_place_target
from .palette_text import palette_space, palette_trim, palette_contains_space
from .palette_fuzzy import PaletteMatcher, FuzzyRange, PALETTE_QUERY_BYTES
from .cli_preflight import _catalog, _validate, CatalogEntry, CliPreflight
from .exec_protocol import Invocation
from .tree_model import _Order, _ordered

comptime MAX_PALETTE_VERBS = 65536
comptime MAX_PALETTE_WORDS = 4096
comptime MAX_PALETTE_GUI_BYTES = 16 * 1024 * 1024


@fieldwise_init
struct PaletteSlot(Copyable, Movable):
    var kind: UInt8
    var level: UInt8
    var label: String


struct PaletteVerb(Copyable, Movable):
    var name: String
    var keywords: String
    var description: String
    var kind: UInt8
    var run: UInt8
    var command: String
    var body: String
    var args: List[String]
    var slots: List[PaletteSlot]
    var raw: Bool

    def __init__(out self):
        self.name = String()
        self.keywords = String()
        self.description = String()
        self.kind = 0
        self.run = 0
        self.command = String()
        self.body = String()
        self.args = List[String]()
        self.slots = List[PaletteSlot]()
        self.raw = False

    def spelled(self) -> String:
        if self.run == 1:
            return self.body.copy()
        if self.run == 2:
            return String("gui")
        if self.run == 3:
            return String("zz")
        var result = self.command.copy()
        for arg in self.args:
            result += " " + arg
        return result^

    def target(self) -> Optional[UInt8]:
        for slot in self.slots:
            if slot.kind == 0:
                return slot.level
        return None


@fieldwise_init
struct PaletteGuiVerb(Copyable, Movable):
    var id: String
    var title: String
    var name: String
    var keywords: String


@fieldwise_init
struct PaletteVerbMatch(Copyable, Movable):
    var index: Int
    var score: Int
    var hits: List[FuzzyRange]


@fieldwise_init
struct PaletteFilled(Copyable, Movable):
    var kind: UInt8
    var place: PalettePlace
    var text: String


@fieldwise_init
struct PaletteCompiled(Copyable, Movable):
    var host: Optional[UInt32]
    var line: String
    var display: String
    var invocation: Optional[Invocation]


def palette_quote(text: String) -> String:
    var safe = text.byte_length() > 0
    var apostrophe = False
    for value in text.as_bytes():
        safe = safe and (
            value >= 48
            and value <= 57
            or value >= 65
            and value <= 90
            or value >= 97
            and value <= 122
            or value == 45
            or value == 95
            or value == 46
            or value == 47
            or value == 58
            or value == 37
            or value == 64
            or value == 43
            or value == 44
            or value == 61
        )
        apostrophe = apostrophe or value == 39
    if safe:
        return text.copy()
    if not apostrophe:
        return "'" + text + "'"
    return (
        '"'
        + text.replace("\\", "\\\\").replace('"', '\\"').replace("$", "\\$")
        + '"'
    )


def palette_command_words(text: String) raises -> List[String]:
    if text.byte_length() > PALETTE_QUERY_BYTES:
        raise Error("palette alias exceeds its byte limit")
    var result = List[String]()
    var quote: UInt32 = 0
    var escaped = False
    var started = False
    var word = String()
    for part in text.codepoint_slices():
        var character = String(part)
        var scalar: UInt32 = 0
        for point in character.codepoints():
            scalar = UInt32(Int(point))
        if escaped:
            word += character
            escaped = False
            started = True
        elif scalar == 92 and quote != 39:
            escaped = True
            started = True
        elif quote != 0:
            if scalar == quote:
                quote = 0
            else:
                word += character
        elif scalar == 39 or scalar == 34:
            quote = scalar
            started = True
        elif scalar == 59 or palette_space(scalar):
            if started:
                result.append(word^)
                word = String()
                started = False
            if scalar == 59:
                result.append(String(";"))
        else:
            word += character
            started = True
        if len(result) > MAX_PALETTE_WORDS:
            raise Error("palette alias exceeds its word limit")
    if escaped:
        word += "\\"
    if started:
        result.append(word^)
    if len(result) > MAX_PALETTE_WORDS:
        raise Error("palette alias exceeds its word limit")
    return result^


def palette_target_name(
    fleet: PaletteFleet, place: PalettePlace
) -> Optional[String]:
    var session = fleet.session_index(place)
    if not session:
        return None
    var host, record = session.value()
    var name = fleet.hosts[host].sessions[record].name.copy()
    var window = fleet.window_index(place)
    if not window:
        return name^
    var index = window.value()[2]
    name += ":" + String(
        fleet.hosts[host].sessions[record].windows[index].index
    )
    var pane = fleet.pane_index(place)
    if pane:
        name += "." + String(
            fleet.hosts[host]
            .sessions[record]
            .windows[index]
            .panes[pane.value()[3]]
            .index
        )
    return name^


def palette_go_to(fleet: PaletteFleet, place: PalettePlace) -> Optional[String]:
    var name = palette_target_name(fleet, place)
    var level = place.level()
    if not name or not level or level.value() == PLACE_HOST:
        return None
    return (
        (
            "switch-client" if level.value()
            == PLACE_SESSION else "select-window" if level.value()
            == PLACE_WINDOW else "select-pane"
        )
        + " -t "
        + palette_quote(name.value())
    )


def _target_level(traits: PaletteTargetTraits) -> Optional[UInt8]:
    if traits.name == "new-session":
        return None
    if traits.name == "new-window":
        return UInt8(PLACE_SESSION)
    if (
        traits.supported
        and traits.kind
        and traits.kind.value() >= 1
        and traits.kind.value() <= 3
    ):
        return traits.kind
    return None


def _base_verb(
    name: String,
    keywords: String,
    description: String,
    canonical: String,
    var args: List[String],
    needs_arguments: Bool,
    traits: PaletteTargetTraits,
) -> PaletteVerb:
    var result = PaletteVerb()
    result.name = name
    result.keywords = keywords
    result.description = description
    result.command = canonical
    result.args = args^
    var target = _target_level(traits)
    if target:
        result.slots.append(PaletteSlot(0, target.value(), ""))
    if traits.minimum > 0:
        result.slots.append(PaletteSlot(1, 0, traits.text_label))
    result.raw = (
        traits.minimum == 0 and len(result.args) == 0 and needs_arguments
    )
    return result^


def _exact_spec(entries: List[CatalogEntry], name: String) -> Optional[Int]:
    for index in range(len(entries)):
        if entries[index].name == name:
            return index
        for alias in entries[index].aliases.split(","):
            if alias.byte_length() > 0 and alias == name:
                return index
    return None


def _open_alias(
    entries: List[CatalogEntry], words: List[String], index: Int
) raises -> Bool:
    var command = Invocation()
    command.name = entries[index].name
    for at in range(1, len(words)):
        if words[at] == ";" or words[at].startswith("{"):
            return False
        command.args.append(List[UInt8](words[at].as_bytes()))
    if words[0] == ";" or words[0].startswith("{"):
        return False
    var spec = entries[index].copy()
    spec.native = False
    spec.minimum = 0
    spec.maximum = 0
    var checked = CliPreflight()
    _validate(command, spec, checked)
    return not checked.error


def palette_verbs(
    commands: PaletteCommands,
    browser: Bool = True,
    editor: Bool = False,
    insert_path: Bool = True,
    aliases: String = "",
    gui: List[PaletteGuiVerb] = List[PaletteGuiVerb](),
) raises -> List[PaletteVerb]:
    if (
        aliases.byte_length() > PALETTE_QUERY_BYTES
        or len(gui) > MAX_PALETTE_VERBS
    ):
        raise Error("palette verb services exceed their limit")
    var gui_bytes = 0
    for offer in gui:
        gui_bytes += (
            offer.id.byte_length()
            + offer.title.byte_length()
            + offer.name.byte_length()
            + offer.keywords.byte_length()
        )
        if gui_bytes > MAX_PALETTE_GUI_BYTES:
            raise Error("palette GUI verbs exceed their text limit")
    var result = List[PaletteVerb]()
    var availability = UInt8(1) << UInt8(Int(browser) * 2 + Int(editor))
    var curated: List[Tuple[String, String, String, String]] = [
        ("read pane", "capture-pane", "-p", "capture print output scrollback"),
        ("split right", "split-window", "-h", "split horizontal new pane"),
        ("split down", "split-window", "-v", "split vertical new pane"),
        ("zoom pane", "resize-pane", "-Z", "zoom maximize unzoom"),
        ("send keys", "send-keys", "", "type text"),
        ("detach", "detach-client", "", "leave"),
    ]
    for item in curated:
        for command in commands.commands:
            if command.name != item[1] or command.allowed & availability == 0:
                continue
            var traits = target_traits(command.name)
            if not traits:
                raise Error("palette command target metadata is incomplete")
            var args = List[String]()
            if item[2].byte_length() > 0:
                args.append(item[2].copy())
            var verb = _base_verb(
                item[0],
                item[1] + " " + item[3],
                command.description,
                command.name,
                args^,
                command.needs_arguments,
                traits.value(),
            )
            if command.name == "send-keys":
                var slots = List[PaletteSlot]()
                for slot in verb.slots:
                    if slot.kind != 1:
                        slots.append(slot.copy())
                slots.append(PaletteSlot(1, 0, "keys"))
                verb.slots = slots^
            result.append(verb^)
    if insert_path:
        var verb = PaletteVerb()
        verb.name = "insert path"
        verb.keywords = "file folder paste browse"
        verb.description = (
            "Pick a file or folder and type its path into the pane"
        )
        verb.kind = 2
        verb.run = 3
        verb.slots.append(PaletteSlot(3, 0, ""))
        result.append(verb^)
    for offer in gui:
        var verb = PaletteVerb()
        verb.name = offer.name
        verb.keywords = offer.keywords
        verb.description = offer.title
        verb.kind = 1
        verb.run = 2
        verb.command = offer.id
        verb.slots.append(PaletteSlot(2, 0, offer.id))
        result.append(verb^)
    for command in commands.commands:
        if command.allowed & availability == 0:
            continue
        var name = command.name.replace("-", " ")
        var duplicate = False
        for verb in result:
            duplicate = duplicate or verb.name == name
        if duplicate:
            continue
        var traits = target_traits(command.name)
        if not traits:
            raise Error("palette command target metadata is incomplete")
        var keywords = command.name.copy()
        if command.aliases.byte_length() > 0:
            keywords += " " + command.aliases
        result.append(
            _base_verb(
                name,
                keywords,
                command.description,
                command.name,
                List[String](),
                command.needs_arguments,
                traits.value(),
            )
        )
    var entries = _catalog()
    for source in aliases.split("\n"):
        var line = palette_trim(String(source))
        if (
            line == "split-pane=split-window"
            or line == "splitp=split-window"
            or line == "server-info=show-messages -JT"
            or line == "info=show-messages -JT"
            or line == "choose-window=choose-tree -w"
            or line == "choose-session=choose-tree -s"
        ):
            continue
        var equal = -1
        for index in range(line.byte_length()):
            if line.as_bytes()[index] == 61:
                equal = index
                break
        if equal < 0:
            continue
        var name = palette_trim(String(from_utf8_lossy=line.as_bytes()[:equal]))
        var body = palette_trim(
            String(from_utf8_lossy=line.as_bytes()[equal + 1 :])
        )
        if name.byte_length() == 0 or palette_contains_space(name):
            continue
        var words = palette_command_words(body)
        if len(words) == 0:
            continue
        var spec = _exact_spec(entries, words[0])
        if not spec:
            continue
        var canonical = entries[spec.value()].name
        var allowed = False
        var description = String()
        for command in commands.commands:
            if command.name == canonical:
                allowed = command.allowed & availability != 0
                description = command.description
        if not allowed:
            continue
        var traits = target_traits(canonical)
        if not traits:
            raise Error("palette command target metadata is incomplete")
        var verb = PaletteVerb()
        verb.name = name
        verb.keywords = body
        verb.description = description
        verb.kind = 3
        verb.run = 1
        verb.command = canonical
        verb.body = body
        if _open_alias(entries, words, spec.value()):
            var target = _target_level(traits.value())
            if target and not has_place_target(body):
                verb.slots.append(PaletteSlot(0, target.value(), ""))
            if traits.value().minimum > 0:
                verb.slots.append(PaletteSlot(1, 0, traits.value().text_label))
        result.append(verb^)
        if len(result) > MAX_PALETTE_VERBS:
            raise Error("palette verbs exceed their row limit")
    if len(result) > MAX_PALETTE_VERBS:
        raise Error("palette verbs exceed their row limit")
    return result^


def match_palette_verbs(
    verbs: List[PaletteVerb],
    query: String,
    matcher: PaletteMatcher,
    target: Optional[UInt8] = None,
) raises -> List[PaletteVerbMatch]:
    var result = List[PaletteVerbMatch]()
    for index in range(len(verbs)):
        ref verb = verbs[index]
        if target and verb.target() != target:
            continue
        var found = matcher.find(query, verb.name)
        var hits = List[FuzzyRange]()
        var score: Int
        if found:
            var matched = found.take()
            score = matched.score
            swap(hits, matched.hits)
        else:
            found = matcher.find(query, verb.keywords)
            if not found:
                continue
            score = found.value().score + 1000
        result.append(PaletteVerbMatch(index, score, hits^))
    var order = List[_Order]()
    for index in range(len(result)):
        order.append(
            _Order(
                UInt64(result[index].score) ^ (UInt64(1) << 63),
                UInt64(index),
                index,
            )
        )
    order = _ordered(order^)
    var sorted = List[PaletteVerbMatch](capacity=len(result))
    for rank in order:
        var item = PaletteVerbMatch(0, 0, List[FuzzyRange]())
        swap(item, result[rank.index])
        sorted.append(item^)
    return sorted^


def compile_palette_verb(
    verb: PaletteVerb, filled: List[PaletteFilled], fleet: PaletteFleet
) raises -> Optional[PaletteCompiled]:
    if verb.run >= 2:
        return None
    if len(filled) < len(verb.slots):
        return None
    var line = verb.name.copy() if verb.run == 1 else verb.spelled()
    var display = verb.body.copy() if verb.run == 1 else line.copy()
    var command = Invocation()
    command.name = verb.name if verb.run == 1 else verb.command
    for arg in verb.args:
        command.args.append(List[UInt8](arg.as_bytes()))
    var structured = True
    var host: Optional[UInt32] = None
    var traits = target_traits(verb.command)
    for index in range(len(verb.slots)):
        ref slot = verb.slots[index]
        ref value = filled[index]
        if slot.kind == 0 and value.kind == 0:
            var place = (
                value.place.above(slot.level + 1) if slot.level
                < PLACE_PANE else value.place.copy()
            )
            host = place.host
            var target: Optional[String] = None
            if traits:
                target = command_place_target(
                    traits.value().name,
                    traits.value().supported,
                    traits.value().kind,
                    place,
                )
            if not target:
                if slot.level >= PLACE_PANE and place.pane:
                    target = "%" + String(place.pane.value())
                elif slot.level >= PLACE_WINDOW and place.window:
                    target = "@" + String(place.window.value())
                elif slot.level >= PLACE_SESSION and place.session:
                    target = "$" + String(place.session.value())
            var readable = palette_target_name(fleet, place)
            if not target or not readable:
                return None
            line += " -t " + palette_quote(target.value())
            display += " -t " + palette_quote(readable.value())
            if structured:
                command.args.append(List[UInt8](String("-t").as_bytes()))
                command.args.append(List[UInt8](target.value().as_bytes()))
        elif slot.kind == 1 and value.kind == 1:
            var single = Bool(traits) and traits.value().single
            var piece = palette_quote(
                value.text
            ) if single else value.text.copy()
            line += " " + piece
            display += " " + piece
            if structured and single:
                command.args.append(List[UInt8](value.text.as_bytes()))
            else:
                structured = False
        else:
            return None
    var invocation: Optional[Invocation] = None
    if structured:
        invocation = command^
    return PaletteCompiled(host, line^, display^, invocation^)
