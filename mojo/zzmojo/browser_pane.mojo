from std.os import makedirs
from mjui.image import ImageCache
from mjui.input import KeyEvent
from mjui.keys import Key
from mjui.web import (
    WebEvent,
    WebPage,
    web_flags,
    WEB_BUTTON_LEFT,
    WEB_BUTTON_MIDDLE,
    WEB_BUTTON_RIGHT,
    WEB_CLOSED,
    WEB_CREATED,
    WEB_FLAG_LEFT,
    WEB_FLAG_MIDDLE,
    WEB_FLAG_RIGHT,
    WEB_LOAD_ERROR,
    WEB_LOADING,
    WEB_POPUP_CREATED,
    WEB_POPUP_REQUESTED,
    WEB_TITLE,
    WEB_URL,
)
from .browser_actions import (
    prepare_browser_action,
    prepare_browser_command,
    prepare_browser_failure,
    prepare_browser_pick_result,
    prepare_browser_retry,
    BROWSER_ACTION_ELEMENT_SELECTOR,
    BROWSER_PANE_FAILED,
)
from .browser_controller import (
    BrowserMetadataEvent,
    BrowserPlan,
    prepare_browser_event,
    prepare_browser_transition,
    BROWSER_EVENT_ADDRESS,
    BROWSER_EVENT_CLOSED,
    BROWSER_EVENT_CREATED,
    BROWSER_EVENT_LOAD_FAILED,
    BROWSER_EVENT_LOADING,
    BROWSER_EVENT_POPUP_CREATED,
    BROWSER_EVENT_POPUP_REQUESTED,
    BROWSER_EVENT_TAB_CLOSED,
    BROWSER_EVENT_TITLE,
)
from .browser_delivery import prepare_browser_delivery
from .browser_host import BrowserHost, BrowserSource
from .browser_input import (
    BrowserInput,
    BrowserKeyRoute,
    BrowserPointerRoute,
    BROWSER_BUTTON_LEFT,
    BROWSER_BUTTON_MIDDLE,
    BROWSER_BUTTON_RIGHT,
    BROWSER_KEY_CANCEL_PICK,
    BROWSER_KEY_LEAVE_ADDRESS,
    BROWSER_POINTER_DOWN,
    BROWSER_POINTER_LEAVE,
    BROWSER_POINTER_MOVE,
    BROWSER_POINTER_UP,
    BROWSER_POINTER_UP_OUTSIDE,
)
from .browser_presentation import BrowserPresentation, browser_presentation
from .browser_state import (
    BrowserEffect,
    BrowserModel,
    restore_browser,
    BROWSER_BACK,
    BROWSER_CANCEL_PICK,
    BROWSER_CLOSE,
    BROWSER_EDIT,
    BROWSER_FOCUS_ADDRESS,
    BROWSER_FOCUS_PAGE,
    BROWSER_FORWARD,
    BROWSER_NAVIGATE,
    BROWSER_RECREATE,
    BROWSER_RELOAD,
    BROWSER_REQUEST,
    BROWSER_SHOW,
    BROWSER_START_PICK,
    BROWSER_ZOOM,
)
from .browser_wire import BrowserCommand, BrowserKeyToken
from .desktop_native_bindings import DesktopNativeBinding
from .input_protocol import KeyInput

comptime MAX_BROWSER_KEY_REPEAT: UInt32 = 9999
comptime MAX_BROWSER_DEFERRED = 64


struct BrowserPage(Movable):
    var tab: UInt64
    var page: WebPage
    var retiring: Bool

    def __init__(out self, tab: UInt64, var page: WebPage):
        self.tab = tab
        self.page = page^
        self.retiring = False


@fieldwise_init
struct _Deferred(Copyable, Movable):
    var kind: UInt8
    var started: Bool
    var toggled: Bool
    var message: String


def browser_profile_cache(root: String, profile: String) -> String:
    var hex = String("0123456789abcdef")
    var path = root + "/profile-"
    for byte in profile.as_bytes():
        path += String(hex[byte=Int(byte >> 4)]) + String(hex[byte=Int(byte & 15)])
    return path^


