from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true
from mjui.image import ImageCache
from mjui.sys.clock import monotonic_seconds
from mjui.web import WebRuntime, WebOptions, WebPage, web_subprocess_check
from zzmojo.browser_pane import BrowserPane
from zzmojo.browser_input import (
    BROWSER_BUTTON_LEFT,
    BROWSER_POINTER_DOWN,
    BROWSER_POINTER_UP,
)
from zzmojo.browser_presentation import BROWSER_STATE_ERROR
from zzmojo.desktop_native_bindings import DesktopNativeBinding


def tick():
    _ = external_call["usleep", Int32](UInt32(2000))


def step(runtime: WebRuntime, mut pane: BrowserPane, mut images: ImageCache) raises:
    _ = runtime.pump()
    _ = pane.poll(images)
    tick()


def wait_label(
    runtime: WebRuntime,
    mut pane: BrowserPane,
    mut images: ImageCache,
    expected: String,
) raises:
    var deadline = monotonic_seconds() + 15
    while monotonic_seconds() < deadline:
        step(runtime, pane, images)
        var view = pane.presentation()
        if view.tabs[view.active].label == expected:
            return
    var view = pane.presentation()
    raise Error(
        "browser pane tab label did not reach "
        + expected
        + "; saw "
        + view.tabs[view.active].label
    )


def wait_views(
    runtime: WebRuntime, mut pane: BrowserPane, mut images: ImageCache, count: Int
) raises:
    var deadline = monotonic_seconds() + 15
    while monotonic_seconds() < deadline:
        step(runtime, pane, images)
        if WebPage.live_views() == count:
            return
    raise Error("native views stayed at " + String(WebPage.live_views()))


def wait_error(
    runtime: WebRuntime, mut pane: BrowserPane, mut images: ImageCache
) raises -> String:
    var deadline = monotonic_seconds() + 15
    while monotonic_seconds() < deadline:
        step(runtime, pane, images)
        var view = pane.presentation()
        if view.state == BROWSER_STATE_ERROR:
            return view.error
    raise Error("browser pane never presented the load error")


def act(mut pane: BrowserPane, kind: UInt8, index: UInt8 = 0) raises -> Bool:
    return pane.action(DesktopNativeBinding(kind, index))


def main() raises:
    web_subprocess_check()
    var args = argv()
    var root = String(args[1])
    var one = String(args[2])
    var two = String(args[3])
    var missing = String(args[4])
    var opts = WebOptions()
    opts.cache_dir = root + "/cache"
    opts.frame_rate = 30
    opts.switches.append(String("renderer-process-limit=2"))
    var runtime = WebRuntime(opts)
    var images = ImageCache()
    var urls = List[String]()
    urls.append(one)
    urls.append(two)
    var pane = BrowserPane(
        7, urls, 0, String("work"), root + "/cache", 1000, 400, 200, 1
    )
    wait_label(runtime, pane, images, String("one"))
    assert_equal(WebPage.live_views(), 1)
    assert_true(act(pane, 1))
    assert_equal(pane.presentation().page_zoom_percent, UInt16(110))
    var active = pane.active_page()
    assert_equal(pane.pages[active].page.page_zoom_percent(), UInt16(110))
    pane.pages[active].page.eval(
        "requestAnimationFrame(()=>document.title='one:'+devicePixelRatio)"
    )
    wait_label(runtime, pane, images, String("one:1.100000023841858"))
    assert_true(act(pane, 8, 1))
    wait_label(runtime, pane, images, String("two"))
    assert_equal(WebPage.live_views(), 2)
    assert_equal(pane.presentation().page_zoom_percent, UInt16(100))
    assert_true(act(pane, 7))
    assert_equal(pane.presentation().page_zoom_percent, UInt16(110))
    pane.set_focused(True)
    _ = pane.pointer(
        BROWSER_POINTER_DOWN, BROWSER_BUTTON_LEFT, 0, 20, 20, 0
    )
    _ = pane.pointer(BROWSER_POINTER_UP, BROWSER_BUTTON_LEFT, 0, 20, 20, 0)
    wait_label(runtime, pane, images, String("popup:1.100000023841858"))
    var adopted = pane.presentation()
    assert_equal(len(adopted.tabs), 3)
    assert_equal(adopted.active, 2)
    assert_true(pane.model.tabs[2].popup)
    assert_equal(WebPage.live_views(), 3)
    var outbox = pane.take_outbox()
    assert_true(len(outbox) > 0)
    assert_true(act(pane, 22))
    wait_views(runtime, pane, images, 2)
    assert_equal(len(pane.presentation().tabs), 2)
    pane.navigate(missing)
    var error = wait_error(runtime, pane, images)
    assert_equal(error, missing + ": ERR_FILE_NOT_FOUND")
    assert_true(pane.presentation().retry)
    pane.retry()
    assert_true(pane.presentation().state != BROWSER_STATE_ERROR)
    error = wait_error(runtime, pane, images)
    assert_equal(error, missing + ": ERR_FILE_NOT_FOUND")
    wait_views(runtime, pane, images, 2)
    pane.navigate(one)
    wait_label(runtime, pane, images, String("one"))
    assert_true(act(pane, 21))
    active = pane.active_page()
    pane.pages[active].page.eval(
        "setTimeout(()=>document.title='selected:'+getSelection().toString().length,50)"
    )
    var deadline = monotonic_seconds() + 15
    while monotonic_seconds() < deadline:
        step(runtime, pane, images)
        var label = pane.presentation().tabs[pane.presentation().active].label
        if label.startswith("selected:") and label != "selected:0":
            break
    assert_true(
        pane.presentation().tabs[pane.presentation().active].label
        != "selected:0"
    )
    assert_true(not act(pane, 22) or len(pane.presentation().tabs) == 1)
    pane.close()
    wait_views(runtime, pane, images, 0)
    _ = pane^
    runtime.shutdown()
    print(
        "browser pane restored, zoomed, switched, adopted a popup, closed it,"
        " presented and retried a load error and selected all on the engine"
    )
