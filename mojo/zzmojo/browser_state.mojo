from std.memory import ArcPointer
from .browser_host import BrowserSource
from .protocol import append_uint, append_text
from .browser_url import browser_url, browser_profile, MAX_BROWSER_URL_BYTES

comptime MAX_BROWSER_TABS = 4096
comptime MAX_BROWSER_RETAINED_BYTES = 4 * 1024 * 1024

comptime BROWSER_SHOW: UInt8 = 0
comptime BROWSER_REQUEST: UInt8 = 1
comptime BROWSER_CLOSE: UInt8 = 2
comptime BROWSER_PUBLISH: UInt8 = 3
comptime BROWSER_CANCEL_PICK: UInt8 = 4
comptime BROWSER_FOCUS_ADDRESS: UInt8 = 5
comptime BROWSER_NAVIGATE: UInt8 = 6
comptime BROWSER_PROFILE: UInt8 = 7
comptime BROWSER_RECREATE: UInt8 = 8
comptime BROWSER_VISIT: UInt8 = 9
comptime BROWSER_TITLE: UInt8 = 10
comptime BROWSER_FAVICON: UInt8 = 11
comptime BROWSER_HISTORY_USE: UInt8 = 12
comptime BROWSER_ZOOM: UInt8 = 13
comptime BROWSER_BACK: UInt8 = 14
comptime BROWSER_FORWARD: UInt8 = 15
comptime BROWSER_RELOAD: UInt8 = 16
comptime BROWSER_DEVTOOLS: UInt8 = 17
comptime BROWSER_EDIT: UInt8 = 18
comptime BROWSER_START_PICK: UInt8 = 19
comptime BROWSER_FOCUS_PAGE: UInt8 = 20
comptime BROWSER_NOTIFY: UInt8 = 21
comptime BROWSER_CLIPBOARD: UInt8 = 22
comptime BROWSER_LAST_EFFECT = BROWSER_CLIPBOARD


def browser_effect_native(kind: UInt8) -> Bool:
    return (
        kind == BROWSER_CANCEL_PICK
        or kind == BROWSER_NAVIGATE
        or kind >= BROWSER_ZOOM
        and kind <= BROWSER_START_PICK
    )


struct BrowserContent(Copyable, Movable):
    var url: String
    var title: String
    var loading: Bool
    var can_go_back: Bool
    var can_go_forward: Bool
    var page_zoom_factor: Float64
    var page_zoom_percent: UInt16
    var error: Optional[String]

    def __init__(out self, url: String = String("about:blank")):
        self.url = url
        self.title = String()
        self.loading = False
        self.can_go_back = False
        self.can_go_forward = False
        self.page_zoom_factor = 1
        self.page_zoom_percent = 100
        self.error = None

    def retained_bytes(self) -> Int:
        return (
            self.url.byte_length()
            + self.title.byte_length()
            + (self.error.value().byte_length() if self.error else 0)
        )


struct BrowserTab(Copyable, Movable):
    var id: UInt64
    var content: BrowserContent
    var started: Bool
    var popup: Bool

    def __init__(out self, id: UInt64, url: String, started: Bool = True):
        self.id = id
        self.content = BrowserContent(url)
        self.started = started
        self.popup = False


struct BrowserEffect(Copyable, Movable):
    var kind: UInt8
    var tab: UInt64
    var text: String
    var zoom: Float64
    var profile: String
    var detail: String
    var flag: Bool
    var payload: List[UInt8]
    var urls: List[String]
    var active: UInt64
    var source: Optional[BrowserSource]
    var source_bound: Bool

    def __init__(
        out self,
        kind: UInt8,
        tab: UInt64,
        text: String,
        zoom: Float64,
        profile: String = String(),
        detail: String = String(),
        flag: Bool = False,
        payload: List[UInt8] = List[UInt8](),
        urls: List[String] = List[String](),
        active: UInt64 = 0,
    ):
        self.kind = kind
        self.tab = tab
        self.text = text
        self.zoom = zoom
        self.profile = profile
        self.detail = detail
        self.flag = flag
        self.payload = payload.copy()
        self.urls = urls.copy()
        self.active = active
        self.source = None
        self.source_bound = False


@fieldwise_init
struct BrowserHistoryUse(Copyable, Movable):
    var tab: UInt64
    var profile: String
    var input: String
    var selected: Bool
    var started: Bool


