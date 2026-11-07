from std.collections import Set
from .palette_controller import (
    PaletteController,
    PaletteEffect,
    palette_place_label,
    PALETTE_LEVEL,
)
from .palette_presentation import (
    PaletteRow,
    PaletteDecoration,
    PaletteDecorations,
    palette_place_row,
    palette_verb_row,
    palette_section,
    palette_level_header,
    palette_is_current,
    palette_row_bytes,
    MAX_PALETTE_PRESENTATION_BYTES,
)
from .palette_places_model import (
    PaletteFleet,
    PalettePlace,
    PLACE_HOST,
    PLACE_SESSION,
    PLACE_WINDOW,
    PLACE_PANE,
)
from .palette_places_parse import parse_palette_line
from .palette_places_search import (
    PaletteCandidate,
    place_key,
    palette_candidates,
    palette_children,
    search_places,
)
from .palette_commands import PaletteCommands
from .palette_verbs import (
    PaletteVerb,
    PaletteFilled,
    palette_verbs,
    match_palette_verbs,
)
from .palette_fuzzy import FuzzyRange
from .palette_navigation import PaletteNavRow, navigate
from .palette_text import (
    palette_trim,
    palette_words,
    palette_space,
    scalar_width,
)
from .palette_target_catalog import target_traits
from .exec_protocol import Invocation
from .palette_shortcuts import palette_shortcut


def palette_has_arguments(input: String) raises -> Bool:
    var offset = 0
    for character in input.codepoints():
        var scalar = UInt32(Int(character))
        if palette_space(scalar):
            return Bool(
                target_traits(String(from_utf8_lossy=input.as_bytes()[:offset]))
            )
        offset += scalar_width(scalar)
    return False


