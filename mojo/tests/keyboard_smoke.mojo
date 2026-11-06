from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true
from mjui.keys import Key
from mjui.input import KeyEvent, TextInput, TEXT_INSERT, MOD_CTRL, MOD_SHIFT
from zzmojo.state import AppModel, terminal_text
from zzmojo.stream_transport import InteractiveConnection, connect_interactive
from zzmojo.input_protocol import input_key
from zzmojo.stream_protocol import input_text
from zzmojo.keyboard_controller import KeyboardController


def key(
    portable: UInt16,
    ch: UInt32 = 0,
    plain: UInt32 = 0,
    mods: UInt32 = 0,
    hi: Int32 = 0,
    release: Bool = False,
) -> KeyEvent:
    return KeyEvent(
        code=portable,
        key=portable,
        mods=mods,
        ch=ch,
        unshifted=plain,
        text_lo=0,
        text_hi=hi,
        repeat=False,
        release=release,
        text=hi > 0,
        consumed=False,
    )


def pause():
    _ = external_call["usleep", Int32](UInt32(10000))


def plane_text(model: AppModel, pane: UInt64) raises -> String:
    if pane in model.terminals:
        return terminal_text(model.terminals[pane])
    return String()


def await_text(
    mut model: AppModel,
    mut connection: InteractiveConnection,
    pane: UInt64,
    text: String,
) raises:
    for _ in range(300):
        model.refresh(connection)
        if text in plane_text(model, pane):
            return
        pause()
    raise Error(
        "keyboard proof did not receive packed terminal marker: " + text
    )


def dispatch(
    mut controller: KeyboardController,
    model: AppModel,
    mut connection: InteractiveConnection,
    event: KeyEvent,
    mut text: TextInput,
    epoch: UInt64,
    focused: Bool = True,
) raises -> UInt64:
    var routed = controller.route(
        model, focused, event, Span(text.bytes), epoch
    )
    if not routed:
        raise Error("physical keyboard event did not route natively")
    var frame = input_key(routed.value().pane, routed.value().input)
    connection.transport.write(Span(frame))
    controller.filter_text(text)
    var committed = 0
    for insert in text.events:
        if insert.kind == TEXT_INSERT and insert.end > insert.start:
            var content = String(
                from_utf8=Span(text.bytes)[Int(insert.start) : Int(insert.end)]
            )
            var literal = input_text(model.pane.value(), content)
            connection.transport.write(Span(literal))
            committed += 1
    assert_equal(committed, 0)
    return routed.value().pane


def main() raises:
    var args = argv()
    var connection = connect_interactive(args[1], "")
    var model = AppModel()
    model.refresh(connection)
    model.sync_focus(connection)
    assert_equal(len(model.panes), 1)
    var first = model.pane.value()
    await_text(model, connection, first, "KEYBOARD_READY")
    assert_true("KEYBOARD_RESULT:" not in plane_text(model, first))
    var controller = KeyboardController()
    var prefix_text = TextInput()
    prefix_text.insert("b")
    assert_equal(
        dispatch(
            controller,
            model,
            connection,
            key(Key.B, 98, 98, MOD_CTRL, 1),
            prefix_text,
            7,
        ),
        first,
    )
    for _ in range(300):
        model.refresh(connection)
        if model.routing.armed:
            break
        pause()
    assert_true(model.routing.armed)
    var split_text = TextInput()
    split_text.insert("%")
    assert_equal(
        dispatch(
            controller,
            model,
            connection,
            key(Key.DIGIT_5, 37, 53, MOD_SHIFT, 1),
            split_text,
            7,
        ),
        first,
    )
    for _ in range(300):
        model.refresh(connection)
        if len(model.panes) == 2 and model.pane != Optional[UInt64](first):
            break
        pause()
    assert_equal(len(model.panes), 2)
    assert_true(model.pane != Optional[UInt64](first))
    var second = model.pane.value()
    await_text(model, connection, second, "KEYBOARD_READY")
    var empty = TextInput()
    assert_equal(
        dispatch(
            controller,
            model,
            connection,
            key(Key.B, 98, 98, MOD_CTRL, release=True),
            empty,
            11,
            focused=False,
        ),
        first,
    )
    assert_equal(
        dispatch(
            controller,
            model,
            connection,
            key(Key.DIGIT_5, 37, 53, MOD_SHIFT, release=True),
            empty,
            11,
            focused=False,
        ),
        first,
    )
    model.refresh(connection)
    assert_equal(model.pane.value(), second)
    for pane in [first, second]:
        model.select_pane(pane)
        model.sync_focus(connection)
        assert_equal(
            dispatch(controller, model, connection, key(Key.ENTER), empty, 13),
            pane,
        )
        assert_equal(
            dispatch(
                controller,
                model,
                connection,
                key(Key.ENTER, release=True),
                empty,
                13,
            ),
            pane,
        )
        await_text(model, connection, pane, "KEYBOARD_RESULT:0:")
    assert_true(not model.routing.armed)
    print(
        "physical Ctrl-b + Shift-5 split; captured releases and two empty PTY"
        " buffers passed"
    )
