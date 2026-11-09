from mjui.ui import Ui, TextStyle
from mjui.input import (
    Clipboard,
    TextInput,
    TextEvent,
    MouseEvent,
    TEXT_COMMAND,
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
from .window_chooser_admission import WindowChooserOwner
from .window_chooser_editor import WindowChooserEditorCheckpoint
from .search_clipboard import SearchClipboard
from .chooser_presentation import CHOOSER_DISPLAY_BYTES
from .editor_pointer import EditorPointerScope, editor_primary

comptime WINDOW_CHOOSER_KEY: UInt64 = 0x7800000000000000


struct WindowChooserRuntime(Movable):
    var session: WindowChooserSession
    var component: WindowChooserComponent
    var host_scope: Optional[UInt64]
    var host_source: ChooserSource
    var marked_seen: UInt64
    var marked_exhausted: Bool
    var returned: Bool

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

    def record_input_owner(mut self, mut ui: Ui):
        if self.owns_input(ui):
            self.host_scope = self.session.owner.scope
            self.host_source = self.session.owner.source
        else:
            self.host_scope = None
            self.component.field.marked.clear()
            self.component.field.disp.clear()

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
        if observed.owner_changed:
            self.component.close(ui)
            self.component.open(ui, self.session.owner.scope)
            self.marked_seen = input.marked_revision
            self.host_scope = None
        elif observed.closed:
            self.component.close(ui)
            self.marked_seen = input.marked_revision
            self.host_scope = None
        if self.opened():
            self.component.synchronize(
                self.session.controller.data, observed.effects
            )
        return observed^

    def prepare(mut self, mut ui: Ui, input: TextInput) raises -> Bool:
        if not self.opened():
            self.marked_seen = input.marked_revision
            return True
        if input.marked_exhausted:
            self.marked_exhausted = True
            self.marked_seen = input.marked_revision
            self.component.field.marked.clear()
        var owned = (
            self.host_scope == Optional[UInt64](self.session.owner.scope)
            and self.host_source.matches(self.session.owner.source)
            and self.owns_input(ui)
        )
        var composition = TextInput()
        if (
            not self.marked_exhausted
            and input.marked_revision != self.marked_seen
        ):
            self.marked_seen = input.marked_revision
            if owned:
                composition.marked = input.marked.copy()
                composition.marked_sel_a = input.marked_sel_a
                composition.marked_sel_b = input.marked_sel_b
            else:
                composition.marked = self.component.field.marked.copy()
                composition.marked_sel_a = self.component.field.marked_a
                composition.marked_sel_b = self.component.field.marked_b
        else:
            composition.marked = self.component.field.marked.copy()
            composition.marked_sel_a = self.component.field.marked_a
            composition.marked_sel_b = self.component.field.marked_b
        var clip = SearchClipboard()
        return self.component.prepare(
            ui,
            composition,
            clip,
            len(input.primaries) == 0 and len(input.pointers) == 0,
        )

    def operation(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        operation: WindowChooserOperation,
        epoch: UInt64,
        expected: Optional[WindowChooserOwner] = None,
    ) raises -> Bool:
        if (
            not self.current(backend)
            or expected
            and not self.session.owner.matches(expected.value())
        ):
            return False
        var effects = self.session.apply(
            backend.queue, operation, self.session.owner, epoch, NATIVE_PASSIVE
        )
        if effects.returned and not self.session.controller.data.finishing:
            effects = self.session.return_default(
                backend.queue, self.session.owner, epoch, NATIVE_PASSIVE
            )
            self.component.close(ui)
            self.host_scope = None
            self.returned = True
        else:
            self.component.synchronize(self.session.controller.data, effects)
        ui.request_wake(ui.time)
        return True

    def apply_editor(
        mut self,
        mut ui: Ui,
        mut backend: AsyncBackend,
        result: WindowChooserComponentResult,
        epoch: UInt64,
    ) raises:
        for operation in result.operations:
            if not self.operation(ui, backend, operation, epoch):
                raise Error(
                    "window chooser editor belongs to a retired source or owner"
                )

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
            self.apply_editor(ui, backend, result, epoch)
            clip.deliver(host)
        except error:
            checkpoint.restore(self.component.field)
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
        var checkpoint = WindowChooserEditorCheckpoint(self.component.field)
        try:
            var result = editor_primary(
                self.component.field, ui, event, host, CHOOSER_DISPLAY_BYTES
            )
            if result.limited:
                raise Error("window chooser PRIMARY exceeds 64 KiB")
            if result.inserted:
                var operation = WindowChooserOperation(1)
                operation.text = self.component.field.text()
                operation.cursor = self.component.field.caret
                if not self.operation(ui, backend, operation, epoch):
                    raise Error(
                        "window chooser PRIMARY belongs to a retired source or"
                        " owner"
                    )
            return result.consumed
        except error:
            checkpoint.restore(self.component.field)
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
        for operation in self.component.pointer(
            ui, event, self.session.controller.data
        ):
            _ = self.operation(ui, backend, operation, epoch)
        if self.component.field.caret != self.session.controller.data.cursor:
            var operation = WindowChooserOperation(1)
            operation.text = self.component.field.text()
            operation.cursor = self.component.field.caret
            _ = self.operation(ui, backend, operation, epoch)

    def paint(mut self, mut ui: Ui, theme: PennaTheme) raises:
        if self.opened():
            self.component.paint(ui, theme, self.session.controller.data)
        self.record_input_owner(ui)
