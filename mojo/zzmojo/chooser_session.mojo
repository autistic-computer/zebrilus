from .chooser_protocol import ChooseAction
from .chooser_presentation import CHOOSER_DISPLAY_BYTES, CHOOSER_ROW_HEIGHT
from .chooser_runtime import (
    ChooserInput,
    chooser_center_scroll,
    chooser_wheel_scroll,
)
from .palette_dialog_state import PaletteDialogs, ChooseSnapshot
from .input_protocol import KeyInput


@fieldwise_init
struct ChooserSource(TrivialRegisterPassable):
    var server: UInt64
    var client: UInt64
    var epoch: UInt64

    def matches(self, other: Self) -> Bool:
        return (
            self.server == other.server
            and self.client == other.client
            and self.epoch == other.epoch
        )


@fieldwise_init
struct ChooserPlan(Copyable, Movable):
    var source: ChooserSource
    var scope: UInt64
    var action: ChooseAction


@fieldwise_init
struct ChooserObservation(TrivialRegisterPassable):
    var owner_changed: Bool
    var closed: Bool
    var selection_changed: Bool


struct ChooserSession(Movable):
    var source: ChooserSource
    var scope: UInt64
    var closed_revision: UInt64
    var snapshot: Optional[ChooseSnapshot]
    var input: ChooserInput
    var scroll: Float32
    var viewport: Float32
    var row_height: Float32
    var line_height: Float32
    var center_pending: Bool
    var marked_seen: UInt64
    var marked_exhausted: Bool

    def __init__(out self):
        self.source = ChooserSource(0, 0, 0)
        self.scope = 0
        self.closed_revision = 0
        self.snapshot = None
        self.input = ChooserInput()
        self.scroll = 0
        self.viewport = 0
        self.row_height = CHOOSER_ROW_HEIGHT
        self.line_height = 0
        self.center_pending = False
        self.marked_seen = 0
        self.marked_exhausted = False

    def opened(self) -> Bool:
        return Bool(self.snapshot)

    def reset(mut self):
        self.snapshot = None
        self.input = ChooserInput()
        self.scroll = 0
        self.viewport = 0
        self.center_pending = False

    def observe(
        mut self,
        dialogs: PaletteDialogs,
        source: ChooserSource,
        enabled: Bool,
        marked_revision: UInt64,
    ) raises -> ChooserObservation:
        var accepted = enabled and Bool(dialogs.chooser)
        if accepted:
            accepted = (
                dialogs.chooser.value().kind == 1
                or dialogs.chooser.value().kind == 2
            )
        if not accepted:
            var closed = self.opened()
            self.reset()
            self.marked_seen = marked_revision
            return ChooserObservation(False, closed, False)
        ref state = dialogs.chooser.value()
        if (
            state.search
            and state.search.value().query.byte_length() > CHOOSER_DISPLAY_BYTES
        ):
            raise Error("chooser search exceeds 64 KiB display limit")
        var owner_changed = (
            not self.opened()
            or not self.source.matches(source)
            or self.closed_revision != dialogs.chooser_closed_revision
        )
        if owner_changed:
            if self.scope == UInt64.MAX:
                raise Error("chooser input ownership exhausted")
            self.scope += 1
            self.source = source
            self.closed_revision = dialogs.chooser_closed_revision
            self.input = ChooserInput()
            self.scroll = 0
            self.center_pending = False
            self.marked_seen = marked_revision
        var result = self.input.synchronize(
            dialogs.chooser_revision, state.selected, Bool(state.search)
        )
        self.snapshot = state.copy()
        self.center_pending = self.center_pending or result.selection_changed
        return ChooserObservation(
            owner_changed, False, result.selection_changed
        )

    def current(self, plan: ChooserPlan) -> Bool:
        return (
            self.opened()
            and self.scope == plan.scope
            and self.source.matches(plan.source)
        )

    def capture(self, action: ChooseAction) raises -> ChooserPlan:
        if not self.opened():
            raise Error("chooser action has no active owner")
        return ChooserPlan(self.source, self.scope, action.copy())

    def key(
        self, input: KeyInput, character: Bool
    ) raises -> Optional[ChooserPlan]:
        if not self.opened() or input.action == 2:
            return None
        if self.input.key_is_text(character, input.modifiers):
            return None
        var action = ChooseAction(18)
        action.key = input.copy()
        action.key.action = 0
        return self.capture(action)

    def commit(mut self, text: String) raises -> Optional[ChooserPlan]:
        if not self.opened():
            return None
        var query = (
            self.snapshot.value()
            .search.value()
            .query.byte_length() if self.snapshot.value()
            .search else 0
        )
        var commit = self.input.replace_text(text, query)
        if not commit:
            return None
        var action = ChooseAction(12)
        action.text = commit.take()
        return self.capture(action)

    def mark(
        mut self,
        text: String,
        revision: UInt64,
        exhausted: Bool,
        owned: Bool,
    ) raises -> Bool:
        if exhausted:
            self.marked_exhausted = True
            self.marked_seen = revision
            return self.input.unmark()
        if self.marked_exhausted or revision == self.marked_seen:
            return False
        self.marked_seen = revision
        if not owned or not self.opened():
            return False
        var query = (
            self.snapshot.value()
            .search.value()
            .query.byte_length() if self.snapshot.value()
            .search else 0
        )
        self.input.mark_text(text, query)
        return True

    def select(
        self, index: UInt32, activate: Bool = False
    ) raises -> ChooserPlan:
        var action = ChooseAction(UInt8(10 if activate else 9))
        action.index = index
        return self.capture(action)

    def close(self) raises -> ChooserPlan:
        return self.capture(ChooseAction(17))

    def geometry(
        mut self, viewport: Float32, row_height: Float32, line_height: Float32
    ) raises:
        var selected = (
            self.input.selected.value() if self.input.selected else UInt32(0)
        )
        var count = UInt32(
            len(self.snapshot.value().items[].values)
        ) if self.snapshot else UInt32(0)
        var centered = chooser_center_scroll(
            self.scroll, row_height, viewport, count, selected
        )
        var clamped = chooser_wheel_scroll(
            self.scroll,
            max(Float32(count) * row_height - viewport, Float32(0)),
            line_height,
            0,
            0,
            True,
        )
        if self.center_pending:
            clamped = centered
        self.scroll = clamped
        self.viewport = viewport
        self.row_height = row_height
        self.line_height = line_height
        self.center_pending = False

    def wheel(mut self, x: Float32, y: Float32, precise: Bool) raises:
        if not self.snapshot:
            return
        var maximum = max(
            Float32(len(self.snapshot.value().items[].values)) * self.row_height
            - self.viewport,
            Float32(0),
        )
        self.scroll = chooser_wheel_scroll(
            self.scroll, maximum, self.line_height, x, y, precise
        )
