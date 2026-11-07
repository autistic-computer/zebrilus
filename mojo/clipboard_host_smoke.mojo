from std.os import getenv
from std.testing import assert_equal, assert_true
from mjui.sdl.window import Window
from mjui.sdl.clipboard import ClipboardData
from mjui.sdl import sdl3 as sdl
from mjui.input import CMD_PASTE, TEXT_COMMAND
from mjui.png import encode_png
from zzmojo.clipboard_host import deliver_clipboard
from zzmojo.clipboard_protocol import (
    ClipboardEvent,
    ClipboardState,
    decode_clipboard_reply,
)
from zzmojo.input_protocol import mjui_key_input
from zzmojo.clipboard_keys import clipboard_shortcut


def main() raises:
    assert_equal(getenv("SDL_VIDEODRIVER", ""), "dummy")
    var win = Window.open("Clipboard exchange", 160, 100, gpu=False)
    var text = String("private héllo 世界\0clipboard\r\n")
    win[].set_clipboard(text)
    assert_equal(win[].get_clipboard_bounded(text.byte_length()), text)
    win[].set_text_input(True)
    var raw = sdl.SDL_Event()
    var down = sdl.SDL_KeyboardEvent()
    down.type = sdl.SDL_EVENT_KEY_DOWN
    down.scancode = 25
    down.mod = 0x0041
    down.down = True
    Pointer(to=raw).unsafe_bitcast[sdl.SDL_KeyboardEvent]().unsafe_write(
        copy=down
    )
    assert_true(win[].event(raw))
    assert_equal(win[].keys[len(win[].keys) - 1].unshifted, UInt32(118))
    var key = win[].keys[len(win[].keys) - 1]
    assert_equal(key.text_event, Int32(0))
    var normalized = mjui_key_input(
        key.key, key.ch, key.unshifted, key.mods, key.action()
    )
    assert_equal(clipboard_shortcut(normalized), UInt8(2))
    var commands = win[].take_text()
    assert_equal(commands.events[0].kind, TEXT_COMMAND)
    assert_equal(commands.events[0].cmd, CMD_PASTE)
    var rejected = False
    try:
        _ = win[].get_clipboard_bounded(text.byte_length() - 1)
    except:
        rejected = True
    assert_true(rejected)
    assert_equal(win[].get_clipboard_bounded(text.byte_length()), text)
    var state = ClipboardState()
    var event = ClipboardEvent()
    event.pane = 7
    event.request_id = 0
    event.target = 1
    event.text = "native server selection"
    state.apply(event)
    _ = deliver_clipboard(win[], state)
    assert_equal(win[].get_clipboard_bounded(100), event.text)
    assert_equal(win[].get_primary_selection(), event.text)
    assert_equal(
        win[].get_primary_selection_bounded(event.text.byte_length()),
        event.text,
    )
    rejected = False
    try:
        _ = win[].get_primary_selection_bounded(event.text.byte_length() - 1)
    except:
        rejected = True
    assert_true(rejected)
    event.producer = 1
    event.text = "native application primary"
    state = ClipboardState()
    state.apply(event)
    _ = deliver_clipboard(win[], state, String("on"))
    assert_equal(win[].get_clipboard_bounded(100), "native server selection")
    assert_equal(win[].get_primary_selection(), event.text)
    event.producer = 0
    event.text = "native disabled mirror"
    state = ClipboardState()
    state.apply(event)
    _ = deliver_clipboard(win[], state, String("off"))
    assert_equal(win[].get_clipboard_bounded(100), "native server selection")
    assert_equal(win[].get_primary_selection(), event.text)
    event.kind = 63
    state = ClipboardState()
    state.apply(event, 11, 21)
    var delivery = deliver_clipboard(win[], state)
    assert_equal(delivery.reply.value().server, UInt64(11))
    assert_equal(delivery.reply.value().client, UInt64(21))
    var reply = decode_clipboard_reply(delivery.reply.value().frame.copy())
    assert_equal(String(from_utf8=Span(reply)), "native server selection")
    var pixel: List[UInt8] = [12, 34, 56, 255]
    var png = encode_png(pixel.unsafe_ptr(), 1, 1, 4, 4)
    var data = ClipboardData(String("image text"), String(), png.copy())
    assert_true(win[].set_clipboard_data(data^))
    assert_true(win[].has_clipboard_image())
    assert_equal(win[].get_clipboard_image(len(png)), png)
    assert_equal(len(win[].get_clipboard_image(len(png) - 1)), 0)
    assert_true(win[].has_clipboard_image())
    assert_equal(win[].get_clipboard_bounded(100), "image text")
    win[].set_clipboard("replacement")
    assert_true(not win[].has_clipboard_image())
    assert_equal(win[].get_clipboard_bounded(100), "replacement")
    print(
        "isolated native clipboard, PRIMARY, bounded reads, binary-safe text"
        " and image ownership passed"
    )
