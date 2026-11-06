from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_snapshot, read_delta, read_ops, read_view
from zzmojo.tree_encode import write_snapshot
from zzmojo.tree_model import TreeOp
from zzmojo.tree_codec import normalize, snapshot_from_words, normalized_words, TreeResult
from zzmojo.native_words import WordBuffer
from zzmojo.tree_gate import TreeGate
from zzmojo.bend_gate import BendGate


def reader(path: String) raises -> Decoder:
    var bytes: List[UInt8]
    with open(path, "r") as source:
        bytes = source.read_bytes()
    var result = Decoder(bytes^)
    result.offset = 0
    return result^


def main() raises:
    var args = argv()
    var directory = args[2]
    var modes: List[UInt8]
    with open(directory + "/modes.bin", "r") as source:
        modes = source.read_bytes()
    var gate: Optional[BendGate] = None
    var tree: Optional[TreeGate] = None
    var buffer = WordBuffer()
    if args[1] == "native":
        gate = BendGate(args[3])
        tree = TreeGate()
    elif args[1] != "codec":
        raise Error("unknown tree probe phase")
    for index in range(len(modes)):
        var suffix = String(index) + ".bin"
        var before_reader = reader(directory + "/before-" + suffix)
        var before = read_snapshot(before_reader)
        before_reader.finish()
        var update_reader = reader(directory + "/update-" + suffix)
        var mode = modes[index]
        var base: UInt64 = 0
        var version: UInt64 = 0
        var focus: Optional[UInt64] = None
        var ops: List[TreeOp]
        if mode == 0:
            var delta = read_delta(update_reader)
            ops = delta.ops^
            delta.ops = List[TreeOp]()
            base = delta.base
            version = delta.version
        elif mode == 1:
            ops = read_ops(update_reader)
        elif mode == 2:
            var view = read_view(update_reader)
            ops = view.overlay^
            view.overlay = List[TreeOp]()
            focus = view.focused_window
        else:
            raise Error("unknown tree probe mode")
        update_reader.finish()
        var result = TreeResult()
        if args[1] == "codec":
            var normalized = normalize(before, ops, mode, base, version, focus)
            assert_equal(len(normalized.current) + len(normalized.update), normalized_words(before, ops, mode, focus))
            buffer.store(normalized.current)
            var words = buffer.load()
            assert_equal(len(words), len(normalized.current))
            for word in range(len(words)):
                assert_equal(words[word], normalized.current[word])
            result.snapshot = snapshot_from_words(words^, normalized.arena, before, ops)
        else:
            result = tree.value()._execute(gate.value(), before, ops, mode, base, version, focus)
        var output = List[UInt8]()
        write_snapshot(output, result.snapshot)
        with open(directory + "/after-" + suffix, "w") as destination:
            destination.write_all(Span(output))
        print(result.status, result.expected, result.received, result.target)
