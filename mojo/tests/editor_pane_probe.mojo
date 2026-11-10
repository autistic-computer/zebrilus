from std.sys import argv
from std.sys.info import CompilationTarget
from std.testing import assert_equal, assert_true, assert_false, assert_raises
from mjui.ui import Ui, Style
from mjui.input import KeyEvent, TextInput, CMD_CANCEL, MOD_CTRL, MOD_CMD
from mjui.keys import Key
from mjui.widgets.highlight import LANG_RUST, LANG_TEXT
from mjui.widgets.key import sub
from mjui.widgets.penna import PennaTheme
from mjui.resources import resource_path
from zzmojo.editor_pane import (
    EDITOR_BUTTON_LEFT,
    EDITOR_CMD_OPEN,
    EDITOR_CMD_RETRY,
    EDITOR_CMD_SAVE,
    EDITOR_CMD_SAVE_AS,
    EDITOR_KEY_PASS,
    EDITOR_KEY_TAKEN,
    EDITOR_KEY_TEXT,
    EDITOR_POINTER_DOWN,
    EditorOutbound,
    EditorPane,
)
from zzmojo.editor_state import EDITOR_RETRY_OPEN, EDITOR_RETRY_SAVE
from zzmojo.editor_workspace import EditorDeck
from zzmojo.search_clipboard import SearchClipboard
from zzmojo.tree_model import (
    MuxSnapshot,
    PaneSnapshot,
    SessionSnapshot,
    WindowSnapshot,
)

comptime BASE: UInt64 = 0x6000000000000000
comptime WIDTH: Float32 = 1120
comptime HEIGHT: Float32 = 720


def shortcut() -> UInt32:
    return MOD_CMD if CompilationTarget.is_macos() else MOD_CTRL


def press(key: UInt16, mods: UInt32 = 0, text: Bool = False) -> KeyEvent:
    return KeyEvent(0, key, mods, 0, 0, 0, 0, False, False, text, False)


def write(path: String, text: String) raises:
    with open(path, "w") as file:
        file.write(text)


def read(path: String) raises -> String:
    with open(path, "r") as file:
        return file.read()


def line(item: EditorOutbound) -> String:
    var out = String(item.pane, "|", item.kind, "|", item.name)
    for argument in item.arguments:
        out += "|" + argument
    return out


def frame(
    mut pane: EditorPane,
    mut ui: Ui,
    th: PennaTheme,
    input: TextInput,
    mut clip: SearchClipboard,
) raises:
    ui.set_time(ui.time + 0.02)
    ui.begin_frame(WIDTH, HEIGHT)
    ui.open(Style.col().width(WIDTH).height(HEIGHT))
    pane.text(ui, input, clip, BASE + pane.pane)
    pane.build(ui, th, BASE + pane.pane)
    ui.close()
    ui.end_frame()


def idle(
    mut pane: EditorPane, mut ui: Ui, th: PennaTheme, mut clip: SearchClipboard, count: Int
) raises:
    var none = TextInput()
    for _ in range(count):
        frame(pane, ui, th, none, clip)


def scan(
    mut pane: EditorPane, mut ui: Ui, th: PennaTheme, mut clip: SearchClipboard
) raises:
    var tries = 0
    idle(pane, ui, th, clip, 1)
    while pane.picker.value().scanning() and tries < 400:
        idle(pane, ui, th, clip, 1)
        tries += 1
    assert_false(pane.picker.value().scanning())
    idle(pane, ui, th, clip, 2)


def click(
    mut pane: EditorPane,
    mut ui: Ui,
    th: PennaTheme,
    mut clip: SearchClipboard,
    x: Float32,
    y: Float32,
) raises:
    ui.pointer_move(x, y)
    idle(pane, ui, th, clip, 1)
    ui.pointer_down(x, y, 1)
    idle(pane, ui, th, clip, 1)
    ui.pointer_up(x, y)
    idle(pane, ui, th, clip, 1)


def typed(text: String) -> TextInput:
    var input = TextInput()
    input.insert(text)
    return input^