def browser_key_event(input: KeyInput) -> KeyEvent:
    var key = Key.NONE
    var ch: UInt32 = 0
    if input.key == 0:
        ch = input.codepoint
        var lower = ch + 32 if ch >= 65 and ch <= 90 else ch
        if lower >= 32 and lower <= 126:
            key = UInt16(lower)
    elif input.key >= 1 and input.key <= 14:
        var named: List[UInt16] = [
            Key.BACKSPACE,
            Key.ENTER,
            Key.TAB,
            Key.ESCAPE,
            Key.DELETE,
            Key.INSERT,
            Key.HOME,
            Key.END,
            Key.PAGE_UP,
            Key.PAGE_DOWN,
            Key.UP,
            Key.DOWN,
            Key.LEFT,
            Key.RIGHT,
        ]
        key = named[Int(input.key) - 1]
    elif input.key == 15 and input.value >= 1 and input.value <= 24:
        key = Key.F1 + input.value - 1
    return KeyEvent(
        0,
        key,
        UInt32(input.modifiers & 15) << 17,
        ch,
        ch,
        0,
        0,
        input.action == 1,
        input.action == 2,
        False,
        False,
    )


def browser_named_key(name: String) -> Optional[KeyInput]:
    var input = KeyInput()
    var rest = name
    while True:
        if rest.startswith("C-"):
            input.modifiers |= 2
        elif rest.startswith("M-"):
            input.modifiers |= 4
        elif rest.startswith("S-"):
            input.modifiers |= 1
        else:
            break
        var tail = String(StringSlice(unsafe_from_utf8=rest.as_bytes()[2:]))
        rest = tail^
    var names: List[String] = [
        "BSpace",
        "Enter",
        "Tab",
        "Escape",
        "DC",
        "IC",
        "Home",
        "End",
        "PPage",
        "NPage",
        "Up",
        "Down",
        "Left",
        "Right",
    ]
    for i in range(len(names)):
        if rest == names[i]:
            input.key = UInt8(i + 1)
            return input^
    if rest == "BTab":
        input.key = 3
        input.modifiers |= 1
        return input^
    if rest == "Space":
        input.key = 0
        input.codepoint = 32
        return input^
    if rest.count_codepoints() == 1:
        for scalar in rest.codepoints():
            input.key = 0
            input.codepoint = UInt32(Int(scalar))
        return input^
    if rest.startswith("F"):
        var digits = rest.as_bytes()[1:]
        if len(digits) == 0 or len(digits) > 3:
            return None
        var value = 0
        for digit in digits:
            if digit < 48 or digit > 57:
                return None
            value = value * 10 + Int(digit - 48)
        if value > 255:
            return None
        input.key = 15
        input.value = UInt16(value)
        return input^
    return None


