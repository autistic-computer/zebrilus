from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import Ui, Style, MOTION_REDUCED
from mjui.input import MouseEvent, MOUSE_DOWN, MOUSE_UP
from mjui.widgets.penna import PennaTheme
from mjui.widgets.key import sub
from zzmojo.chooser_live import LiveChooser
from zzmojo.chooser_session import ChooserSource
from zzmojo.chooser_protocol import ChooseEvent, ChooseState, ChooseItem
from zzmojo.palette_dialog_state import PaletteDialogs


def main() raises:
    var ui = Ui(1)
    ui.native_scene = True
    ui.native_paths = True
    ui.motion = MOTION_REDUCED
    var regular = Int32(ui.add_font("bench/assets/fonts/Inter-Regular.ttf"))
    var bold = Int32(ui.add_font("bench/assets/fonts/Inter-SemiBold.ttf"))
    var mono = Int32(
        ui.add_font("bench/assets/fonts/JetBrainsMono-Regular.ttf")
    )
    var icons = Int32(ui.add_font("bench/assets/fonts/lucide.ttf"))
    var theme = PennaTheme(0, regular, bold, mono, icons)
    var dialogs = PaletteDialogs()
    var event = ChooseEvent()
    var state = ChooseState()
    state.kind = 1
    state.selected = 15
    for index in range(20):
        var item = ChooseItem()
        item.label = "pane " + String(index)
        item.target_kind = 2
        item.target = UInt64(index)
        state.items.append(item^)
    event.state = state^
    dialogs.apply_chooser(event^)
    var runtime = LiveChooser()
    ui.set_time(1)
    ui.begin_frame(1000, 700)
    ui.open(Style.col().width(1000).height(700))
    ui.close()
    var observed = runtime.prepare(
        ui, dialogs, ChooserSource(1, 2, 3), True, False, 0
    )
    assert_true(observed.owner_changed)
    assert_true(runtime.input_scope(ui))
    var effects = runtime.paint(ui, theme, bold)
    assert_equal(len(effects), 0)
    ui.end_frame()
    assert_true(ui.native_scene_complete)
    assert_equal(runtime.session.scroll, Float32(400))
    assert_equal(runtime.rows[0][1], UInt32(10))
    assert_equal(runtime.viewport.height, Float32(400))
    assert_equal(ui.caret_rect.x0, runtime.panel.x)
    assert_equal(ui.caret_rect.y0, runtime.panel.y)
    assert_equal(ui.caret_rect.x1, runtime.panel.x)
    assert_equal(ui.caret_rect.y1, runtime.panel.y)
    var rect = ui.rect_of(runtime.rows[0][0])
    var click = MouseEvent(
        MOUSE_DOWN, (rect.x0 + rect.x1) / 2, (rect.y0 + rect.y1) / 2, 1, 1, 0
    )
    var selected = runtime.pointer(ui, click)
    assert_true(Bool(selected))
    assert_equal(selected.value().action.kind, UInt8(9))
    assert_equal(selected.value().action.index, UInt32(10))
    click.clicks = 2
    selected = runtime.pointer(ui, click)
    assert_equal(selected.value().action.kind, UInt8(10))
    var close = ui.rect_of(sub(runtime.key, 2))
    click.x = (close.x0 + close.x1) / 2
    click.y = (close.y0 + close.y1) / 2
    assert_false(Bool(runtime.pointer(ui, click)))
    click.kind = MOUSE_UP
    selected = runtime.pointer(ui, click)
    assert_equal(selected.value().action.kind, UInt8(17))
    click.kind = MOUSE_DOWN
    click.x = 1
    click.y = 1
    selected = runtime.pointer(ui, click)
    assert_equal(selected.value().action.kind, UInt8(17))
    assert_true(runtime.wheel(ui, rect.x0 + 1, rect.y0 + 1, 0, 1, True))
    assert_equal(runtime.session.scroll, Float32(399))
    runtime.reset(ui)
    assert_false(runtime.opened())
    assert_false(ui.layer_is_open(runtime.layer))
    print(
        "Live chooser first layout, retained input, pointer actions and source"
        " scroll passed"
    )
