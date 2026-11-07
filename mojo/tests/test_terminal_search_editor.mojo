from std.testing import assert_equal, assert_true, assert_false
from std.sys import argv
from mjui.ui import Ui, TextStyle
from mjui.input import TextInput, CMD_PASTE, CMD_NEWLINE, CMD_CANCEL, CMD_SELECT_ALL, CMD_COPY, CMD_TAB, MOD_SHIFT
from mjui.resources import resource_path
from zzmojo.protocol import Decoder
from zzmojo.search_clipboard import SearchClipboard
from zzmojo.terminal_search import SEARCH_ACCEPT_AND_CLOSE
from zzmojo.terminal_search_editor import SearchEditor, SEARCH_TEXT_BYTES
from zzmojo.terminal_search_chrome import SearchKeys, build_search_chrome, search_navigation_enabled
from zzmojo.terminal_search_chrome import build_mode_indicator
from mjui.widgets.penna import PennaTheme
from zzmojo.terminal_model import TerminalSearch, TerminalMode
from mjui.testing import frame_pixels
from mjui.png import encode_png
from mjui.sys.file import file_write


def action(var frame: List[UInt8]) raises -> UInt64:
    var reader = Decoder(frame^)
    assert_equal(reader.uint(), UInt64(9))
    assert_equal(reader.uint(), UInt64(5))
    assert_equal(reader.uint(), UInt64(7))
    return reader.uint()


def main() raises:
    var ui = Ui(1)
    ui.sem_on = True
    var font = Int32(ui.add_font(resource_path("fonts/Inter-Regular.ttf")))
    var bold = Int32(ui.add_font(resource_path("fonts/Inter-SemiBold.ttf")))
    var mono = Int32(ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf")))
    var icons = Int32(ui.add_font(resource_path("fonts/lucide.ttf")))
    var theme = PennaTheme(0, font, bold, mono, icons)
    var keys = SearchKeys(100, 101, 102, 103, 104, 105, 106, 107)
    var editor = SearchEditor(keys.field, TextStyle(font, 13, theme.flat.fg, False))
    _ = editor.session.begin(7, 1)
    var clip = SearchClipboard()
    clip.text = "P"
    var input = TextInput()
    input.insert("a")
    input.command(CMD_PASTE)
    input.insert("b")
    input.command(CMD_NEWLINE)
    input.command(CMD_NEWLINE, MOD_SHIFT)
    ui.focus = keys.field
    ui.begin_frame(600, 200)
    var result = editor.update(ui, input, clip, 7)
    assert_equal(editor.field.text(), String("aPb"))
    assert_equal(editor.session.query.value().text, String("aPb"))
    assert_equal(result.consumed, 5)
    assert_equal(len(result.effects), 5)
    assert_equal(action(result.effects[0].frame.copy()), UInt64(18))
    assert_equal(action(result.effects[1].frame.copy()), UInt64(18))
    assert_equal(action(result.effects[2].frame.copy()), UInt64(18))
    assert_equal(action(result.effects[3].frame.copy()), UInt64(20))
    assert_equal(action(result.effects[4].frame.copy()), UInt64(19))
    var status = TerminalSearch(2, 7, False, False)
    assert_true(search_navigation_enabled(status))
    assert_false(search_navigation_enabled(TerminalSearch(2, 7, True, False)))
    assert_false(search_navigation_enabled(TerminalSearch(2, 7, False, True)))
    assert_false(search_navigation_enabled(TerminalSearch(0, 0, False, False)))
    assert_false(search_navigation_enabled(None))
    _ = editor.session.toggle_regex(7)
    _ = editor.session.toggle_case(7)
    _ = build_search_chrome(editor, ui, theme, keys, 7, 8, 8, 460, status)
    build_mode_indicator(ui, theme, 108, TerminalMode(1, 12, 20, False), 3, 468, 60)
    ui.end_frame()
    assert_true(ui.rect_of(keys.panel).x1 > 450)
    assert_true(ui.rect_of(keys.field).x1 > ui.rect_of(keys.field).x0)
    assert_true(ui.rect_of(keys.field).x0 >= ui.rect_of(keys.container).x0 + 27)
    assert_equal(ui.rect_of(keys.container).y1 - ui.rect_of(keys.container).y0, Float32(28))
    assert_equal(ui.rect_of(keys.regex).x1 - ui.rect_of(keys.regex).x0, Float32(24))
    var args = argv()
    if len(args) > 1:
        var pixels = frame_pixels(ui, theme.flat.bg)
        var png = encode_png(pixels.unsafe_ptr(), 600, 200, 600 * 4, 4)
        var output = file_write(args[1])
        output.write_bytes(Span(png))
    input = TextInput()
    input.command(CMD_TAB)
    ui.begin_frame(600, 200)
    result = editor.update(ui, input, clip, 7)
    assert_equal(result.consumed, 1)
    assert_equal(ui.focus, keys.regex)
    assert_false(editor.field.focused)
    ui.end_frame()
    ui.focus = keys.field
    input = TextInput()
    input.command(CMD_SELECT_ALL)
    input.command(CMD_COPY)
    input.command(CMD_CANCEL)
    input.insert("after")
    ui.begin_frame(600, 200)
    result = editor.update(ui, input, clip, 7)
    assert_equal(result.consumed, 3)
    assert_equal(clip.pending_clipboard.value(), String("aPb"))
    assert_false(Bool(editor.session.query))
    assert_equal(action(result.effects[0].frame.copy()), UInt64(21))
    ui.end_frame()
    editor.reset_text()
    _ = editor.session.begin(7, 0, SEARCH_ACCEPT_AND_CLOSE)
    input = TextInput()
    input.insert("accepted")
    input.command(CMD_NEWLINE)
    input.insert("PTY")
    ui.begin_frame(600, 200)
    result = editor.update(ui, input, clip, 7)
    assert_equal(result.consumed, 2)
    assert_equal(len(result.effects), 2)
    assert_equal(len(result.effects[1].frame), 0)
    assert_false(Bool(editor.session.query))
    assert_equal(editor.field.text(), String("accepted"))
    ui.end_frame()
    editor.reset_text()
    _ = editor.session.begin(7)
    var bytes = List[UInt8](length=SEARCH_TEXT_BYTES, fill=UInt8(120))
    var large = String(from_utf8=Span(bytes))
    clip.text = large.copy()
    input = TextInput()
    input.command(CMD_PASTE)
    input.insert("overflow")
    ui.begin_frame(600, 200)
    result = editor.update(ui, input, clip, 7)
    assert_true(result.limited)
    assert_equal(editor.field.text().byte_length(), SEARCH_TEXT_BYTES)
    assert_equal(editor.session.query.value().text.byte_length(), SEARCH_TEXT_BYTES)
    assert_true(editor.owned_bytes() < 8_388_608)
    ui.end_frame()
    print("search editor passed: ordered changes and navigation, copy, close prefix, accepted query, source toolbar states and bounded paste")
