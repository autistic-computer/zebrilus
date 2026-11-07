from std.sys import argv
from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui, TextStyle
from mjui.input import (
    TextInput,
    CMD_PASTE,
    CMD_CANCEL,
    CMD_SELECT_ALL,
    CMD_DELETE_BACK,
    CMD_BACKTAB,
    CMD_TAB,
    CMD_NEWLINE,
    CMD_DOWN,
    CMD_RIGHT,
)
from mjui.widgets.penna import PennaTheme
from mjui.resources import resource_path
from mjui.testing import frame_pixels
from mjui.png import encode_png
from mjui.sys.file import file_write
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_uint
from zzmojo.palette_component import PaletteComponent, palette_kit_row
from zzmojo.palette_component_model import (
    palette_bounds,
    palette_component_usage,
    palette_enter_hint,
)
from zzmojo.palette_controller import PALETTE_INPUT_BYTES, PALETTE_EXECUTE
from zzmojo.palette_controller_model import PaletteModel
from zzmojo.palette_places_model import PaletteFleet, PalettePlace
from zzmojo.palette_presentation import PaletteDecorations, PaletteRow
from zzmojo.palette_shortcuts import PaletteShortcut
from zzmojo.palette_verbs import PaletteFilled
from zzmojo.search_clipboard import SearchClipboard
from palette_fleet_fixture import read_fleet


def repeated(count: Int) -> String:
    var bytes = List[UInt8](length=count, fill=UInt8(120))
    return String(unsafe_from_utf8=Span(bytes))


def begin(mut ui: Ui):
    ui.set_time(ui.time + 0.02)
    ui.begin_frame(1120, 720)


def events(
    mut component: PaletteComponent,
    mut ui: Ui,
    var input: TextInput,
    mut clip: SearchClipboard,
    fleet: PaletteFleet,
    decorations: PaletteDecorations,
) raises:
    begin(ui)
    _ = component.prepare(ui, input, clip, fleet, decorations)
    for event in input.events:
        _ = component.event(ui, input, event, clip, fleet, decorations)
    _ = component.build(
        ui, PennaTheme(4, 0, 1, 2, 3), fleet, decorations, 1120, 720
    )
    ui.end_frame()


def editor(
    mut component: PaletteComponent,
    mut ui: Ui,
    fleet: PaletteFleet,
    decorations: PaletteDecorations,
) raises:
    var clip = SearchClipboard()
    begin(ui)
    component.open(ui, fleet, decorations)
    assert_equal(component.model.controller.query, "")
    assert_equal(component.field.text(), "")
    assert_true(Bool(component.model.controller.selected))
    assert_equal(ui.focus, component.field_key)
    _ = component.build(
        ui, PennaTheme(4, 0, 1, 2, 3), fleet, decorations, 1120, 720
    )
    ui.end_frame()
    var input = TextInput()
    input.insert("a")
    input.command(CMD_PASTE)
    input.insert("b")
    clip.text = "P"
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(component.field.text(), "aPb")
    assert_equal(component.model.controller.query, "aPb")
    input = TextInput()
    input.command(CMD_SELECT_ALL)
    input.command(CMD_PASTE)
    clip.text = "λ\r\nβ"
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(component.field.text(), "λ β")
    input = TextInput()
    input.command(CMD_SELECT_ALL)
    input.insert("é", composed=True)
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(component.field.text(), "é")
    input = TextInput()
    input.command(CMD_SELECT_ALL)
    input.command(CMD_DELETE_BACK)
    events(component, ui, input^, clip, fleet, decorations)
    input = TextInput()
    input.set_marked(String("λ").unsafe_ptr(), 2, 0, 2)
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(component.field.text(), "")
    assert_equal(len(component.field.marked), 2)
    input = TextInput()
    input.insert("λ", composed=True)
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(component.field.text(), "λ")
    assert_equal(len(component.field.marked), 0)
    input = TextInput()
    input.command(CMD_SELECT_ALL)
    input.command(CMD_DELETE_BACK)
    input.insert("@local:%100")
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(component.field.text(), "")
    assert_equal(component.model.controller.scope().pane, Optional[UInt64](100))
    input = TextInput()
    input.command(CMD_BACKTAB)
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(
        component.model.controller.scope().window, Optional[UInt64](10)
    )
    assert_false(Bool(component.model.controller.scope().pane))
    input = TextInput()
    input.command(CMD_DELETE_BACK)
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(
        component.model.controller.scope().session, Optional[UInt64](0)
    )
    assert_false(Bool(component.model.controller.scope().window))
    begin(ui)
    component.open(ui, fleet, decorations, command=True)
    assert_equal(component.field.text(), ":")
    input = TextInput()
    input.insert("split-window -h")
    input.command(CMD_NEWLINE)
    var effect = UInt8(0)
    for event in input.events:
        var result = component.event(ui, input, event, clip, fleet, decorations)
        if result.effect.kind != 0:
            effect = result.effect.kind
            assert_equal(result.effect.command.value().name, "if-shell")
            assert_equal(result.effect.line, "split-window -h")
    assert_equal(effect, PALETTE_EXECUTE)
    assert_true(component.model.controller.finishing)
    component.close(ui)
    ui.end_frame()
    begin(ui)
    component.open(ui, fleet, decorations)
    input = TextInput()
    input.command(CMD_CANCEL)
    input.insert("after")
    assert_true(
        component.event(
            ui, input, input.events[0], clip, fleet, decorations
        ).closed
    )
    assert_false(
        component.event(
            ui, input, input.events[1], clip, fleet, decorations
        ).consumed
    )
    assert_equal(component.field.text(), "")
    ui.end_frame()


