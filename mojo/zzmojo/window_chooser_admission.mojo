from .actions import Action, ActionQueue, ActionBatch
from .chooser_protocol import choose_request
from .chooser_session import ChooserSource
from .window_chooser_controller import (
    WindowChooserController,
    WindowChooserOperation,
    WindowChooserChange,
)


@fieldwise_init
struct WindowChooserOwner(TrivialRegisterPassable):
    var source: ChooserSource
    var scope: UInt64

    def matches(self, other: Self) -> Bool:
        return self.scope == other.scope and self.source.matches(other.source)


@fieldwise_init
struct WindowChooserAdmission(Movable):
    var owner: WindowChooserOwner
    var change: WindowChooserChange
    var batch: Optional[ActionBatch]


def stage_window_chooser(
    controller: WindowChooserController,
    queue: ActionQueue,
    operation: WindowChooserOperation,
    owner: WindowChooserOwner,
    selection_epoch: UInt64,
    passive_kind: Int,
) raises -> WindowChooserAdmission:
    var change = controller.prepare(operation)
    var batch: Optional[ActionBatch] = None
    if len(change.effects.actions) > 0:
        var actions = List[Action](capacity=len(change.effects.actions))
        for action in change.effects.actions:
            actions.append(
                Action(
                    passive_kind,
                    String(),
                    None,
                    selection_epoch,
                    None,
                    choose_request(action),
                    (owner.source.server, owner.source.client),
                    None,
                )
            )
        batch = queue.stage(actions^)
    return WindowChooserAdmission(owner, change^, batch^)


def commit_window_chooser(
    mut controller: WindowChooserController,
    mut queue: ActionQueue,
    var admission: WindowChooserAdmission,
    current: WindowChooserOwner,
) raises:
    if not current.matches(admission.owner):
        raise Error(
            "window chooser admission belongs to a retired source or owner"
        )
    controller.validate(admission.change)
    if admission.batch:
        queue.validate_batch(admission.batch.value())
        var batch = admission.batch.take()
        queue.commit_batch(batch)
    controller.commit(admission.change.copy())
