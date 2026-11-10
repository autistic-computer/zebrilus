from mjui.ui import Ui
from mjui.input import Clipboard, KeyEvent, TextInput
from mjui.widgets.penna import PennaTheme
from .tree_model import MuxSnapshot
from .editor_pane import (
    EDITOR_KEY_PASS,
    EditorKeyRoute,
    EditorOutbound,
    EditorPane,
    editor_descriptor_refusal,
)

comptime MAX_EDITOR_PANES = 64


@fieldwise_init
struct EditorDescriptor(Copyable, Movable):
    var pane: UInt64
    var path: Optional[String]
    var cwd: String

    def same(self, other: Self) -> Bool:
        if self.cwd != other.cwd or Bool(self.path) != Bool(other.path):
            return False
        if self.path:
            return self.path.value() == other.path.value()
        return True


def editor_descriptors(snapshot: MuxSnapshot) raises -> List[EditorDescriptor]:
    var result = List[EditorDescriptor]()
    for session in snapshot.sessions:
        for window in session.windows:
            for entry in window.panes.items():
                ref kind = entry.value.kind
                if kind.kind != 3:
                    continue
                if len(result) == MAX_EDITOR_PANES:
                    raise Error("editor panes exceed the 64-pane host limit")
                if not kind.cwd:
                    raise Error("editor pane has no working directory")
                result.append(
                    EditorDescriptor(entry.key, kind.path.copy(), kind.cwd.value())
                )
    return result^


struct EditorDeck(Movable):
    var panes: List[EditorPane]
    var seen: List[EditorDescriptor]
    var refused: List[EditorDescriptor]
    var focused: Optional[UInt64]

    def __init__(out self):
        self.panes = List[EditorPane]()
        self.seen = List[EditorDescriptor]()
        self.refused = List[EditorDescriptor]()
        self.focused = None

    def _index(self, pane: UInt64) -> Int:
        for index in range(len(self.panes)):
            if self.panes[index].pane == pane:
                return index
        return -1

    def _refused(self, pane: UInt64) -> Int:
        for index in range(len(self.refused)):
            if self.refused[index].pane == pane:
                return index
        return -1

    def owns(self, pane: UInt64) -> Bool:
        return self._index(pane) >= 0

    def count(self) -> Int:
        return len(self.panes)

    def title(self, pane: UInt64) -> Optional[String]:
        var index = self._index(pane)
        if index < 0:
            return None
        return Optional[String](self.panes[index].title())

    def _refuse(mut self, descriptor: EditorDescriptor):
        var index = self._refused(descriptor.pane)
        if index < 0:
            self.refused.append(descriptor.copy())
        else:
            self.refused[index] = descriptor.copy()

    def sync(mut self, snapshot: MuxSnapshot, th: PennaTheme) raises:
        var descriptors = editor_descriptors(snapshot)
        var failure = Optional[String]()
        for descriptor in descriptors:
            var index = self._index(descriptor.pane)
            var refusal = editor_descriptor_refusal(descriptor.path, descriptor.cwd)
            if refusal:
                var known = self._refused(descriptor.pane)
                if known < 0 or not self.refused[known].same(descriptor):
                    self._refuse(descriptor)
                    if not failure:
                        failure = refusal.copy()
                continue
            var known = self._refused(descriptor.pane)
            if known >= 0:
                _ = self.refused.pop(known)
            if index < 0:
                var pane = EditorPane(
                    descriptor.pane, descriptor.path, descriptor.cwd, th
                )
                pane.set_focused(self.focused == Optional[UInt64](descriptor.pane))
                self.panes.append(pane^)
                self.seen.append(descriptor.copy())
            elif not self.seen[index].same(descriptor):
                self.panes[index].descriptor(descriptor.path, descriptor.cwd)
                self.seen[index] = descriptor.copy()
        var index = len(self.panes) - 1
        while index >= 0:
            var live = False
            for descriptor in descriptors:
                if descriptor.pane == self.panes[index].pane:
                    live = True
                    break
            if not live:
                _ = self.panes.pop(index)
                _ = self.seen.pop(index)
            index -= 1
        index = len(self.refused) - 1
        while index >= 0:
            var live = False
            for descriptor in descriptors:
                if descriptor.pane == self.refused[index].pane:
                    live = True
                    break
            if not live:
                _ = self.refused.pop(index)
            index -= 1
        if failure:
            raise Error(failure.value())

    def focus(mut self, pane: Optional[UInt64]):
        self.focused = pane
        for index in range(len(self.panes)):
            self.panes[index].set_focused(
                pane == Optional[UInt64](self.panes[index].pane)
            )

    def build(
        mut self, mut ui: Ui, th: PennaTheme, pane: UInt64, key: UInt64
    ) raises -> Bool:
        var index = self._index(pane)
        if index < 0:
            return False
        self.panes[index].build(ui, th, key)
        return True

    def text[C: Clipboard](
        mut self,
        mut ui: Ui,
        input: TextInput,
        mut clip: C,
        pane: UInt64,
        key: UInt64,
        skip: Int = 0,
    ) raises:
        var index = self._index(pane)
        if index >= 0:
            self.panes[index].text(ui, input, clip, key, skip)

    def key(mut self, pane: UInt64, event: KeyEvent) raises -> EditorKeyRoute:
        var index = self._index(pane)
        if index < 0:
            return EditorKeyRoute(EDITOR_KEY_PASS)
        return self.panes[index].key(event)

    def pointer(
        mut self,
        pane: UInt64,
        phase: UInt8,
        button: UInt8,
        pressed: UInt8,
        x: Float32,
        y: Float32,
        mods: UInt32,
        clicks: Int32 = 1,
    ) raises -> Bool:
        var index = self._index(pane)
        if index < 0:
            return False
        return self.panes[index].pointer(phase, button, pressed, x, y, mods, clicks)

    def wheel(
        self,
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
        return self.panes[index].wheel(x, y, dx, dy, mods)

    def command(mut self, pane: UInt64, kind: UInt8, text: String = String()) raises:
        var index = self._index(pane)
        if index < 0:
            raise Error("unknown editor pane")
        self.panes[index].command(kind, text)

    def take_outbox(mut self) -> List[EditorOutbound]:
        var result = List[EditorOutbound]()
        for index in range(len(self.panes)):
            var items = self.panes[index].take_outbox()
            for item in items:
                result.append(item.copy())
        return result^