def open_edit_save(mut ui: Ui, th: PennaTheme, root: String) raises:
    var clip = SearchClipboard()
    var pane = EditorPane(7, None, root, th)
    pane.set_focused(True)
    var out = pane.take_outbox()
    assert_equal(len(out), 1)
    assert_equal(line(out[0]), "7|0|select-pane|-t|%7|-T|editor")
    assert_equal(pane.title(), "editor")
    idle(pane, ui, th, clip, 2)
    assert_equal(pane.left, 0)
    assert_equal(pane.top, 0)
    assert_equal(pane.width, WIDTH)
    assert_equal(pane.height, HEIGHT)
    assert_true(pane.model.scratch_hint(pane.editor.store.byte_count() == 0))
    assert_equal(pane.key(press(Key.F5)).kind, EDITOR_KEY_PASS)
    assert_equal(pane.key(press(Key.O, text=True)).kind, EDITOR_KEY_TEXT)
    var released = press(Key.O, shortcut())
    released.release = True
    assert_equal(pane.key(released).kind, EDITOR_KEY_PASS)
    assert_false(pane.picking())
    assert_equal(pane.key(press(Key.O, shortcut())).kind, EDITOR_KEY_TAKEN)
    assert_true(pane.picking())
    assert_equal(pane.key(press(Key.O, shortcut())).kind, EDITOR_KEY_TAKEN)
    scan(pane, ui, th, clip)
    assert_equal(len(pane.picker.value().rows), 2)
    assert_equal(pane.picker.value().rows[0].relative, "notes.md")
    assert_equal(pane.picker.value().rows[1].relative, "src/main.rs")
    assert_equal(pane.picker.value().flow.selected.value(), 0)
    frame(pane, ui, th, typed("main"), clip)
    assert_equal(pane.field.text(), "main")
    assert_equal(pane.editor.text(), "")
    assert_equal(len(pane.picker.value().rows), 1)
    assert_equal(pane.picker.value().rows[0].relative, "src/main.rs")
    assert_equal(pane.picker.value().flow.selected.value(), 0)
    assert_equal(pane.key(press(Key.ENTER)).kind, EDITOR_KEY_TAKEN)
    assert_false(pane.picking())
    var path = root + "/src/main.rs"
    assert_equal(pane.model.path.value(), path)
    assert_equal(pane.editor.text(), "fn main() {}\n")
    assert_equal(pane.editor.lang, LANG_RUST)
    assert_equal(pane.title(), "main.rs")
    assert_false(pane.dirty())
    out = pane.take_outbox()
    assert_equal(len(out), 2)
    assert_equal(line(out[0]), "7|0|set-editor-path|-t|%7|" + path)
    assert_equal(line(out[1]), "7|0|select-pane|-t|%7|-T|main.rs")
    pane.descriptor(Optional[String](path), root)
    assert_equal(len(pane.take_outbox()), 0)
    idle(pane, ui, th, clip, 2)
    frame(pane, ui, th, typed("// "), clip)
    assert_equal(pane.editor.text(), "// fn main() {}\n")
    assert_true(pane.dirty())
    assert_equal(pane.title(), "main.rs •")
    out = pane.take_outbox()
    assert_equal(len(out), 1)
    assert_equal(line(out[0]), "7|0|select-pane|-t|%7|-T|main.rs •")
    pane.set_focused(False)
    frame(pane, ui, th, typed("lost"), clip)
    assert_equal(pane.editor.text(), "// fn main() {}\n")
    pane.set_focused(True)
    assert_equal(read(path), "fn main() {}\n")
    assert_equal(pane.key(press(Key.S, shortcut())).kind, EDITOR_KEY_TAKEN)
    assert_equal(read(path), "// fn main() {}\n")
    assert_false(pane.dirty())
    assert_equal(pane.title(), "main.rs")
    assert_false(Bool(pane.model.error))
    out = pane.take_outbox()
    assert_equal(len(out), 1)
    assert_equal(line(out[0]), "7|0|select-pane|-t|%7|-T|main.rs")
    write(path, "fn main() {}\n")


def retry_open(mut ui: Ui, th: PennaTheme, root: String) raises:
    var clip = SearchClipboard()
    var gone = root + "/gone.txt"
    var pane = EditorPane(8, Optional[String](gone), root, th)
    pane.set_focused(True)
    assert_equal(pane.model.error.value().title(), "Couldn’t open this file")
    assert_equal(
        pane.model.error.value().message,
        "Could not open " + gone + ": No such file or directory (os error 2)",
    )
    assert_equal(pane.model.error.value().retry, EDITOR_RETRY_OPEN)
    assert_equal(pane.title(), "gone.txt")
    var out = pane.take_outbox()
    assert_equal(len(out), 1)
    assert_equal(line(out[0]), "8|0|select-pane|-t|%8|-T|gone.txt")
    idle(pane, ui, th, clip, 3)
    var button = ui.rect_of(sub(BASE + 8, 1))
    assert_true(button.key != 0)
    assert_true(button.x1 - button.x0 > 20)
    var x = (button.x0 + button.x1) * 0.5
    var y = (button.y0 + button.y1) * 0.5
    click(pane, ui, th, clip, x, y)
    assert_equal(
        pane.model.error.value().message,
        "Could not open " + gone + ": No such file or directory (os error 2)",
    )
    write(gone, "back\n")
    click(pane, ui, th, clip, x, y)
    assert_false(Bool(pane.model.error))
    assert_equal(pane.editor.text(), "back\n")
    assert_equal(pane.editor.lang, LANG_TEXT)
    assert_equal(pane.model.path.value(), gone)
    assert_equal(len(pane.take_outbox()), 0)