def bounds(
    mut component: PaletteComponent,
    mut ui: Ui,
    fleet: PaletteFleet,
    decorations: PaletteDecorations,
) raises:
    var clip = SearchClipboard()
    begin(ui)
    component.open(ui, fleet, decorations)
    var exact = repeated(PALETTE_INPUT_BYTES)
    assert_true(component.insert(exact.as_bytes()))
    assert_equal(len(component.field.buf), PALETTE_INPUT_BYTES)
    component.field.select(1, 3)
    var input = TextInput()
    input.command(CMD_PASTE)
    clip.text = repeated(6 * 1024 * 1024)
    var result = component.event(
        ui, input, input.events[0], clip, fleet, decorations
    )
    assert_true(result.limited)
    assert_equal(component.field.text(), exact)
    assert_equal(component.field.anchor, 1)
    assert_equal(component.field.caret, 3)
    clip.text = "λ"
    result = component.event(
        ui, input, input.events[0], clip, fleet, decorations
    )
    assert_false(result.limited)
    assert_equal(len(component.field.buf), PALETTE_INPUT_BYTES)
    component.field.select(1, 3)
    var before = component.field.text()
    clip.text = ""
    result = component.event(
        ui, input, input.events[0], clip, fleet, decorations
    )
    assert_false(result.limited)
    assert_equal(component.field.text(), before)
    assert_equal(component.field.selection(), (1, 3))
    component.field.select(0, len(component.field.buf))
    input = TextInput()
    input.set_marked(exact.unsafe_ptr(), exact.byte_length(), 0, 0)
    result = component.prepare(ui, input, clip, fleet, decorations)
    assert_false(result.limited)
    assert_equal(len(component.field.marked), PALETTE_INPUT_BYTES)
    input = TextInput()
    input.command(CMD_RIGHT)
    result = component.event(
        ui, input, input.events[0], clip, fleet, decorations
    )
    assert_true(result.limited)
    assert_equal(component.field.selection(), (0, PALETTE_INPUT_BYTES))
    var too_large = repeated(PALETTE_INPUT_BYTES + 1)
    input = TextInput()
    input.set_marked(too_large.unsafe_ptr(), too_large.byte_length(), 0, 0)
    result = component.prepare(ui, input, clip, fleet, decorations)
    assert_true(result.limited)
    assert_equal(len(component.field.marked), PALETTE_INPUT_BYTES)
    component.close(ui)
    ui.end_frame()
    var normal = palette_bounds(1120, 720, 100)
    assert_equal(normal.x, Float32(280))
    assert_equal(normal.width, Float32(560))
    assert_equal(normal.list_height, Float32(440))
    var narrow = palette_bounds(320, 100, 3)
    assert_equal(narrow.x, Float32(16))
    assert_equal(narrow.width, Float32(288))
    assert_equal(narrow.y, Float32(12))
    assert_equal(narrow.list_height, Float32(40))


