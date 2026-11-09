from std.sys import is_defined
from std.os import getenv, makedirs
from mjui.ui import Ui, Style
from mjui.image import ImageCache
from mjui.input import KeyEvent
from mjui.sys.dispatch import MainQueue
from mjui.web import WebRuntime, WebOptions, web_flags
from mjui.widgets.key import sub
from mjui.widgets.penna import PennaTheme, label, button
from .tree_model import MuxSnapshot
from .browser_pane import BrowserPane
from .browser_wire import BrowserCommand
from .browser_state import BrowserEffect
from .browser_presentation import BrowserPresentation
from .browser_input import (
    BrowserInput,
    BrowserKeyRoute,
    BROWSER_KEY_PASS,
    BROWSER_POINTER_LEAVE,
)
from .desktop_native_bindings import DesktopNativeBinding

comptime BROWSER_HOST = is_defined["ZZ_BROWSER"]()
comptime BROWSER_IMAGE_BASE: UInt64 = 0x7A00000000000000
comptime BROWSER_IMAGES_PER_PANE: UInt64 = 1024
comptime BROWSER_HEADER: Float32 = 30
comptime MAX_BROWSER_PANES = 64


@fieldwise_init
struct BrowserOutbound(Copyable, Movable):
    var pane: UInt64
    var effect: BrowserEffect


@fieldwise_init
struct BrowserDescriptor(Copyable, Movable):
    var pane: UInt64
    var urls: List[String]
    var active: UInt64
    var profile: String

    def same(self, other: Self) -> Bool:
        if (
            self.active != other.active
            or self.profile != other.profile
            or len(self.urls) != len(other.urls)
        ):
            return False
        for index in range(len(self.urls)):
            if self.urls[index] != other.urls[index]:
                return False
        return True


def browser_descriptors(snapshot: MuxSnapshot) raises -> List[BrowserDescriptor]:
    var result = List[BrowserDescriptor]()
    for session in snapshot.sessions:
        for window in session.windows:
            for entry in window.panes.items():
                ref kind = entry.value.kind
                if kind.kind != 2:
                    continue
                if len(result) == MAX_BROWSER_PANES:
                    raise Error("browser panes exceed the 64-pane host limit")
                result.append(
                    BrowserDescriptor(
                        entry.key,
                        kind.tabs.copy(),
                        kind.active_tab,
                        kind.profile.copy(),
                    )
                )
    return result^


def browser_cache_dir() -> String:
    var data = getenv("XDG_DATA_HOME", "")
    if data.byte_length() == 0:
        data = getenv("HOME", "/tmp") + "/.local/share"
    return data + "/zz-mojo/browser"


trait WorkspaceBrowser(Movable):
    def __init__(out self):
        ...

    def available(self) -> Bool:
        ...

    def failure(self) -> Optional[String]:
        ...

    def owns(self, pane: UInt64) -> Bool:
        ...

    def count(self) -> Int:
        ...

    def presentation(self, pane: UInt64) -> Optional[BrowserPresentation]:
        ...

    def sync(mut self, snapshot: MuxSnapshot) raises:
        ...

    def poll(mut self, mut images: ImageCache) raises -> Bool:
        ...

    def build(
        mut self, mut ui: Ui, th: PennaTheme, pane: UInt64, key: UInt64
    ) raises:
        ...

    def show(mut self, visible: List[UInt64]):
        ...

    def focus(mut self, pane: Optional[UInt64]):
        ...

    def pointer(
        mut self,
        pane: UInt64,
        phase: UInt8,
        button: UInt8,
        pressed: UInt8,
        x: Float32,
        y: Float32,
        mods: UInt32,
        clicks: Int32,
    ) raises -> Bool:
        ...

    def wheel(
        mut self,
        pane: UInt64,
        x: Float32,
        y: Float32,
        dx: Float32,
        dy: Float32,
        mods: UInt32,
    ) -> Bool:
        ...

    def action(
        mut self, pane: UInt64, binding: DesktopNativeBinding
    ) raises -> Bool:
        ...

    def key(mut self, pane: UInt64, event: KeyEvent) raises -> BrowserKeyRoute:
        ...

    def text(mut self, pane: UInt64, text: String):
        ...

    def command(
        mut self, pane: UInt64, command: BrowserCommand, provider: UInt32
    ) raises:
        ...

    def take_outbox(mut self) -> List[BrowserOutbound]:
        ...

    def watch(mut self, queue: MainQueue) raises:
        ...

    def pump(mut self) -> Bool:
        ...

    def pending(self) -> Bool:
        ...