def save_as(mut ui: Ui, th: PennaTheme, root: String) raises:
    var clip = SearchClipboard()
    var pane = EditorPane(9, None, root, th)
    pane.set_focused(True)
    _ = pane.take_outbox()
    idle(pane, ui, th, clip, 2)
    frame(pane, ui, th, typed("hello"), clip)
    assert_equal(pane.editor.text(), "hello")
    assert_true(pane.dirty())
    var out = pane.take_outbox()
    assert_equal(len(out), 1)
    assert_equal(line(out[0]), "9|0|select-pane|-t|%9|-T|editor •")
    assert_equal(pane.key(press(Key.S, shortcut())).kind, EDITOR_KEY_TAKEN)
    out = pane.take_outbox()
    assert_equal(len(out), 1)
    assert_equal(line(out[0]), "9|1|" + root + "|untitled")
    assert_true(pane.dirty())
    var target = root + "/new/out.txt"
    pane.command(EDITOR_CMD_SAVE_AS, target)
    assert_equal(read(target), "hello")
    assert_equal(pane.model.path.value(), target)
    assert_equal(pane.title(), "out.txt")
    assert_false(pane.dirty())
    out = pane.take_outbox()
    assert_equal(len(out), 2)
    assert_equal(line(out[0]), "9|0|set-editor-path|-t|%9|" + target)
    assert_equal(line(out[1]), "9|0|select-pane|-t|%9|-T|out.txt")
    var blocked = root + "/notes.md/x.txt"
    pane.command(EDITOR_CMD_SAVE_AS, blocked)
    assert_equal(pane.model.error.value().title(), "Couldn’t save this file")
    assert_equal(pane.model.error.value().retry, EDITOR_RETRY_SAVE)
    assert_equal(pane.model.error.value().path, blocked)
    assert_equal(
        pane.model.error.value().message,
        "Could not save " + blocked + ": File exists (os error 17)",
    )
    assert_equal(pane.model.path.value(), target)
    pane.command(EDITOR_CMD_RETRY)
    assert_equal(
        pane.model.error.value().message,
        "Could not save " + blocked + ": File exists (os error 17)",
    )
    pane.command(EDITOR_CMD_SAVE_AS, "relative.txt")
    assert_equal(pane.model.error.value().message, "editor path must be absolute")
    assert_equal(pane.model.error.value().retry, EDITOR_RETRY_SAVE)
    assert_equal(len(pane.take_outbox()), 0)
    pane.command(EDITOR_CMD_SAVE)
    assert_false(Bool(pane.model.error))
    assert_equal(read(target), "hello")
    with assert_raises(contains="unknown editor command"):
        pane.command(UInt8(9))
    with assert_raises(contains="editor path cannot contain control characters"):
        pane.descriptor(Optional[String](String("/a\tb")), root)
    with assert_raises(contains="editor working directory must be absolute"):
        pane.descriptor(None, "cwd")
    with assert_raises(contains="editor path must be absolute"):
        _ = EditorPane(10, Optional[String](String("relative")), root, th)


