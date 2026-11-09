from .browser_state import BrowserModel

comptime BROWSER_STATE_PAGE: UInt8 = 0
comptime BROWSER_STATE_EMPTY: UInt8 = 1
comptime BROWSER_STATE_ERROR: UInt8 = 2
comptime BROWSER_PICK_STATUS = "Select an element · Esc to cancel"
comptime BROWSER_RETRY_LABEL = "Try again"


@fieldwise_init
struct BrowserTabLabel(Copyable, Movable):
    var id: UInt64
    var label: String
    var detail: String


struct BrowserPresentation(Copyable, Movable):
    var tabs: List[BrowserTabLabel]
    var active: Int
    var address: String
    var loading: Bool
    var can_go_back: Bool
    var can_go_forward: Bool
    var page_zoom_percent: UInt16
    var state: UInt8
    var error: String
    var retry: Bool
    var pick_status: String

    def __init__(out self):
        self.tabs = List[BrowserTabLabel]()
        self.active = 0
        self.address = String()
        self.loading = False
        self.can_go_back = False
        self.can_go_forward = False
        self.page_zoom_percent = 100
        self.state = BROWSER_STATE_PAGE
        self.error = String()
        self.retry = False
        self.pick_status = String()


def browser_tab_label(url: String) -> String:
    if url.byte_length() == 0 or url == "about:blank":
        return String("New Tab")
    var rest = url
    var scheme = url.find("://")
    if scheme >= 0:
        rest = String(
            StringSlice(unsafe_from_utf8=url.as_bytes()[scheme + 3 :])
        )
    var bytes = rest.as_bytes()
    var end = len(bytes)
    for i in range(len(bytes)):
        if bytes[i] == 47 or bytes[i] == 63 or bytes[i] == 35:
            end = i
            break
    var host = String(StringSlice(unsafe_from_utf8=bytes[:end]))
    while host.startswith("www."):
        var trimmed = String(StringSlice(unsafe_from_utf8=host.as_bytes()[4:]))
        host = trimmed^
    if host.byte_length() == 0:
        return String("New Tab")
    return host^


def browser_presentation(model: BrowserModel) -> BrowserPresentation:
    var view = BrowserPresentation()
    for tab in model.tabs:
        var active = tab.id == model.active_tab
        var url = model.current.url if active else tab.content.url
        var title = model.current.title if active else tab.content.title
        var blank_title = title.byte_length() == 0
        view.tabs.append(
            BrowserTabLabel(
                tab.id,
                browser_tab_label(url) if blank_title else title,
                url if blank_title else title,
            )
        )
    view.active = model.active_index()
    var blank = model.current.url == "about:blank"
    view.address = String() if blank else model.current.url
    view.loading = model.current.loading
    view.can_go_back = model.current.can_go_back
    view.can_go_forward = model.current.can_go_forward
    view.page_zoom_percent = model.current.page_zoom_percent
    if model.current.error:
        view.state = BROWSER_STATE_ERROR
        view.error = model.current.error.value()
        view.retry = model.recoverable
    elif blank and not model.tabs[model.active_index()].popup:
        view.state = BROWSER_STATE_EMPTY
    if model.element_pick_active:
        view.pick_status = String(BROWSER_PICK_STATUS)
    return view^
