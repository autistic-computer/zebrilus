from .actions import ActionQueue
from .chooser_session import ChooserSource
from .palette_dialog_state import PaletteDialogs
from .window_chooser_controller import (
    WindowChooserController,
    WindowChooserOperation,
    WindowChooserEffects,
)
from .window_chooser_admission import (
    WindowChooserOwner,
    WindowChooserAdmission,
    stage_window_chooser,
    commit_window_chooser,
)


struct WindowChooserObservation(Movable):
    var owner_changed: Bool
    var closed: Bool
    var replaced_local: Bool
    var effects: WindowChooserEffects

    def __init__(out self):
        self.owner_changed = False
        self.closed = False
        self.replaced_local = False
        self.effects = WindowChooserEffects()


struct WindowChooserSession(Movable):
    var controller: WindowChooserController
    var owner: WindowChooserOwner
    var active: Bool
    var closed_revision: UInt64
    var local_revision: Optional[UInt64]
    var pending_close: Optional[UInt64]

    def __init__(out self, grouped: Bool = True):
        self.controller = WindowChooserController(grouped)
        self.owner = WindowChooserOwner(ChooserSource(0, 0, 0), 0)
        self.active = False
        self.closed_revision = 0
        self.local_revision = None
        self.pending_close = None

    def reset(mut self):
        self.active = False
        self.controller = WindowChooserController(self.controller.grouped)

    def local_opened(mut self, revision: UInt64):
        self.local_revision = revision

    def observe(
        mut self,
        mut queue: ActionQueue,
        dialogs: PaletteDialogs,
        source: ChooserSource,
        enabled: Bool,
        local_open: Bool,
        selection_epoch: UInt64,
        passive_kind: Int,
    ) raises -> WindowChooserObservation:
        var result = WindowChooserObservation()
        var source_changed = not self.owner.source.matches(source)
        if source_changed:
            self.pending_close = None
            self.local_revision = None
        if (
            self.pending_close
            and self.pending_close.value() != dialogs.chooser_closed_revision
        ):
            self.pending_close = None
        var accepted = (
            enabled
            and not dialogs.prompt
            and not self.pending_close
            and Bool(dialogs.chooser)
        )
        if accepted:
            accepted = dialogs.chooser.value().kind == 0 and not (
                local_open
                and self.local_revision
                == Optional[UInt64](dialogs.chooser_revision)
            )
        if not accepted:
            result.closed = self.active
            if self.active:
                self.reset()
            return result^
        var operation = WindowChooserOperation(0)
        operation.snapshot = dialogs.chooser.value().copy()
        operation.revision = dialogs.chooser_revision
        var changed = (
            not self.active
            or source_changed
            or self.closed_revision != dialogs.chooser_closed_revision
        )
        if changed:
            if self.owner.scope == UInt64.MAX:
                raise Error("window chooser input ownership exhausted")
            var owner = WindowChooserOwner(source, self.owner.scope + 1)
            var controller = WindowChooserController(self.controller.grouped)
            var admission = stage_window_chooser(
                controller,
                queue,
                operation,
                owner,
                selection_epoch,
                passive_kind,
            )
            var effects = admission.change.effects.copy()
            commit_window_chooser(controller, queue, admission^, owner)
            self.controller = controller^
            self.owner = owner
            self.closed_revision = dialogs.chooser_closed_revision
            self.active = True
            result.owner_changed = True
            result.replaced_local = local_open
            result.effects = effects^
        elif self.controller.data.revision != dialogs.chooser_revision:
            result.effects = self.apply(
                queue, operation, self.owner, selection_epoch, passive_kind
            )
        return result^

    def apply(
        mut self,
        mut queue: ActionQueue,
        operation: WindowChooserOperation,
        owner: WindowChooserOwner,
        selection_epoch: UInt64,
        passive_kind: Int,
    ) raises -> WindowChooserEffects:
        self.check(owner)
        return self.admit(
            queue,
            stage_window_chooser(
                self.controller,
                queue,
                operation,
                owner,
                selection_epoch,
                passive_kind,
            ),
        )

    def apply(
        mut self,
        mut queue: ActionQueue,
        operations: Span[WindowChooserOperation, _],
        owner: WindowChooserOwner,
        selection_epoch: UInt64,
        passive_kind: Int,
    ) raises -> WindowChooserEffects:
        self.check(owner)
        return self.admit(
            queue,
            stage_window_chooser(
                self.controller,
                queue,
                operations,
                owner,
                selection_epoch,
                passive_kind,
            ),
        )

    def check(self, owner: WindowChooserOwner) raises:
        if not self.active or not self.owner.matches(owner):
            raise Error(
                "window chooser action belongs to a retired source or owner"
            )

    def admit(
        mut self, mut queue: ActionQueue, var admission: WindowChooserAdmission
    ) raises -> WindowChooserEffects:
        var effects = admission.change.effects.copy()
        commit_window_chooser(self.controller, queue, admission^, self.owner)
        return effects^

    def return_default(
        mut self,
        mut queue: ActionQueue,
        owner: WindowChooserOwner,
        selection_epoch: UInt64,
        passive_kind: Int,
    ) raises -> WindowChooserEffects:
        var effects = self.apply(
            queue,
            WindowChooserOperation(8),
            owner,
            selection_epoch,
            passive_kind,
        )
        if self.controller.data.local:
            self.pending_close = self.closed_revision
            self.local_revision = None
            self.active = False
        return effects^
