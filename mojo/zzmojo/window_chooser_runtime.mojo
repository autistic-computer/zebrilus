from mjui.ui import Ui, TextStyle
from mjui.input import (
    Clipboard,
    TextInput,
    TextEvent,
    MouseEvent,
    TEXT_INSERT,
    TEXT_COMMAND,
    TEXT_PRIMARY,
    TEXT_POINTER,
    CMD_PASTE,
)
from mjui.widgets.penna import PennaTheme
from .backend import AsyncBackend, NATIVE_PASSIVE
from .chooser_session import ChooserSource
from .palette_dialog_state import PaletteDialogs
from .window_chooser_component import (
    WindowChooserComponent,
    WindowChooserComponentResult,
)
from .window_chooser_controller import (
    WindowChooserOperation,
    WindowChooserEffects,
)
from .window_chooser_session import (
    WindowChooserSession,
    WindowChooserObservation,
)
from .window_chooser_editor import WindowChooserEditorCheckpoint
from .search_clipboard import SearchClipboard
from .chooser_presentation import CHOOSER_DISPLAY_BYTES
from .editor_pointer import EditorPointerScope, pointer_payload

comptime WINDOW_CHOOSER_KEY: UInt64 = 0x7800000000000000


struct WindowChooserRuntime(Movable):
    var session: WindowChooserSession
    var component: WindowChooserComponent
    var host_scope: Optional[UInt64]
    var host_source: ChooserSource
    var marked_seen: UInt64
    var marked_exhausted: Bool
    var returned: Bool
    var unmark: Bool

    def __init__(out self):
        self.session = WindowChooserSession()
        self.component = WindowChooserComponent(
            WINDOW_CHOOSER_KEY, TextStyle(0, 12, 0xFFFFFFFF, False)
        )
        self.host_scope = None
        self.host_source = ChooserSource(0, 0, 0)
        self.marked_seen = 0
        self.marked_exhausted = False
        self.returned = False
        self.unmark = False

    def opened(self) -> Bool:
        return self.session.active

    def current(self, backend: AsyncBackend) -> Bool:
        if (
            not self.opened()
            or not backend.online
            or not backend.job[].native_chooser
            or not backend.path_source
        ):
            return False
        var source = backend.path_source.value()
        return self.session.owner.source.matches(
            ChooserSource(source.server, source.client, source.epoch)
        )

    def owns_input(self, mut ui: Ui) -> Bool:
        return self.opened() and self.component.input_scope(ui)

    def reset(mut self, mut ui: Ui):
        self.component.close(ui)
        self.session.reset()
        self.host_scope = None

    def local_opened(mut self, revision: UInt64):
        self.session.local_opened(revision)

    def record_input_owner(mut self, mut ui: Ui):
        if self.owns_input(ui):
            self.host_scope = self.session.owner.scope
            self.host_source = self.session.owner.source
        else:
            self.host_scope = None
            self.component.unmark()

    def observe(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        dialogs: PaletteDialogs,
        source: ChooserSource,
        local_open: Bool,
        epoch: UInt64,
        input: TextInput,
    ) raises -> WindowChooserObservation:
        var enabled = (
            backend.job[].native_chooser
            and backend.online
            and Bool(backend.path_source)
        )
        if enabled:
            var actual = backend.path_source.value()
            enabled = source.matches(
                ChooserSource(actual.server, actual.client, actual.epoch)
            )
        var observed = self.session.observe(
            backend.queue,
            dialogs,
            source,
            enabled,
            local_open,
            epoch,
            NATIVE_PASSIVE,
        )
        self.returned = False
        self.unmark = False
        if observed.owner_changed:
            self.component.close(ui)
            self.component.open(ui, self.session.owner.scope)
            self.marked_seen = input.marked_revision
            self.host_scope = None
            self.unmark = True
        elif observed.closed:
            self.component.close(ui)
            self.marked_seen = input.marked_revision
            self.host_scope = None
            self.unmark = True
        if self.opened():
            self.component.synchronize(
                self.session.controller.data, observed.effects
            )
        return observed^

    def prepare(mut self, mut ui: Ui, input: TextInput):
        if not self.opened():
            return
        var clip = SearchClipboard()
        self.component.prepare(
            ui, clip, len(input.primaries) == 0 and len(input.pointers) == 0
        )

    def compose(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        input: TextInput,
        epoch: UInt64,
    ) raises -> Bool:
        if not self.opened():
            self.marked_seen = input.marked_revision
            return True
        if input.marked_exhausted and not self.marked_exhausted:
            self.marked_exhausted = True
            self.component.unmark()
        if self.marked_exhausted or input.marked_revision == self.marked_seen:
            self.marked_seen = input.marked_revision
            return True
        self.marked_seen = input.marked_revision
        if not (
            self.host_scope == Optional[UInt64](self.session.owner.scope)
            and self.host_source.matches(self.session.owner.source)
            and self.owns_input(ui)
            and self.current(backend)
        ):
            return True
        var checkpoint = WindowChooserEditorCheckpoint(self.component.field)
        try:
            if not self.component.compose(
                Span(input.marked), input.marked_sel_a, input.marked_sel_b
            ):
                return False
            var edited = self.component.edited(self.session.controller.data)
            if edited and not self.operation(ui, backend, edited.value(), epoch):
                raise Error(
                    "window chooser composition belongs to a retired source or"
                    " owner"
                )
        except error:
            checkpoint^.restore(self.component.field)
            raise Error(String(error))
        return True

    def ordered[
        Host: Clipboard
    ](
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        input: TextInput,
        event: TextEvent,
        mut host: Host,
        epoch: UInt64,
    ) raises -> Bool:
        if not self.opened():
            return False
        if event.kind == TEXT_INSERT or event.kind == TEXT_COMMAND:
            return self.event(ui, backend, input, event, host, epoch).consumed
        if event.kind == TEXT_PRIMARY:
            return self.primary(
                ui, backend, pointer_payload(input, event), host, epoch
            )
        if event.kind == TEXT_POINTER:
            self.pointer(ui, backend, pointer_payload(input, event), epoch)
            return True
        return False

    def operation(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        operation: WindowChooserOperation,
        epoch: UInt64,
    ) raises -> Bool:
        if not self.current(backend):
            return False
        var owner = self.session.owner
        var effects = self.session.apply(
            backend.queue, operation, owner, epoch, NATIVE_PASSIVE
        )
        self.settle(ui, backend, effects^, epoch)
        return True

    def operations(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        operations: Span[WindowChooserOperation, _],
        epoch: UInt64,
    ) raises -> Bool:
        if len(operations) == 0:
            return True
        if not self.current(backend):
            return False
        var owner = self.session.owner
        var effects = self.session.apply(
            backend.queue, operations, owner, epoch, NATIVE_PASSIVE
        )
        self.settle(ui, backend, effects^, epoch)
        return True

    def settle(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        var effects: WindowChooserEffects,
        epoch: UInt64,
    ) raises:
        if effects.returned and not self.session.controller.data.finishing:
            var owner = self.session.owner
            _ = self.session.return_default(
                backend.queue, owner, epoch, NATIVE_PASSIVE
            )
            self.component.close(ui)
            self.host_scope = None
            self.returned = True
            self.unmark = True
        else:
            self.component.synchronize(self.session.controller.data, effects)
        ui.request_wake(ui.time)

    def event[
        Host: Clipboard
    ](
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        input: TextInput,
        event: TextEvent,
        mut host: Host,
        epoch: UInt64,
    ) raises -> WindowChooserComponentResult:
        var result = WindowChooserComponentResult()
        if not self.owns_input(ui):
            return result^
        result.consumed = True
        if not self.current(backend):
            return result^
        var checkpoint = WindowChooserEditorCheckpoint(self.component.field)
        var clip = SearchClipboard()
        try:
            if event.kind == TEXT_COMMAND and event.cmd == CMD_PASTE:
                var selection = self.component.field.selection()
                clip.load(
                    host,
                    CHOOSER_DISPLAY_BYTES
                    - len(self.component.field.buf)
                    + selection[1]
                    - selection[0],
                )
            result = self.component.event(
                ui, input, event, clip, self.session.controller.data
            )
            if not self.operations(
                ui, backend, Span(result.operations), epoch
            ):
                raise Error(
                    "window chooser editor belongs to a retired source or owner"
                )
            clip.deliver(host)
        except error:
            checkpoint^.restore(self.component.field)
            raise Error(String(error))
        return result^

    def primary[
        Host: Clipboard
    ](
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        event: MouseEvent,
        mut host: Host,
        epoch: UInt64,
    ) raises -> Bool:
        if not self.opened() or not self.current(backend):
            return False
        if ui.hit_test(event.x, event.y) != self.component.field.key:
            return False
        if not host.supports_primary():
            return True
        var position = ui.text_hit(self.component.field.key, event.x, event.y)
        if position.byte < 0:
            return True
        var checkpoint = WindowChooserEditorCheckpoint(self.component.field)
        try:
            var text = host.get_primary_selection_bounded(CHOOSER_DISPLAY_BYTES)
            self.component.unmark()
            ui.focus = self.component.field.key
            ui.text_input = True
            self.component.field.focused = True
            self.component.field.select(position.byte, position.byte)
            if not self.component.insert(text.as_bytes()):
                raise Error("window chooser PRIMARY exceeds 64 KiB")
            self.component.field.blink_t0 = ui.time
            var edited = self.component.edited(self.session.controller.data)
            if edited and not self.operation(ui, backend, edited.value(), epoch):
                raise Error(
                    "window chooser PRIMARY belongs to a retired source or owner"
                )
            return True
        except error:
            checkpoint^.restore(self.component.field)
            raise Error(String(error))

    def pointer(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        event: MouseEvent,
        epoch: UInt64,
    ) raises:
        if not self.current(backend):
            return
        var state = EditorPointerScope(ui)
        state.apply(ui, event)
        var input = TextInput()
        input.marked = self.component.field.marked.copy()
        input.marked_sel_a = self.component.field.marked_a
        input.marked_sel_b = self.component.field.marked_b
        var clip = SearchClipboard()
        if ui.retained_input_begin(
            self.component.layer, self.component.field.key
        ):
            self.component.field.update(ui, input, clip)
            ui.retained_input_end()
        state.restore(ui)
        var operations = self.component.pointer(
            ui, event, self.session.controller.data
        )
        var edited = self.component.edited(self.session.controller.data)
        if edited:
            operations.insert(0, edited.take())
        _ = self.operations(ui, backend, Span(operations), epoch)

    def wheel(
        mut self,
        ui: Ui,
        x: Float32,
        y: Float32,
        dx: Float32,
        dy: Float32,
        precise: Bool,
    ) raises -> Bool:
        if not self.opened():
            return False
        return self.component.wheel(
            ui,
            x,
            y,
            dx,
            dy,
            precise,
            len(self.session.controller.data.rows[].rows),
        )

    def paint(mut self, mut ui: Ui, theme: PennaTheme) raises:
        if self.opened():
            self.component.paint(ui, theme, self.session.controller.data)
        self.record_input_owner(ui)
