from std.memory import ArcPointer, Pointer
from .chooser_protocol import ChooseAction
from .chooser_presentation import CHOOSER_DISPLAY_BYTES
from .palette_dialog_state import ChooseSnapshot
from .palette_navigation import navigate, parent_index, child_index
from .palette_text import palette_trim
from .window_chooser_model import WindowChooserBuilder, WindowChooserRows
from .window_chooser_search import WindowChooserSearch, WindowChooserTarget


struct WindowChooserData(Copyable, Movable):
    var snapshot: Optional[ChooseSnapshot]
    var revision: UInt64
    var rows: ArcPointer[WindowChooserRows]
    var selected: Optional[Int]
    var query: String
    var cursor: Int
    var search: WindowChooserSearch
    var activation: Optional[WindowChooserTarget]
    var finishing: Bool
    var navigation: Bool
    var local: Bool

    def __init__(out self):
        self.snapshot = None
        self.revision = 0
        self.rows = ArcPointer(WindowChooserRows())
        self.selected = None
        self.query = String()
        self.cursor = 0
        self.search = WindowChooserSearch()
        self.activation = None
        self.finishing = False
        self.navigation = False
        self.local = False

    def target(self) -> Optional[WindowChooserTarget]:
        if not self.snapshot or not self.selected:
            return None
        var row = self.selected.value()
        if row < 0 or row >= len(self.rows[].rows):
            return None
        var index = self.rows[].rows[row].index
        ref items = self.snapshot.value().items[].values
        if index < 0 or index >= len(items):
            return None
        return WindowChooserTarget(
            items[index].target_kind, items[index].target
        )

    def row_for(self, target: WindowChooserTarget) -> Optional[Int]:
        if not self.snapshot:
            return None
        ref items = self.snapshot.value().items[].values
        for row in range(len(self.rows[].rows)):
            var index = self.rows[].rows[row].index
            if (
                index >= 0
                and index < len(items)
                and target.matches(items[index])
            ):
                return row
        return None


struct WindowChooserEffects(Copyable, Movable):
    var actions: List[ChooseAction]
    var returned: Bool
    var scroll: Optional[Tuple[Int, UInt8]]

    def __init__(out self):
        self.actions = List[ChooseAction]()
        self.returned = False
        self.scroll = None

    def choose(mut self, kind: UInt8, index: UInt32 = 0):
        var action = ChooseAction(kind)
        action.index = index
        self.actions.append(action^)


struct WindowChooserOperation(Copyable, Movable):
    var kind: UInt8
    var snapshot: Optional[ChooseSnapshot]
    var revision: UInt64
    var text: String
    var cursor: Int
    var index: Int
    var direction: Int
    var flag: Bool

    def __init__(out self, kind: UInt8):
        self.kind = kind
        self.snapshot = None
        self.revision = 0
        self.text = String()
        self.cursor = 0
        self.index = 0
        self.direction = 0
        self.flag = False


@fieldwise_init
struct WindowChooserChange(Copyable, Movable):
    var generation: UInt64
    var actor: ArcPointer[UInt8]
    var data: WindowChooserData
    var effects: WindowChooserEffects


