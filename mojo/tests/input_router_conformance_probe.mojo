from std.sys import argv
from std.testing import assert_equal
from zzmojo.verified_input_router import RouterKey, RouterPayload, RouterAction, RouterInput, RouterState
from zzmojo.input_router_codec import RouterReader, encode_request, encode_outcome, state_words, operation_words
from zzmojo.input_router_gate import RouterMapArena, RouterPayloadArena, InputRouterGate, InputRouterSession


struct Cursor(Movable):
    var words: List[UInt32]
    var index: Int

    def __init__(out self, path: String) raises:
        var bytes: List[UInt8]
        with open(path, "r") as source:
            bytes = source.read_bytes()
        if len(bytes) > 16 * 1024 * 1024 or len(bytes) % 4 != 0:
            raise Error("fixture exceeds word budget")
        self.words = List[UInt32]()
        self.index = 0
        for offset in range(0, len(bytes), 4):
            var word: UInt32 = 0
            for byte in range(4):
                word |= UInt32(bytes[offset + byte]) << UInt32(8 * byte)
            self.words.append(word)

    def word(mut self) raises -> UInt32:
        if self.index == len(self.words):
            raise Error("fixture is truncated")
        var value = self.words[self.index]
        self.index += 1
        return value

    def identifier(mut self) raises -> UInt64:
        var high = self.word()
        var low = self.word()
        return (UInt64(high) << 32) | UInt64(low)

    def vector(mut self) raises -> List[UInt32]:
        var count = Int(self.word())
        if count > 262144 or count > len(self.words) - self.index:
            raise Error("fixture vector exceeds limit")
        var result = List[UInt32]()
        for _ in range(count):
            result.append(self.word())
        return result^

    def blob(mut self) raises -> List[UInt8]:
        var size = Int(self.word())
        if size > 4 * 1024 * 1024 or (size + 3) // 4 > len(self.words) - self.index:
            raise Error("fixture blob exceeds limit")
        var result = List[UInt8]()
        for base in range(0, size, 4):
            var word = self.word()
            for byte in range(min(size - base, 4)):
                result.append(UInt8((word >> UInt32(byte * 8)) & 255))
        return result^


def same(actual: List[UInt32], expected: List[UInt32]) raises:
    assert_equal(len(actual), len(expected))
    for index in range(len(actual)):
        assert_equal(actual[index], expected[index], String("word ", index))


def main() raises:
    var args = argv()
    var directory = args[2]
    var corpus = Cursor(directory + "/payloads.bin")
    var key_count = Int(corpus.word())
    var keys = List[RouterKey]()
    for _ in range(key_count):
        keys.append(RouterKey(corpus.identifier()))
    var count = Int(corpus.word())
    var inputs = List[RouterInput]()
    var bodies = List[List[UInt8]]()
    for _ in range(count):
        var key = RouterKey(corpus.identifier())
        var unshifted: Optional[RouterKey] = None
        if corpus.word() != 0:
            unshifted = RouterKey(corpus.identifier())
        var action = UInt8(corpus.word())
        inputs.append(RouterInput(key, unshifted, action, RouterPayload(corpus.identifier())))
        bodies.append(corpus.blob())
    assert_equal(corpus.index, len(corpus.words))
    var path: Optional[String] = None
    if args[1] == "native" or args[1] == "failed":
        path = String(args[3])
    var gate = InputRouterGate(path)
    var manifest = Cursor(directory + "/traces.bin")
    var traces = Int(manifest.word())
    var expected_operations = Int(manifest.word())
    assert_equal(manifest.index, len(manifest.words))
    var operations = 0
    var bytes_compared = 0
    for index in range(traces):
        var fixture = Cursor(directory + "/trace-" + String(index) + ".bin")
        var action_count = Int(fixture.word())
        var actions = List[RouterAction]()
        var action_bodies = List[List[UInt8]]()
        for _ in range(action_count):
            actions.append(RouterAction(fixture.identifier()))
            action_bodies.append(fixture.blob())
        var version = fixture.identifier()
        var scope = UInt64(fixture.word())
        var map = RouterMapArena(version, actions^, action_bodies^)
        var arena = RouterPayloadArena(scope, keys.copy(), inputs.copy(), bodies.copy())
        var session = InputRouterSession(RouterState(version), map^, arena^)
        var steps = Int(fixture.word())
        for _ in range(steps):
            var packet = fixture.vector()
            var original = packet.copy()
            var reader = RouterReader(packet^)
            var expected_before = reader.state()
            var operation = reader.operation()
            reader.complete()
            same(encode_request(session.snapshot(), operation), original)
            assert_equal(len(original), 2 + state_words(expected_before) + operation_words(operation))
            var expected = fixture.vector()
            var prepared = session.prepare(gate, operation)
            same(encode_request(session.snapshot(), operation), original)
            same(encode_outcome(prepared.outcome), expected)
            var expected_delivery = prepared.outcome.answer.effects.copy()
            session.commit(prepared^)
            var delivered = session.take_effects()
            assert_equal(len(delivered), len(expected_delivery))
            assert_equal(len(session.take_effects()), 0)
            operations += 1
            bytes_compared += len(expected) * 4
        assert_equal(fixture.index, len(fixture.words))
    assert_equal(operations, expected_operations)
    if args[1] == "native":
        assert_equal(gate.native_count, UInt64(operations))
        assert_equal(gate.fallback_count, UInt64(0))
    else:
        assert_equal(gate.fallback_count, UInt64(operations))
    print(operations, bytes_compared, gate.native_count, gate.fallback_count)
