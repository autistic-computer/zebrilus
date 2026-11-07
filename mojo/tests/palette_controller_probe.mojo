from std.sys import argv
from std.testing import assert_equal, assert_true, assert_false
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_uint
from zzmojo.palette_places_model import (
    PaletteFleet,
    PaletteFleetHost,
    PaletteFleetSession,
    PaletteFleetWindow,
    PaletteFleetPane,
    PalettePlace,
    MAX_FLEET_NODES,
    MAX_FLEET_BYTES,
)
from zzmojo.palette_places_parse import parse_palette_line, read_written_place
from zzmojo.palette_commands import PaletteCommands
from zzmojo.palette_controller import (
    PaletteController,
    PALETTE_INPUT_BYTES,
    PALETTE_EXECUTE,
    PALETTE_CONNECT,
    PALETTE_NOTIFY,
)
from zzmojo.palette_verbs import (
    PaletteFilled,
    PaletteGuiVerb,
    palette_verbs,
    palette_command_words,
    MAX_PALETTE_GUI_BYTES,
    MAX_PALETTE_WORDS,
)
from zzmojo.palette_presentation import (
    PaletteRow,
    PaletteDecorations,
    palette_section,
    MAX_PALETTE_PRESENTATION_BYTES,
    palette_place_pills,
    palette_place_hints,
    palette_level_header,
)
from zzmojo.palette_fuzzy import PALETTE_QUERY_BYTES
from zzmojo.palette_controller_model import PaletteModel
from palette_fleet_fixture import read_fleet


def repeated(count: Int) raises -> String:
    var bytes = List[UInt8](capacity=count)
    bytes.resize(count, UInt8(120))
    return String(from_utf8=Span(bytes))


def transitions(fleet: PaletteFleet) raises:
    var controller = PaletteController()
    controller.start(fleet)
    assert_true(controller.opened)
    assert_equal(controller.query, "")
    assert_equal(len(controller.chips), 1)
    assert_true(controller.chips[0].automatic)
    assert_equal(controller.chips[0].label, "local")
    assert_equal(controller.scope().host, Optional[UInt32](0))
    assert_equal(controller.placeholder(), "Type a place or an action")
    var decorations = PaletteDecorations()
    var pills = palette_place_pills(controller, decorations)
    assert_equal(len(pills), 1)
    assert_equal(pills[0].kind, 1)
    assert_equal(pills[0].status, Optional[UInt8](3))
    var hints = palette_place_hints(controller, None, "choose")
    assert_equal(len(hints), 3)
    assert_equal(hints[1].key, "backspace")
    assert_true(controller.commit_typed("@local:%100", fleet))
    assert_equal(len(controller.chips), 4)
    assert_false(controller.chips[0].automatic)
    assert_equal(controller.scope().pane, Optional[UInt64](100))
    assert_false(controller.commit_typed("@local:~/path", fleet))
    assert_true(controller.backspace())
    assert_equal(controller.scope().window, Optional[UInt64](10))
    assert_false(Bool(controller.scope().pane))
    var popped = controller.step_back()
    assert_true(popped.handled)
    assert_equal(popped.highlight.value().window, Optional[UInt64](10))
    assert_equal(controller.scope().session, Optional[UInt64](0))
    controller.set_query(":select-pane ")
    assert_equal(controller.raw_command().value(), "select-pane ")
    assert_false(controller.backspace())
    var commands = PaletteCommands()
    var verbs = palette_verbs(commands, True, True, True)
    var rename = -1
    var split = -1
    for index in range(len(verbs)):
        if verbs[index].name == "rename session":
            rename = index
        if verbs[index].name == "split right":
            split = index
    assert_true(rename >= 0 and split >= 0)
    var effect = controller.start_verb(verbs[rename], fleet)
    assert_equal(effect.kind, 0)
    assert_equal(controller.placeholder(), "New name")
    assert_true(Bool(controller.draft))
    assert_equal(len(controller.draft.value().filled), 1)
    assert_equal(controller.draft.value().labels[0], "")
    assert_true(controller.backspace())
    assert_false(Bool(controller.draft))
    assert_equal(controller.query, "")
    controller.start(fleet)
    effect = controller.start_verb(verbs[rename], fleet)
    assert_equal(controller.placeholder(), "Pick a session")
    effect = controller.fill(
        PaletteFilled(0, PalettePlace(UInt32(0), UInt64(0)), ""),
        "Work µ",
        fleet,
    )
    assert_equal(controller.placeholder(), "New name")
    popped = controller.step_back()
    assert_true(Bool(controller.draft))
    assert_equal(len(controller.draft.value().filled), 0)
    assert_equal(controller.query, "")
    effect = controller.fill(
        PaletteFilled(0, PalettePlace(UInt32(0), UInt64(0)), ""),
        "Work µ",
        fleet,
    )
    effect = controller.fill(
        PaletteFilled(1, PalettePlace(), "fresh name"), "fresh name", fleet
    )
    assert_equal(effect.kind, PALETTE_EXECUTE)
    assert_equal(effect.command.value().name, "rename-session")
    assert_equal(effect.message, "Ran rename-session")
    assert_equal(effect.line, "rename-session -t '$0' 'fresh name'")
    assert_equal(effect.display, "rename-session -t work 'fresh name'")
    assert_equal(len(effect.command.value().args), 3)
    assert_equal(
        String(from_utf8=Span(effect.command.value().args[2])), "fresh name"
    )
    assert_true(controller.finishing)
    controller.start(fleet)
    effect = controller.enter_place(
        PalettePlace(UInt32(0), UInt64(0), UInt64(10), UInt64(100)), fleet
    )
    effect = controller.start_verb(verbs[split], fleet)
    assert_equal(effect.kind, PALETTE_EXECUTE)
    assert_equal(effect.line, "split-window -h -t %100")
    assert_equal(effect.command.value().name, "split-window")
    controller.start(fleet, True)
    assert_equal(controller.query, ":")
    effect = controller.enter_place(
        PalettePlace(UInt32(0), UInt64(0), UInt64(10), UInt64(100)), fleet
    )
    assert_true(controller.chips[0].automatic)
    effect = controller.run_line("select-pane -Z", None, fleet)
    assert_equal(effect.command.value().name, "if-shell")
    assert_equal(
        String(from_utf8=Span(effect.command.value().args[2])),
        "select-pane -t %100 -Z",
    )
    effect = controller.enter_place(PalettePlace(UInt32(1)), fleet)
    assert_equal(effect.kind, PALETTE_CONNECT)
    effect = controller.run_line("display-message test", None, fleet)
    assert_equal(effect.kind, PALETTE_NOTIFY)
    assert_equal(
        effect.message, "Connect to this host before running a command"
    )
    assert_equal(
        palette_level_header(fleet, 2, PalettePlace(UInt32(0), UInt64(0))),
        "Windows in Work µ",
    )
    assert_equal(
        palette_level_header(
            fleet, 3, PalettePlace(UInt32(0), UInt64(0), UInt64(10))
        ),
        "Panes in 2 editor",
    )