struct WindowChooserController(Movable):
    var builder: WindowChooserBuilder
    var grouped: Bool
    var generation: UInt64
    var actor: ArcPointer[UInt8]
    var data: WindowChooserData

    def __init__(out self, grouped: Bool = True):
        self.builder = WindowChooserBuilder()
        self.grouped = grouped
        self.generation = 0
        self.actor = ArcPointer(UInt8(0))
        self.data = WindowChooserData()

    def refresh(
        self, mut data: WindowChooserData, mut effects: WindowChooserEffects
    ) raises:
        if not data.snapshot:
            return
        var rows = self.builder.build(
            Span(data.snapshot.value().items[].values), data.query, self.grouped
        )
        data.rows = ArcPointer(rows^)
        if (
            not data.selected
            or data.selected.value() < 0
            or data.selected.value() >= len(data.rows[].rows)
        ):
            if palette_trim(data.query).byte_length() == 0:
                data.selected = data.rows[].initial
            else:
                data.selected = Optional[Int](0) if len(
                    data.rows[].rows
                ) > 0 else Optional[Int](None)
        if not data.navigation:
            effects.scroll = (0, UInt8(0))

    def advance(
        self, mut data: WindowChooserData, mut effects: WindowChooserEffects
    ) raises:
        if not data.snapshot or data.finishing:
            return
        ref snapshot = data.snapshot.value()
        ref items = snapshot.items[].values
        var searching = palette_trim(data.query).byte_length() > 0
        if (
            searching
            and not data.search.selection
            and UInt64(snapshot.selected) < UInt64(len(items))
        ):
            ref item = items[Int(snapshot.selected)]
            data.search.selection = WindowChooserTarget(
                item.target_kind, item.target
            )
        if data.activation:
            if data.search.waiting(Span(items)):
                return
            var target = data.activation.take()
            for index in range(len(items)):
                if target.matches(items[index]):
                    data.finishing = True
                    effects.choose(10, UInt32(index))
                    return
        var next = data.search.advance(Span(items), searching)
        if next:
            effects.choose(9, next.value()[0])
            effects.choose(UInt8(7 if next.value()[1] else 6))
        elif (
            not searching and not data.search.pending and data.search.selection
        ):
            var target = data.search.selection.take()
            var row = data.row_for(target)
            if row:
                data.selected = row
            if data.selected:
                effects.scroll = (data.selected.value(), UInt8(1))

    def synchronize(
        self,
        mut data: WindowChooserData,
        mut effects: WindowChooserEffects,
        snapshot: ChooseSnapshot,
        revision: UInt64,
    ) raises:
        if data.snapshot and data.revision == revision:
            return
        var target = data.target()
        if not target and UInt64(snapshot.selected) < UInt64(
            len(snapshot.items[].values)
        ):
            ref item = snapshot.items[].values[Int(snapshot.selected)]
            target = WindowChooserTarget(item.target_kind, item.target)
        data.snapshot = snapshot.copy()
        data.revision = revision
        data.finishing = False
        self.refresh(data, effects)
        if target:
            var row = data.row_for(target.value())
            if row:
                data.selected = row
        if data.selected:
            effects.scroll = (data.selected.value(), UInt8(1))
        self.advance(data, effects)

    def query(
        self,
        mut data: WindowChooserData,
        mut effects: WindowChooserEffects,
        text: String,
        cursor: Int,
        programmatic: Bool,
    ) raises:
        if (
            text.byte_length() > CHOOSER_DISPLAY_BYTES
            or cursor < 0
            or cursor > text.byte_length()
        ):
            raise Error("window chooser query or cursor exceeds its limit")
        if not programmatic and data.query == text and data.cursor == cursor:
            return
        if (
            not programmatic
            and palette_trim(data.query).byte_length() == 0
            and palette_trim(text).byte_length() > 0
        ):
            data.search.selection = data.target()
        data.query = text.copy()
        data.cursor = cursor
        data.navigation = False
        data.selected = 0
        self.refresh(data, effects)
        if not programmatic:
            self.advance(data, effects)

    def move_tree(
        self,
        mut data: WindowChooserData,
        mut effects: WindowChooserEffects,
        expand: Bool,
    ) raises:
        if (
            data.query.byte_length() != 0
            or not data.snapshot
            or not data.selected
        ):
            return
        var index = data.selected.value()
        if index < 0 or index >= len(data.rows[].rows):
            return
        if data.rows[].rows[index].expanded == Optional[Bool](not expand):
            if data.search.pending:
                return
            data.navigation = True
            var target = data.target()
            if target:
                data.search.pending = (target.value(), expand)
                effects.choose(9, UInt32(data.rows[].rows[index].index))
                effects.choose(UInt8(7 if expand else 6))
        else:
            var rows = data.rows[].navigation()
            var next = child_index(
                Span(rows), index
            ) if expand else parent_index(Span(rows), index)
            if next:
                data.selected = next
                data.navigation = True
                effects.scroll = (next.value(), UInt8(1))

    def apply(
        self,
        mut data: WindowChooserData,
        mut effects: WindowChooserEffects,
        operation: WindowChooserOperation,
    ) raises:
        var kind = operation.kind
        if kind == 0:
            if not operation.snapshot:
                raise Error("window chooser synchronization has no snapshot")
            self.synchronize(
                data, effects, operation.snapshot.value(), operation.revision
            )
        elif kind == 1 or kind == 10:
            self.query(
                data, effects, operation.text, operation.cursor, kind == 10
            )
        elif kind == 2:
            var rows = data.rows[].navigation()
            var next = navigate(Span(rows), data.selected, operation.direction)
            if next:
                data.selected = next
                data.navigation = True
                effects.scroll = (next.value(), UInt8(1))
        elif kind == 3:
            self.move_tree(data, effects, operation.flag)
        elif kind == 4:
            if not data.selected or not data.snapshot:
                return
            var row = data.selected.value()
            if row < 0 or row >= len(data.rows[].rows):
                return
            if data.search.pending:
                data.activation = data.target()
                return
            data.finishing = True
            effects.choose(10, UInt32(data.rows[].rows[row].index))
        elif kind == 5:
            if (
                operation.index >= 0
                and operation.index < len(data.rows[].rows)
                and data.selected != Optional[Int](operation.index)
            ):
                data.selected = operation.index
                data.navigation = True
        elif kind == 6:
            var count = len(data.rows[].rows)
            if count > 0:
                var index = 0 if operation.flag else count - 1
                data.selected = index
                data.navigation = True
                effects.scroll = (index, UInt8(1))
        elif kind == 7:
            if not data.finishing:
                data.finishing = True
                if data.snapshot:
                    effects.choose(17)
        elif kind == 8:
            if not data.finishing and data.snapshot:
                data.snapshot = None
                data.local = True
                data.search = WindowChooserSearch()
                data.activation = None
                data.rows = ArcPointer(WindowChooserRows())
                data.selected = None
                data.navigation = False
                data.query = String()
                data.cursor = 0
                effects.choose(17)
                effects.scroll = (0, UInt8(0))
        elif kind == 9:
            effects.returned = data.query.byte_length() == 0 and Bool(
                data.snapshot
            )
        elif kind == 12:
            self.advance(data, effects)
        elif kind != 11:
            raise Error("unknown window chooser operation")

    def prepare(
        self, operation: WindowChooserOperation
    ) raises -> WindowChooserChange:
        if self.generation == UInt64.MAX:
            raise Error("window chooser controller generation exhausted")
        var data = self.data.copy()
        var effects = WindowChooserEffects()
        self.apply(data, effects, operation)
        return WindowChooserChange(
            self.generation, self.actor.copy(), data^, effects^
        )

    def validate(self, change: WindowChooserChange) raises:
        if Int(Pointer(to=self.actor[])) != Int(Pointer(to=change.actor[])):
            raise Error(
                "window chooser transition belongs to another controller"
            )
        if (
            self.generation == UInt64.MAX
            or change.generation != self.generation
        ):
            raise Error("stale window chooser transition")

    def commit(mut self, var change: WindowChooserChange) raises:
        self.validate(change)
        self.data = change.data.copy()
        self.generation += 1
