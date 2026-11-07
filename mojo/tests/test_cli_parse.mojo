from std.testing import assert_equal, assert_true
from zzmojo.cli_parse import parse_globals, MAX_CLI_WORDS, MAX_CLI_BYTES


def main() raises:
    var words = List[List[UInt8]]()
    for _ in range(MAX_CLI_WORDS):
        words.append(List[UInt8](String("x").as_bytes()))
    var parsed = parse_globals(words, String("/private").as_bytes())
    assert_equal(parsed.error_kind, UInt8(0))
    assert_equal(len(parsed.remaining), MAX_CLI_WORDS)
    words.append(List[UInt8](String("x").as_bytes()))
    var rejected = False
    try:
        _ = parse_globals(words, String("/private").as_bytes())
    except:
        rejected = True
    assert_true(rejected)
    words.clear()
    var large = String("x") * MAX_CLI_BYTES
    words.append(List[UInt8](large.as_bytes()))
    parsed = parse_globals(words, String("/private").as_bytes())
    assert_equal(parsed.error_kind, UInt8(0))
    assert_equal(len(parsed.remaining[0]), MAX_CLI_BYTES)
    words.append(List[UInt8](String("x").as_bytes()))
    rejected = False
    try:
        _ = parse_globals(words, String("/private").as_bytes())
    except:
        rejected = True
    assert_true(rejected)
    print("bounded CLI argument and byte limits passed")
