from .geometry import GridSize

comptime MAX_PENDING_INPUT_BYTES = 4 * 1024 * 1024
comptime MAX_PENDING_ACTIONS = 4096


@fieldwise_init
struct Action(Copyable, Movable):
    var kind: Int
    var text: String
    var pane: Optional[UInt64]
    var selection_epoch: UInt64
    var geometry: Optional[GridSize]
    var frame: List[UInt8]
    var source: Optional[Tuple[UInt64, UInt64]]
    var output_owner: Optional[UInt64]


@fieldwise_init
struct ActionBatch(Movable):
    var generation: UInt64
    var items: List[Action]
    var pending_bytes: Int


struct ActionQueue(Movable, Sized):
    var items: List[Action]
    var pending_bytes: Int
    var _generation: UInt64

    def __init__(out self):
        self.items = List[Action]()
        self.pending_bytes = 0
        self._generation = 0

    def __len__(self) -> Int:
        return len(self.items)

    def append(
        mut self,
        kind: Int,
        text: String,
        pane: Optional[UInt64],
        selection_epoch: UInt64 = 0,
        geometry: Optional[GridSize] = None,
        frame: List[UInt8] = List[UInt8](),
        source: Optional[Tuple[UInt64, UInt64]] = None,
        output_owner: Optional[UInt64] = None,
    ) raises:
        if self._generation == UInt64.MAX:
            raise Error("action queue generation exhausted")
        var size = text.byte_length() + len(frame)
        if geometry:
            for index in range(len(self.items) - 1, -1, -1):
                if (
                    self.items[index].kind == kind
                    and self.items[index].pane == pane
                ):
                    var previous_size = self.items[
                        index
                    ].text.byte_length() + len(self.items[index].frame)
                    if (
                        size
                        > MAX_PENDING_INPUT_BYTES
                        - self.pending_bytes
                        + previous_size
                    ):
                        raise Error(
                            "pending terminal input exceeds queue limit"
                        )
                    self.pending_bytes -= previous_size
                    self.items[index] = Action(
                        kind,
                        text,
                        pane,
                        selection_epoch,
                        geometry,
                        frame.copy(),
                        source,
                        output_owner,
                    )
                    self.pending_bytes += size
                    self._generation += 1
                    return
        if (
            len(self.items) >= MAX_PENDING_ACTIONS
            or size > MAX_PENDING_INPUT_BYTES - self.pending_bytes
        ):
            raise Error("pending terminal input exceeds queue limit")
        self.items.append(
            Action(
                kind,
                text,
                pane,
                selection_epoch,
                geometry,
                frame.copy(),
                source,
                output_owner,
            )
        )
        self.pending_bytes += size
        self._generation += 1

    def generation(self) -> UInt64:
        return self._generation

    def stage(self, batch: List[Action]) raises -> ActionBatch:
        if self._generation == UInt64.MAX or len(batch) > MAX_PENDING_ACTIONS:
            raise Error("action batch exceeds capacity or generation limit")
        var size = 0
        for action in batch:
            var amount = action.text.byte_length() + len(action.frame)
            if amount > MAX_PENDING_INPUT_BYTES - size:
                raise Error("action batch exceeds byte limit")
            size += amount
        var staged = ActionQueue()
        staged.items = self.items.copy()
        staged.pending_bytes = self.pending_bytes
        for action in batch:
            staged.append(action.kind, action.text, action.pane, action.selection_epoch, action.geometry, action.frame, action.source, action.output_owner)
        var items = List[Action]()
        swap(items, staged.items)
        return ActionBatch(self._generation, items^, staged.pending_bytes)

    def validate_batch(self, batch: ActionBatch) raises:
        if batch.generation != self._generation or self._generation == UInt64.MAX:
            raise Error("action batch is stale")
        if len(batch.items) > MAX_PENDING_ACTIONS:
            raise Error("action batch exceeds count limit")
        var size = 0
        for action in batch.items:
            var amount = action.text.byte_length() + len(action.frame)
            if amount > MAX_PENDING_INPUT_BYTES - size:
                raise Error("action batch exceeds byte limit")
            size += amount
        if size != batch.pending_bytes:
            raise Error("action batch byte receipt does not match")

    def commit_batch(mut self, mut batch: ActionBatch) raises:
        self.validate_batch(batch)
        swap(self.items, batch.items)
        self.pending_bytes = batch.pending_bytes
        self._generation += 1

    def take(mut self) -> List[Action]:
        var items = self.items^
        self.items = List[Action]()
        self.pending_bytes = 0
        if self._generation != UInt64.MAX:
            self._generation += 1
        return items^
