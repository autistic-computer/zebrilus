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


struct ActionQueue(Movable, Sized):
    var items: List[Action]
    var pending_bytes: Int

    def __init__(out self):
        self.items = List[Action]()
        self.pending_bytes = 0

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
    ) raises:
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
                    )
                    self.pending_bytes += size
                    return
        if (
            len(self.items) >= MAX_PENDING_ACTIONS
            or size > MAX_PENDING_INPUT_BYTES - self.pending_bytes
        ):
            raise Error("pending terminal input exceeds queue limit")
        self.items.append(
            Action(kind, text, pane, selection_epoch, geometry, frame.copy())
        )
        self.pending_bytes += size

    def take(mut self) -> List[Action]:
        var items = self.items^
        self.items = List[Action]()
        self.pending_bytes = 0
        return items^
