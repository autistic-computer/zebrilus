from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text, read_bool
from zzmojo.which_key_timing import WhichKeyTiming, WhichKeyChange, arm_table


def read_state(mut source: Decoder) raises -> WhichKeyTiming:
    var generation = source.uint()
    var timer = read_bool(source)
    return WhichKeyTiming(generation, timer, read_bool(source))


def transition(state: WhichKeyTiming, operation: UInt64, arguments: List[UInt64]) raises -> WhichKeyChange:
    var expected = [0, 3, 2, 4, 1, 1]
    if operation > 5 or len(arguments) != expected[Int(operation)]:
        raise Error("which-key timing case has an unknown operation shape")
    if operation == 0:
        return state.hide()
    if operation == 1:
        return state.input(arguments[0] != 0, arguments[1] != 0, arguments[2] != 0)
    if operation == 2:
        return state.table_changed(arguments[0] != 0, arguments[1] != 0)
    if operation == 3:
        return state.fire(arguments[0], arguments[1] != 0, arguments[2] != 0, arguments[3] != 0)
    if operation == 4:
        return state.render(arguments[0] != 0)
    return state.show_all(arguments[0] != 0)


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(String(args[1]), "r") as file:
        bytes = file.read_bytes()
    if len(bytes) > 1024 * 1024:
        raise Error("which-key timing reference exceeds fixture limit")
    var source = Decoder(bytes^)
    source.offset = 0
    var count = Int(source.uint())
    var operations = List[Int](length=6, fill=0)
    var notified = 0
    var built = 0
    for index in range(count):
        var before = read_state(source)
        var operation = source.uint()
        var arity = Int(source.uint())
        if arity > 4:
            raise Error("which-key timing case has too many arguments")
        var arguments = List[UInt64]()
        for _ in range(arity):
            arguments.append(source.uint())
        var actual = transition(before, operation, arguments)
        var after = read_state(source)
        var notify = read_bool(source)
        var build = read_bool(source)
        try:
            assert_equal(actual.state.generation, after.generation)
            assert_equal(actual.state.timer, after.timer)
            assert_equal(actual.state.visible, after.visible)
            assert_equal(actual.notify, notify)
            assert_equal(actual.build, build)
        except error:
            print("which-key timing case", index, "operation", operation)
            raise error
        operations[Int(operation)] += 1
        notified += Int(notify)
        built += Int(build)
    var tables = Int(source.uint())
    for index in range(tables):
        var table: Optional[Tuple[String, Bool]] = None
        if read_bool(source):
            var name = read_text(source)
            table = Tuple(name^, read_bool(source))
        var expected: Optional[String] = None
        if read_bool(source):
            expected = read_text(source)
        try:
            assert_equal(arm_table(table), expected)
        except error:
            print("which-key arm table case", index)
            raise error
    source.finish()
    print(count, tables, operations[0], operations[1], operations[2], operations[3], operations[4], operations[5], notified, built)