def preview(fleet: PaletteFleet, decorations: PaletteDecorations) raises:
    var model = PaletteModel(browser=False)
    model.controller.start(fleet)
    model.rebuild(fleet, decorations)
    var rename = -1
    for index in range(len(model.verbs)):
        if model.verbs[index].name == "rename session":
            rename = index
    assert_true(rename >= 0)
    _ = model.controller.start_verb(model.verbs[rename], fleet)
    model.rebuild(fleet, decorations)
    assert_equal(
        palette_component_usage(model, fleet),
        Optional[String]("runs  rename-session"),
    )
    _ = model.controller.fill(
        PaletteFilled(0, PalettePlace(UInt32(0), UInt64(0)), ""), "work", fleet
    )
    model.controller.set_query("  new name  ")
    model.rebuild(fleet, decorations)
    assert_equal(
        palette_component_usage(model, fleet),
        Optional[String]("runs  rename-session -t work 'new name'"),
    )
    model.controller.start(fleet, True)
    model.controller.set_query(":splitw -h")
    model.rebuild(fleet, decorations)
    assert_true(Bool(palette_component_usage(model, fleet)))
    var row = PaletteRow()
    row.label = "λ row"
    var shortcut = PaletteShortcut()
    shortcut.kind = 1
    row.shortcut = shortcut^
    var kit = palette_kit_row(row)
    assert_true(Bool(kit.shortcut))
    assert_equal(kit.shortcut.value().kind, UInt8(1))
    assert_equal(kit.shortcut.value().text, "")
    row.icon = 'Asset("icons/openai.svg")'
    kit = palette_kit_row(row)
    assert_equal(kit.icon_name, "SquareTerminal")
    assert_equal(row.icon, 'Asset("icons/openai.svg")')
    assert_equal(palette_enter_hint(model), "choose")


def pointer(
    mut component: PaletteComponent,
    mut ui: Ui,
    fleet: PaletteFleet,
    decorations: PaletteDecorations,
) raises:
    begin(ui)
    ui.focus = 1234
    component.open(ui, fleet, decorations)
    _ = component.build(
        ui, PennaTheme(4, 0, 1, 2, 3), fleet, decorations, 1120, 720
    )
    ui.end_frame()
    var input = TextInput()
    input.insert("alpha λ beta")
    var clip = SearchClipboard()
    events(component, ui, input^, clip, fleet, decorations)
    var field = ui.rect_of(component.field_key)
    ui.pointer_down(field.x0 + 2, (field.y0 + field.y1) * 0.5, 3)
    input = TextInput()
    input.command(CMD_PASTE)
    clip.text = "P"
    events(component, ui, input^, clip, fleet, decorations)
    assert_equal(component.field.text(), "P")
    ui.pointer_up(field.x0 + 2, (field.y0 + field.y1) * 0.5)
    events(component, ui, TextInput(), clip, fleet, decorations)
    ui.pointer_down(4, 4, 1)
    begin(ui)
    input = TextInput()
    var result = component.prepare(ui, input, clip, fleet, decorations)
    assert_true(result.closed)
    assert_false(component.opened())
    assert_equal(ui.focus, UInt64(1234))
    assert_false(ui.interact(1234, focusable=True).pressed)
    ui.end_frame()
    ui.pointer_up(4, 4)


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var fleets = Int(read_uint(reader, 64))
    var fleet = PaletteFleet()
    for index in range(fleets):
        var next = read_fleet(reader)
        if index == 1:
            fleet = next^
    var decorations = PaletteDecorations()
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    var component = PaletteComponent(
        0x5000000000000000, TextStyle(0, 12, 0xFFFFFFFF, False)
    )
    editor(component, ui, fleet, decorations)
    bounds(component, ui, fleet, decorations)
    preview(fleet, decorations)
    pointer(component, ui, fleet, decorations)
    begin(ui)
    component.open(ui, fleet, decorations)
    var previous = component.model.controller.selected
    var input = TextInput()
    input.command(CMD_DOWN)
    var clip = SearchClipboard()
    _ = component.event(ui, input, input.events[0], clip, fleet, decorations)
    assert_true(component.model.controller.selected != previous)
    _ = component.build(
        ui, PennaTheme(4, 0, 1, 2, 3), fleet, decorations, 1120, 720
    )
    ui.end_frame()
    var pixels = frame_pixels(ui, PennaTheme(4, 0, 1, 2, 3).flat.bg)
    var png = encode_png(pixels.unsafe_ptr(), 1120, 720, 1120 * 4, 4)
    var output = file_write(args[2])
    output.write_bytes(Span(png))
    var rect = ui.rect_of(component.key)
    assert_true(rect.y1 - rect.y0 > 100)
    assert_equal(rect.x0, Float32(280))
    print(
        "palette component: ordered edits, IME, slots, preview, bounded"
        " replacement, focus and real retained layout passed"
    )
