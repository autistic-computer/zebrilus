from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui, Style, TextStyle, MOTION_REDUCED
from mjui.input import (
    TextInput,
    TextEvent,
    MouseEvent,
    TEXT_INSERT,
    TEXT_COMMAND,
    MOUSE_DOWN,
    MOUSE_UP,
    CMD_UP,
    CMD_NEWLINE,
    CMD_PASTE,
)
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_palette import (
    PaletteRow,
    PaletteRowResponse,
    palette_tree_row,
)
from zzmojo.window_chooser_component import WindowChooserComponent
from zzmojo.window_chooser_controller import (
    WindowChooserController,
    WindowChooserOperation,
    WindowChooserEffects,
)
from zzmojo.chooser_protocol import ChooseState, ChooseItem
from zzmojo.palette_dialog_state import ChooseSnapshot
from zzmojo.search_clipboard import SearchClipboard


@inline(.never)
def disclosure_frame(
    mut ui: Ui, theme: PennaTheme
) raises -> PaletteRowResponse:
    ui.begin_frame(300, 100)
    ui.open(Style.col().width(300).height(100))
    var row = PaletteRow("session")
    row.expanded = True
    var result = palette_tree_row(ui, theme, 31, row, False, 280, 32)
    ui.close()
    ui.end_frame()
    return result


def main() raises:
    var ui = Ui(1)
    ui.motion = MOTION_REDUCED
    ui.native_scene = True
    ui.native_paths = True
    var regular = Int32(ui.add_font("bench/assets/fonts/Inter-Regular.ttf"))
    var bold = Int32(ui.add_font("bench/assets/fonts/Inter-SemiBold.ttf"))
    var mono = Int32(
        ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    )
    var icons = Int32(ui.add_font("bench/assets/fonts/lucide.ttf"))
    var theme = PennaTheme(4, regular, bold, mono, icons)
    var state = ChooseState()
    for index in range(30):
        var item = ChooseItem()
        item.target = UInt64(index)
        item.label = "session " + String(index)
        item.flags = 3
        state.items.append(item^)
    var controller = WindowChooserController()
    var operation = WindowChooserOperation(0)
    operation.snapshot = ChooseSnapshot(state^)
    operation.revision = 1
    var change = controller.prepare(operation)
    controller.commit(change^)
    var component = WindowChooserComponent(
        0x7800000000000000, TextStyle(regular, 12, 0xFFFFFFFF, False)
    )
    ui.set_time(1)
    ui.begin_frame(1000, 700)
    ui.open(Style.col().width(1000).height(700))
    ui.close()
    component.open(ui, 1)
    assert_true(component.input_scope(ui))
    component.paint(ui, theme, controller.data)
    ui.end_frame()
    assert_true(ui.native_scene_complete)
    assert_equal(component.bounds.list_height, Float32(440))
    assert_equal(len(component.visible), 17)
    var disclosure = ui.rect_of(component.disclosure_key)
    assert_true(disclosure.x1 > disclosure.x0)
    var press = MouseEvent(
        MOUSE_DOWN,
        (disclosure.x0 + disclosure.x1) / 2,
        (disclosure.y0 + disclosure.y1) / 2,
        1,
        1,
        0,
    )
    assert_equal(ui.hit_test(press.x, press.y), component.disclosure_key)
    assert_equal(len(component.pointer(ui, press, controller.data)), 0)
    press.kind = MOUSE_UP
    var operations = component.pointer(ui, press, controller.data)
    assert_equal(len(operations), 2)
    assert_equal(operations[0].kind, UInt8(5))
    assert_equal(operations[1].kind, UInt8(3))
    assert_false(operations[1].flag)
    var row = ui.rect_of(component.row_key + 1)
    press.kind = MOUSE_DOWN
    press.x = row.x1 - 30
    press.y = (row.y0 + row.y1) / 2
    assert_equal(len(component.pointer(ui, press, controller.data)), 0)
    press.kind = MOUSE_UP
    operations = component.pointer(ui, press, controller.data)
    assert_equal(operations[1].kind, UInt8(4))
    assert_true(component.wheel(ui, press.x, press.y, 0, -1, True, 30))
    assert_equal(component.scroll, Float32(1))
    var input = TextInput()
    input.insert("session 2")
    var clip = SearchClipboard()
    var result = component.event(
        ui, input, input.events[0], clip, controller.data
    )
    assert_equal(len(result.operations), 1)
    assert_equal(result.operations[0].text, "session 2")
    change = controller.prepare(result.operations[0])
    controller.commit(change^)
    assert_equal(controller.data.query, "session 2")
    component.synchronize(controller.data, WindowChooserEffects())
    clip.text = String("x") * 65536
    result = component.event(
        ui,
        input,
        TextEvent(TEXT_COMMAND, CMD_PASTE, 0, 0, 0),
        clip,
        controller.data,
    )
    assert_true(result.limited)
    assert_equal(component.field.text(), "session 2")
    result = component.event(
        ui,
        input,
        TextEvent(TEXT_COMMAND, CMD_UP, 0, 0, 0),
        clip,
        controller.data,
    )
    assert_equal(result.operations[0].kind, UInt8(2))
    result = component.event(
        ui,
        input,
        TextEvent(TEXT_COMMAND, CMD_NEWLINE, 0, 0, 0),
        clip,
        controller.data,
    )
    assert_equal(result.operations[0].kind, UInt8(4))
    component.close(ui)
    assert_false(component.input_scope(ui))
    ui.focus = 99
    _ = disclosure_frame(ui, theme)
    disclosure = ui.rect_of(32)
    var x = (disclosure.x0 + disclosure.x1) / 2
    var y = (disclosure.y0 + disclosure.y1) / 2
    ui.pointer_down(x, y, 1)
    var response = disclosure_frame(ui, theme)
    assert_false(response.clicked)
    assert_false(response.toggled)
    assert_equal(ui.active, UInt64(32))
    assert_equal(ui.focus, UInt64(99))
    ui.pointer_up(x, y)
    response = disclosure_frame(ui, theme)
    assert_false(response.clicked)
    assert_true(response.toggled)
    assert_equal(ui.focus, UInt64(99))
    ui.pointer_down(200, y, 1)
    response = disclosure_frame(ui, theme)
    assert_false(response.clicked)
    assert_false(response.toggled)
    ui.pointer_up(200, y)
    response = disclosure_frame(ui, theme)
    assert_true(response.clicked)
    assert_false(response.toggled)
    print(
        "Window chooser retained editor, disclosure, pointer and bounded input"
        " passed"
    )