struct PaletteModel(Movable):
    var controller: PaletteController
    var commands: PaletteCommands
    var verbs: List[PaletteVerb]
    var rows: List[PaletteRow]
    var browser: Bool
    var editor: Bool

    def __init__(
        out self,
        browser: Bool = True,
        editor: Bool = False,
        aliases: String = "",
        insert_path: Bool = True,
    ) raises:
        self.controller = PaletteController()
        self.commands = PaletteCommands()
        self.verbs = palette_verbs(
            self.commands, browser, editor, insert_path, aliases
        )
        self.rows = List[PaletteRow]()
        self.browser = browser
        self.editor = editor

    def _places(
        self,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
        candidates: List[PaletteCandidate],
        parent: PalettePlace,
        action: UInt8 = 1,
    ) raises -> List[PaletteRow]:
        var rows = List[PaletteRow]()
        for index in range(min(50, len(candidates))):
            var row = palette_place_row(
                fleet, decorations, candidates[index], parent
            )
            row.action = action
            rows.append(row^)
        return rows^

    def _empty(
        self,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
        boosts: Dict[String, Int],
    ) raises -> List[PaletteRow]:
        var rows = List[PaletteRow]()
        var needs = List[PaletteRow]()
        var needs_bytes = 0
        var root = PalettePlace()
        for host in fleet.hosts:
            if not host.online:
                continue
            for session in host.sessions:
                for window in session.windows:
                    for pane in window.panes:
                        if not pane.needs_you:
                            continue
                        var place = PalettePlace(
                            host.id, session.id, window.id, pane.id
                        )
                        var label = palette_place_label(
                            fleet, place.above(PLACE_PANE)
                        )
                        if label:
                            var candidate = PaletteCandidate(
                                place,
                                PLACE_WINDOW,
                                label.value(),
                                List[FuzzyRange](),
                                0,
                            )
                            var row = palette_place_row(
                                fleet, decorations, candidate, root
                            )
                            var detail = decorations.panes.get(
                                place_key(place), PaletteDecoration()
                            )
                            row.detail = detail.detail
                            needs_bytes += palette_row_bytes(row)
                            if needs_bytes > MAX_PALETTE_PRESENTATION_BYTES:
                                raise Error(
                                    "palette presentation exceeds its text"
                                    " limit"
                                )
                            needs.append(row^)
                        break
        palette_section(rows, "Needs you", needs^)
        var recent = List[PaletteRow]()
        var seen = Set[String]()
        var availability = UInt8(1) << UInt8(
            Int(self.browser) * 2 + Int(self.editor)
        )
        for line in self.controller.history:
            var words = palette_words(line)
            var name = words[0].copy() if len(words) > 0 else line.copy()
            var allowed = True
            for command in self.commands.commands:
                if command.name == name:
                    allowed = command.allowed & availability != 0
                for alias in command.aliases.split(" "):
                    if alias == name:
                        allowed = command.allowed & availability != 0
            if not allowed or line in seen:
                continue
            seen.add(line)
            var row = PaletteRow()
            row.label = name
            row.right = line
            row.icon = "Terminal"
            row.action = 4
            row.command = line
            recent.append(row^)
            if len(recent) == 3:
                break
        palette_section(rows, "Recent actions", recent^)
        var scope = self.controller.scope()
        if scope.host:
            palette_section(
                rows,
                palette_level_header(fleet, PLACE_SESSION, scope),
                self._places(
                    fleet,
                    decorations,
                    palette_children(
                        fleet, scope, self.commands.matcher, boosts
                    ),
                    scope,
                ),
            )
        var hosts = List[PaletteRow]()
        for host in fleet.hosts:
            if scope.host == Optional[UInt32](host.id):
                continue
            var candidate = PaletteCandidate(
                PalettePlace(host.id), PLACE_HOST, host.name
            )
            hosts.append(palette_place_row(fleet, decorations, candidate, root))
        palette_section(rows, "Other hosts" if scope.host else "Hosts", hosts^)
        return rows^

    def _rebuild(
        self,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
        boosts: Dict[String, Int],
        shortcuts: Dict[String, String],
    ) raises -> List[PaletteRow]:
        fleet.validate()
        var rows = List[PaletteRow]()
        var scope = self.controller.scope()
        var slot = self.controller.next_slot()
        var text = palette_trim(self.controller.query)
        if self.controller.draft:
            if slot and slot.value().kind == 0:
                var host = PalettePlace(
                    scope.host.value() if scope.host else fleet.here
                )
                var found = palette_candidates(
                    fleet,
                    host,
                    slot.value().level,
                    text,
                    self.commands.matcher,
                    boosts,
                )
                if text.byte_length() == 0:
                    var sorted = List[PaletteCandidate]()
                    var order = List[Int]()
                    for current in range(2):
                        for index in range(len(found)):
                            if palette_is_current(
                                fleet.current(),
                                found[index].place,
                                slot.value().level,
                            ) == (current == 0):
                                order.append(index)
                    for index in order:
                        var item = PaletteCandidate()
                        swap(item, found[index])
                        sorted.append(item^)
                    found = sorted^
                palette_section(
                    rows,
                    palette_level_header(fleet, slot.value().level, host),
                    self._places(fleet, decorations, found, host, 5),
                )
            elif slot and slot.value().kind == 1 and text.byte_length() > 0:
                var row = PaletteRow()
                row.label = text
                row.detail = slot.value().label
                row.right = self.controller.draft.value().verb.name
                row.icon = "Pencil"
                row.action = 6
                rows.append(row^)
            return rows^
        var line = parse_palette_line(self.controller.query)
        if line.command:
            var matches = self.commands.rows(
                palette_trim(line.command.value()), self.browser, self.editor
            )
            for matched in matches:
                ref command = self.commands.commands[matched.command]
                var row = PaletteRow()
                row.label = command.name
                row.detail = command.description
                row.icon = "Terminal"
                row.hits = matched.hits.copy()
                if command.name in shortcuts:
                    row.shortcut = palette_shortcut(shortcuts[command.name])
                row.action = 3
                row.command = command.name
                rows.append(row^)
            return rows^
        if not line.search and scope.level() != Optional[UInt8](PLACE_PANE):
            var automatic = True
            for chip in self.controller.chips:
                automatic = automatic and chip.automatic
            if automatic:
                return self._empty(fleet, decorations, boosts)
            var level = (
                scope.level().value() + 1 if scope.level()
                and scope.level().value() < PLACE_PANE else PLACE_HOST
            )
            palette_section(
                rows,
                palette_level_header(fleet, level, scope),
                self._places(
                    fleet,
                    decorations,
                    palette_children(
                        fleet, scope, self.commands.matcher, boosts
                    ),
                    scope,
                ),
            )
            return rows^
        var search = line.search.value().copy() if line.search else String()
        var drilled = scope.level() and scope.level().value() > PLACE_HOST
        var here = not scope.host or scope.host.value() == fleet.here
        var matches = match_palette_verbs(
            self.verbs, search, self.commands.matcher
        )
        var verb_rows = List[PaletteRow]()
        var first_score: Optional[Int] = None
        for found in matches:
            ref verb = self.verbs[found.index]
            if drilled and not verb.target() or not here and verb.kind == 3:
                continue
            if not first_score:
                first_score = found.score
            if len(verb_rows) < 30:
                verb_rows.append(palette_verb_row(self.verbs, found, shortcuts))
        var places = search_places(
            fleet, scope, search, self.commands.matcher, boosts
        )
        var place_rows = self._places(fleet, decorations, places, scope)
        var label = palette_place_label(fleet, scope)
        var actions = (
            "Actions on " + label.value() if drilled
            and label else String("Actions")
        )
        var verbs_first = (
            drilled
            or first_score
            and (len(places) == 0 or first_score.value() < places[0].score)
        )
        if verbs_first:
            palette_section(rows, actions, verb_rows^)
            palette_section(rows, "Places", place_rows^)
        else:
            palette_section(rows, "Places", place_rows^)
            palette_section(rows, actions, verb_rows^)
        var has_host = False
        for chip in self.controller.chips:
            has_host = has_host or chip.level == PLACE_HOST
        if not has_host:
            var root = PalettePlace()
            var hosts = palette_candidates(
                fleet, root, PLACE_HOST, search, self.commands.matcher, boosts
            )
            var entries = List[PaletteRow]()
            for candidate in hosts:
                entries.append(
                    palette_place_row(fleet, decorations, candidate, root)
                )
            palette_section(rows, "Hosts", entries^)
        return rows^

    def rebuild(
        mut self,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
        boosts: Dict[String, Int] = Dict[String, Int](),
        shortcuts: Dict[String, String] = Dict[String, String](),
    ) raises:
        var selected: Optional[PalettePlace] = None
        if self.controller.selected and self.controller.selected.value() < len(
            self.rows
        ):
            ref row = self.rows[self.controller.selected.value()]
            if row.action == 1 or row.action == 5:
                selected = row.place.copy()
        self.rows = self._rebuild(fleet, decorations, boosts, shortcuts)
        if selected:
            self.controller.selected = None
            for index in range(len(self.rows)):
                if self.rows[index].place == selected.value() and (
                    self.rows[index].action == 1 or self.rows[index].action == 5
                ):
                    self.controller.selected = index
                    break
        if (
            not self.controller.selected
            or self.controller.selected.value() >= len(self.rows)
            or self.rows[self.controller.selected.value()].action == 0
        ):
            self.controller.selected = None
            for index in range(len(self.rows)):
                if self.rows[index].action != 0:
                    self.controller.selected = index
                    break

    def navigate(mut self, direction: Int) raises:
        var rows = List[PaletteNavRow]()
        for row in self.rows:
            rows.append(
                PaletteNavRow(
                    UInt8(0 if row.action == 0 else 3),
                    row.right == "current",
                    row.indent,
                    row.expanded,
                    0,
                )
            )
        self.controller.selected = navigate(
            Span(rows), self.controller.selected, direction
        )
        self.controller.navigation_engaged = True

    def _command(
        mut self, name: String, complete: Bool, fleet: PaletteFleet
    ) raises -> PaletteEffect:
        var needed = False
        for command in self.commands.commands:
            if command.name == name:
                needed = command.needs_arguments
                break
        if complete or needed:
            self.controller.set_query(":" + name + " ")
            return PaletteEffect()
        var command = Invocation()
        command.name = name
        return self.controller.run_line(name, command^, fleet)

    def activate(
        mut self, complete: Bool, fleet: PaletteFleet
    ) raises -> PaletteEffect:
        var selected = -1
        if self.controller.selected and self.controller.selected.value() < len(
            self.rows
        ):
            selected = self.controller.selected.value()
        var raw = self.controller.raw_command()
        if raw:
            if (
                not complete
                and not self.controller.navigation_engaged
                and palette_has_arguments(raw.value())
            ):
                return self.controller.run_line(raw.value(), None, fleet)
            if selected >= 0:
                ref row = self.rows[selected]
                if row.action == 3:
                    var command = row.command.copy()
                    return self._command(command, complete, fleet)
                if row.action == 4:
                    return self.controller.run_line(row.command, None, fleet)
            if not complete and palette_trim(raw.value()).byte_length() > 0:
                return self.controller.run_line(raw.value(), None, fleet)
            return PaletteEffect()
        var slot = self.controller.next_slot()
        if slot:
            if slot.value().kind >= 2:
                var effect = PaletteEffect()
                effect.kind = PALETTE_LEVEL
                effect.slot = slot.copy()
                effect.level_key = UInt8(1 if complete else 0)
                return effect^
            if slot.value().kind == 1:
                var text = palette_trim(self.controller.query)
                if text.byte_length() > 0:
                    return self.controller.fill(
                        PaletteFilled(1, PalettePlace(), text), text, fleet
                    )
            elif selected >= 0 and self.rows[selected].action == 5:
                var place = self.rows[selected].place.copy()
                var label = palette_place_label(fleet, place)
                return self.controller.fill(
                    PaletteFilled(0, place^, ""),
                    label.value() if label else String(),
                    fleet,
                )
            return PaletteEffect()
        if selected >= 0:
            ref row = self.rows[selected]
            if row.action == 1:
                if complete:
                    return self.controller.enter_place(row.place, fleet)
                return self.controller.activate_place(row.place, fleet)
            if row.action == 2 and row.index < len(self.verbs):
                return self.controller.start_verb(self.verbs[row.index], fleet)
            if row.action == 3:
                var command = row.command.copy()
                return self._command(command, complete, fleet)
            if row.action == 4 and not complete:
                return self.controller.run_line(row.command, None, fleet)
        elif not complete:
            var scope = self.controller.scope()
            var manual = False
            for chip in self.controller.chips:
                manual = manual or not chip.automatic
            if not (scope == PalettePlace()) and manual:
                return self.controller.activate_place(scope, fleet)
        return PaletteEffect()

    def leave(
        mut self,
        backspace: Bool,
        fleet: PaletteFleet,
        decorations: PaletteDecorations,
    ) raises -> Bool:
        if backspace:
            if not self.controller.backspace():
                return False
            self.rebuild(fleet, decorations)
            return True
        var result = self.controller.step_back()
        if not result.handled:
            return False
        self.rebuild(fleet, decorations)
        if result.highlight:
            for index in range(len(self.rows)):
                if (
                    self.rows[index].action == 1
                    and self.rows[index].place == result.highlight.value()
                ):
                    self.controller.selected = index
                    self.controller.navigation_engaged = True
                    break
        return True
