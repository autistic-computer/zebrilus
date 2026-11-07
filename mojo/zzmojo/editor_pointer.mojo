from mjui.ui import Ui
from mjui.input import (
    TextInput,
    TextEvent,
    MouseEvent,
    Clipboard,
    MOUSE_DOWN,
    MOUSE_UP,
    MOUSE_MOVED,
    MOUSE_DRAGGED,
    MOUSE_RIGHT_DOWN,
    MOUSE_EXITED,
    MOUSE_RIGHT_DRAGGED,
    MOUSE_RIGHT_UP,
    MOUSE_MIDDLE_UP,
    MOUSE_MIDDLE_DRAGGED,
    TEXT_PRIMARY,
    TEXT_POINTER,
    MAX_PRIMARY_EVENTS,
)
from mjui.widgets.textedit import TextEdit, _OTHER


def editor_composition(mut field: TextEdit, mut ui: Ui, input: TextInput):
    var focused = ui.focus == field.key
    if field.focused != focused:
        field.focused = focused
        field.dirty = True
        field.blink_t0 = ui.time
    if not focused:
        if len(field.marked) > 0:
            field.marked.clear()
            field.dirty = True
        return
    ui.text_input = True
    var same = (
        len(field.marked) == len(input.marked)
        and field.marked_a == input.marked_sel_a
        and field.marked_b == input.marked_sel_b
    )
    if same:
        for index in range(len(field.marked)):
            if field.marked[index] != input.marked[index]:
                same = False
                break
    if not same:
        field.marked = input.marked.copy()
        field.marked_a = input.marked_sel_a
        field.marked_b = input.marked_sel_b
        field.follow = True
        field.dirty = True
        field.blink_t0 = ui.time


@fieldwise_init
struct EditorPrimaryResult(TrivialRegisterPassable):
    var consumed: Bool
    var limited: Bool
    var inserted: Bool


def editor_primary[
    Host: Clipboard
](
    mut field: TextEdit,
    mut ui: Ui,
    event: MouseEvent,
    mut host: Host,
    maximum: Int,
) raises -> EditorPrimaryResult:
    if ui.hit_test(event.x, event.y) != field.key:
        return EditorPrimaryResult(False, False, False)
    if not host.supports_primary():
        return EditorPrimaryResult(True, False, False)
    var position = ui.text_hit(field.key, event.x, event.y)
    if position.byte < 0:
        return EditorPrimaryResult(True, False, False)
    var remaining = maximum - len(field.buf)
    if remaining < 0:
        return EditorPrimaryResult(True, True, False)
    var text: String
    try:
        text = host.get_primary_selection_bounded(remaining)
    except:
        return EditorPrimaryResult(True, True, False)
    if text.byte_length() > remaining:
        return EditorPrimaryResult(True, True, False)
    ui.focus = field.key
    ui.text_input = True
    field.focused = True
    field.select(position.byte, position.byte)
    field.marked.clear()
    field.insert(text.unsafe_ptr(), text.byte_length(), _OTHER)
    field.blink_t0 = ui.time
    return EditorPrimaryResult(True, False, True)


