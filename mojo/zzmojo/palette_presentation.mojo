from .palette_places_model import (
    PalettePlace,
    PaletteFleet,
    PLACE_HOST,
    PLACE_SESSION,
    PLACE_WINDOW,
    PLACE_PANE,
)
from .palette_places_search import PaletteCandidate, place_key
from .palette_controller import PaletteController, palette_place_label
from .palette_verbs import PaletteVerb, PaletteVerbMatch
from .palette_fuzzy import FuzzyRange
from .palette_shortcuts import PaletteShortcut, palette_shortcut

comptime MAX_PALETTE_PRESENTATION_BYTES = 16 * 1024 * 1024


struct PaletteDecoration(Copyable, Movable):
    var detail: String
    var right: String
    var icon: String
    var status: Optional[UInt8]
    var running: Bool

    def __init__(out self):
        self.detail = String()
        self.right = String()
        self.icon = String()
        self.status = None
        self.running = False


struct PaletteDecorations(Copyable, Movable):
    var hosts: Dict[UInt32, PaletteDecoration]
    var panes: Dict[String, PaletteDecoration]

    def __init__(out self):
        self.hosts = Dict[UInt32, PaletteDecoration]()
        self.panes = Dict[String, PaletteDecoration]()

    def running(self, fleet: PaletteFleet, place: PalettePlace) raises -> Bool:
        if not place.host:
            return False
        var host = fleet.host_index(place.host.value())
        if not host:
            return False
        for session in fleet.hosts[host.value()].sessions:
            if place.session and session.id != place.session.value():
                continue
            for window in session.windows:
                if place.window and window.id != place.window.value():
                    continue
                for pane in window.panes:
                    if place.pane and pane.id != place.pane.value():
                        continue
                    var key = place_key(
                        PalettePlace(place.host, session.id, window.id, pane.id)
                    )
                    if key in self.panes and self.panes[key].running:
                        return True
        return False


struct PaletteRow(Copyable, Movable):
    var label: String
    var detail: String
    var right: String
    var hits: List[FuzzyRange]
    var muted_prefix: Int
    var status: Optional[UInt8]
    var running: Bool
    var icon: String
    var indent: Float32
    var expanded: Optional[Bool]
    var shortcut: Optional[PaletteShortcut]
    var action: UInt8
    var place: PalettePlace
    var index: Int
    var command: String
    var hint: String

    def __init__(out self):
        self.label = String()
        self.detail = String()
        self.right = String()
        self.hits = List[FuzzyRange]()
        self.muted_prefix = 0
        self.status = None
        self.running = False
        self.icon = String()
        self.indent = 0
        self.expanded = None
        self.shortcut = None
        self.action = 0
        self.place = PalettePlace()
        self.index = 0
        self.command = String()
        self.hint = String()


@fieldwise_init
struct PalettePill(Copyable, Movable):
    var kind: UInt8
    var prefix: String
    var label: String
    var status: Optional[UInt8]


@fieldwise_init
struct PaletteHint(Copyable, Movable):
    var key: String
    var label: String


def palette_count(count: Int, noun: String) -> String:
    return String(count) + " " + noun + ("" if count == 1 else "s")


def palette_row_bytes(row: PaletteRow) -> Int:
    return (
        row.label.byte_length()
        + row.detail.byte_length()
        + row.right.byte_length()
        + row.icon.byte_length()
        + (row.shortcut.value().byte_length() if row.shortcut else 0)
        + row.command.byte_length()
        + row.hint.byte_length()
    )


def palette_level_header(
    fleet: PaletteFleet, level: UInt8, parent: PalettePlace
) -> String:
    if level == PLACE_HOST:
        return String("Hosts")
    var noun = String(
        "Sessions" if level
        == PLACE_SESSION else "Windows" if level
        == PLACE_WINDOW else "Panes"
    )
    var host: Optional[Int] = None
    if parent.host:
        host = fleet.host_index(parent.host.value())
    var generic = (
        noun + " on " + fleet.hosts[host.value()].name if host else noun.copy()
    )
    if level == PLACE_SESSION:
        return generic^
    if level == PLACE_PANE:
        var window = fleet.window_index(parent)
        if window:
            var h, s, w = window.value()
            ref record = fleet.hosts[h].sessions[s].windows[w]
            return "Panes in " + String(record.index) + " " + record.name
    var session = fleet.session_index(parent)
    if session:
        var h, s = session.value()
        return noun + " in " + fleet.hosts[h].sessions[s].label
    return generic^


def palette_crumbs(
    fleet: PaletteFleet, place: PalettePlace, level: UInt8, parent: PalettePlace
) -> String:
    var result = String()
    var before = parent.level()
    for between in range(3):
        if (
            between >= Int(level)
            or before
            and between <= Int(before.value())
            or between == 0
            and len(fleet.hosts) <= 1
        ):
            continue
        var label = palette_place_label(fleet, place.above(UInt8(between + 1)))
        if label:
            if result.byte_length() > 0:
                result += " › "
            result += label.value()
    if result.byte_length() > 0:
        result += " › "
    return result^


