from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false, assert_raises, TestSuite
from zzmojo.tree_model import MuxSnapshot, SessionSnapshot, TreeDelta, TreeOp
from zzmojo.tree_codec import (
    MAX_TREE_BYTES, MAX_LIVE_TREE_BYTES, normalize,
    snapshot_from_words, normalized_words, eligible_delta,
)
from zzmojo.native_words import WordBuffer


def test_exact_live_budget_is_eligible_and_next_session_is_not() raises:
    var snapshot = MuxSnapshot()
    var delta = TreeDelta()
    var count = (MAX_LIVE_TREE_BYTES // 4 - 9) // 7
    for index in range(count):
        var session = SessionSnapshot()
        session.id = UInt64(index)
        snapshot.sessions.append(session^)
    assert_equal(normalized_words(snapshot, delta.ops, 0) * 4, MAX_LIVE_TREE_BYTES)
    assert_true(eligible_delta(snapshot, delta))
    snapshot.sessions.append(SessionSnapshot())
    assert_false(eligible_delta(snapshot, delta))
    with assert_raises():
        _ = normalize(snapshot, delta.ops, 0)


def test_tree_arena_rejects_cross_group_references_and_excessive_counts() raises:
    var snapshot = MuxSnapshot()
    snapshot.sessions.append(SessionSnapshot())
    var ops = List[TreeOp]()
    var normalized = normalize(snapshot, ops, 1)
    normalized.current[6] = normalized.current[9]
    with assert_raises():
        _ = snapshot_from_words(normalized.current.copy(), normalized.arena, snapshot, ops)
    var words: List[UInt32] = [0, 0, 0, UInt32.MAX]
    with assert_raises():
        _ = snapshot_from_words(words^, normalized.arena, snapshot, ops)


def test_word_storage_rejects_partial_words_and_oversized_results_before_loading() raises:
    var storage = WordBuffer()
    assert_equal(external_call["ftruncate", Int32](Int(storage.fd), Int(3)), Int32(0))
    with assert_raises():
        _ = storage.load()
    assert_equal(external_call["ftruncate", Int32](Int(storage.fd), Int(MAX_TREE_BYTES + 4)), Int32(0))
    with assert_raises():
        _ = storage.load()


def main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()
