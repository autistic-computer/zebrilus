import argparse
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

from path_list_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
DATA = PORT / "zzmojo/path_case_data.mojo"


def case_source(rows):
    encoded = "".join(f"{word:08x}" for row in rows for word in row)
    return f'''comptime CASE_DATA = "{encoded}"


@inline(.never)
def path_case_table() -> List[UInt32]:
    var bytes = CASE_DATA.as_bytes()
    var result = List[UInt32](capacity=len(bytes) // 8)
    for offset in range(0, len(bytes), 8):
        var value: UInt32 = 0
        for index in range(8):
            var digit = bytes[offset + index]
            value = (value << 4) | UInt32(digit - 48 if digit <= 57 else digit - 87)
        result.append(value)
    return result^
'''


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--generate", action="store_true")
    parser.add_argument("--rules", action="store_true")
    parser.add_argument("--prefilter", action="store_true")
    parser.add_argument("--score", action="store_true")
    parser.add_argument("--rank", action="store_true")
    parser.add_argument("--vector-bytes",type=int,choices=[16,32,64])
    options = parser.parse_args()
    if not options.generate and not options.rules and not options.prefilter and not options.score and not options.rank:
        for flags in [["--rules"],["--score"],["--rank"]]:
            subprocess.run([sys.executable,__file__,*flags],cwd=ROOT,check=True)
        reference = json.loads((TARGET/"path-rank-private-vectors.json").read_text())
        for width in [16,32]:
            if width < reference["host_vector_bytes"]:
                subprocess.run([sys.executable,__file__,"--score","--vector-bytes",str(width)],cwd=ROOT,check=True)
        return
    result = subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-client", "--example", "path_rank_vectors"], cwd=ROOT, check=True, stdout=subprocess.PIPE, timeout=300)
    vectors = json.loads(result.stdout)
    (TARGET / "path-rank-vectors.json").write_text(json.dumps(vectors) + "\n")
    generated = case_source(vectors["unicode"])
    if options.generate:
        DATA.write_text(generated)
        print(json.dumps({"generated": str(DATA), "unicode_records": len(vectors["unicode"])}))
        return
    if DATA.read_text() != generated:
        raise AssertionError("path case data is stale; run --generate")
    probe = TARGET / "path-rank-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/path_rank_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    if options.rank:
        request = bytearray()
        expected = bytearray()
        records = []
        uint(request, len(vectors["cases"]))
        for case in vectors["cases"]:
            text(request, case["query"])
            uint(request, len(case["labels"]))
            for label,changed in zip(case["labels"],case["changed"]):
                text(request, label)
                request.append(int(changed))
            uint(request, case["limit"])
            record = bytearray()
            uint(record, len(case["order"]))
            for index in case["order"]:
                uint(record, index)
            for hits in case["hits"]:
                uint(record, len(hits))
                for start,end in hits:
                    uint(record, start)
                    uint(record, end)
            records.append(record)
            expected.extend(record)
        with tempfile.TemporaryDirectory(prefix="zzpath-rank-") as directory:
            source = Path(directory) / "source.bin"
            actual = Path(directory) / "actual.bin"
            source.write_bytes(request)
            subprocess.run([str(probe), "rank", str(source), str(actual)], check=True, timeout=180)
            output = actual.read_bytes()
            offset = 0
            for case,record in zip(vectors["cases"],records):
                if output[offset:offset+len(record)] != record:
                    raise AssertionError("path rank differs: "+json.dumps(case)+" actual="+repr(output[offset:offset+len(record)]))
                offset += len(record)
            if offset != len(output):
                raise AssertionError("path rank output length differs")
        print(json.dumps({"passed":True,"cases":len(vectors["cases"]),"bytes":len(expected),"scope":"actual public path_rank order and kept byte highlight ranges"}))
        return
    if options.prefilter or options.score:
        from path_rank_native import capture
        reference = capture(vectors,options.vector_bytes)
        if options.vector_bytes is None and reference["public_mismatches"]:
            raise AssertionError("private native observer differs from public matcher")
        request = bytearray()
        expected = bytearray()
        uint(request, reference["host_vector_bytes"])
        uint(request, reference["vector_bytes"])
        uint(request, len(reference["cases"]))
        for case in reference["cases"]:
            text(request, case["query"])
            text(request, case["label"])
            expected.append(int(case["accepted"]))
            if case["accepted"]:
                uint(expected, case["start"])
                uint(expected, case["end"])
                if options.score:
                    uint(expected, case["score"])
                    expected.append(int(case["exact"]))
                    uint(expected, len(case["indices"]))
                    for index in case["indices"]:
                        uint(expected, index)
        with tempfile.TemporaryDirectory(prefix="zzpath-window-") as directory:
            source = Path(directory) / "source.bin"
            actual = Path(directory) / "actual.bin"
            source.write_bytes(request)
            subprocess.run([str(probe), "score" if options.score else "prefilter", str(source), str(actual)], check=True, timeout=180)
            output = actual.read_bytes()
            if output != expected:
                offset = 0
                for case in reference["cases"]:
                    record = bytearray([int(case["accepted"])])
                    if case["accepted"]:
                        uint(record, case["start"])
                        uint(record, case["end"])
                        if options.score:
                            uint(record, case["score"])
                            record.append(int(case["exact"]))
                            uint(record, len(case["indices"]))
                            for index in case["indices"]:
                                uint(record, index)
                    if output[offset:offset+len(record)] != record:
                        raise AssertionError("path window differs: " + json.dumps(case) + " actual=" + repr(output[offset:offset+len(record)]))
                    offset += len(record)
                raise AssertionError("path window output length differs")
        print(json.dumps({"passed":True,"pairs":len(reference["cases"]),"bytes":len(expected),"vector_bytes":reference["vector_bytes"],"public_observer_differences":len(reference["public_mismatches"]),"scope":"actual pinned native matcher scores/raw indices" if options.score else "actual pinned native prefilter acceptance and byte windows"}))
        return
    request = bytearray()
    expected = bytearray()
    uint(request, len(vectors["cases"]))
    for case in vectors["cases"]:
        text(request, case["query"])
        uint(request, len(case["labels"]))
        for index, label in enumerate(case["labels"]):
            text(request, label)
            request.append(int(case["changed"][index]))
            uint(expected, case["depth"][index])
            expected.append(int(case["basename"][index]))
            uint(expected, case["prior"][index])
    uint(request, len(vectors["browse"]))
    for left, left_dir, right, right_dir, order in vectors["browse"]:
        text(request, left)
        request.append(int(left_dir))
        text(request, right)
        request.append(int(right_dir))
        expected.append(order + 1)
    uint(request, len(vectors["unicode"]))
    for scalar, upper, flipped, lower in vectors["unicode"]:
        uint(request, scalar)
        expected.append(upper)
        uint(expected, flipped)
        uint(expected, lower)
    with tempfile.TemporaryDirectory(prefix="zzpath-rank-") as directory:
        source = Path(directory) / "source.bin"
        actual = Path(directory) / "actual.bin"
        source.write_bytes(request)
        subprocess.run([str(probe), "rules", str(source), str(actual)], check=True, timeout=120)
        output = actual.read_bytes()
        if output != expected:
            offset = next((i for i, (left, right) in enumerate(zip(output, expected)) if left != right), min(len(output), len(expected)))
            raise AssertionError(f"path rules differ at byte {offset}: {output[offset:offset+32]!r} != {expected[offset:offset+32]!r}")
    print(json.dumps({"passed": True, "cases": len(vectors["cases"]), "browse": len(vectors["browse"]), "unicode_records": len(vectors["unicode"]), "bytes": len(expected), "reference": "actual path_rank prior/basename/depth/browse helpers and native Unicode properties", "scope": "ranking policy and Unicode data; matcher score/order/highlights remain separate"}))


if __name__ == "__main__":
    main()
