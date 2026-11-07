from .palette_places_model import (
    PaletteFleet,
    PalettePlace,
    PaletteFleetWindow,
    PLACE_HOST,
    PLACE_SESSION,
    PLACE_WINDOW,
    PLACE_PANE,
)
from .palette_places_parse import place_index
from .palette_text import palette_contains_space
from .palette_fuzzy import (
    PaletteMatcher,
    FuzzyMatch,
    FuzzyRange,
    PALETTE_QUERY_BYTES,
)
from .tree_model import _Order, _ordered


struct PaletteCandidate(Copyable, Movable):
    var place: PalettePlace
    var level: UInt8
    var label: String
    var hits: List[FuzzyRange]
    var score: Int

    def __init__(
        out self,
        place: PalettePlace = PalettePlace(),
        level: UInt8 = 0,
        label: String = "",
        var hits: List[FuzzyRange] = List[FuzzyRange](),
        score: Int = 0,
    ):
        self.place = place.copy()
        self.level = level
        self.label = label
        self.hits = hits^
        self.score = score


def place_key(place: PalettePlace) -> String:
    return (
        (String(place.host.value()) if place.host else "-")
        + "/"
        + (String(place.session.value()) if place.session else "-")
        + "/"
        + (String(place.window.value()) if place.window else "-")
        + "/"
        + (String(place.pane.value()) if place.pane else "-")
    )


def _sort_candidates(
    var values: List[PaletteCandidate],
) -> List[PaletteCandidate]:
    var ranks = List[_Order]()
    for index in range(len(values)):
        ranks.append(
            _Order(
                UInt64(values[index].score) ^ (UInt64(1) << 63),
                UInt64(index),
                index,
            )
        )
    ranks = _ordered(ranks^)
    var sorted = List[PaletteCandidate](capacity=len(values))
    for rank in ranks:
        var item = PaletteCandidate()
        swap(item, values[rank.index])
        sorted.append(item^)
    return sorted^


def _distance(current: PalettePlace, place: PalettePlace) -> Int:
    if place.host != current.host:
        return 3
    if place.session and place.session != current.session:
        return 2
    return Int(Bool(place.window) and place.window != current.window)


def _rate(
    matcher: PaletteMatcher,
    query: String,
    label: String,
    metadata: String,
    path: String,
) raises -> Optional[FuzzyMatch]:
    var found = matcher.find(query, label)
    if found:
        return found^
    if query.byte_length() == 0:
        return None
    if palette_contains_space(query):
        found = matcher.find(query, path)
        if found:
            return FuzzyMatch(found.value().score, List[FuzzyRange]())
    if metadata.byte_length() == 0:
        return None
    found = matcher.find(query, metadata)
    if found:
        return FuzzyMatch(found.value().score + 1000, List[FuzzyRange]())
    return None


def _push(
    mut result: List[PaletteCandidate],
    place: PalettePlace,
    level: UInt8,
    label: String,
    var found: Optional[FuzzyMatch],
    current: PalettePlace,
    boosts: Dict[String, Int],
):
    if not found:
        return
    var key = place_key(place)
    var boost = boosts.get(key, 0)
    var matched = found.take()
    var hits = List[FuzzyRange]()
    swap(hits, matched.hits)
    result.append(
        PaletteCandidate(
            place,
            level,
            label,
            hits^,
            matched.score + 4 * _distance(current, place) - boost,
        )
    )


def _parent_session(
    fleet: PaletteFleet, scope: PalettePlace
) -> Optional[Tuple[UInt32, UInt64]]:
    if scope.host and scope.session:
        return (scope.host.value(), scope.session.value())
    var host = fleet.host_index(
        scope.host.value() if scope.host else fleet.here
    )
    if host and fleet.hosts[host.value()].current:
        return (
            fleet.hosts[host.value()].id,
            fleet.hosts[host.value()].current.value(),
        )
    return None


def _host_scope(id: UInt32, online: Bool, scope: PalettePlace) -> Bool:
    return id == scope.host.value() if scope.host else online


def _programs(window: PaletteFleetWindow) -> String:
    var result = String()
    var started = False
    for pane in window.panes:
        if pane.program:
            if started:
                result += " "
            result += pane.program.value()
            started = True
    return result^