def model_actions(fleet: PaletteFleet) raises:
    var model = PaletteModel(True, True)
    var display = PaletteDecorations()
    model.controller.start(fleet)
    model.rebuild(fleet, display)
    model.navigate(1)
    assert_true(model.controller.navigation_engaged)
    assert_true(Bool(model.controller.selected))
    var effect = model.controller.enter_place(
        PalettePlace(UInt32(0), UInt64(0), UInt64(10), UInt64(100)), fleet
    )
    model.rebuild(fleet, display)
    assert_true(model.leave(False, fleet, display))
    assert_equal(model.controller.scope().window, Optional[UInt64](10))
    assert_true(model.controller.navigation_engaged)
    assert_equal(
        model.rows[model.controller.selected.value()].place.pane,
        Optional[UInt64](100),
    )
    assert_true(model.leave(True, fleet, display))
    assert_equal(model.controller.scope().session, Optional[UInt64](0))
    assert_false(Bool(model.controller.scope().window))
    model.controller.set_query("split right")
    model.rebuild(fleet, display)
    var selected = -1
    for index in range(len(model.rows)):
        if (
            model.rows[index].action == 2
            and model.rows[index].label == "split right"
        ):
            selected = index
    assert_true(selected >= 0)
    model.controller.selected = selected
    effect = model.activate(False, fleet)
    assert_equal(effect.kind, PALETTE_EXECUTE)
    assert_equal(effect.line, "split-window -h -t '$0'")
    model.controller.start(fleet)
    model.controller.set_query(":select-pane -Z")
    model.rebuild(fleet, display)
    effect = model.activate(False, fleet)
    assert_equal(effect.command.value().name, "if-shell")
    assert_equal(
        String(from_utf8=Span(effect.command.value().args[2])), "select-pane -Z"
    )
    model.controller.set_query(":select-pane")
    model.rebuild(fleet, display)
    effect = model.activate(True, fleet)
    assert_equal(effect.kind, 0)
    assert_equal(model.controller.query, ":select-pane ")
    model.controller.set_query(":rename-session")
    model.rebuild(fleet, display)
    effect = model.activate(False, fleet)
    assert_equal(effect.kind, 0)
    assert_equal(model.controller.query, ":rename-session ")
    var target = PalettePlace(UInt32(0), UInt64(0), UInt64(10), UInt64(100))
    effect = model.controller.activate_place(target, fleet)
    assert_equal(effect.place.pane, Optional[UInt64](100))
    assert_false(effect.connect)
    effect = model.controller.activated(target, True, fleet)
    assert_false(model.controller.opened)
    assert_equal(effect.message, "Switched to 0 λShell")