struct NoBrowser(WorkspaceBrowser):
    var unused: Bool

    def __init__(out self):
        self.unused = False

    def available(self) -> Bool:
        return False

    def failure(self) -> Optional[String]:
        return String("Browser panes need a client built with the browser host")

    def owns(self, pane: UInt64) -> Bool:
        return False

    def count(self) -> Int:
        return 0

    def presentation(self, pane: UInt64) -> Optional[BrowserPresentation]:
        return None

    def sync(mut self, snapshot: MuxSnapshot) raises:
        pass

    def poll(mut self, mut images: ImageCache) raises -> Bool:
        return False

    def build(
        mut self, mut ui: Ui, th: PennaTheme, pane: UInt64, key: UInt64
    ) raises:
        pass

    def show(mut self, visible: List[UInt64]):
        pass

    def focus(mut self, pane: Optional[UInt64]):
        pass

    def pointer(
        mut self,
        pane: UInt64,
        phase: UInt8,
        button: UInt8,
        pressed: UInt8,
        x: Float32,
        y: Float32,
        mods: UInt32,
        clicks: Int32,
    ) raises -> Bool:
        return False

    def wheel(
        mut self,
        pane: UInt64,
        x: Float32,
        y: Float32,
        dx: Float32,
        dy: Float32,
        mods: UInt32,
    ) -> Bool:
        return False

    def action(
        mut self, pane: UInt64, binding: DesktopNativeBinding
    ) raises -> Bool:
        return False

    def key(mut self, pane: UInt64, event: KeyEvent) raises -> BrowserKeyRoute:
        return BrowserKeyRoute(BROWSER_KEY_PASS, False)

    def text(mut self, pane: UInt64, text: String):
        pass

    def command(
        mut self, pane: UInt64, command: BrowserCommand, provider: UInt32
    ) raises:
        pass

    def take_outbox(mut self) -> List[BrowserOutbound]:
        return List[BrowserOutbound]()

    def watch(mut self, queue: MainQueue) raises:
        pass

    def pump(mut self) -> Bool:
        return False

    def pending(self) -> Bool:
        return False


