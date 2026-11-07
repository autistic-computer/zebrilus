from std.testing import assert_equal, assert_true, assert_false
from mjui.ui import TextStyle
from mjui.input import Clipboard, TextInput, CMD_PASTE
from zzmojo.input_protocol import KeyInput
from zzmojo.terminal_search import SEARCH_ACCEPT_AND_CLOSE
from zzmojo.terminal_search_chrome import SearchKeys
from zzmojo.terminal_search_controller import TerminalSearchController, SearchTarget, SEARCH_POOL_BYTES
from zzmojo.terminal_search_editor import SEARCH_TEXT_BYTES
from zzmojo.search_clipboard import SearchClipboard, capture_search_clipboard
from zzmojo.search_ui_commands import SearchUiCommands
from zzmojo.terminal_search_protocol import SearchUiCommand


struct Host(Copyable, Movable, Clipboard):
    var text: String
    var primary: String
    var allow_reads: Bool

    def __init__(out self):
        self.text = "clipboard"
        self.primary = "primary"
        self.allow_reads = False

    def get_clipboard(self) -> String:
        return self.text

    def get_clipboard_bounded(self, maximum: Int) raises -> String:
        if not self.allow_reads or self.text.byte_length() > maximum:
            raise Error("unexpected clipboard read")
        return self.text

    def set_clipboard(mut self, text: String):
        self.text = text

    def supports_primary(self) -> Bool:
        return True

    def get_primary_selection(self) -> String:
        return self.primary

    def get_primary_selection_bounded(self, maximum: Int) raises -> String:
        if not self.allow_reads or self.primary.byte_length() > maximum:
            raise Error("unexpected primary read")
        return self.primary

    def set_primary_selection(mut self, text: String):
        self.primary = text