struct EditorPointerScope(Movable):
    var position: SIMD[.float32, 4]
    var secondary: SIMD[.float32, 2]
    var middle: SIMD[.float32, 2]
    var hot: Tuple[UInt64, UInt64, UInt64, UInt64]
    var modifiers: UInt32
    var clicks: Int32
    var flags: UInt16

    def __init__(out self, ui: Ui):
        self.position = SIMD[.float32, 4](ui.mx, ui.my, ui.press_x, ui.press_y)
        self.secondary = SIMD[.float32, 2](ui.secondary_x, ui.secondary_y)
        self.middle = SIMD[.float32, 2](ui.middle_x, ui.middle_y)
        self.hot = (
            ui.hot,
            ui.press_hot,
            ui.middle_press_hot,
            ui.middle_release_hot,
        )
        self.modifiers = ui.mods
        self.clicks = ui.clicks
        self.flags = (
            UInt16(ui.down)
            | (UInt16(ui.pressed) << 1)
            | (UInt16(ui.released) << 2)
            | (UInt16(ui.secondary) << 3)
            | (UInt16(ui.middle) << 4)
            | (UInt16(ui.middle_released) << 5)
            | (UInt16(ui.pressing) << 6)
            | (UInt16(ui.pointer_in) << 7)
        )

    def apply(mut self, mut ui: Ui, event: MouseEvent):
        ui.mx = event.x
        ui.my = event.y
        ui.mods = event.mods
        ui.pressed = event.kind == MOUSE_DOWN
        ui.released = event.kind == MOUSE_UP
        ui.secondary = event.kind == MOUSE_RIGHT_DOWN
        ui.middle = False
        ui.middle_released = False
        ui.down = event.kind == MOUSE_DOWN or event.kind == MOUSE_DRAGGED
        ui.pointer_in = event.kind != MOUSE_EXITED
        ui.hot = ui.hit_test(event.x, event.y) if ui.pointer_in else UInt64(0)
        ui.press_hot = ui.hot if ui.pressed else UInt64(0)
        ui.pressing = ui.pressed or ui.sem_on or len(ui.layers.open) > 0
        ui.press_x = event.x
        ui.press_y = event.y
        ui.clicks = event.clicks
        ui.secondary_x = event.x
        ui.secondary_y = event.y

    def restore(self, mut ui: Ui):
        ui.mx = self.position[0]
        ui.my = self.position[1]
        ui.press_x = self.position[2]
        ui.press_y = self.position[3]
        ui.secondary_x = self.secondary[0]
        ui.secondary_y = self.secondary[1]
        ui.middle_x = self.middle[0]
        ui.middle_y = self.middle[1]
        ui.hot = self.hot[0]
        ui.press_hot = self.hot[1]
        ui.middle_press_hot = self.hot[2]
        ui.middle_release_hot = self.hot[3]
        ui.mods = self.modifiers
        ui.clicks = self.clicks
        ui.down = self.flags & 1 != 0
        ui.pressed = self.flags & 2 != 0
        ui.released = self.flags & 4 != 0
        ui.secondary = self.flags & 8 != 0
        ui.middle = self.flags & 16 != 0
        ui.middle_released = self.flags & 32 != 0
        ui.pressing = self.flags & 64 != 0
        ui.pointer_in = self.flags & 128 != 0


def pointer_payload(input: TextInput, event: TextEvent) raises -> MouseEvent:
    if event.start < 0 or event.start != event.end:
        raise Error("invalid indexed pointer range")
    var index = Int(event.start)
    if event.kind == TEXT_PRIMARY:
        if index >= len(input.primaries):
            raise Error("PRIMARY marker has no indexed pointer")
        var pointer = input.primaries[index]
        if (
            pointer.kind != MOUSE_DOWN
            and pointer.kind != MOUSE_MIDDLE_UP
            and pointer.kind != MOUSE_MIDDLE_DRAGGED
        ):
            raise Error("invalid PRIMARY pointer kind")
        return pointer
    if event.kind != TEXT_POINTER or index >= len(input.pointers):
        raise Error("pointer marker has no indexed pointer")
    var pointer = input.pointers[index]
    if pointer.kind > MOUSE_RIGHT_UP:
        raise Error("invalid ordinary pointer kind")
    return pointer


def validate_pointer_input(input: TextInput) raises:
    if (
        input.pointer_overflow
        or len(input.primaries) + len(input.pointers) > MAX_PRIMARY_EVENTS
    ):
        raise Error("pointer input frame exceeds 4096 ordered events")
    var primary = Int32(0)
    var pointer = Int32(0)
    for event in input.events:
        if event.kind != TEXT_PRIMARY and event.kind != TEXT_POINTER:
            continue
        _ = pointer_payload(input, event)
        if event.kind == TEXT_PRIMARY:
            if event.start != primary:
                raise Error(
                    "PRIMARY markers must reference each payload once in order"
                )
            primary += 1
        else:
            if event.start != pointer:
                raise Error(
                    "pointer markers must reference each payload once in order"
                )
            pointer += 1
    if Int(primary) != len(input.primaries) or Int(pointer) != len(
        input.pointers
    ):
        raise Error("indexed pointer payload has no ordered marker")