struct BrowserDeck(WorkspaceBrowser):
    var runtime: Optional[WebRuntime]
    var started: Bool
    var error: Optional[String]
    var panes: List[BrowserPane]
    var seen: List[BrowserDescriptor]
    var retiring: List[BrowserPane]
    var cache_dir: String
    var focused: Optional[UInt64]

    def __init__(out self):
        self.runtime = None
        self.started = False
        self.error = None
        self.panes = List[BrowserPane]()
        self.seen = List[BrowserDescriptor]()
        self.retiring = List[BrowserPane]()
        self.cache_dir = browser_cache_dir()
        self.focused = None

    def available(self) -> Bool:
        return Bool(self.runtime)

    def failure(self) -> Optional[String]:
        return self.error.copy()

    def _index(self, pane: UInt64) -> Int:
        for index in range(len(self.panes)):
            if self.panes[index].pane == pane:
                return index
        return -1

    def owns(self, pane: UInt64) -> Bool:
        return self._index(pane) >= 0

    def count(self) -> Int:
        return len(self.panes) + len(self.retiring)

    def presentation(self, pane: UInt64) -> Optional[BrowserPresentation]:
        var index = self._index(pane)
        if index < 0:
            return None
        return self.panes[index].presentation()

    def _start(mut self):
        if self.started:
            return
        self.started = True
        try:
            makedirs(self.cache_dir, 0o700, exist_ok=True)
            var options = WebOptions()
            options.cache_dir = self.cache_dir
            options.frame_rate = 60
            self.runtime = WebRuntime(options)
        except error:
            self.error = String(error)

    def sync(mut self, snapshot: MuxSnapshot) raises:
        var descriptors = browser_descriptors(snapshot)
        if len(descriptors) > 0:
            self._start()
        if not self.runtime:
            return
        for descriptor in descriptors:
            var index = self._index(descriptor.pane)
            if index < 0:
                var image_base = BROWSER_IMAGE_BASE + (
                    descriptor.pane % (UInt64(1) << 40)
                ) * BROWSER_IMAGES_PER_PANE
                self.panes.append(
                    BrowserPane(
                        descriptor.pane,
                        descriptor.urls,
                        descriptor.active,
                        descriptor.profile,
                        self.cache_dir,
                        image_base,
                    )
                )
                self.seen.append(descriptor.copy())
            elif not self.seen[index].same(descriptor):
                self.panes[index].descriptor(
                    descriptor.urls, descriptor.active, descriptor.profile
                )
                self.seen[index] = descriptor.copy()
        var index = 0
        while index < len(self.panes):
            var live = False
            for descriptor in descriptors:
                live = live or descriptor.pane == self.panes[index].pane
            if live:
                index += 1
                continue
            var retired = self.panes.pop(index)
            _ = self.seen.pop(index)
            retired.close()
            self.retiring.append(retired^)

    def poll(mut self, mut images: ImageCache) raises -> Bool:
        var changed = False
        for index in range(len(self.panes)):
            if self.panes[index].poll(images):
                changed = True
        var index = 0
        while index < len(self.retiring):
            _ = self.retiring[index].poll(images)
            if len(self.retiring[index].pages) == 0:
                _ = self.retiring.pop(index)
                changed = True
            else:
                index += 1
        return changed

    def build(
        mut self, mut ui: Ui, th: PennaTheme, pane: UInt64, key: UInt64
    ) raises:
        var found = self._index(pane)
        if found < 0:
            return
        var view = self.panes[found].presentation()
        ui.open(
            Style.row()
            .grow()
            .height(BROWSER_HEADER)
            .pad(6, 2)
            .spacing(4)
            .center()
            .fill(th.flat.panel)
        )
        var selected: Optional[UInt64] = None
        for index in range(len(view.tabs)):
            var title = view.tabs[index].label.copy()
            if index == view.active:
                title = String("• ", title)
            if button(ui, th, sub(key, 8 + index), title):
                selected = view.tabs[index].id
        ui.open(Style.col().grow())
        ui.close()
        label(ui, th, view.address, th.flat.dim)
        ui.close()
        self.panes[found].build(ui, th, key)
        if selected:
            self.panes[found].select_tab(selected.value())

    def show(mut self, visible: List[UInt64]):
        for index in range(len(self.panes)):
            var shown = self.panes[index].pane in visible
            if self.panes[index].visible != shown:
                self.panes[index].set_visible(shown)

    def focus(mut self, pane: Optional[UInt64]):
        if self.focused == pane:
            return
        if self.focused:
            var previous = self._index(self.focused.value())
            if previous >= 0:
                self.panes[previous].set_focused(False)
        self.focused = pane
        if pane:
            var current = self._index(pane.value())
            if current >= 0:
                self.panes[current].set_focused(True)

    def pointer(
        mut self,
        pane: UInt64,
        phase: UInt8,
        button: UInt8,
        pressed: UInt8,
        x: Float32,
        y: Float32,
        mods: UInt32,
        clicks: Int32,
    ) raises -> Bool:
        var index = self._index(pane)
        if index < 0:
            return False
        return self.panes[index].pointer(
            phase, button, pressed, x, y, mods, clicks
        )

    def wheel(
        mut self,
        pane: UInt64,
        x: Float32,
        y: Float32,
        dx: Float32,
        dy: Float32,
        mods: UInt32,
    ) -> Bool:
        var index = self._index(pane)
        if index < 0:
            return False
        var probe = BrowserInput()
        ref target = self.panes[index]
        var route = probe.pointer(
            BROWSER_POINTER_LEAVE,
            0,
            0,
            target.left,
            target.top,
            target.width,
            target.height,
            x,
            y,
        )
        var page = target.active_page()
        if not route.forward or page < 0:
            return False
        target.pages[page].page.wheel(
            Float32(route.x), Float32(route.y), web_flags(mods), dx, dy
        )
        return True

    def action(
        mut self, pane: UInt64, binding: DesktopNativeBinding
    ) raises -> Bool:
        var index = self._index(pane)
        if index < 0:
            return False
        return self.panes[index].action(binding)

    def key(mut self, pane: UInt64, event: KeyEvent) raises -> BrowserKeyRoute:
        var index = self._index(pane)
        if index < 0:
            return BrowserKeyRoute(BROWSER_KEY_PASS, False)
        return self.panes[index].key(event)

    def text(mut self, pane: UInt64, text: String):
        var index = self._index(pane)
        if index >= 0:
            self.panes[index].text(text)

    def command(
        mut self, pane: UInt64, command: BrowserCommand, provider: UInt32
    ) raises:
        var index = self._index(pane)
        if index >= 0:
            self.panes[index].command(command, provider)

    def take_outbox(mut self) -> List[BrowserOutbound]:
        var result = List[BrowserOutbound]()
        for index in range(len(self.panes)):
            var effects = self.panes[index].take_outbox()
            for effect in effects:
                result.append(BrowserOutbound(self.panes[index].pane, effect.copy()))
        return result^

    def watch(mut self, queue: MainQueue) raises:
        if self.runtime:
            self.runtime.value().watch(queue)

    def pump(mut self) -> Bool:
        if not self.runtime:
            return False
        return self.runtime.value().pump()

    def pending(self) -> Bool:
        if not self.runtime:
            return False
        return self.runtime.value().pending()


comptime ActiveBrowser: WorkspaceBrowser = BrowserDeck if BROWSER_HOST else NoBrowser
