import argparse
import itertools
import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
DATA = ROOT / "mojo/zzmojo/cli_attach_data.mojo"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    data = value.encode() if isinstance(value, str) else bytes(value)
    uint(output, len(data))
    output.extend(data)


def vectors():
    cases = [[]]
    for value in range(256):
        raw = bytes([value])
        cases += [[b"-" + raw], [raw], [b"-t", raw], [b"-c", raw], [b"-f", raw]]
    for size in range(1, 4):
        for flags in itertools.product("dErxtcf?", repeat=size):
            word = ("-" + "".join(flags)).encode()
            cases += [[word], [word, b"work"], [word, b"--restart-daemon"]]
    values = [b"", b"-", b"--", b"work", b"--restart-daemon", b"a\0b", "世界".encode(), b"\xff\0", b"\xc0\xaf", b"\xf0\x80\x80", b"\xed\xa0\x80", b"\xf4\x90\x80\x80"]
    for flag in (b"-t", b"-c", b"-f"):
        for first in values:
            cases += [[flag, first], [flag + first], [flag, first, b"work"], [b"work", flag, first], [b"--", flag, first], [flag, first, b"--restart-daemon"], [flag, first, flag, b"latest"]]
    for prefix in ([], [b"work"], [b"-t", b"work"], [b"-c", b"/tmp"], [b"--"]):
        for count in range(1, 4):
            for tail in ([], [b"arg"], [b"--"], [b"-d"]):
                cases.append([*prefix, *([b"--restart-daemon"] * count), *tail])
    for options in ([b"--restart"], [b"--restart-daemon="], [b"--restart-daemon\0"], [b"-d", b"--", b"-t"], [b"-xErdf", b""], [b"-t", b"", b""], [b"-c", b""], [b"-f", b""], [b"-d", b"--restart-daemon", b"work"], [b"work", b"--", b"--restart-daemon"]):
        cases.append(options)
    cases += [[b"-d"] * 4096, [b"x" * (4 * 1024 * 1024)]]
    return list(dict.fromkeys(tuple(case) for case in cases))


def expected(case):
    output = bytearray()
    uint(output, case["code"])
    uint(output, int(case["error"] is not None))
    if case["error"] is not None:
        text(output, case["error"])
    else:
        uint(output, int(case["restart"]))
        uint(output, int(case["default_attach"]))
        text(output, case["command"]["name"])
        uint(output, len(case["command"]["args"]))
        for argument in case["command"]["args"]:
            text(output, argument)
        output.extend(b"\0\0\0\0\0")
    return output


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--generate", action="store_true")
    options = parser.parse_args()
    cases = vectors()
    subprocess.run(["cargo", "build", "--locked", "-p", "zz-cli", "--example", "cli_attach_vectors"], cwd=ROOT, check=True, timeout=300)
    request = {"cases": [[list(word) for word in case] for case in cases]}
    result = subprocess.run([str(ROOT / "target/debug/examples/cli_attach_vectors")], input=json.dumps(request).encode(), cwd=ROOT, capture_output=True, check=True, timeout=120)
    source = json.loads(result.stdout)
    generated = "comptime NATIVE_ATTACH_USAGE = " + json.dumps(source["native_usage"]) + "\ncomptime TMUX_ATTACH_USAGE = " + json.dumps(source["tmux_usage"]) + "\n"
    if options.generate:
        DATA.write_text(generated)
        print(json.dumps({"generated": str(DATA), "cases": len(cases)}))
        return
    if DATA.read_text() != generated:
        raise AssertionError("native attach usage data is stale; run --generate")
    results = source["cases"]
    bounded = [([b"-d"] * 4097, "zz: attach arguments exceed their count limit"), ([b"x" * (4 * 1024 * 1024 + 1)], "zz: attach arguments exceed their byte limit")]
    for words, message in bounded:
        cases.append(tuple(words))
        results.append({"error": message, "code": 2})
    requests = bytearray()
    wanted = bytearray()
    boundaries = []
    uint(requests, len(cases))
    for case, parsed in zip(cases, results, strict=True):
        uint(requests, len(case))
        for word in case:
            text(requests, word)
        item = expected(parsed)
        boundaries.append(len(wanted))
        uint(wanted, len(item))
        wanted.extend(item)
    request_path = TARGET / "cli-attach-input.bin"
    output = TARGET / "cli-attach-output.bin"
    binary = TARGET / "cli-attach-probe"
    request_path.write_bytes(requests)
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/cli_attach_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request_path), str(output)], cwd=ROOT, check=True, timeout=90)
    actual = output.read_bytes()
    if actual != wanted:
        difference = next((index for index, pair in enumerate(zip(actual, wanted)) if pair[0] != pair[1]), min(len(actual), len(wanted)))
        case_index = max(0, sum(boundary <= difference for boundary in boundaries) - 1)
        short = [word[:80] for word in cases[case_index][:10]]
        raise AssertionError(f"attach differs near case {case_index}, byte {difference}: {short!r}; sizes {len(actual)} != {len(wanted)}")
    report = {"passed": True, "cases": len(cases), "source_cases": len(cases) - len(bounded), "bounded_cases": len(bounded), "bytes": len(wanted), "source": "actual Rust native attach parser, command normalization and error output", "scope": "raw/lossy values, flag clusters/attached values, complete normalized invocation, restart grammar, default attach classification, explicit boundary, exact messages and distinct usage codes"}
    (TARGET / "cli-attach-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
