from std.sys import argv
from std.testing import assert_equal, assert_raises, assert_true
from browser_fixture import Source
from zzmojo.browser_url import browser_url, browser_profile, browser_load_error
from zzmojo.browser_state import (
    BrowserContent,
    BrowserTab,
    BrowserModel,
    restore_browser,
)


def read_content(mut source: Source) raises -> BrowserContent:
    var content = BrowserContent(source.text())
    content.title = source.text()
    content.loading = source.uint(1) != 0
    content.can_go_back = source.uint(1) != 0
    content.can_go_forward = source.uint(1) != 0
    var bits = source.uint(8)
    content.page_zoom_factor = Pointer(to=bits).unsafe_bitcast[Float64]()[]
    content.page_zoom_percent = UInt16(source.uint(2))
    if source.uint(1) != 0:
        content.error = source.text()
    return content^


def check_content(mut source: Source, content: BrowserContent) raises:
    assert_equal(content.url, source.text())
    assert_equal(content.title, source.text())
    assert_equal(content.loading, source.uint(1) != 0)
    assert_equal(content.can_go_back, source.uint(1) != 0)
    assert_equal(content.can_go_forward, source.uint(1) != 0)
    assert_equal(
        Pointer(to=content.page_zoom_factor).unsafe_bitcast[UInt64]()[],
        source.uint(8),
    )
    assert_equal(content.page_zoom_percent, UInt16(source.uint(2)))
    var error = source.uint(1) != 0
    assert_equal(Bool(content.error), error)
    if error:
        assert_equal(content.error.value(), source.text())


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    assert_true(len(bytes) < 4 * 1024 * 1024)
    var source = Source(bytes^)
    var urls = source.uint(4)
    for _ in range(Int(urls)):
        var operation = UInt32(source.uint(1))
        var provider = UInt32(source.uint(1))
        var input = source.text()
        var actual = browser_url(input, operation, provider)
        assert_equal(actual.status, UInt8(source.uint(1)))
        assert_equal(actual.text, source.text())
    var profiles = source.uint(4)
    for _ in range(Int(profiles)):
        var input = source.text()
        var valid = source.uint(1) != 0
        var expected = source.text()
        if valid:
            assert_equal(browser_profile(input), expected)
        else:
            with assert_raises():
                _ = browser_profile(input)
    var descriptors = source.uint(4)
    for _ in range(Int(descriptors)):
        var count = Int(source.uint(4))
        var inputs = List[String]()
        for _ in range(count):
            inputs.append(source.text())
        var active = source.uint(8)
        var profile = source.text()
        var model = restore_browser(inputs, active, profile)
        assert_equal(len(model.tabs), Int(source.uint(4)))
        assert_equal(model.active_tab, source.uint(8))
        assert_equal(model.next_tab_id, source.uint(8))
        assert_equal(model.current.url, source.text())
        assert_equal(model.profile, source.text())
        assert_equal(model.initial_url_error, UInt8(source.uint(1)))
        for tab in model.tabs:
            assert_equal(tab.id, source.uint(8))
            assert_equal(tab.content.url, source.text())
            assert_equal(tab.started, source.uint(1) != 0)
            assert_equal(tab.popup, source.uint(1) != 0)
            assert_equal(tab.content.title, String())
            assert_equal(tab.content.page_zoom_factor, Float64(1))
            assert_equal(tab.content.page_zoom_percent, UInt16(100))
        assert_equal(model.active_index(), Int(source.uint(4)))
        for id_index in range(5):
            var id = UInt64(id_index) if id_index < 4 else UInt64.MAX
            var expected = source.uint(8)
            var actual = model.neighbor(id)
            assert_equal(actual.value() if actual else UInt64.MAX, expected)
        for offset_index in range(5):
            var offset = (
                -17 if offset_index
                == 0 else -1 if offset_index
                == 1 else 0 if offset_index
                == 2 else 1 if offset_index
                == 3 else 17
            )
            var expected = source.uint(8)
            var actual = model.cycle(Int64(offset))
            assert_equal(actual.value() if actual else UInt64.MAX, expected)
    var initial = List[String]()
    initial.append(String("old"))
    var model = restore_browser(initial, 0, String("default"))
    model.current = BrowserContent(String("https://new/"))
    model.current.title = String("title\0東京")
    model.current.loading = True
    model.current.can_go_back = True
    model.current.page_zoom_factor = 1.25
    model.current.page_zoom_percent = 125
    model.current.error = String("ERR")
    model.save_active()
    var saved = model.tabs[0].content.copy()
    assert_equal(saved.url, source.text())
    assert_equal(saved.title, source.text())
    assert_equal(saved.loading, source.uint(1) != 0)
    assert_equal(saved.can_go_back, source.uint(1) != 0)
    assert_equal(saved.can_go_forward, source.uint(1) != 0)
    assert_equal(
        Pointer(to=saved.page_zoom_factor).unsafe_bitcast[UInt64]()[],
        source.uint(8),
    )
    assert_equal(saved.page_zoom_percent, UInt16(source.uint(2)))
    assert_equal(saved.error.value(), source.text())
    var switches = source.uint(4)
    for _ in range(Int(switches)):
        var candidate = BrowserModel()
        var count = Int(source.uint(4))
        candidate.active_tab = source.uint(8)
        var target = source.uint(8)
        candidate.next_tab_id = UInt64(count)
        for _ in range(count):
            var id = source.uint(8)
            var tab = BrowserTab(id, String())
            tab.content = read_content(source)
            tab.started = source.uint(1) != 0
            tab.popup = source.uint(1) != 0
            candidate.tabs.append(tab^)
        candidate.current = read_content(source)
        candidate.activate(target)
        assert_equal(candidate.active_tab, source.uint(8))
        check_content(source, candidate.current)
        for tab in candidate.tabs:
            check_content(source, tab.content)
            assert_equal(tab.started, source.uint(1) != 0)
            assert_equal(tab.popup, source.uint(1) != 0)
    var errors = source.uint(4)
    for _ in range(Int(errors)):
        var url = source.text()
        var description = source.text()
        var background = source.uint(1) != 0
        assert_equal(
            browser_load_error(url, description, background), source.text()
        )
    assert_equal(source.offset, len(source.bytes))
    var opened = model.open_tab(String("localhost:8080"), True)
    assert_equal(opened, UInt64(1))
    assert_equal(model.current.url, String("http://localhost:8080/"))
    model.close_tab(opened)
    assert_equal(model.active_tab, UInt64(0))
    assert_equal(model.current.title, String("title\0東京"))
    assert_equal(model.current.error.value(), String("ERR"))
    var before = model.generation
    var original = model.current.url
    model.next_tab_id = UInt64.MAX
    with assert_raises():
        _ = model.open_tab(None, False)
    assert_equal(model.generation, before)
    assert_equal(model.current.url, original)
    print(urls, profiles, descriptors, switches, errors)