def picker_pointer(mut ui: Ui, th: PennaTheme, root: String) raises:
    var clip = SearchClipboard()
    var pane = EditorPane(11, None, root, th)
    pane.set_focused(True)
    idle(pane, ui, th, clip, 2)
    pane.command(EDITOR_CMD_OPEN)
    scan(pane, ui, th, clip)
    assert_equal(len(pane.picker.value().rows), 4)
    assert_equal(pane.picker.value().rows[0].relative, "gone.txt")
    assert_equal(pane.picker.value().rows[1].relative, "new/out.txt")
    assert_equal(pane.picker.value().rows[2].relative, "notes.md")
    assert_equal(pane.picker.value().rows[3].relative, "src/main.rs")
    var above = ui.rect_of(sub(sub(BASE + 11, 6), 2))
    var row = ui.rect_of(sub(sub(BASE + 11, 6), 3))
    assert_true(row.key != 0)
    assert_equal(row.y1 - row.y0, 26)
    assert_equal(row.y0 - above.y0, 28)
    assert_true(row.x0 >= pane.surface_left and row.x1 <= pane.surface_right)
    click(pane, ui, th, clip, (row.x0 + row.x1) * 0.5, (row.y0 + row.y1) * 0.5)
    assert_false(pane.picking())
    assert_equal(pane.model.path.value(), root + "/src/main.rs")
    assert_equal(pane.editor.text(), "fn main() {}\n")
    ui.pointer_move(4, 4)
    pane.command(EDITOR_CMD_OPEN)
    scan(pane, ui, th, clip)
    assert_equal(pane.surface_right - pane.surface_left, 560)
    assert_equal(pane.surface_left, 280)
    assert_true(
        pane.pointer(
            EDITOR_POINTER_DOWN,
            EDITOR_BUTTON_LEFT,
            1,
            (pane.surface_left + pane.surface_right) * 0.5,
            pane.surface_top + 10,
            0,
        )
    )
    assert_true(pane.picking())
    assert_false(pane.pointer(1, EDITOR_BUTTON_LEFT, 0, 2000, 2000, 0))
    assert_true(pane.picking())
    assert_equal(pane.picker.value().flow.selected.value(), 0)
    assert_equal(pane.key(press(Key.DOWN)).kind, EDITOR_KEY_TAKEN)
    assert_equal(pane.picker.value().flow.selected.value(), 1)
    assert_equal(pane.key(press(Key.UP)).kind, EDITOR_KEY_TAKEN)
    assert_equal(pane.picker.value().flow.selected.value(), 0)
    assert_true(pane.pointer(EDITOR_POINTER_DOWN, EDITOR_BUTTON_LEFT, 1, 4, 4, 0))
    assert_false(pane.picking())
    pane.command(EDITOR_CMD_OPEN)
    scan(pane, ui, th, clip)
    var cancel = TextInput()
    cancel.command(CMD_CANCEL)
    frame(pane, ui, th, cancel, clip)
    assert_false(pane.picking())
    pane.command(EDITOR_CMD_OPEN)
    assert_equal(pane.key(press(Key.ESCAPE)).kind, EDITOR_KEY_TAKEN)
    assert_false(pane.picking())
    assert_equal(pane.key(press(Key.ESCAPE)).kind, EDITOR_KEY_PASS)
    assert_equal(pane.model.path.value(), root + "/src/main.rs")
    assert_true(pane.wheel(10, 10, 0, -1, 0))
    assert_false(pane.wheel(2000, 10, 0, -1, 0))


def snapshot(root: String, first: Optional[String], extra: Int, bad: Bool) -> MuxSnapshot:
    var result = MuxSnapshot()
    var session = SessionSnapshot()
    session.id = 1
    session.active_window = 2
    var window = WindowSnapshot()
    window.id = 2
    var shell = PaneSnapshot()
    shell.id = 1
    window.panes[UInt64(1)] = shell^
    var one = PaneSnapshot()
    one.id = 20
    one.kind.kind = 3
    one.kind.path = first.copy()
    one.kind.cwd = Optional[String](root)
    window.panes[UInt64(20)] = one^
    for index in range(extra):
        var pane = PaneSnapshot()
        pane.id = UInt64(21 + index)
        pane.kind.kind = 3
        pane.kind.cwd = Optional[String](root)
        window.panes[UInt64(21 + index)] = pane^
    if bad:
        var pane = PaneSnapshot()
        pane.id = 900
        pane.kind.kind = 3
        pane.kind.path = Optional[String](String("relative"))
        pane.kind.cwd = Optional[String](root)
        window.panes[UInt64(900)] = pane^
    session.windows.append(window^)
    result.sessions.append(session^)
    return result^


def deck_frame(
    mut deck: EditorDeck,
    mut ui: Ui,
    th: PennaTheme,
    input: TextInput,
    mut clip: SearchClipboard,
) raises:
    ui.set_time(ui.time + 0.02)
    ui.begin_frame(WIDTH, HEIGHT)
    ui.open(Style.row().width(WIDTH).height(HEIGHT))
    for pane in [UInt64(20), UInt64(21)]:
        ui.open(Style.col().width(WIDTH * 0.5).height(HEIGHT))
        deck.text(ui, input, clip, pane, BASE + pane)
        assert_true(deck.build(ui, th, pane, BASE + pane))
        ui.close()
    ui.close()
    ui.end_frame()


