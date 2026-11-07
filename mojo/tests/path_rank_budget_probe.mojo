from zzmojo.path_rank import PathRanker
from zzmojo.path_pattern import PathPattern, PathWindow


def main() raises:
    var ranker = PathRanker()
    var query = String()
    for _ in range(1024):
        query += "a"
    var pattern = PathPattern(query, ranker.characters)
    var window = pattern.filter(query)
    var result = ranker.scorer.score(pattern, query, window)
    if (
        result.score != 16404
        or not result.exact
        or len(result.indices) != 1024
        or result.indices[0] != 1023
        or result.indices[1023] != 0
    ):
        raise Error("maximal short-window source result differs")
    var bytes = ranker.scorer.retained_bytes()
    if bytes > 1031 * 1088 * 3:
        raise Error("path score scratch exceeds its explicit byte budget")
    var refused = False
    try:
        _ = ranker.scorer.score(pattern, query, PathWindow(True, 0, 1025))
    except:
        refused = True
    if not refused:
        raise Error("path scorer accepted an out-of-range byte window")
    print("path score retained scratch bytes", bytes)
