from std.testing import assert_equal, assert_true, assert_false, assert_raises, TestSuite
from zzmojo.desktop_router_delivery import DesktopUiEffect, DesktopEnvelope
from zzmojo.exec_protocol import Invocation
from zzmojo.prefix_bindings import PrefixBinding, PrefixTable
from zzmojo.which_key_live import LiveWhichKey, WhichKeyIntent, WhichKeyContext

comptime MS: UInt64 = 1_000_000


def context(table: Optional[String], repeat: Bool = False, blocked: Bool = False) -> WhichKeyContext:
    return WhichKeyContext(table.copy(), repeat, String("C-b"), blocked, 32, 40)


def stock(live: LiveWhichKey) -> List[PrefixTable]:
    return [PrefixTable("prefix", live.model._stock.copy())]


def ids(live: LiveWhichKey) -> List[String]:
    var result = List[String]()
    for row in live.sheet.value().rows:
        result.append(row.id.copy())
    return result^


def table(mut live: LiveWhichKey, time: UInt64, name: Optional[String], repeat: Bool = False, delay: UInt64 = 400) raises:
    live.apply(WhichKeyIntent.table_changed(time * MS, name, repeat, delay))


def test_prefix_pause_shows_the_short_sheet_and_the_next_key_hides_it() raises:
    var live = LiveWhichKey()
    var tables = stock(live)
    live.apply(WhichKeyIntent.input(True, False, False))
    table(live, 0, String("prefix"))
    live.advance(tables, context(String("prefix")), 399 * MS)
    assert_false(Bool(live.sheet))
    assert_equal(live.wake().value(), 400 * MS)
    live.advance(tables, context(String("prefix")), 400 * MS)
    ref sheet = live.sheet.value()
    assert_equal(sheet.table, "prefix")
    assert_equal(sheet.prefix_raw, "C-b")
    assert_true(Bool(sheet.prefix))
    assert_equal(sheet.more.value().raw, "?")
    for row in sheet.rows:
        if row.id == "%":
            assert_equal(row.group, 1)
            assert_false(row.caps[0].yours)
    assert_true(String("%") in ids(live))
    assert_false(String("M-1") in ids(live))
    assert_false(Bool(live.wake()))
    live.apply(WhichKeyIntent.input(True, False, True))
    live.advance(tables, context(String("prefix")), 401 * MS)
    assert_false(Bool(live.sheet))
    assert_equal(live.timing.generation, 3)


def test_a_held_prefix_key_keeps_the_timer() raises:
    var live = LiveWhichKey()
    var tables = stock(live)
    table(live, 0, String("prefix"))
    live.advance(tables, context(String("prefix")), 200 * MS)
    live.apply(WhichKeyIntent.input(True, True, True))
    live.advance(tables, context(String("prefix")), 401 * MS)
    assert_true(Bool(live.sheet))
    live.apply(WhichKeyIntent.input(False, False, True))
    live.advance(tables, context(String("prefix")), 402 * MS)
    assert_true(Bool(live.sheet))


def test_repeat_tables_cleared_tables_and_blocking_overlays_hide_it() raises:
    var live = LiveWhichKey()
    var tables = stock(live)
    table(live, 0, String("prefix"))
    live.advance(tables, context(String("prefix")), 401 * MS)
    assert_true(Bool(live.sheet))
    table(live, 401, String("prefix"), True)
    live.advance(tables, context(String("prefix"), True), 2401 * MS)
    assert_false(Bool(live.sheet))
    assert_false(Bool(live.wake()))
    table(live, 2401, String("prefix"))
    live.advance(tables, context(String("prefix")), 2601 * MS)
    table(live, 2601, None)
    live.advance(tables, context(None), 4601 * MS)
    assert_false(Bool(live.sheet))
    assert_false(Bool(live.wake()))
    table(live, 4601, String("prefix"))
    live.advance(tables, context(String("prefix")), 5002 * MS)
    assert_true(Bool(live.sheet))
    live.advance(tables, context(String("prefix"), blocked=True), 5003 * MS)
    assert_false(Bool(live.sheet))
    table(live, 5003, String("prefix"))
    live.advance(tables, context(String("prefix"), blocked=True), 5404 * MS)
    assert_false(Bool(live.sheet))
    assert_false(Bool(live.wake()))
    assert_equal(live.timing.generation, 6)


def test_a_custom_table_lists_flat_rows_and_its_own_keys_hide_it() raises:
    var live = LiveWhichKey()
    var bindings = List[PrefixBinding]()
    for pair in [("h", "-L"), ("l", "-R")]:
        var command = Invocation()
        command.name = "resize-pane"
        command.args.append(List[UInt8](String(pair[1]).as_bytes()))
        var commands: List[Invocation] = [command^]
        bindings.append(PrefixBinding(String(pair[0]), commands^, True, None))
    var tables = stock(live)
    tables.append(PrefixTable("resize", bindings^))
    table(live, 0, String("resize"))
    live.advance(tables, context(String("resize")), 401 * MS)
    ref sheet = live.sheet.value()
    assert_equal(sheet.table, "resize")
    assert_false(Bool(sheet.prefix))
    assert_equal(sheet.prefix_raw, "")
    assert_false(Bool(sheet.more))
    assert_equal(len(sheet.rows), 1)
    assert_equal(sheet.rows[0].caps[0].raw, "h l")
    assert_equal(sheet.rows[0].group, 0)
    assert_true(sheet.rows[0].repeat)
    live.apply(WhichKeyIntent.input(True, False, False))
    live.advance(tables, context(String("resize")), 402 * MS)
    assert_false(Bool(live.sheet))
    table(live, 402, String("resize"))
    live.advance(tables, context(String("resize")), 803 * MS)
    assert_true(Bool(live.sheet))


def test_the_all_keys_intent_opens_every_key_until_the_next_key() raises:
    var live = LiveWhichKey()
    var tables = stock(live)
    table(live, 0, String("prefix"))
    live.apply(WhichKeyIntent.input(True, False, True))
    var effect = DesktopUiEffect(3, None, DesktopEnvelope(1, 2, 3, 4, None), UInt64(0), live.model._stock.copy(), String("C-b"))
    live.apply(WhichKeyIntent.all_keys(effect))
    live.advance(tables, context(String("prefix")), 10 * MS)
    assert_false(Bool(live.sheet.value().more))
    assert_true(String("M-1") in ids(live))
    assert_false(Bool(live.wake()))
    live.advance(tables, context(String("prefix")), 1000 * MS)
    assert_true(String("M-1") in ids(live))
    live.apply(WhichKeyIntent.input(True, False, True))
    live.advance(tables, context(String("prefix")), 1001 * MS)
    assert_false(Bool(live.sheet))
    effect.kind = 4
    with assert_raises():
        _ = WhichKeyIntent.all_keys(effect)


def test_zero_delay_never_arms_and_unknown_intents_are_refused() raises:
    var live = LiveWhichKey()
    var tables = stock(live)
    table(live, 0, String("prefix"), delay=0)
    live.advance(tables, context(String("prefix")), 5000 * MS)
    assert_false(Bool(live.sheet))
    assert_false(Bool(live.wake()))
    with assert_raises():
        _ = WhichKeyIntent.table_changed(0, String("prefix"), False, 2001)
    var forged = WhichKeyIntent.input(True, False, False)
    forged.kind = 3
    with assert_raises():
        live.apply(forged)


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
