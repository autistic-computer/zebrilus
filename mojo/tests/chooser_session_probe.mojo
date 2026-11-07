from std.testing import assert_equal, assert_true, assert_false
from zzmojo.chooser_session import ChooserSession, ChooserSource
from zzmojo.chooser_protocol import (
    ChooseEvent,
    ChooseState,
    ChooseItem,
    ChooseSearch,
)
from zzmojo.palette_dialog_state import PaletteDialogs
from zzmojo.input_protocol import KeyInput


def open(mut dialogs: PaletteDialogs, kind: UInt8 = 1):
    var event = ChooseEvent()
    var state = ChooseState()
    state.kind = kind
    state.search = ChooseSearch("query", False)
    for _ in range(20):
        state.items.append(ChooseItem())
    state.selected = 10
    event.state = state^
    dialogs.apply_chooser(event^)


def main() raises:
    var dialogs = PaletteDialogs()
    open(dialogs)
    var session = ChooserSession()
    var source = ChooserSource(11, 22, 33)
    var observed = session.observe(dialogs, source, False, 4)
    assert_false(session.opened())
    observed = session.observe(dialogs, source, True, 4)
    assert_true(observed.owner_changed)
    assert_true(session.opened())
    assert_false(session.mark("old pane mark", 4, False, True))
    assert_true(session.mark("😀", 5, False, True))
    assert_equal(session.input.marked_utf16_length(), Optional[Int](2))
    var plan = session.select(10)
    session.geometry(200, 40, 18)
    assert_equal(session.scroll, Float32(320))
    session.wheel(0, -0.5, False)
    assert_equal(session.scroll, Float32(329))
    var update = ChooseEvent()
    update.update = True
    update.selected = 10
    update.search = ChooseSearch("new", True)
    dialogs.apply_chooser(update^)
    observed = session.observe(dialogs, source, True, 5)
    assert_false(observed.owner_changed)
    assert_true(session.current(plan))
    assert_equal(session.input.marked, Optional[String]("😀"))
    session.geometry(200, 40, 18)
    assert_equal(session.scroll, Float32(329))
    var character = KeyInput()
    character.key = 0
    character.codepoint = 97
    character.modifiers = 1
    character.action = 1
    assert_false(Bool(session.key(character, True)))
    character.modifiers = 2
    var key = session.key(character, True)
    assert_true(Bool(key))
    assert_equal(key.value().action.key.action, UInt8(0))
    var committed = session.commit("λ\n😀")
    assert_true(Bool(committed))
    assert_equal(committed.value().action.kind, UInt8(12))
    assert_equal(committed.value().action.text, String("λ\n😀"))
    assert_false(Bool(session.input.marked))
    var no_search = ChooseEvent()
    no_search.update = True
    no_search.selected = 10
    dialogs.apply_chooser(no_search^)
    _ = session.observe(dialogs, source, True, 5)
    assert_false(session.mark("stale", 5, False, True))
    assert_true(session.mark("callback", 6, False, True))
    assert_false(Bool(session.commit("ignored")))
    assert_false(Bool(session.input.marked))
    var revision = dialogs.chooser_revision
    var closed = dialogs.chooser_closed_revision
    dialogs.apply_chooser(ChooseEvent())
    open(dialogs)
    dialogs.finish_group(revision, closed)
    _ = session.observe(dialogs, source, True, 6)
    assert_true(session.current(plan))
    dialogs.apply_chooser(ChooseEvent())
    open(dialogs)
    _ = session.observe(dialogs, source, True, 6)
    assert_false(session.current(plan))
    assert_false(session.mark("old owner", 6, False, True))
    assert_false(session.mark("wrong owner", 7, False, False))
    assert_true(session.mark("fresh", 8, False, True))
    var before = session.input.marked.copy()
    var refused = False
    try:
        _ = session.commit(String("x") * 65536)
    except:
        refused = True
    assert_true(refused)
    assert_equal(session.input.marked, before)
    assert_true(session.mark("", UInt64.MAX, True, True))
    assert_false(session.mark("counter rollover", 1, False, True))
    var previous_scope = session.scope
    source.epoch += 1
    _ = session.observe(dialogs, source, True, 1)
    assert_true(session.scope != previous_scope)
    assert_false(Bool(session.input.marked))
    open(dialogs, 0)
    observed = session.observe(dialogs, source, True, 1)
    assert_true(observed.closed)
    assert_false(session.opened())
    print(
        "Chooser ownership, composition, native actions and scroll checks"
        " passed"
    )
