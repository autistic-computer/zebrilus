from .actions import MAX_PENDING_INPUT_BYTES


def should_paste_primary(command_output: Bool, mouse_tracking: Bool, shift: Bool) -> Bool:
    return not command_output and (shift or not mouse_tracking)


@fieldwise_init
struct PrimaryTarget(TrivialRegisterPassable):
    var pane: UInt64
    var server: UInt64
    var client: UInt64
    var epoch: UInt64


@fieldwise_init
struct PrimaryPaste(Copyable, Movable):
    var target: PrimaryTarget
    var text: String

struct PrimaryPasteController(Movable):
    var pressed: Optional[PrimaryTarget]
    var pending: Optional[PrimaryTarget]

    def __init__(out self):
        self.pressed = None
        self.pending = None

    def reset(mut self):
        self.pressed = None
        self.pending = None

    def press(mut self, target: PrimaryTarget, command_output: Bool, tracking: Bool, shift: Bool) -> Bool:
        self.pressed = None
        self.pending = None
        if not should_paste_primary(command_output, tracking, shift):
            return False
        self.pressed = target
        self.pending = target
        return True

    def release(mut self) -> Bool:
        var consumed = Bool(self.pressed)
        self.pressed = None
        return consumed

    def accepts(self, target: PrimaryTarget) -> Bool:
        if not self.pending:
            return False
        var owner = self.pending.value()
        return owner.pane == target.pane and owner.server == target.server and owner.client == target.client and owner.epoch == target.epoch

    def prepare(mut self, target: PrimaryTarget, text: String) raises -> Optional[PrimaryPaste]:
        if not self.accepts(target):
            self.pending = None
            return None
        var owner = self.pending.take()
        if text.byte_length() == 0:
            return None
        if text.byte_length() > MAX_PENDING_INPUT_BYTES:
            raise Error("PRIMARY paste exceeds the terminal paste limit")
        return PrimaryPaste(owner, text.copy())
