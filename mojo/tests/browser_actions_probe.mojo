from std.testing import assert_equal, assert_false, assert_raises, assert_true
from mjui.input import MOD_CTRL, MOD_SHIFT
from mjui.keys import Key
from zzmojo.browser_state import (
    restore_browser,
    BrowserModel,
    BROWSER_BACK,
    BROWSER_EDIT,
    BROWSER_FOCUS_PAGE,
    BROWSER_NOTIFY,
    BROWSER_RECREATE,
    BROWSER_START_PICK,
    BROWSER_ZOOM,
)
from zzmojo.browser_host import BrowserHost
from zzmojo.browser_wire import BrowserCommand
from zzmojo.browser_controller import (
    BrowserMetadataEvent,
    prepare_browser_event,
    BROWSER_EVENT_LOADING,
    BROWSER_EVENT_LOAD_FAILED,
    BROWSER_EVENT_POPUP_CREATED,
    BROWSER_EVENT_TAB_CLOSED,
)
from zzmojo.browser_actions import (
    prepare_browser_action,
    prepare_browser_command,
    prepare_browser_failure,
    prepare_browser_pick_result,
    prepare_browser_retry,
    BROWSER_PANE_FAILED,
    BROWSER_RUNTIME_FAILED,
)
from zzmojo.browser_input import (
    BrowserInput,
    BROWSER_BUTTON_LEFT,
    BROWSER_BUTTON_RIGHT,
    BROWSER_KEY_CANCEL_PICK,
    BROWSER_KEY_CLEAR_SUGGESTIONS,
    BROWSER_KEY_LEAVE_ADDRESS,
    BROWSER_KEY_PAGE,
    BROWSER_POINTER_DOWN,
    BROWSER_POINTER_LEAVE,
    BROWSER_POINTER_MOVE,
    BROWSER_POINTER_UP,
    BROWSER_POINTER_UP_OUTSIDE,
)
from zzmojo.browser_presentation import (
    browser_presentation,
    browser_tab_label,
    BROWSER_STATE_EMPTY,
    BROWSER_STATE_ERROR,
)
from zzmojo.desktop_native_bindings import DesktopNativeBinding


def act(
    mut model: BrowserModel, host: BrowserHost, kind: UInt8, index: UInt8 = 0
) raises -> Bool:
    return prepare_browser_action(
        3, model, host, DesktopNativeBinding(kind, index)
    ).commit(model, host)


def last(model: BrowserModel) -> UInt8:
    return model.effects[len(model.effects) - 1].kind


