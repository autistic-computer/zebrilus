from std.sys import argv
from zzmojo.protocol import Decoder, append_uint
from zzmojo.tree_wire import read_bool
from zzmojo.path_case import PathCase
from zzmojo.path_pattern import PathPattern
from zzmojo.path_score import PathScorer
from zzmojo.path_cpu import path_vector_bytes
from zzmojo.path_rank import PathLabels, PathRanker
from zzmojo.path_rank_rules import (
    path_depth,
    path_in_basename,
    path_prior,
    path_browse_order,
)


@fieldwise_init
struct VectorLabels(Movable, PathLabels):
    var labels: List[String]
    var marks: List[Bool]

    def count(self) -> Int:
        return len(self.labels)

    def label(self, index: Int) -> String:
        return self.labels[index].copy()

    def changed(self, index: Int) -> Bool:
        return self.marks[index]


def main() raises:
    var args = argv()
    if (
        args[1] != "rules"
        and args[1] != "prefilter"
        and args[1] != "score"
        and args[1] != "rank"
    ):
        raise Error("unknown path rank probe mode")
    var input: List[UInt8]
    with open(args[2], "r") as file:
        input = file.read_bytes()
    var reader = Decoder(input^)
    reader.offset = 0
    var output = List[UInt8]()
    var characters = PathCase()
    if args[1] == "rank":
        var ranker = PathRanker()
        for _ in range(Int(reader.uint())):
            var query = reader.text()
            var labels = List[String]()
            var marks = List[Bool]()
            for _ in range(Int(reader.uint())):
                labels.append(reader.text())
                marks.append(read_bool(reader))
            var limit = Int(reader.uint())
            var source = VectorLabels(labels^, marks^)
            var ranked = ranker.rank(query, source, limit)
            append_uint(output, UInt64(len(ranked)))
            for item in ranked:
                append_uint(output, UInt64(item.index))
            for item in ranked:
                var hits = ranker.highlights(query, source.labels[item.index])
                append_uint(output, UInt64(len(hits)))
                for hit in hits:
                    append_uint(output, UInt64(hit.start))
                    append_uint(output, UInt64(hit.end))
        reader.finish()
        with open(args[3], "w") as file:
            file.write_bytes(Span(output))
        return
    if args[1] == "prefilter" or args[1] == "score":
        var vector_bytes = Int(reader.uint())
        if path_vector_bytes() != vector_bytes:
            raise Error("path CPU dispatch differs from the native reference")
        var scorer = PathScorer(Int(reader.uint()))
        for _ in range(Int(reader.uint())):
            var query = reader.text()
            var label = reader.text()
            var pattern = PathPattern(query, characters)
            var window = pattern.filter(label)
            output.append(UInt8(window.accepted))
            if window.accepted:
                append_uint(output, UInt64(window.start))
                append_uint(output, UInt64(window.end))
                if args[1] == "score":
                    var result = scorer.score(pattern, label, window)
                    append_uint(output, UInt64(result.score))
                    output.append(UInt8(result.exact))
                    append_uint(output, UInt64(len(result.indices)))
                    for index in result.indices:
                        append_uint(output, UInt64(index))
        reader.finish()
        with open(args[3], "w") as file:
            file.write_bytes(Span(output))
        return
    for _ in range(Int(reader.uint())):
        var query = reader.text()
        for _ in range(Int(reader.uint())):
            var label = reader.text()
            var changed = read_bool(reader)
            append_uint(output, UInt64(path_depth(label)))
            output.append(UInt8(path_in_basename(query, label, characters)))
            append_uint(
                output, UInt64(path_prior(query, label, changed, characters))
            )
    for _ in range(Int(reader.uint())):
        var left = reader.text()
        var left_dir = read_bool(reader)
        var right = reader.text()
        var right_dir = read_bool(reader)
        output.append(
            UInt8(path_browse_order(left, left_dir, right, right_dir) + 1)
        )
    for _ in range(Int(reader.uint())):
        var scalar = UInt32(reader.uint())
        output.append(UInt8(characters.upper(scalar)))
        append_uint(output, UInt64(characters.flipped(scalar)))
        append_uint(output, UInt64(characters.lower_first(scalar)))
    reader.finish()
    with open(args[3], "w") as file:
        file.write_bytes(Span(output))
