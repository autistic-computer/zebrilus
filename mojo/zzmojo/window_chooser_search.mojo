from .chooser_protocol import ChooseItem, CHOOSER_MAX_ITEMS


@fieldwise_init
struct WindowChooserTarget(TrivialRegisterPassable):
    var kind: UInt8
    var id: UInt64

    def matches(self, item: ChooseItem) -> Bool:
        return self.kind == item.target_kind and self.id == item.target

    def equals(self, other: Self) -> Bool:
        return self.kind == other.kind and self.id == other.id


struct WindowChooserSearch(Copyable, Movable):
    var expanded: List[WindowChooserTarget]
    var pending: Optional[Tuple[WindowChooserTarget, Bool]]
    var selection: Optional[WindowChooserTarget]

    def __init__(out self):
        self.expanded = List[WindowChooserTarget]()
        self.pending = None
        self.selection = None

    def waiting(self, items: Span[ChooseItem, _]) -> Bool:
        if self.pending:
            var target = self.pending.value()[0]
            var expanded = self.pending.value()[1]
            for item in items:
                if (
                    target.matches(item)
                    and Bool(item.flags & 1) != expanded
                    and (not expanded or item.flags & 2 != 0)
                ):
                    return True
        return False

    def advance(
        mut self, items: Span[ChooseItem, _], searching: Bool
    ) raises -> Optional[Tuple[UInt32, Bool]]:
        if len(items) > CHOOSER_MAX_ITEMS:
            raise Error("window chooser search exceeds 65,536 rows")
        if self.waiting(items):
            return None
        var next = Optional[UInt32](None)
        var target = WindowChooserTarget(0, 0)
        if searching:
            for index in range(len(items)):
                if items[index].flags & 2 != 0 and items[index].flags & 1 == 0:
                    next = UInt32(index)
                    target = WindowChooserTarget(
                        items[index].target_kind, items[index].target
                    )
                    break
        else:
            for previous in range(len(self.expanded) - 1, -1, -1):
                for index in range(len(items)):
                    if (
                        self.expanded[previous].matches(items[index])
                        and items[index].flags & 1 != 0
                    ):
                        next = UInt32(index)
                        target = self.expanded[previous]
                        break
                if next:
                    break
        if not next:
            self.pending = None
            if not searching:
                self.expanded.clear()
            return None
        var recorded = False
        for previous in self.expanded:
            recorded = recorded or previous.equals(target)
        if (
            searching
            and not recorded
            and len(self.expanded) == CHOOSER_MAX_ITEMS
        ):
            raise Error(
                "window chooser expansion history exceeds 65,536 targets"
            )
        if searching and not recorded:
            self.expanded.append(target)
        self.pending = (target, searching)
        return (next.value(), searching)