def main() raises:
    var controller = TerminalSearchController()
    var ordinary = SearchTarget(7, False, 7)
    var output = SearchTarget(7, True, 8)
    var keys = SearchKeys(100, 101, 102, 103, 104, 105, 106, 107)
    var output_keys = SearchKeys(200, 201, 202, 203, 204, 205, 206, 207)
    var valid = List[SearchTarget]()
    valid.append(ordinary)
    valid.append(output)
    controller.observe(1, 2, Span(valid))
    controller.ensure(ordinary, keys, TextStyle(0, 13, 0, False))
    controller.ensure(output, output_keys, TextStyle(0, 13, 0, False))
    _ = controller.begin(ordinary, 1)
    _ = controller.begin(output, 0, SEARCH_ACCEPT_AND_CLOSE)
    assert_equal(len(controller.editors), 2)
    assert_equal(controller.editors[ordinary.name()].session.query.value().direction, UInt8(1))
    assert_equal(controller.editors[output.name()].session.behavior, UInt8(1))
    var input = KeyInput()
    input.key = 0
    input.codepoint = 102
    input.modifiers = 3
    var routed = controller.route_key(ordinary, input, False, True, True, False)
    assert_false(routed.handled)
    routed = controller.route_key(ordinary, input, False, True, False, True)
    assert_false(routed.handled)
    routed = controller.route_key(ordinary, input, False, True, False, False)
    assert_true(routed.handled)
    assert_equal(controller.editors[ordinary.name()].session.query.value().direction, UInt8(0))
    input.codepoint = 99
    input.modifiers = 4
    routed = controller.route_key(ordinary, input, True, False, False, False)
    assert_true(routed.handled)
    assert_equal(controller.editors[ordinary.name()].session.query.value().case_policy, UInt8(1))
    input.action = 2
    routed = controller.route_key(ordinary, input, True, False, False, False)
    assert_false(routed.handled)
    assert_equal(controller.editors[ordinary.name()].session.query.value().case_policy, UInt8(1))
    _ = controller.editors[ordinary.name()].session.escape(7)
    input.key = 4
    input.action = 1
    routed = controller.route_key(ordinary, input, False, True, False, False)
    assert_true(routed.handled)
    input.action = 2
    routed = controller.route_key(ordinary, input, False, True, False, False)
    assert_true(routed.handled)
    routed = controller.route_key(ordinary, input, False, True, False, False)
    assert_false(routed.handled)
    controller.editors[ordinary.name()].key_pressed(4)
    _ = controller.editors[ordinary.name()].key_released(4)
    _ = controller.editors[ordinary.name()].session.escape(7)
    controller.editors[ordinary.name()].settle_released_key()
    assert_false(Bool(controller.editors[ordinary.name()].session.swallowed))
    controller.editors[ordinary.name()].key_pressed(4)
    _ = controller.editors[ordinary.name()].session.escape(7)
    assert_true(Bool(controller.editors[ordinary.name()].session.swallowed))
    _ = controller.editors[ordinary.name()].key_released(4)
    assert_false(Bool(controller.editors[ordinary.name()].session.swallowed))
    controller.closing_owners[0x100] = ordinary
    controller.editors[ordinary.name()].session.swallowed = UInt8(2)
    controller.reset_keys()
    assert_equal(len(controller.closing_owners), 0)
    assert_false(Bool(controller.editors[ordinary.name()].session.swallowed))
    var bytes = List[UInt8](length=SEARCH_TEXT_BYTES, fill=UInt8(120))
    var text = String(from_utf8=Span(bytes))
    for pane in range(6):
        var target = SearchTarget(UInt64(pane), False, UInt64(pane))
        controller.ensure(target, keys, TextStyle(0, 13, 0, False))
        _ = controller.begin(target)
        controller.editors[target.name()].field.set_text(text)
        _ = controller.editors[target.name()].session.edit(UInt64(pane), text)
    assert_true(controller.owned_bytes() > SEARCH_POOL_BYTES)
    var evicted = controller.enforce(SearchTarget(5, False, 5).name())
    assert_true(len(evicted) >= 2)
    assert_true(controller.owned_bytes() <= SEARCH_POOL_BYTES)
    for effect in evicted:
        assert_true(effect.passive)
        assert_equal(effect.effect.focus, UInt8(0))
        assert_true(len(effect.effect.frame) > 0)
    assert_true(SearchTarget(5, False, 5).name() in controller.editors)
    controller.observe(1, 2, Span(valid))
    assert_true(len(controller.editors) <= 2)
    controller.observe(1, 3, Span(valid))
    assert_equal(len(controller.editors), 0)
    var host = Host()
    var events = TextInput()
    var clip = capture_search_clipboard(host, events, 16, False, False)
    assert_equal(clip.text, String())
    events.command(CMD_PASTE)
    clip = capture_search_clipboard(host, events, 16, False, False)
    host.allow_reads = True
    clip = capture_search_clipboard(host, events, 16, True, False)
    assert_equal(clip.text, String("clipboard"))
    clip.set_clipboard("query\0🦓")
    clip.set_primary_selection("selection")
    assert_equal(clip.get_clipboard(), String("query\0🦓"))
    assert_equal(clip.get_primary_selection(), String("selection"))
    clip.deliver(host)
    assert_equal(host.text, String("query\0🦓"))
    assert_equal(host.primary, String("selection"))
    assert_false(Bool(clip.pending_clipboard))
    assert_false(Bool(clip.pending_primary))
    var commands = SearchUiCommands()
    commands.push(SearchUiCommand(7, 1), 1, 2)
    var captured = commands.take()
    assert_equal(len(commands.events), 0)
    assert_equal(captured[0].server, UInt64(1))
    assert_equal(captured[0].client, UInt64(2))
    assert_equal(captured[0].pane, UInt64(7))
    assert_equal(captured[0].direction, UInt8(1))
    commands.merge(captured^)
    assert_equal(len(commands.events), 1)
    print("search controller passed: source identity, distinct output owner, shortcut modal/prefix ownership, release swallowing, aggregate eviction and bounded clipboard capture")