def palette_is_current(
    current: PalettePlace, place: PalettePlace, level: UInt8
) -> Bool:
    if place.host != current.host:
        return False
    return (
        True if level
        == PLACE_HOST else place.session
        == current.session if level
        == PLACE_SESSION else place.window
        == current.window if level
        == PLACE_WINDOW else place.pane
        == current.pane
    )


def palette_place_row(
    fleet: PaletteFleet,
    decorations: PaletteDecorations,
    candidate: PaletteCandidate,
    parent: PalettePlace,
) raises -> PaletteRow:
    var row = PaletteRow()
    var prefix = palette_crumbs(fleet, candidate.place, candidate.level, parent)
    row.label = prefix + candidate.label
    row.muted_prefix = prefix.byte_length()
    for hit in candidate.hits:
        row.hits.append(
            FuzzyRange(
                hit.start + prefix.byte_length(), hit.end + prefix.byte_length()
            )
        )
    row.place = candidate.place.copy()
    row.action = 1
    if candidate.level == PLACE_HOST:
        row.icon = "HardDrive"
        if (
            candidate.place.host
            and candidate.place.host.value() in decorations.hosts
        ):
            ref host = decorations.hosts[candidate.place.host.value()]
            row.detail = host.right
            row.status = host.status
    elif candidate.level == PLACE_SESSION:
        row.icon = "Layers"
        var session = fleet.session_index(candidate.place)
        if session:
            var h, s = session.value()
            row.detail = palette_count(
                len(fleet.hosts[h].sessions[s].windows), "window"
            )
    elif candidate.level == PLACE_WINDOW:
        row.icon = "AppWindow"
        var window = fleet.window_index(candidate.place)
        if window:
            var h, s, w = window.value()
            row.detail = palette_count(
                len(fleet.hosts[h].sessions[s].windows[w].panes), "pane"
            )
    else:
        row.icon = "SquareTerminal"
        var key = place_key(candidate.place)
        if key in decorations.panes:
            ref pane = decorations.panes[key]
            row.detail = pane.detail
            row.icon = pane.icon
    if palette_is_current(fleet.current(), candidate.place, candidate.level):
        row.right = "current"
    row.running = candidate.level != PLACE_HOST and decorations.running(
        fleet, candidate.place
    )
    return row^


def palette_verb_row(
    verbs: List[PaletteVerb],
    found: PaletteVerbMatch,
    shortcuts: Dict[String, String] = Dict[String, String](),
) raises -> PaletteRow:
    var row = PaletteRow()
    ref verb = verbs[found.index]
    row.label = verb.name
    row.detail = verb.description
    row.right = verb.spelled()
    row.hits = found.hits.copy()
    row.icon = (
        "Settings" if verb.kind
        == 1 else "File" if verb.kind
        == 2 else "Terminal"
    )
    row.action = 2
    row.index = found.index
    if verb.run == 0 and len(verb.args) == 0 and verb.command in shortcuts:
        row.shortcut = palette_shortcut(shortcuts[verb.command])
    return row^


def palette_section(
    mut rows: List[PaletteRow], label: String, var entries: List[PaletteRow]
) raises:
    if len(entries) == 0:
        return
    if len(rows) + len(entries) + 1 > 65536:
        raise Error("palette presentation exceeds its row limit")
    var size = label.byte_length()
    for row in rows:
        size += palette_row_bytes(row)
    for row in entries:
        size += palette_row_bytes(row)
    if size > MAX_PALETTE_PRESENTATION_BYTES:
        raise Error("palette presentation exceeds its text limit")
    var header = PaletteRow()
    header.label = label
    rows.append(header^)
    for index in range(len(entries)):
        var item = PaletteRow()
        swap(item, entries[index])
        rows.append(item^)


def palette_place_pills(
    controller: PaletteController, decorations: PaletteDecorations
) -> List[PalettePill]:
    var result = List[PalettePill]()
    for chip in controller.chips:
        var status: Optional[UInt8] = None
        if chip.level == PLACE_HOST:
            status = UInt8(3)
            if chip.place.host:
                var host = decorations.hosts.get(
                    chip.place.host.value(), PaletteDecoration()
                )
                if host.status:
                    status = host.status
        result.append(
            PalettePill(
                UInt8(1 if chip.level == PLACE_HOST else 3),
                "",
                chip.label,
                status,
            )
        )
    if controller.draft:
        ref draft = controller.draft.value()
        result.append(PalettePill(4, "", draft.verb.name, None))
        for label in draft.labels:
            if label.byte_length() > 0:
                result.append(PalettePill(3, "", label, None))
    return result^


def palette_place_hints(
    controller: PaletteController, row: Optional[PaletteRow], enter: String
) -> List[PaletteHint]:
    var result = List[PaletteHint]()
    if row and row.value().action == 1:
        result.append(PaletteHint("tab", "into"))
    result.append(PaletteHint("enter", enter))
    if controller.query.byte_length() == 0 and (
        len(controller.chips) > 0 or controller.draft
    ):
        result.append(PaletteHint("backspace", "back"))
    result.append(PaletteHint("escape", "close"))
    return result^
