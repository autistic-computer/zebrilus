from .palette_controller_model import PaletteModel
from .palette_places_model import PaletteFleet, PalettePlace, PLACE_HOST
from .palette_presentation import PaletteRow, PaletteHint, palette_place_hints
from .palette_verbs import (
    PaletteVerb,
    PaletteFilled,
    compile_palette_verb,
    palette_go_to,
)
from .palette_text import palette_trim, palette_words


@fieldwise_init
struct PaletteBounds(TrivialRegisterPassable):
    var x: Float32
    var y: Float32
    var width: Float32
    var list_height: Float32


def palette_bounds(width: Float32, height: Float32, rows: Int) -> PaletteBounds:
    var surface_width = min(Float32(560), max(width - 32, Float32(0)))
    var limit = (height * 0.88 - 175).clamp(40, 440)
    return PaletteBounds(
        (width - surface_width) * 0.5,
        height * 0.12,
        surface_width,
        min(Float32(rows) * 28, limit),
    )


def palette_selected(model: PaletteModel) -> Optional[Int]:
    var selected = model.controller.selected
    if (
        selected
        and selected.value() >= 0
        and selected.value() < len(model.rows)
    ):
        return selected
    return None


def palette_enter_hint(model: PaletteModel) -> String:
    var selected = palette_selected(model)
    if not selected:
        return String("choose")
    var action = model.rows[selected.value()].action
    return String(
        "go" if action
        == 1 else "choose" if action
        == 2 else "run command" if action
        == 3
        or action
        == 4 else "pick" if action
        == 5 else "run" if action
        == 6 else "choose"
    )


def palette_component_hints(model: PaletteModel) -> List[PaletteHint]:
    var selected = palette_selected(model)
    var row: Optional[PaletteRow] = None
    if selected:
        row = model.rows[selected.value()].copy()
    return palette_place_hints(model.controller, row, palette_enter_hint(model))


def _verb_usage(
    verb: PaletteVerb, values: List[PaletteFilled], fleet: PaletteFleet
) raises -> String:
    if verb.kind == 1:
        return String("gui · no tmux command")
    if verb.kind == 2:
        return "zz · " + verb.description
    var compiled = compile_palette_verb(verb, values, fleet)
    return "runs  " + (compiled.value().display if compiled else verb.spelled())


def _command_usage(model: PaletteModel, name: String) -> Optional[String]:
    for command in model.commands.commands:
        if command.name == name:
            return command.usage.copy()
        for alias in palette_words(command.aliases):
            if alias == name:
                return command.usage.copy()
    return None


def palette_component_usage(
    model: PaletteModel, fleet: PaletteFleet
) raises -> Optional[String]:
    var selected = palette_selected(model)
    var raw = model.controller.raw_command()
    if raw:
        if selected and model.rows[selected.value()].action == 3:
            return _command_usage(model, model.rows[selected.value()].command)
        var words = palette_words(raw.value())
        return _command_usage(model, words[0]) if len(words) > 0 else Optional[
            String
        ](None)
    if model.controller.draft:
        ref draft = model.controller.draft.value()
        var values = draft.filled.copy()
        var slot = draft.next()
        if (
            slot
            and slot.value().kind == 0
            and selected
            and model.rows[selected.value()].action == 5
        ):
            values.append(
                PaletteFilled(
                    0, model.rows[selected.value()].place.copy(), String()
                )
            )
        elif slot and slot.value().kind == 1:
            var text = palette_trim(model.controller.query)
            if text.byte_length() > 0:
                values.append(PaletteFilled(1, PalettePlace(), text^))
        return _verb_usage(draft.verb, values, fleet)
    if not selected:
        return None
    ref row = model.rows[selected.value()]
    if row.action == 2 and row.index >= 0 and row.index < len(model.verbs):
        ref verb = model.verbs[row.index]
        var values = List[PaletteFilled]()
        var scope = model.controller.scope()
        if (
            len(verb.slots) > 0
            and verb.slots[0].kind == 0
            and scope.level()
            and scope.level().value() > PLACE_HOST
        ):
            values.append(PaletteFilled(0, scope^, String()))
        return _verb_usage(verb, values, fleet)
    if row.action == 1:
        var line = palette_go_to(fleet, row.place)
        return "runs  " + line.value() if line else Optional[String](None)
    return None