def bounds(fleet: PaletteFleet) raises:
    var controller = PaletteController()
    controller.set_query(repeated(PALETTE_INPUT_BYTES))
    var refused = False
    try:
        controller.set_query(repeated(PALETTE_INPUT_BYTES + 1))
    except:
        refused = True
    assert_true(refused)
    var line = parse_palette_line(repeated(PALETTE_QUERY_BYTES))
    assert_equal(line.search.value().byte_length(), PALETTE_QUERY_BYTES)
    refused = False
    try:
        _ = parse_palette_line(repeated(PALETTE_QUERY_BYTES + 1))
    except:
        refused = True
    assert_true(refused)
    refused = False
    try:
        var written = read_written_place(
            repeated(PALETTE_QUERY_BYTES + 1), fleet
        )
    except:
        refused = True
    assert_true(refused)
    var nodes = PaletteFleet()
    for index in range(MAX_FLEET_NODES):
        nodes.hosts.append(
            PaletteFleetHost(
                UInt32(index), "", False, None, List[PaletteFleetSession]()
            )
        )
    nodes.validate()
    nodes.hosts.append(
        PaletteFleetHost(0, "", False, None, List[PaletteFleetSession]())
    )
    refused = False
    try:
        nodes.validate()
    except:
        refused = True
    assert_true(refused)
    nodes.hosts.clear()
    nodes.hosts.append(
        PaletteFleetHost(
            0,
            repeated(MAX_FLEET_BYTES),
            False,
            None,
            List[PaletteFleetSession](),
        )
    )
    nodes.validate()
    nodes.hosts[0].name += "x"
    refused = False
    try:
        nodes.validate()
    except:
        refused = True
    assert_true(refused)
    var words = String()
    for _ in range(MAX_PALETTE_WORDS):
        words += "x "
    assert_equal(len(palette_command_words(words)), MAX_PALETTE_WORDS)
    refused = False
    try:
        _ = palette_command_words(words + "x")
    except:
        refused = True
    assert_true(refused)
    var commands = PaletteCommands()
    var gui = List[PaletteGuiVerb]()
    gui.append(PaletteGuiVerb(repeated(MAX_PALETTE_GUI_BYTES), "", "", ""))
    var verbs = palette_verbs(commands, True, True, True, "", gui)
    assert_true(len(verbs) > len(commands.commands))
    gui[0].id += "x"
    refused = False
    try:
        _ = palette_verbs(commands, True, True, True, "", gui)
    except:
        refused = True
    assert_true(refused)
    var rows = List[PaletteRow]()
    var entries = List[PaletteRow]()
    var row = PaletteRow()
    row.label = repeated(MAX_PALETTE_PRESENTATION_BYTES - 1)
    entries.append(row^)
    palette_section(rows, "H", entries.copy())
    assert_equal(len(rows), 2)
    rows.clear()
    entries[0].label += "x"
    refused = False
    try:
        palette_section(rows, "H", entries^)
    except:
        refused = True
    assert_true(refused)
    rows.clear()
    entries = List[PaletteRow](length=65535, fill=PaletteRow())
    palette_section(rows, "", entries.copy())
    assert_equal(len(rows), 65536)
    rows.clear()
    entries.append(PaletteRow())
    refused = False
    try:
        palette_section(rows, "", entries^)
    except:
        refused = True
    assert_true(refused)
    var repeated_fleet = PaletteFleet()
    var windows = List[PaletteFleetWindow]()
    for index in range(200):
        var panes = List[PaletteFleetPane]()
        panes.append(
            PaletteFleetPane(UInt64(index), 0, "", None, "", True, True)
        )
        windows.append(
            PaletteFleetWindow(
                UInt64(index), UInt32(index), "", False, UInt64(index), panes^
            )
        )
    var sessions = List[PaletteFleetSession]()
    sessions.append(
        PaletteFleetSession(0, "work", repeated(1024 * 1024), windows^)
    )
    repeated_fleet.hosts.append(
        PaletteFleetHost(0, "local", True, UInt64(0), sessions^)
    )
    var model = PaletteModel()
    refused = False
    try:
        model.rebuild(repeated_fleet, PaletteDecorations())
    except:
        refused = True
    assert_true(refused)
    assert_equal(len(model.rows), 0)


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var count = Int(read_uint(reader, 64))
    var fleets = List[PaletteFleet]()
    for _ in range(count):
        fleets.append(read_fleet(reader))
    assert_equal(reader.uint(), 0)
    reader.finish()
    transitions(fleets[1])
    model_actions(fleets[1])
    bounds(fleets[1])
    print("palette controller transitions and exact input/fleet bounds passed")