struct BrowserPane(Movable):
    var pane: UInt64
    var model: BrowserModel
    var host: BrowserHost
    var input: BrowserInput
    var pages: List[BrowserPage]
    var cache_root: String
    var next_image: UInt64
    var left: Float32
    var top: Float32
    var width: Float32
    var height: Float32
    var scale: Float32
    var visible: Bool
    var focused: Bool
    var outbox: List[BrowserEffect]
    var _deferred: List[_Deferred]

    def __init__(
        out self,
        pane: UInt64,
        urls: List[String],
        active: UInt64,
        profile: String,
        cache_root: String,
        image_base: UInt64,
        width: Float32 = 800,
        height: Float32 = 500,
        scale: Float32 = 1,
    ) raises:
        self.pane = pane
        self.model = restore_browser(urls, active, profile, pane)
        self.host = BrowserHost()
        self.input = BrowserInput()
        self.pages = List[BrowserPage]()
        self.cache_root = cache_root
        self.next_image = image_base
        self.left = 0
        self.top = 0
        self.width = width
        self.height = height
        self.scale = scale
        self.visible = True
        self.focused = False
        self.outbox = List[BrowserEffect]()
        self._deferred = List[_Deferred]()
        var url = self.model.current.url.copy()
        self._request(
            self.model.active_tab, url, self.model.current.page_zoom_factor
        )
        self._settle()

    def presentation(self) -> BrowserPresentation:
        return browser_presentation(self.model)

    def take_outbox(mut self) -> List[BrowserEffect]:
        var out = self.outbox^
        self.outbox = List[BrowserEffect]()
        return out^

    def active_page(self) -> Int:
        for entry in self.host.sessions:
            if entry.pane == self.pane and entry.tab == self.model.active_tab:
                return self._page(entry.session)
        return -1

    def frame_image(self) -> Optional[UInt64]:
        var index = self.active_page()
        if index < 0 or self.pages[index].page.frame_w == 0:
            return None
        return self.pages[index].page.img

    def layout(
        mut self,
        left: Float32,
        top: Float32,
        width: Float32,
        height: Float32,
        scale: Float32,
    ):
        self.left = left
        self.top = top
        self.width = max(width, 1)
        self.height = max(height, 1)
        self.scale = scale
        for i in range(len(self.pages)):
            self.pages[i].page.resize(self._columns(), self._rows(), self.scale)

    def set_visible(mut self, visible: Bool):
        self.visible = visible
        self._show()

    def set_focused(mut self, focused: Bool):
        self.focused = focused
        var index = self.active_page()
        if index >= 0:
            self.pages[index].page.focus(focused)

    def poll(mut self, mut images: ImageCache) raises -> Bool:
        var changed = False
        var i = 0
        while i < len(self.pages):
            if self.pages[i].page.poll(images):
                changed = True
            var events = self.pages[i].page.take_events()
            var session = self.pages[i].page.session
            for event in events:
                self._native(i, session, event)
            if self.pages[i].page.closed:
                self._unbind(session)
                images.drop(self.pages[i].page.img)
                images.drop(self.pages[i].page.popup_img)
                _ = self.pages.pop(i)
                changed = True
            else:
                i += 1
        return changed

    def action(mut self, binding: DesktopNativeBinding) raises -> Bool:
        var handled = prepare_browser_action(
            self.pane, self.model, self.host, binding
        ).commit(self.model, self.host)
        self._settle()
        return handled

    def descriptor(
        mut self, urls: List[String], active: UInt64, profile: String
    ) raises:
        self._commit(
            prepare_browser_transition(
                self.pane, self.model, self.host, 5, input=profile
            )
        )
        self._commit(
            prepare_browser_transition(
                self.pane, self.model, self.host, 4, target=active, urls=urls
            )
        )

    def navigate(mut self, address: String, provider: UInt32 = 0) raises:
        self._commit(
            prepare_browser_transition(
                self.pane,
                self.model,
                self.host,
                6,
                input=address,
                provider=provider,
            )
        )
        self.input.focus_page()

    def select_tab(mut self, tab: UInt64) raises:
        self._commit(
            prepare_browser_transition(
                self.pane, self.model, self.host, 0, target=tab
            )
        )

    def close_tab(mut self, tab: UInt64) raises:
        self._commit(
            prepare_browser_transition(
                self.pane, self.model, self.host, 2, target=tab
            )
        )

    def retry(mut self) raises:
        self._commit(prepare_browser_retry(self.pane, self.model, self.host))

    def fail(
        mut self, kind: UInt8, message: String, running: Bool = False
    ) raises:
        self._commit(
            prepare_browser_failure(
                self.pane, self.model, self.host, kind, message, running
            )
        )

    def command(
        mut self, command: BrowserCommand, provider: UInt32 = 0
    ) raises:
        if command.kind <= 3:
            self._commit(
                prepare_browser_command(
                    self.pane, self.model, self.host, command, provider
                )
            )
            return
        if command.kind == 6:
            raise Error("browser screenshot command has no frame writer yet")
        var index = self.active_page()
        if index < 0:
            return
        if command.kind == 5:
            if command.key:
                self.pages[index].page.key(
                    browser_key_event(command.key.value())
                )
            return
        var count = 1 if command.kind == 4 else Int(
            min(command.count, MAX_BROWSER_KEY_REPEAT)
        )
        for _ in range(count):
            for token in command.tokens:
                self._token(index, token)

    def pointer(
        mut self,
        phase: UInt8,
        button: UInt8,
        pressed: UInt8,
        x: Float32,
        y: Float32,
        mods: UInt32,
        clicks: Int32 = 1,
    ) raises -> Bool:
        var route = self.input.pointer(
            phase,
            button,
            pressed,
            self.left,
            self.top,
            self.width,
            self.height,
            x,
            y,
        )
        if route.focus_page:
            self._focus_page()
        var index = self.active_page()
        if not route.forward or index < 0:
            return route.forward
        var flags = web_flags(mods)
        var held = pressed if phase == BROWSER_POINTER_MOVE else (
            button if phase == BROWSER_POINTER_DOWN else UInt8(0)
        )
        if held & BROWSER_BUTTON_LEFT != 0:
            flags |= WEB_FLAG_LEFT
        if held & BROWSER_BUTTON_RIGHT != 0:
            flags |= WEB_FLAG_RIGHT
        if held & BROWSER_BUTTON_MIDDLE != 0:
            flags |= WEB_FLAG_MIDDLE
        ref page = self.pages[index].page
        var px = Float32(route.x)
        var py = Float32(route.y)
        if phase == BROWSER_POINTER_DOWN:
            page.mouse_click(px, py, flags, _web_button(button), False, clicks)
        elif phase == BROWSER_POINTER_UP or phase == BROWSER_POINTER_UP_OUTSIDE:
            page.mouse_click(px, py, flags, _web_button(button), True, clicks)
        elif phase == BROWSER_POINTER_MOVE:
            page.mouse_move(px, py, flags)
        elif phase == BROWSER_POINTER_LEAVE:
            page.mouse_move(px, py, flags, True)
        return True

    def wheel(
        mut self, x: Float32, y: Float32, dx: Float32, dy: Float32, mods: UInt32
    ) -> Bool:
        var route = BrowserInput().pointer(
            BROWSER_POINTER_LEAVE,
            0,
            0,
            self.left,
            self.top,
            self.width,
            self.height,
            x,
            y,
        )
        var index = self.active_page()
        if not route.forward or index < 0:
            return False
        self.pages[index].page.wheel(
            Float32(route.x), Float32(route.y), web_flags(mods), dx, dy
        )
        return True

    def key(mut self, event: KeyEvent) raises -> BrowserKeyRoute:
        var route = self.input.key(
            event.key, event.mods, event.release, self.model.element_pick_active
        )
        if route.kind == BROWSER_KEY_CANCEL_PICK:
            _ = self.action(
                DesktopNativeBinding(BROWSER_ACTION_ELEMENT_SELECTOR, 0)
            )
        elif route.kind == BROWSER_KEY_LEAVE_ADDRESS:
            self._focus_page()
        return route

    def text(mut self, text: String):
        var index = self.active_page()
        if index >= 0 and text.byte_length() > 0:
            self.pages[index].page.text(text)

    def close(mut self):
        for i in range(len(self.pages)):
            self.pages[i].retiring = True
            self.pages[i].page.close()

    def _token(mut self, index: Int, token: BrowserKeyToken):
        if token.kind == 0:
            self.pages[index].page.text(token.text)
        elif token.kind == 1:
            var input = browser_named_key(token.text)
            if input:
                self.pages[index].page.key(browser_key_event(input.value()))

    def _columns(self) -> Int:
        return Int(max(self.width, 1) + 0.5)

    def _rows(self) -> Int:
        return Int(max(self.height, 1) + 0.5)

    def _page(self, session: UInt64) -> Int:
        for i in range(len(self.pages)):
            if self.pages[i].page.session == session:
                return i
        return -1

    def _session(self, tab: UInt64) -> Optional[UInt64]:
        for entry in self.host.sessions:
            if entry.pane == self.pane and entry.tab == tab:
                return entry.session
        return None

    def _unbind(mut self, session: UInt64) raises:
        var source = self.host.source(session)
        if source:
            self.host.retire(source.value())

    def _focus_page(mut self):
        self.input.focus_page()
        self.outbox.append(
            BrowserEffect(BROWSER_FOCUS_PAGE, self.model.active_tab, String(), 0)
        )
        var index = self.active_page()
        if index >= 0:
            self.pages[index].page.focus(True)

    def _show(mut self):
        for i in range(len(self.pages)):
            if not self.pages[i].retiring:
                self.pages[i].page.hidden(
                    not (
                        self.visible
                        and self.pages[i].tab == self.model.active_tab
                    )
                )

    def _request(mut self, tab: UInt64, url: String, zoom: Float64) raises:
        var cache = browser_profile_cache(self.cache_root, self.model.profile)
        try:
            makedirs(cache, exist_ok=True)
            var page = WebPage(
                url,
                self._columns(),
                self._rows(),
                self.scale,
                self.next_image,
                self.next_image + 1,
                profile_cache=cache,
                page_zoom=zoom,
            )
            self.next_image += 2
            page.hidden(not (self.visible and tab == self.model.active_tab))
            self.host.bind(self.pane, tab, page.session)
            self.pages.append(BrowserPage(tab, page^))
        except error:
            self._deferred.append(
                _Deferred(BROWSER_PANE_FAILED, False, False, String(error))
            )

    def _retire_tab(mut self, tab: UInt64) raises:
        var session = self._session(tab)
        if not session:
            return
        var index = self._page(session.value())
        self._unbind(session.value())
        if index >= 0:
            self.pages[index].retiring = True
            self.pages[index].page.close()

    def _native(mut self, index: Int, session: UInt64, event: WebEvent) raises:
        var source = self.host.source(session)
        if not source:
            return
        if event.kind == WEB_POPUP_CREATED:
            self._adopt(index, event)
            return
        var meta: BrowserMetadataEvent
        if event.kind == WEB_CREATED:
            meta = BrowserMetadataEvent(BROWSER_EVENT_CREATED)
        elif event.kind == WEB_URL:
            meta = BrowserMetadataEvent(BROWSER_EVENT_ADDRESS, event.text)
        elif event.kind == WEB_TITLE:
            meta = BrowserMetadataEvent(BROWSER_EVENT_TITLE, event.text)
        elif event.kind == WEB_LOADING:
            meta = BrowserMetadataEvent(
                BROWSER_EVENT_LOADING,
                loading=event.a != 0,
                back=event.b != 0,
                forward=event.c != 0,
            )
        elif event.kind == WEB_LOAD_ERROR:
            meta = BrowserMetadataEvent(
                BROWSER_EVENT_LOAD_FAILED, event.detail, event.text
            )
        elif event.kind == WEB_POPUP_REQUESTED:
            meta = BrowserMetadataEvent(
                BROWSER_EVENT_POPUP_REQUESTED,
                url=event.text,
                foreground=event.a != 0,
            )
        elif event.kind == WEB_CLOSED:
            meta = BrowserMetadataEvent(BROWSER_EVENT_CLOSED)
        else:
            return
        self._commit(
            prepare_browser_event(
                self.pane, self.model, self.host, source.value(), meta
            )
        )
        if event.kind == WEB_CLOSED and not self.pages[index].retiring:
            var current = self.host.source(session)
            if current:
                self._commit(
                    prepare_browser_event(
                        self.pane,
                        self.model,
                        self.host,
                        current.value(),
                        BrowserMetadataEvent(BROWSER_EVENT_TAB_CLOSED),
                    )
                )

    def _adopt(mut self, opener: Int, event: WebEvent) raises:
        var tab = self.model.next_tab_id
        var popup: WebPage
        try:
            popup = WebPage(
                opener=self.pages[opener].page,
                popup=event.related_session,
                img=self.next_image,
                popup_img=self.next_image + 1,
            )
        except:
            return
        self.next_image += 2
        var session = popup.session
        var opener_session = self.pages[opener].page.session
        self.host.bind(self.pane, tab, session)
        self.pages.append(BrowserPage(tab, popup^))
        try:
            self._commit(
                prepare_browser_event(
                    self.pane,
                    self.model,
                    self.host,
                    self.host.source(opener_session).value(),
                    BrowserMetadataEvent(
                        BROWSER_EVENT_POPUP_CREATED,
                        url=event.text,
                        session=session,
                        foreground=event.a != 0,
                    ),
                )
            )
        except:
            self._unbind(session)
            self.pages[len(self.pages) - 1].retiring = True
            self.pages[len(self.pages) - 1].page.close()

    def _commit(mut self, var plan: BrowserPlan) raises:
        plan^.commit(self.model, self.host)
        self._settle()

    def _settle(mut self) raises:
        for _ in range(MAX_BROWSER_DEFERRED):
            if len(self.model.effects) > 0:
                var delivery = prepare_browser_delivery(
                    self.pane, self.model, self.host
                )
                delivery.validate(self.model, self.host)
                delivery.acknowledge(self.model)
                for item in delivery.items:
                    if item.dispatch:
                        self._apply(item.effect, item.source)
            if len(self._deferred) == 0:
                return
            var next = self._deferred.pop(0)
            var plan: BrowserPlan
            if next.kind == BROWSER_START_PICK:
                plan = prepare_browser_pick_result(
                    self.pane, self.model, self.host, next.started, next.toggled
                )
            else:
                plan = prepare_browser_failure(
                    self.pane, self.model, self.host, next.kind, next.message
                )
            plan^.commit(self.model, self.host)
        raise Error("browser deferred native results did not settle")

    def _apply(
        mut self, effect: BrowserEffect, source: Optional[BrowserSource]
    ) raises:
        var kind = effect.kind
        if kind == BROWSER_SHOW:
            self._show()
            self.set_focused(self.focused)
        elif kind == BROWSER_REQUEST:
            self._request(effect.tab, effect.text, effect.zoom)
        elif kind == BROWSER_CLOSE:
            self._retire_tab(effect.tab)
        elif kind == BROWSER_RECREATE:
            self._retire_tab(effect.tab)
            self._request(effect.tab, effect.text, effect.zoom)
        elif kind == BROWSER_FOCUS_PAGE:
            self._focus_page()
        elif kind == BROWSER_FOCUS_ADDRESS:
            self.input.focus_address()
            self.outbox.append(effect.copy())
        elif kind == BROWSER_START_PICK:
            self._deferred.append(
                _Deferred(BROWSER_START_PICK, False, effect.flag, String())
            )
        elif kind == BROWSER_CANCEL_PICK:
            return
        elif source:
            var index = self._page(source.value().session)
            if index < 0:
                return
            ref page = self.pages[index].page
            if kind == BROWSER_NAVIGATE:
                page.load_native(effect.text)
            elif kind == BROWSER_ZOOM:
                page.set_page_zoom(effect.zoom)
            elif kind == BROWSER_BACK:
                page.back()
            elif kind == BROWSER_FORWARD:
                page.forward()
            elif kind == BROWSER_RELOAD:
                page.reload()
            elif kind == BROWSER_EDIT:
                page.edit(_edit_command(effect.text))
            else:
                self.outbox.append(effect.copy())
        else:
            self.outbox.append(effect.copy())


def _web_button(button: UInt8) -> Int32:
    if button == BROWSER_BUTTON_RIGHT:
        return WEB_BUTTON_RIGHT
    if button == BROWSER_BUTTON_MIDDLE:
        return WEB_BUTTON_MIDDLE
    return WEB_BUTTON_LEFT


def _edit_command(name: String) -> Int32:
    var names: List[String] = [
        "undo",
        "redo",
        "cut",
        "copy",
        "paste",
        "paste-and-match-style",
        "select-all",
    ]
    for i in range(len(names)):
        if names[i] == name:
            return Int32(i)
    return -1