struct BrowserModel(Copyable, Movable):
    var actor: ArcPointer[UInt8]
    var pane: Optional[UInt64]
    var tabs: List[BrowserTab]
    var active_tab: UInt64
    var next_tab_id: UInt64
    var current: BrowserContent
    var profile: String
    var mux_tabs: List[String]
    var mux_active: UInt64
    var applying_mux: Bool
    var effects: List[BrowserEffect]
    var generation: UInt64
    var initial_url_error: UInt8
    var pending_history: List[BrowserHistoryUse]
    var recoverable: Bool
    var element_pick_active: Bool

    def __init__(out self, pane: Optional[UInt64] = None):
        self.actor = ArcPointer(UInt8(0))
        self.pane = pane
        self.tabs = List[BrowserTab]()
        self.active_tab = 0
        self.next_tab_id = 0
        self.current = BrowserContent()
        self.profile = String("default")
        self.mux_tabs = List[String]()
        self.mux_active = 0
        self.applying_mux = False
        self.effects = List[BrowserEffect]()
        self.generation = 0
        self.initial_url_error = 0
        self.pending_history = List[BrowserHistoryUse]()
        self.recoverable = True
        self.element_pick_active = False

    def index(self, id: UInt64) -> Int:
        for i in range(len(self.tabs)):
            if self.tabs[i].id == id:
                return i
        return -1

    def active_index(self) -> Int:
        return max(self.index(self.active_tab), 0)

    def neighbor(self, id: UInt64) -> Optional[UInt64]:
        var index = self.index(id)
        if len(self.tabs) <= 1 or index < 0:
            return None
        return self.tabs[
            index + 1 if index + 1 < len(self.tabs) else index - 1
        ].id

    def cycle(self, offset: Int64) raises -> Optional[UInt64]:
        if len(self.tabs) <= 1:
            return None
        if (
            offset < Int64.MIN + MAX_BROWSER_TABS
            or offset > Int64.MAX - MAX_BROWSER_TABS
        ):
            raise Error("browser cycle offset exceeds source arithmetic range")
        var count = Int64(len(self.tabs))
        var next = (Int64(self.active_index()) + offset) % count
        if next < 0:
            next += count
        return self.tabs[Int(next)].id

    def identity(self) -> List[UInt8]:
        var bytes = List[UInt8]()
        append_uint(bytes, UInt64(Int(Pointer(to=self.actor[]))))
        bytes.append(UInt8(Bool(self.pane)))
        if self.pane:
            append_uint(bytes, self.pane.value())
        append_uint(bytes, self.active_tab)
        append_uint(bytes, self.next_tab_id)
        append_uint(bytes, self.mux_active)
        append_uint(bytes, self.generation)
        bytes.append(UInt8(self.applying_mux))
        bytes.append(self.initial_url_error)
        bytes.append(UInt8(self.recoverable))
        bytes.append(UInt8(self.element_pick_active))
        append_text(bytes, self.profile)
        _append_content(bytes, self.current)
        append_uint(bytes, UInt64(len(self.tabs)))
        for tab in self.tabs:
            append_uint(bytes, tab.id)
            _append_content(bytes, tab.content)
            bytes.append(UInt8(tab.started))
            bytes.append(UInt8(tab.popup))
        append_uint(bytes, UInt64(len(self.mux_tabs)))
        for url in self.mux_tabs:
            append_text(bytes, url)
        append_uint(bytes, UInt64(len(self.pending_history)))
        for item in self.pending_history:
            append_uint(bytes, item.tab)
            append_text(bytes, item.profile)
            append_text(bytes, item.input)
            bytes.append(UInt8(item.selected))
            bytes.append(UInt8(item.started))
        append_uint(bytes, UInt64(len(self.effects)))
        for effect in self.effects:
            bytes.append(effect.kind)
            append_uint(bytes, effect.tab)
            append_text(bytes, effect.text)
            append_uint(
                bytes, Pointer(to=effect.zoom).unsafe_bitcast[UInt64]()[]
            )
            append_text(bytes, effect.profile)
            append_text(bytes, effect.detail)
            bytes.append(UInt8(effect.flag))
            append_uint(bytes, UInt64(len(effect.payload)))
            bytes.extend(Span(effect.payload))
            append_uint(bytes, UInt64(len(effect.urls)))
            for url in effect.urls:
                append_text(bytes, url)
            append_uint(bytes, effect.active)
            bytes.append(UInt8(effect.source_bound))
            bytes.append(UInt8(Bool(effect.source)))
            if effect.source:
                append_uint(bytes, effect.source.value().pane)
                append_uint(bytes, effect.source.value().tab)
                append_uint(bytes, effect.source.value().session)
                append_uint(bytes, effect.source.value().generation)
                append_uint(
                    bytes,
                    UInt64(Int(Pointer(to=effect.source.value().actor[]))),
                )
        return bytes^

    def matches_identity(self, expected: List[UInt8]) -> Bool:
        var actual = self.identity()
        if len(actual) != len(expected):
            return False
        for i in range(len(actual)):
            if actual[i] != expected[i]:
                return False
        return True

    def validate(self) raises:
        if len(self.tabs) == 0 or len(self.tabs) > MAX_BROWSER_TABS:
            raise Error("browser tab count exceeds admission")
        if len(self.effects) > MAX_BROWSER_TABS * 8:
            raise Error("browser pending effects exceed admission")
        var records = (
            len(self.tabs)
            + len(self.mux_tabs)
            + len(self.effects)
            + len(self.pending_history)
        )
        var size = self.profile.byte_length() + self.current.retained_bytes()
        for tab in self.tabs:
            size += tab.content.retained_bytes()
        for url in self.mux_tabs:
            size += url.byte_length()
        for effect in self.effects:
            records += len(effect.urls)
            size += (
                effect.text.byte_length()
                + effect.profile.byte_length()
                + effect.detail.byte_length()
                + len(effect.payload)
            )
            for url in effect.urls:
                size += url.byte_length()
        if len(self.pending_history) > MAX_BROWSER_TABS:
            raise Error("browser pending history exceeds admission")
        for pending in self.pending_history:
            size += pending.profile.byte_length() + pending.input.byte_length()
        if records > 65536:
            raise Error("browser retained record count exceeds admission")
        if size > MAX_BROWSER_RETAINED_BYTES:
            raise Error("browser retained text exceeds admission")

    def drop_history(mut self, id: UInt64):
        var pending = List[BrowserHistoryUse]()
        for item in self.pending_history:
            if item.tab != id:
                pending.append(item.copy())
        self.pending_history = pending^

    def history_started(mut self, id: UInt64):
        for i in range(len(self.pending_history)):
            if self.pending_history[i].tab == id:
                self.pending_history[i].started = True

    def history_completed(mut self, id: UInt64, url: String, succeeded: Bool):
        var completed = Optional[BrowserHistoryUse](None)
        for item in self.pending_history:
            if item.tab == id and item.started:
                completed = item.copy()
                break
        if completed:
            if succeeded:
                self.effects.append(
                    BrowserEffect(
                        12,
                        id,
                        url,
                        0,
                        completed.value().profile,
                        completed.value().input,
                        completed.value().selected,
                    )
                )
            self.drop_history(id)

    def acknowledge_effects(mut self, generation: UInt64, count: Int) raises:
        if self.generation != generation or len(self.effects) != count:
            raise Error("browser effects receipt is stale")
        if self.generation == UInt64.MAX:
            raise Error("browser model generation exhausted")
        self.effects.clear()
        self.generation += 1

    def save_active(mut self):
        if len(self.tabs) != 0:
            self.tabs[self.active_index()].content = self.current.copy()

    def publish(mut self):
        if self.applying_mux:
            return
        var urls = List[String]()
        for tab in self.tabs:
            urls.append(
                self.current.url if tab.id
                == self.active_tab else tab.content.url
            )
        var index = UInt64(self.active_index())
        var same = index == self.mux_active and len(urls) == len(self.mux_tabs)
        if same:
            for i in range(len(urls)):
                if urls[i] != self.mux_tabs[i]:
                    same = False
                    break
        if same:
            return
        self.mux_tabs = urls^
        self.mux_active = index
        self.effects.append(
            BrowserEffect(
                3,
                self.active_tab,
                String(),
                0,
                urls=self.mux_tabs,
                active=index,
            )
        )

    def _activate(mut self, id: UInt64):
        var index = self.index(id)
        if id == self.active_tab or index < 0:
            return
        self.effects.append(BrowserEffect(4, self.active_tab, String(), 0))
        self.element_pick_active = False
        self.save_active()
        self.active_tab = id
        self.current = self.tabs[index].content.copy()
        self.effects.append(BrowserEffect(0, id, String(), 0))
        if not self.tabs[index].started:
            self.tabs[index].started = True
            self.effects.append(
                BrowserEffect(
                    1,
                    id,
                    self.current.url,
                    self.current.page_zoom_factor,
                    self.profile,
                )
            )
        self.publish()

    @inline(.never)
    def activate(mut self, id: UInt64) raises:
        var next = self.copy()
        next._activate(id)
        next.validate()
        self._commit(next^)

    def _commit(mut self, var next: Self) raises:
        if self.generation == UInt64.MAX:
            raise Error("browser model generation exhausted")
        next.generation = self.generation + 1
        self = next^

    @inline(.never)
    def open_tab(
        mut self, url: Optional[String], foreground: Bool
    ) raises -> UInt64:
        if len(self.tabs) >= MAX_BROWSER_TABS or self.next_tab_id == UInt64.MAX:
            raise Error("browser tab identity or count exhausted")
        var canonical = String("about:blank")
        if url:
            var result = browser_url(url.value())
            if result.status == 0:
                canonical = result.text
        var next = self.copy()
        var id = next.next_tab_id
        next.next_tab_id += 1
        next.tabs.append(BrowserTab(id, canonical))
        if foreground:
            next._activate(id)
        next.effects.append(BrowserEffect(1, id, canonical, 1, next.profile))
        if foreground and next.current.url == "about:blank":
            next.effects.append(BrowserEffect(5, id, String(), 0))
        next.publish()
        next.validate()
        self._commit(next^)
        return id

    @inline(.never)
    def close_tab(mut self, id: UInt64) raises:
        if len(self.tabs) <= 1 or self.index(id) < 0:
            return
        var next = self.copy()
        if id == next.active_tab:
            next._activate(next.neighbor(id).value())
        var tabs = List[BrowserTab]()
        for tab in next.tabs:
            if tab.id != id:
                tabs.append(tab.copy())
        next.tabs = tabs^
        next.drop_history(id)
        next.effects.append(BrowserEffect(2, id, String(), 0))
        next.publish()
        next.validate()
        self._commit(next^)


