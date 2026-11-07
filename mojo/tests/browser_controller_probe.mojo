from std.testing import assert_equal, assert_raises, assert_true
from zzmojo.browser_state import restore_browser, BrowserHistoryUse
from zzmojo.browser_host import BrowserHost
from zzmojo.browser_delivery import prepare_browser_delivery
from zzmojo.browser_controller import (
    BrowserMetadataEvent,
    prepare_browser_event,
    navigate_browser_tab,
    synchronize_browser_tabs,
    synchronize_browser_profile,
    accept_browser_address,
    prepare_browser_transition,
)


def event(
    kind: UInt8,
    text: String = String(),
    url: String = String(),
    loading: Bool = False,
    back: Bool = False,
    forward: Bool = False,
) -> BrowserMetadataEvent:
    return BrowserMetadataEvent(
        kind, text, url, loading, back, forward, List[UInt8]()
    )


def main() raises:
    var urls = List[String]()
    urls.append(String("https://old/"))
    urls.append(String("https://background/"))
    var model = restore_browser(urls, 0, String("work"), UInt64(3))
    var host = BrowserHost()
    host.bind(3, 0, 10)
    host.bind(3, 1, 11)
    var source = host.source(10).value().copy()
    var foreign = restore_browser(urls, 0, String("work"), UInt64(3))
    var foreign_plan = prepare_browser_event(
        3, model, host, source, event(2, String("FOREIGN"))
    )
    with assert_raises():
        foreign_plan^.commit(foreign, host)
    assert_equal(foreign.current.title, String("Browser"))
    with assert_raises():
        _ = prepare_browser_event(
            4, model, host, source, event(2, String("WRONG PANE"))
        )
    var same_generation = prepare_browser_event(
        3, model, host, source, event(2, String("REPLACED"))
    )
    var changed = model.copy()
    changed.current.title = String("changed without generation")
    with assert_raises():
        same_generation^.commit(changed, host)
    assert_equal(changed.current.title, String("changed without generation"))
    var too_large = String()
    for _ in range(65537):
        too_large += "x"
    var identity = model.identity()
    with assert_raises():
        _ = prepare_browser_event(3, model, host, source, event(2, too_large))
    assert_true(model.matches_identity(identity))
    var title = prepare_browser_event(
        3, model, host, source, event(2, String("FIRST"))
    )
    title^.commit(model, host)
    assert_equal(model.current.title, String("FIRST"))
    assert_equal(model.tabs[0].content.title, String())
    assert_equal(model.effects[0].kind, UInt8(10))
    assert_equal(model.effects[0].text, String("https://old/"))
    assert_equal(model.effects[0].profile, String("work"))
    model.pending_history.append(
        BrowserHistoryUse(0, String("work"), String("typed query"), True, False)
    )
    var start = prepare_browser_event(
        3, model, host, source, event(3, loading=True, back=True)
    )
    start^.commit(model, host)
    assert_true(model.pending_history[0].started)
    var address = prepare_browser_event(
        3, model, host, source, event(1, String("https://new/"))
    )
    address^.commit(model, host)
    var complete = prepare_browser_event(
        3, model, host, source, event(3, back=True)
    )
    complete^.commit(model, host)
    assert_equal(len(model.pending_history), 0)
    assert_equal(model.effects[len(model.effects) - 1].kind, UInt8(12))
    assert_equal(
        model.effects[len(model.effects) - 1].text, String("https://new/")
    )
    assert_equal(
        model.effects[len(model.effects) - 1].detail, String("typed query")
    )
    assert_true(model.effects[len(model.effects) - 1].flag)
    var failed = prepare_browser_event(
        3,
        model,
        host,
        source,
        event(4, String("denied"), String("https://u:p@x/a?secret#private")),
    )
    failed^.commit(model, host)
    assert_equal(model.current.error.value(), String("https://x/a: denied"))
    var inactive = host.source(11).value().copy()
    var background = prepare_browser_event(
        3,
        model,
        host,
        inactive,
        event(4, String("denied"), String("https://u:p@x/a?secret#private")),
    )
    background^.commit(model, host)
    assert_equal(
        model.tabs[1].content.error.value(),
        String("Could not load https://x/a: denied"),
    )
    model.activate(1)
    assert_equal(model.tabs[0].content.title, String("FIRST"))
    assert_equal(model.tabs[0].content.url, String("https://new/"))
    assert_true(model.tabs[1].started)
    var same = model.current.url
    navigate_browser_tab(model, 1, String("javascript:bad"))
    assert_equal(model.current.url, same)
    synchronize_browser_profile(model, String("next"))
    assert_equal(model.profile, String("next"))
    assert_equal(len(model.tabs), 1)
    assert_equal(model.active_tab, UInt64(1))
    assert_equal(model.next_tab_id, UInt64(2))
    assert_true(not model.current.error)
    var count = len(model.effects)
    synchronize_browser_profile(model, String("next"))
    assert_equal(len(model.effects), count + 1)
    assert_equal(model.effects[count].kind, UInt8(7))
    var pending = prepare_browser_event(
        3, model, host, inactive, event(2, String("STALE"))
    )
    accept_browser_address(model, String("some words"), 1)
    with assert_raises():
        pending^.commit(model, host)
    assert_equal(model.current.title, String())
    assert_equal(
        model.current.url, String("https://duckduckgo.com/?q=some+words")
    )
    var stale_host = prepare_browser_event(
        3, model, host, inactive, event(2, String("OLD SESSION"))
    )
    host.bind(3, 1, 12)
    with assert_raises():
        stale_host^.commit(model, host)
    assert_equal(model.current.title, String())
    var expected_generation = model.generation
    var expected_effects = len(model.effects)
    with assert_raises():
        _ = prepare_browser_event(
            3, model, host, host.source(12).value(), event(8)
        )
    assert_equal(model.generation, expected_generation)
    assert_equal(len(model.effects), expected_effects)
    model.acknowledge_effects(model.generation, len(model.effects))
    assert_equal(len(model.effects), 0)
    with assert_raises():
        model.acknowledge_effects(expected_generation, expected_effects)
    var replacements = List[String]()
    replacements.append(String("https://one/"))
    replacements.append(String("https://two/"))
    replacements.append(String("https://three/"))
    var synchronized = prepare_browser_transition(
        3, model, host, 4, target=2, urls=replacements
    )
    synchronized^.commit(model, host)
    assert_equal(model.active_tab, UInt64(3))
    assert_equal(model.next_tab_id, UInt64(4))
    assert_equal(model.current.url, String("https://three/"))
    assert_equal(model.mux_active, UInt64(2))
    assert_equal(len(model.mux_tabs), 3)
    for item in model.effects:
        assert_true(item.kind != 3)
    var navigate_absent = prepare_browser_transition(
        3, model, host, 3, target=2, input=String("https://absent/")
    )
    navigate_absent^.commit(model, host)
    var delivery = prepare_browser_delivery(3, model, host)
    delivery.validate(model, host)
    var absent = 0
    for item in delivery.items:
        if item.effect.kind == 6 and item.effect.tab != 1:
            assert_true(not item.dispatch)
            absent += 1
    assert_true(absent > 0)
    host.bind(3, 3, 30)
    with assert_raises():
        delivery.validate(model, host)
    var live = prepare_browser_delivery(3, model, host)
    live.validate(model, host)
    for item in live.items:
        if item.effect.kind == 6 and item.effect.tab == 3:
            assert_true(not item.dispatch)
    var original_count = len(model.effects)
    live.acknowledge(model)
    assert_equal(len(model.effects), 0)
    assert_true(original_count > 0)
    with assert_raises():
        live.acknowledge(model)
    print(
        "browser metadata, navigation, history, source receipts and atomic"
        " refusal passed"
    )
