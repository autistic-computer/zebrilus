from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from zzmojo.stream_transport import connect_interactive, InteractiveConnection
from zzmojo.state import AppModel
from zzmojo.palette_programs import PalettePrograms
from zzmojo.palette_snapshot import project_palette_snapshot
from zzmojo.palette_controller_model import PaletteModel
from zzmojo.palette_controller import (
    PaletteEffect,
    PALETTE_EXECUTE,
    PALETTE_ACTIVATE,
)
from zzmojo.palette_places_model import PalettePlace


def execute(
    mut connection: InteractiveConnection, effect: PaletteEffect
) raises:
    assert_equal(effect.kind, PALETTE_EXECUTE)
    assert_true(Bool(effect.command))
    ref command = effect.command.value()
    var args = List[String]()
    for argument in command.args:
        args.append(String(from_utf8=Span(argument)))
    var response = connection.command(command.name, Span(args))
    assert_equal(response.exit_code, 0)
    assert_equal(response.error, "")
    assert_equal(len(response.output), 0)


def wait_tree(
    mut model: AppModel,
    mut connection: InteractiveConnection,
    panes: Int,
    session_name: String = "",
    window_name: String = "",
) raises:
    for _ in range(200):
        model.refresh(connection)
        var session_ok = session_name.byte_length() == 0
        var window_ok = window_name.byte_length() == 0
        for session in model.snapshot.sessions:
            session_ok = session_ok or session.name == session_name
            for window in session.windows:
                window_ok = window_ok or window.name == window_name
        if len(model.panes) == panes and session_ok and window_ok:
            return
        _ = external_call["usleep", Int32](UInt32(10000))
    raise Error("palette live tree did not converge")


def choose(mut palette: PaletteModel, label: String) raises:
    for index in range(len(palette.rows)):
        if (
            palette.rows[index].action == 2
            and palette.rows[index].label == label
        ):
            palette.controller.selected = index
            return
    raise Error("palette verb is missing")


def main() raises:
    var args = argv()
    var connection = connect_interactive(args[1], "")
    var model = AppModel()
    model.refresh(connection)
    model.sync_focus(connection)
    wait_tree(model, connection, 1)
    assert_true(Bool(model.session) and Bool(model.window) and Bool(model.pane))
    var session = model.session.value()
    var window = model.window.value()
    var programs = PalettePrograms()
    var projection = project_palette_snapshot(
        model.snapshot, model.session, "localhost", programs
    )
    var palette = PaletteModel(False, False)
    palette.controller.start(projection.fleet)
    palette.controller.set_query("split right")
    palette.rebuild(projection.fleet, projection.decorations)
    choose(palette, "split right")
    var effect = palette.activate(False, projection.fleet)
    assert_equal(effect.kind, 0)
    assert_equal(palette.controller.next_slot().value().kind, 0)
    palette.rebuild(projection.fleet, projection.decorations)
    assert_equal(palette.rows[palette.controller.selected.value()].action, 5)
    effect = palette.activate(False, projection.fleet)
    execute(connection, effect)
    wait_tree(model, connection, 2)
    assert_false(Bool(model.command_output.viewport))
    projection = project_palette_snapshot(
        model.snapshot, model.session, "localhost", programs
    )
    palette.controller.start(projection.fleet)
    effect = palette.controller.enter_place(
        PalettePlace(UInt32(0), session), projection.fleet
    )
    palette.controller.set_query("rename session")
    palette.rebuild(projection.fleet, projection.decorations)
    choose(palette, "rename session")
    effect = palette.activate(False, projection.fleet)
    assert_equal(palette.controller.next_slot().value().kind, 1)
    palette.controller.set_query("Palette Renamed")
    palette.rebuild(projection.fleet, projection.decorations)
    effect = palette.activate(False, projection.fleet)
    assert_equal(effect.command.value().name, "rename-session")
    execute(connection, effect)
    wait_tree(model, connection, 2, "Palette Renamed")
    projection = project_palette_snapshot(
        model.snapshot, model.session, "localhost", programs
    )
    palette.controller.start(projection.fleet)
    palette.controller.set_query(
        ":rename-window -t @" + String(window) + " 'palette raw'"
    )
    palette.rebuild(projection.fleet, projection.decorations)
    effect = palette.activate(False, projection.fleet)
    assert_equal(effect.command.value().name, "if-shell")
    execute(connection, effect)
    wait_tree(model, connection, 2, "Palette Renamed", "palette raw")
    var options: List[String] = [
        "-s",
        "command-alias[42]",
        "palette-r=rename-window -t @" + String(window),
    ]
    var response = connection.command("set-option", Span(options))
    assert_equal(response.exit_code, 0)
    for _ in range(200):
        model.refresh(connection)
        if (
            20 in model.routing.options
            and "palette-r=" in model.routing.options[20].value
        ):
            break
        _ = external_call["usleep", Int32](UInt32(10000))
    assert_true(20 in model.routing.options)
    var aliases = model.routing.options[20].value.copy()
    assert_true("palette-r=" in aliases)
    projection = project_palette_snapshot(
        model.snapshot, model.session, "localhost", programs
    )
    var alias_palette = PaletteModel(False, False, aliases)
    alias_palette.controller.start(projection.fleet)
    alias_palette.controller.set_query("palette-r")
    alias_palette.rebuild(projection.fleet, projection.decorations)
    choose(alias_palette, "palette-r")
    effect = alias_palette.activate(False, projection.fleet)
    assert_equal(alias_palette.controller.next_slot().value().kind, 1)
    alias_palette.controller.set_query("alias name")
    alias_palette.rebuild(projection.fleet, projection.decorations)
    effect = alias_palette.activate(False, projection.fleet)
    assert_equal(effect.command.value().name, "palette-r")
    execute(connection, effect)
    wait_tree(model, connection, 2, "Palette Renamed", "alias name")
    alias_palette.controller.start(projection.fleet)
    alias_palette.controller.set_query(":palette-r 'raw alias'")
    alias_palette.rebuild(projection.fleet, projection.decorations)
    effect = alias_palette.activate(False, projection.fleet)
    assert_equal(effect.command.value().name, "if-shell")
    execute(connection, effect)
    wait_tree(model, connection, 2, "Palette Renamed", "raw alias")
    var create: List[String] = [
        "-d",
        "-t",
        "$" + String(session),
        "-n",
        "palette-other",
    ]
    response = connection.command("new-window", Span(create))
    assert_equal(response.exit_code, 0)
    wait_tree(model, connection, 3)
    var other: Optional[UInt64] = None
    for record in model.snapshot.sessions:
        for item in record.windows:
            if item.name == "palette-other":
                other = item.id
    assert_true(Bool(other))
    projection = project_palette_snapshot(
        model.snapshot, model.session, "localhost", programs
    )
    palette.controller.start(projection.fleet)
    var target = PalettePlace(UInt32(0), session, other)
    effect = palette.controller.activate_place(target, projection.fleet)
    assert_equal(effect.kind, PALETTE_ACTIVATE)
    assert_equal(effect.place.window, other)
    model.select_window(other.value())
    model.sync_focus(connection)
    assert_equal(connection.model.view.focused_window, other)
    effect = palette.controller.activated(target, True, projection.fleet)
    assert_false(palette.controller.opened)
    assert_false(Bool(model.command_output.viewport))
    print(
        "palette native split, rename, daemon-context parsing, aliases and"
        " activation passed"
    )