@inline(.never)
def restore_browser(
    urls: List[String],
    active: UInt64,
    profile: String,
    pane: Optional[UInt64] = None,
) raises -> BrowserModel:
    if len(urls) > MAX_BROWSER_TABS:
        raise Error("browser restored tab count exceeds admission")
    var retained = profile.byte_length()
    for url in urls:
        if url.byte_length() > MAX_BROWSER_URL_BYTES:
            raise Error("browser restored URL exceeds admission")
        retained += url.byte_length()
    if retained > MAX_BROWSER_RETAINED_BYTES:
        raise Error("browser restored text exceeds admission")
    var model = BrowserModel(pane)
    model.mux_tabs = urls.copy()
    model.mux_active = active
    var normalized = browser_url(
        urls[Int(active)] if active
        < UInt64(len(urls)) else (
            urls[0] if len(urls) else String("about:blank")
        )
    )
    model.initial_url_error = normalized.status
    model.current = BrowserContent(
        normalized.text if normalized.status == 0 else String("about:blank")
    )
    model.current.title = String("Browser")
    try:
        model.profile = browser_profile(profile)
    except:
        model.profile = String("default")
    var selected = min(active, UInt64(max(len(urls) - 1, 0)))
    for i in range(len(urls)):
        var url = model.current.url
        if UInt64(i) != selected:
            var result = browser_url(urls[i])
            url = result.text if result.status == 0 else String("about:blank")
        model.tabs.append(BrowserTab(UInt64(i), url, UInt64(i) == selected))
    if len(model.tabs) == 0:
        model.tabs.append(BrowserTab(0, model.current.url))
    model.active_tab = selected
    model.next_tab_id = UInt64(len(model.tabs))
    model.validate()
    return model^


@inline(.never)
def _append_content(mut bytes: List[UInt8], content: BrowserContent):
    append_text(bytes, content.url)
    append_text(bytes, content.title)
    bytes.append(UInt8(content.loading))
    bytes.append(UInt8(content.can_go_back))
    bytes.append(UInt8(content.can_go_forward))
    append_uint(
        bytes, Pointer(to=content.page_zoom_factor).unsafe_bitcast[UInt64]()[]
    )
    append_uint(bytes, UInt64(content.page_zoom_percent))
    bytes.append(UInt8(Bool(content.error)))
    if content.error:
        append_text(bytes, content.error.value())
