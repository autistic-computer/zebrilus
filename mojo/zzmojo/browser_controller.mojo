from std.memory import ArcPointer
from .browser_url import (
    browser_url,
    browser_profile,
    browser_load_error,
    MAX_BROWSER_URL_BYTES,
)
from .browser_state import (
    BrowserModel,
    BrowserEffect,
    BrowserHistoryUse,
    BrowserTab,
    MAX_BROWSER_TABS,
    MAX_BROWSER_RETAINED_BYTES,
)
from .browser_host import BrowserHost, BrowserSource
from .palette_text import palette_trim


@fieldwise_init
struct BrowserMetadataEvent(Copyable, Movable):
    var kind: UInt8
    var text: String
    var url: String
    var loading: Bool
    var back: Bool
    var forward: Bool
    var payload: List[UInt8]


@fieldwise_init
struct BrowserPlan(Movable):
    var generation: UInt64
    var expected: List[UInt8]
    var source: Optional[BrowserSource]
    var host_generation: UInt64
    var host_actor: ArcPointer[UInt8]
    var model: BrowserModel

    def commit(var self, mut current: BrowserModel, host: BrowserHost) raises:
        current.validate()
        if (
            not host.same_actor(self.host_actor)
            or host.generation != self.host_generation
        ):
            raise Error("browser host changed after preparation")
        if self.source:
            host.validate(self.source.value())
        if (
            current.generation != self.generation
            or not current.matches_identity(self.expected)
        ):
            raise Error("browser plan was prepared against stale tab state")
        self.model.validate()
        var next = BrowserModel()
        swap(self.model, next)
        current._commit(next^)


def browser_metadata_supported(kind: UInt8) -> Bool:
    return kind <= 7


@inline(.never)
def prepare_browser_event(
    pane: UInt64,
    model: BrowserModel,
    host: BrowserHost,
    source: BrowserSource,
    event: BrowserMetadataEvent,
) raises -> BrowserPlan:
    model.validate()
    if not model.pane or model.pane.value() != pane:
        raise Error("browser state belongs to another pane")
    host.validate(source)
    if source.pane != pane:
        raise Error("browser event belongs to a different pane")
    if not browser_metadata_supported(event.kind):
        raise Error("browser event requires its native host consumer")
    if (
        event.text.byte_length() > MAX_BROWSER_URL_BYTES
        or event.url.byte_length() > MAX_BROWSER_URL_BYTES
        or len(event.payload) > 8192
    ):
        raise Error("browser event exceeds retained input admission")
    var next = model.copy()
    var active = source.tab == next.active_tab
    var index = next.index(source.tab)
    if event.kind == 6:
        next.drop_history(source.tab)
        if active:
            next.element_pick_active = False
    elif index < 0:
        raise Error("browser event targets a removed tab")
    elif event.kind == 0:
        if active:
            next.current.error = None
            next.recoverable = True
    elif event.kind == 1:
        if active:
            next.current.url = event.text
        else:
            next.tabs[index].content.url = event.text
        next.effects.append(
            BrowserEffect(9, source.tab, event.text, 0, next.profile)
        )
        next.publish()
    elif event.kind == 2:
        var url = next.current.url if active else next.tabs[index].content.url
        if active:
            next.current.title = event.text
        else:
            next.tabs[index].content.title = event.text
        next.effects.append(
            BrowserEffect(10, source.tab, url, 0, next.profile, event.text)
        )
    elif event.kind == 3:
        var content = next.current.copy() if active else next.tabs[
            index
        ].content.copy()
        content.loading = event.loading
        content.can_go_back = event.back
        content.can_go_forward = event.forward
        if event.loading:
            content.error = None
            next.history_started(source.tab)
        else:
            next.history_completed(
                source.tab, content.url, not Bool(content.error)
            )
        if active:
            next.current = content^
        else:
            next.tabs[index].content = content^
    elif event.kind == 4:
        var message = browser_load_error(event.url, event.text, not active)
        if active:
            next.current.error = message^
            next.recoverable = True
        else:
            next.tabs[index].content.error = message^
    elif event.kind == 5:
        if active:
            next.drop_history(source.tab)
            next.element_pick_active = False
            next.current.error = String("Renderer stopped: ") + event.text
            next.recoverable = True
    elif event.kind == 7:
        next.effects.append(
            BrowserEffect(
                11,
                source.tab,
                event.url,
                0,
                next.profile,
                String(),
                False,
                event.payload,
            )
        )
    next.validate()
    _freeze_host_effects(pane, len(model.effects), next, host)
    return BrowserPlan(
        model.generation,
        model.identity(),
        Optional(source.copy()),
        host.generation,
        host.actor.copy(),
        next^,
    )


@inline(.never)
def navigate_browser_tab(
    mut model: BrowserModel, index: Int, input: String
) raises:
    var result = browser_url(input)
    if result.status != 0 or index < 0 or index >= len(model.tabs):
        return
    var next = model.copy()
    var id = next.tabs[index].id
    next.drop_history(id)
    if id == next.active_tab:
        if result.text != next.current.url:
            next.element_pick_active = False
            next.effects.append(BrowserEffect(4, id, String(), 0))
            next.current.error = None
            next.current.url = result.text
            next.effects.append(BrowserEffect(6, id, result.text, 0))
    elif result.text != next.tabs[index].content.url:
        next.tabs[index].content.url = result.text
        if next.tabs[index].started:
            next.effects.append(BrowserEffect(6, id, result.text, 0))
    next.validate()
    model._commit(next^)