def main() raises:
    var urls = List[String]()
    urls.append(String("https://www.www.example.com/a?b"))
    urls.append(String("https://b.test/"))
    var model = restore_browser(urls, 0, String("work"), UInt64(3))
    var host = BrowserHost()
    host.bind(3, 0, 10)
    assert_true(act(model, host, 1))
    assert_true(act(model, host, 1))
    assert_equal(model.current.page_zoom_factor, 1.25)
    assert_equal(model.current.page_zoom_percent, UInt16(125))
    assert_equal(last(model), BROWSER_ZOOM)
    assert_equal(model.effects[len(model.effects) - 1].zoom, 1.25)
    assert_equal(
        model.effects[len(model.effects) - 1].source.value().session,
        UInt64(10),
    )
    _ = act(model, host, 2)
    assert_equal(model.current.page_zoom_percent, UInt16(110))
    _ = act(model, host, 3)
    assert_equal(model.current.page_zoom_factor, 1.0)
    _ = act(model, host, 1)
    _ = act(model, host, 1)
    var count = len(model.effects)
    _ = act(model, host, 10)
    assert_equal(len(model.effects), count)
    var loading = prepare_browser_event(
        3,
        model,
        host,
        host.source(10).value(),
        BrowserMetadataEvent(BROWSER_EVENT_LOADING, back=True),
    )
    loading^.commit(model, host)
    _ = act(model, host, 10)
    assert_equal(last(model), BROWSER_BACK)
    _ = act(model, host, 20)
    assert_equal(last(model), BROWSER_EDIT)
    assert_equal(
        model.effects[len(model.effects) - 1].text,
        String("paste-and-match-style"),
    )
    _ = act(model, host, 14)
    assert_equal(last(model), BROWSER_START_PICK)
    assert_true(model.effects[len(model.effects) - 1].flag)
    prepare_browser_pick_result(3, model, host, True, True).commit(model, host)
    assert_true(model.element_pick_active)
    assert_equal(last(model), BROWSER_FOCUS_PAGE)
    _ = act(model, host, 1)
    assert_equal(model.current.page_zoom_percent, UInt16(150))
    assert_equal(last(model), BROWSER_START_PICK)
    assert_false(model.effects[len(model.effects) - 1].flag)
    prepare_browser_pick_result(3, model, host, False, False).commit(
        model, host
    )
    assert_false(model.element_pick_active)
    assert_equal(last(model), BROWSER_NOTIFY)
    assert_equal(
        model.effects[len(model.effects) - 1].text,
        String("Element picker is unavailable"),
    )
    assert_true(act(model, host, 8, 7))
    assert_equal(model.active_tab, UInt64(0))
    host.bind(3, 2, 77)
    with assert_raises():
        _ = prepare_browser_event(
            3,
            model,
            host,
            host.source(10).value(),
            BrowserMetadataEvent(
                BROWSER_EVENT_POPUP_CREATED,
                url=String("https://popup.test/"),
                session=78,
                foreground=True,
            ),
        )
    prepare_browser_event(
        3,
        model,
        host,
        host.source(10).value(),
        BrowserMetadataEvent(
            BROWSER_EVENT_POPUP_CREATED,
            url=String("https://popup.test/"),
            session=77,
            foreground=True,
        ),
    ).commit(model, host)
    assert_equal(len(model.tabs), 3)
    assert_equal(model.active_tab, UInt64(2))
    assert_true(model.tabs[2].popup)
    assert_equal(model.current.page_zoom_percent, UInt16(150))
    assert_equal(last(model), BROWSER_FOCUS_PAGE)
    assert_equal(model.mux_tabs[2], String("https://popup.test/"))
    var popup_view = browser_presentation(model)
    assert_equal(popup_view.tabs[0].label, String("Browser"))
    assert_equal(
        browser_tab_label(String("https://www.www.example.com/a?b")),
        String("example.com"),
    )
    assert_equal(popup_view.tabs[1].label, String("b.test"))
    assert_equal(popup_view.tabs[2].detail, String("https://popup.test/"))
    assert_equal(popup_view.active, 2)
    prepare_browser_event(
        3,
        model,
        host,
        host.source(77).value(),
        BrowserMetadataEvent(BROWSER_EVENT_TAB_CLOSED),
    ).commit(model, host)
    assert_equal(len(model.tabs), 2)
    assert_equal(model.active_tab, UInt64(1))
    assert_true(act(model, host, 22))
    assert_equal(len(model.tabs), 1)
    assert_false(act(model, host, 22))
    prepare_browser_failure(
        3,
        model,
        host,
        BROWSER_RUNTIME_FAILED,
        String("browser runtime unavailable"),
        False,
    ).commit(model, host)
    var failed = browser_presentation(model)
    assert_equal(failed.state, BROWSER_STATE_ERROR)
    assert_equal(failed.error, String("browser runtime unavailable"))
    assert_false(failed.retry)
    count = len(model.effects)
    prepare_browser_retry(3, model, host).commit(model, host)
    assert_equal(len(model.effects), count)
    prepare_browser_failure(
        3, model, host, BROWSER_PANE_FAILED, String("session lost")
    ).commit(model, host)
    assert_true(browser_presentation(model).retry)
    prepare_browser_retry(3, model, host).commit(model, host)
    assert_equal(last(model), BROWSER_RECREATE)
    assert_equal(
        model.effects[len(model.effects) - 1].text,
        String("https://www.www.example.com/a?b"),
    )
    assert_equal(model.effects[len(model.effects) - 1].zoom, 1.5)
    assert_equal(browser_presentation(model).state, UInt8(0))
    prepare_browser_event(
        3,
        model,
        host,
        host.source(10).value(),
        BrowserMetadataEvent(
            BROWSER_EVENT_LOAD_FAILED,
            String("ERR_FILE_NOT_FOUND"),
            String("file:///tmp/missing.html"),
        ),
    ).commit(model, host)
    var missing = browser_presentation(model)
    assert_equal(missing.error, String("file:///tmp/missing.html: ERR_FILE_NOT_FOUND"))
    assert_true(missing.retry)
    var command = BrowserCommand(0)
    command.text = String("some words")
    prepare_browser_command(3, model, host, command, 1).commit(model, host)
    assert_equal(
        model.current.url, String("https://duckduckgo.com/?q=some+words")
    )
    prepare_browser_command(3, model, host, BrowserCommand(2)).commit(
        model, host
    )
    assert_equal(last(model), BROWSER_BACK)
    _ = act(model, host, 5)
    assert_equal(browser_presentation(model).state, BROWSER_STATE_EMPTY)
    assert_equal(browser_tab_label(String("about:blank")), String("New Tab"))
    assert_equal(browser_tab_label(String("file:///x")), String("New Tab"))
    assert_equal(browser_tab_label(String("localhost:3000/a")), String("localhost:3000"))

    var input = BrowserInput()
    var away = input.pointer(
        BROWSER_POINTER_DOWN, BROWSER_BUTTON_LEFT, 0, 100, 50, 400, 200, 40, 20
    )
    assert_false(away.forward)
    assert_equal(input.buttons, UInt8(0))
    assert_false(
        input.pointer(
            BROWSER_POINTER_UP, BROWSER_BUTTON_LEFT, 0, 100, 50, 400, 200, 40, 20
        ).forward
    )
    var down = input.pointer(
        BROWSER_POINTER_DOWN, BROWSER_BUTTON_LEFT, 0, 100, 50, 400, 200, 110.5, 70.49
    )
    assert_true(down.forward and down.focus_page)
    assert_equal(down.x, Int32(11))
    assert_equal(down.y, Int32(20))
    assert_false(
        input.pointer(
            BROWSER_POINTER_MOVE, 0, BROWSER_BUTTON_RIGHT, 100, 50, 400, 200, 120, 60
        ).forward
    )
    var drag = input.pointer(
        BROWSER_POINTER_MOVE, 0, BROWSER_BUTTON_LEFT, 100, 50, 400, 200, 650, 20
    )
    assert_equal(drag.x, Int32(400))
    assert_equal(drag.y, Int32(0))
    var outside = input.pointer(
        BROWSER_POINTER_UP_OUTSIDE, BROWSER_BUTTON_LEFT, 0, 100, 50, 400, 200, 650, 20
    )
    assert_true(outside.forward)
    assert_equal(input.buttons, UInt8(0))
    assert_false(
        input.pointer(
            BROWSER_POINTER_UP_OUTSIDE, BROWSER_BUTTON_LEFT, 0, 100, 50, 400, 200, 650, 20
        ).forward
    )
    assert_false(
        input.pointer(
            BROWSER_POINTER_LEAVE, 0, 0, 100, 50, 400, 200, 500, 60
        ).forward
    )
    assert_equal(input.key(Key.ESCAPE, 0, False, True).kind, BROWSER_KEY_CANCEL_PICK)
    assert_equal(input.key(Key.A, 0, False, False).kind, BROWSER_KEY_PAGE)
    assert_true(input.key(Key.A, 0, False, False).text_follows)
    assert_false(input.key(Key.A, MOD_CTRL, False, False).text_follows)
    assert_false(input.key(Key.TAB, 0, False, False).text_follows)
    assert_false(input.key(Key.F5, 0, False, False).text_follows)
    input.focus_address()
    input.omnibox_suggestions = 3
    assert_equal(
        input.key(Key.ESCAPE, 0, False, False).kind, BROWSER_KEY_CLEAR_SUGGESTIONS
    )
    assert_equal(input.key(Key.DELETE, MOD_SHIFT, False, False).kind, UInt8(8))
    assert_equal(
        input.key(Key.ESCAPE, 0, False, False).kind, BROWSER_KEY_LEAVE_ADDRESS
    )
    assert_true(input.page_focused and not input.address_editing)
    print("browser actions, zoom, popup adoption, errors, retry and input ownership passed")