def palette_candidates(
    fleet: PaletteFleet,
    scope: PalettePlace,
    level: UInt8,
    query: String,
    matcher: PaletteMatcher,
    boosts: Dict[String, Int] = Dict[String, Int](),
) raises -> List[PaletteCandidate]:
    fleet.validate()
    if level > PLACE_PANE or query.byte_length() > PALETTE_QUERY_BYTES:
        raise Error("palette candidate level or query exceeds its limit")
    var above = scope.above(level)
    var current = fleet.current()
    var index = place_index(query)
    var found = List[PaletteCandidate]()
    if level == PLACE_HOST:
        for host in fleet.hosts:
            _push(
                found,
                PalettePlace(host.id),
                level,
                host.name,
                matcher.find(query, host.name),
                current,
                boosts,
            )
    elif level == PLACE_SESSION:
        var exact = False
        if index:
            for host in fleet.hosts:
                if _host_scope(host.id, host.online, above):
                    for session in host.sessions:
                        exact = exact or session.name == query
        for host in fleet.hosts:
            if not _host_scope(host.id, host.online, above):
                continue
            for session in host.sessions:
                var place = PalettePlace(host.id, session.id)
                var scored: Optional[FuzzyMatch] = None
                if exact:
                    if session.name == query:
                        scored = FuzzyMatch(0, List[FuzzyRange]())
                else:
                    scored = _rate(
                        matcher, query, session.label, session.name, ""
                    )
                _push(
                    found, place, level, session.label, scored^, current, boosts
                )
    else:
        var parent: Optional[Tuple[UInt32, UInt64]] = None
        var parent_window: Optional[Tuple[UInt32, UInt64]] = None
        if level == PLACE_WINDOW:
            if above.session or index:
                parent = _parent_session(fleet, above)
        else:
            if above.window:
                if above.host and above.session:
                    parent_window = (above.host.value(), above.window.value())
            elif index:
                var session = _parent_session(fleet, above)
                if session:
                    var pair = session.value()
                    var at = fleet.session_index(PalettePlace(pair[0], pair[1]))
                    if at:
                        var host, record = at.value()
                        var active = (
                            fleet.hosts[host].sessions[record].active_window()
                        )
                        if active:
                            parent_window = (
                                pair[0],
                                fleet.hosts[host]
                                .sessions[record]
                                .windows[active.value()]
                                .id,
                            )
        for host in fleet.hosts:
            for session in host.sessions:
                for window in session.windows:
                    var in_scope: Bool
                    if level == PLACE_WINDOW:
                        in_scope = host.id == parent.value()[
                            0
                        ] and session.id == parent.value()[
                            1
                        ] if parent else _host_scope(
                            host.id, host.online, above
                        )
                    else:
                        in_scope = (
                            host.id == parent_window.value()[0]
                            and window.id
                            == parent_window.value()[
                                1
                            ] if parent_window else _host_scope(
                                host.id, host.online, above
                            )
                            and (
                                not above.session
                                or session.id == above.session.value()
                            )
                        )
                    if not in_scope:
                        continue
                    if level == PLACE_WINDOW:
                        var label = String(window.index) + " " + window.name
                        var scored: Optional[FuzzyMatch] = None
                        if index:
                            if window.index == index.value():
                                scored = FuzzyMatch(0, List[FuzzyRange]())
                        else:
                            scored = _rate(
                                matcher,
                                query,
                                label,
                                _programs(window),
                                session.label + " " + label,
                            )
                        _push(
                            found,
                            PalettePlace(host.id, session.id, window.id),
                            level,
                            label,
                            scored^,
                            current,
                            boosts,
                        )
                    else:
                        for pane in window.panes:
                            var label = String(pane.index) + " " + pane.name()
                            var scored: Optional[FuzzyMatch] = None
                            if index:
                                if pane.index == index.value():
                                    scored = FuzzyMatch(0, List[FuzzyRange]())
                            else:
                                var path = (
                                    session.label
                                    + " "
                                    + String(window.index)
                                    + " "
                                    + window.name
                                    + " "
                                    + label
                                )
                                scored = _rate(
                                    matcher,
                                    query,
                                    label,
                                    pane.program.value() if pane.program else String(),
                                    path,
                                )
                            _push(
                                found,
                                PalettePlace(
                                    host.id, session.id, window.id, pane.id
                                ),
                                level,
                                label,
                                scored^,
                                current,
                                boosts,
                            )
    return _sort_candidates(found^)


def palette_children(
    fleet: PaletteFleet,
    scope: PalettePlace,
    matcher: PaletteMatcher,
    boosts: Dict[String, Int] = Dict[String, Int](),
) raises -> List[PaletteCandidate]:
    var level = scope.level()
    if level == Optional[UInt8](PLACE_PANE):
        return List[PaletteCandidate]()
    return palette_candidates(
        fleet,
        scope,
        UInt8(level.value() + 1) if level else PLACE_HOST,
        "",
        matcher,
        boosts,
    )


def search_places(
    fleet: PaletteFleet,
    scope: PalettePlace,
    query: String,
    matcher: PaletteMatcher,
    boosts: Dict[String, Int] = Dict[String, Int](),
) raises -> List[PaletteCandidate]:
    var below = scope.level().value() if scope.level() else PLACE_HOST
    var result = List[PaletteCandidate]()
    for level in range(1, 4):
        if level > Int(below):
            var found = palette_candidates(
                fleet, scope, UInt8(level), query, matcher, boosts
            )
            for index in range(len(found)):
                var item = PaletteCandidate()
                swap(item, found[index])
                result.append(item^)
    return _sort_candidates(result^)