def deck(mut ui: Ui, th: PennaTheme, root: String) raises:
    var clip = SearchClipboard()
    var none = TextInput()
    var deck = EditorDeck()
    deck.sync(snapshot(root, Optional[String](root + "/src/main.rs"), 1, False), th)
    assert_equal(deck.count(), 2)
    assert_true(deck.owns(20))
    assert_true(deck.owns(21))
    assert_false(deck.owns(1))
    assert_false(Bool(deck.title(1)))
    assert_equal(deck.title(20).value(), "main.rs")
    assert_equal(deck.title(21).value(), "editor")
    var out = deck.take_outbox()
    assert_equal(len(out), 2)
    assert_equal(line(out[0]), "20|0|select-pane|-t|%20|-T|main.rs")
    assert_equal(line(out[1]), "21|0|select-pane|-t|%21|-T|editor")
    assert_equal(deck.panes[0].editor.text(), "fn main() {}\n")
    deck.sync(snapshot(root, Optional[String](root + "/src/main.rs"), 1, False), th)
    assert_equal(len(deck.take_outbox()), 0)
    deck.sync(snapshot(root, Optional[String](root + "/notes.md"), 1, False), th)
    assert_equal(deck.count(), 2)
    assert_equal(deck.panes[0].editor.text(), "# notes\n")
    out = deck.take_outbox()
    assert_equal(len(out), 1)
    assert_equal(line(out[0]), "20|0|select-pane|-t|%20|-T|notes.md")
    deck.focus(Optional[UInt64](21))
    deck_frame(deck, ui, th, none, clip)
    deck_frame(deck, ui, th, none, clip)
    assert_equal(deck.panes[0].width, 560)
    assert_equal(deck.panes[1].left, 560)
    deck_frame(deck, ui, th, typed("x"), clip)
    assert_equal(deck.panes[0].editor.text(), "# notes\n")
    assert_equal(deck.panes[1].editor.text(), "x")
    assert_equal(deck.title(21).value(), "editor •")
    out = deck.take_outbox()
    assert_equal(len(out), 1)
    assert_equal(line(out[0]), "21|0|select-pane|-t|%21|-T|editor •")
    assert_equal(deck.key(1, press(Key.O, shortcut())).kind, EDITOR_KEY_PASS)
    assert_equal(deck.key(21, press(Key.O, shortcut())).kind, EDITOR_KEY_TAKEN)
    assert_true(deck.panes[1].picking())
    assert_false(deck.panes[0].picking())
    assert_true(deck.pointer(21, EDITOR_POINTER_DOWN, EDITOR_BUTTON_LEFT, 1, 1100, 700, 0))
    assert_false(deck.panes[1].picking())
    assert_false(deck.pointer(20, EDITOR_POINTER_DOWN, EDITOR_BUTTON_LEFT, 1, 1100, 700, 0))
    assert_false(deck.pointer(1, EDITOR_POINTER_DOWN, EDITOR_BUTTON_LEFT, 1, 10, 10, 0))
    assert_true(deck.wheel(20, 10, 10, 0, -1, 0))
    assert_false(deck.wheel(21, 10, 10, 0, -1, 0))
    with assert_raises(contains="unknown editor pane"):
        deck.command(1, EDITOR_CMD_SAVE)
    with assert_raises(contains="editor path must be absolute"):
        deck.sync(snapshot(root, Optional[String](root + "/notes.md"), 1, True), th)
    assert_equal(deck.count(), 2)
    assert_false(deck.owns(900))
    deck.sync(snapshot(root, Optional[String](root + "/notes.md"), 1, True), th)
    assert_equal(deck.count(), 2)
    deck.sync(snapshot(root, Optional[String](root + "/notes.md"), 0, False), th)
    assert_equal(deck.count(), 1)
    assert_true(deck.owns(20))
    assert_false(deck.owns(21))
    assert_equal(len(deck.refused), 0)
    deck.sync(snapshot(root, None, 63, False), th)
    assert_equal(deck.count(), 64)
    with assert_raises(contains="editor panes exceed the 64-pane host limit"):
        deck.sync(snapshot(root, None, 64, False), th)
    assert_equal(deck.count(), 64)
    deck.sync(MuxSnapshot(), th)
    assert_equal(deck.count(), 0)


def main() raises:
    var args = argv()
    var root = String(args[1])
    var ui = Ui(1)
    ui.sem_on = True
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    var th = PennaTheme(4, 0, 1, 2, 3)
    open_edit_save(ui, th, root)
    retry_open(ui, th, root)
    save_as(ui, th, root)
    picker_pointer(ui, th, root)
    deck(ui, th, root)
    print("5")
