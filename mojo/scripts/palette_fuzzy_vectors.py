import argparse
import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
UNICODE = ROOT / "mojo/zzmojo/palette_unicode.mojo"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    data = value.encode()
    uint(output, len(data))
    output.extend(data)


def unicode_source(source):
    rows = []
    for scalar, folded in source["lowercase"]:
        if not 1 <= len(folded) <= 3 or any(value == 0 for value in folded):
            raise AssertionError("unexpected lowercase mapping shape")
        rows.extend([scalar, *folded, *([0] * (3 - len(folded)))])
    lower = "".join(f"{value:08x}" for value in rows)
    whitespace = "".join(f"{value:08x}" for value in source["whitespace"])
    return f'''comptime LOWERCASE = "{lower}"
comptime WHITESPACE = "{whitespace}"


@inline(.never)
def _table(source: StringSlice) -> List[UInt32]:
    var bytes = source.as_bytes()
    var result = List[UInt32](capacity=len(bytes) // 8)
    for offset in range(0, len(bytes), 8):
        var value: UInt32 = 0
        for index in range(8):
            var digit = bytes[offset + index]
            value = (value << 4) | UInt32(digit - 48 if digit <= 57 else digit - 87)
        result.append(value)
    return result^


def lowercase_table() -> List[UInt32]:
    return _table(LOWERCASE)


def whitespace_table() -> List[UInt32]:
    return _table(WHITESPACE)
'''


def expected(case):
    output = bytearray()
    if case.get("reject"):
        uint(output, 0)
    elif case["expected"] is None:
        uint(output, 1)
    else:
        uint(output, 2)
        uint(output, case["expected"]["score"])
        uint(output, len(case["expected"]["hits"]))
        for start, end in case["expected"]["hits"]:
            uint(output, start)
            uint(output, end)
    return output


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--generate", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = TARGET / "palette-fuzzy-source.json"
    with reference.open("w") as destination:
        subprocess.run(["cargo", "run", "--locked", "-p", "zz-client", "--example", "palette_fuzzy_vectors"], cwd=ROOT, stdout=destination, check=True, timeout=300)
    source = json.loads(reference.read_text())
    generated = unicode_source(source)
    if options.generate:
        UNICODE.write_text(generated)
        print(json.dumps({"generated": str(UNICODE), "lowercase": len(source["lowercase"]), "whitespace": len(source["whitespace"])}))
        return
    if UNICODE.read_text() != generated:
        raise AssertionError("palette Unicode data is stale; run --generate")
    cases = source["cases"]
    cases += [
        {"query": " " * (1024 * 1024), "label": "", "expected": {"score": 0, "hits": []}},
        {"query": " " * (1024 * 1024 + 1), "label": "", "reject": True},
        {"query": "", "label": "x" * (4 * 1024 * 1024), "expected": {"score": 0, "hits": []}},
        {"query": "", "label": "x" * (4 * 1024 * 1024 + 1), "reject": True},
    ]
    requests = bytearray()
    wanted = bytearray()
    uint(requests, len(cases))
    for case in cases:
        text(requests, case["query"])
        text(requests, case["label"])
        item = expected(case)
        uint(wanted, len(item))
        wanted.extend(item)
    request = TARGET / "palette-fuzzy-input.bin"
    output = TARGET / "palette-fuzzy-output.bin"
    binary = TARGET / "palette-fuzzy-probe"
    request.write_bytes(requests)
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/palette_fuzzy_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request), str(output)], cwd=ROOT, check=True, timeout=60)
    actual = output.read_bytes()
    if actual != wanted:
        difference = next((index for index, pair in enumerate(zip(actual, wanted)) if pair[0] != pair[1]), min(len(actual), len(wanted)))
        raise AssertionError(f"palette fuzzy output differs at byte {difference}: {len(actual)} != {len(wanted)}")
    report = {"passed": True, "cases": len(cases), "bytes": len(wanted), "lowercase": len(source["lowercase"]), "whitespace": len(source["whitespace"]), "reference": "actual Rust fuzzy_match and complete scalar lowercase/whitespace properties", "scope": "Unicode expansions, greedy per-label-scalar matching, whitespace removal, exact score and UTF-8 hit ranges; bounded to 1 MiB query and 4 MiB label"}
    (TARGET / "palette-fuzzy-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