@inline(.never)
def synchronize_browser_tabs(
    mut model: BrowserModel, urls: List[String], active: UInt64
) raises:
    if len(urls) == 0:
        return
    if len(urls) > MAX_BROWSER_TABS:
        raise Error("browser synchronized tabs exceed admission")
    var same = model.mux_active == active and len(model.mux_tabs) == len(urls)
    if same:
        for i in range(len(urls)):
            if model.mux_tabs[i] != urls[i]:
                same = False
                break
    if same:
        return
    var size = 0
    for url in urls:
        if url.byte_length() > MAX_BROWSER_URL_BYTES:
            raise Error("browser synchronized URL exceeds admission")
        size += url.byte_length()
    if size > MAX_BROWSER_RETAINED_BYTES:
        raise Error("browser synchronized text exceeds admission")
    var next = model.copy()
    next.mux_tabs = urls.copy()
    next.mux_active = active
    next.applying_mux = True
    while len(next.tabs) > len(urls):
        next.close_tab(next.tabs[len(next.tabs) - 1].id)
    for i in range(len(urls)):
        if i < len(next.tabs):
            navigate_browser_tab(next, i, urls[i])
        else:
            if next.next_tab_id == UInt64.MAX:
                raise Error("browser tab identity exhausted")
            var result = browser_url(urls[i])
            next.tabs.append(
                BrowserTab(
                    next.next_tab_id,
                    result.text if result.status
                    == 0 else String("about:blank"),
                    False,
                )
            )
            next.next_tab_id += 1
    next._activate(next.tabs[Int(min(active, UInt64(len(next.tabs) - 1)))].id)
    next.applying_mux = False
    next.validate()
    model._commit(next^)


@inline(.never)
def synchronize_browser_profile(mut model: BrowserModel, input: String) raises:
    var profile: String
    try:
        profile = browser_profile(input)
    except:
        return
    var next = model.copy()
    next.effects.append(BrowserEffect(7, next.active_tab, profile, 0))
    if next.profile != profile:
        next.pending_history.clear()
        next.profile = profile
        var keep = List[BrowserTab]()
        for tab in next.tabs:
            if tab.id == next.active_tab:
                keep.append(tab.copy())
            else:
                next.effects.append(BrowserEffect(2, tab.id, String(), 0))
        next.tabs = keep^
        next.element_pick_active = False
        next.effects.append(BrowserEffect(4, next.active_tab, String(), 0))
        next.current.error = None
        next.effects.append(
            BrowserEffect(
                8,
                next.active_tab,
                next.current.url,
                next.current.page_zoom_factor,
                next.profile,
            )
        )
    next.validate()
    model._commit(next^)


@inline(.never)
def accept_browser_address(
    mut model: BrowserModel,
    input: String,
    provider: UInt32,
    history: Optional[BrowserHistoryUse] = None,
) raises:
    if palette_trim(input).byte_length() == 0:
        return
    var result = browser_url(input, 1, provider)
    var next = model.copy()
    next.drop_history(next.active_tab)
    if result.status == 0:
        next.element_pick_active = False
        next.effects.append(BrowserEffect(4, next.active_tab, String(), 0))
        next.current.error = None
        next.current.url = result.text
        if history:
            var pending = history.value().copy()
            pending.tab = next.active_tab
            next.pending_history.append(pending^)
        next.effects.append(BrowserEffect(6, next.active_tab, result.text, 0))
    else:
        next.current.error = (
            String("enter a URL") if result.status
            == 1 else String(
                "only http, https, file, and about:blank are supported"
            ) if result.status
            == 2 else String("the address is not a valid URL")
        )
    next.validate()
    model._commit(next^)


@inline(.never)
def _freeze_host_effects(
    pane: UInt64, start: Int, mut model: BrowserModel, host: BrowserHost
):
    for i in range(start, len(model.effects)):
        if model.effects[i].kind == 4 or model.effects[i].kind == 6:
            for entry in host.sessions:
                if entry.pane == pane and entry.tab == model.effects[i].tab:
                    model.effects[i].source = host.source(entry.session)
                    break
            model.effects[i].source_bound = True


@inline(.never)
def prepare_browser_transition(
    pane: UInt64,
    model: BrowserModel,
    host: BrowserHost,
    kind: UInt8,
    target: UInt64 = 0,
    input: Optional[String] = None,
    foreground: Bool = False,
    urls: List[String] = List[String](),
    provider: UInt32 = 0,
    history: Optional[BrowserHistoryUse] = None,
    offset: Int64 = 0,
) raises -> BrowserPlan:
    model.validate()
    if not model.pane or model.pane.value() != pane:
        raise Error("browser state belongs to another pane")
    var next = model.copy()
    if kind == 0:
        next.activate(target)
    elif kind == 1:
        _ = next.open_tab(input, foreground)
    elif kind == 2:
        next.close_tab(target)
    elif kind == 3:
        if not input or target > UInt64(Int.MAX):
            raise Error("browser navigation transition has invalid arguments")
        navigate_browser_tab(next, Int(target), input.value())
    elif kind == 4:
        synchronize_browser_tabs(next, urls, target)
    elif kind == 5:
        if not input:
            raise Error("browser profile transition has no profile")
        synchronize_browser_profile(next, input.value())
    elif kind == 6:
        if not input:
            raise Error("browser address transition has no address")
        accept_browser_address(next, input.value(), provider, history)
    elif kind == 7:
        var target = next.cycle(offset)
        if target:
            next.activate(target.value())
    else:
        raise Error("unknown browser transition")
    _freeze_host_effects(pane, len(model.effects), next, host)
    next.validate()
    return BrowserPlan(
        model.generation,
        model.identity(),
        None,
        host.generation,
        host.actor.copy(),
        next^,
    )
