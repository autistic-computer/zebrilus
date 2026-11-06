from .bend_gate import BendGate
from .native_words import WordBuffers
from .tree_model import MuxSnapshot, TreeOp, TreeDelta, ClientView
from .tree_codec import TreeResult, normalize, result_from_words


struct TreeGate(Movable):
    var buffers: WordBuffers

    def __init__(out self) raises:
        self.buffers = WordBuffers()

    def _execute(
        mut self, mut gate: BendGate, snapshot: MuxSnapshot, ops: List[TreeOp],
        mode: UInt8, base: UInt64 = 0, version: UInt64 = 0,
        focus: Optional[UInt64] = None,
    ) raises -> TreeResult:
        var normalized = normalize(snapshot, ops, mode, base, version, focus)
        self.buffers.current.store(normalized.current)
        self.buffers.update.store(normalized.update)
        self.buffers.result.clear()
        var kind = String("delta") if mode == 0 else String("overlay") if mode == 1 else String("view")
        var status = gate.tree(kind, self.buffers.current.path, self.buffers.update.path, self.buffers.result.path, Int32(8000))
        var result = result_from_words(self.buffers.result.load(), normalized.arena, snapshot, ops)
        if result.status != status:
            raise Error("Bend tree response status disagrees with its result packet")
        return result^

    def delta(mut self, mut gate: BendGate, snapshot: MuxSnapshot, delta: TreeDelta) raises -> TreeResult:
        return self._execute(gate, snapshot, delta.ops, 0, delta.base, delta.version)

    def overlay(mut self, mut gate: BendGate, snapshot: MuxSnapshot, ops: List[TreeOp]) raises -> TreeResult:
        return self._execute(gate, snapshot, ops, 1)

    def view(mut self, mut gate: BendGate, snapshot: MuxSnapshot, view: ClientView) raises -> TreeResult:
        return self._execute(gate, snapshot, view.overlay, 2, focus=view.focused_window)
