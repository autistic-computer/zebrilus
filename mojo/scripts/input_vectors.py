import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def blob(output, value):
    uint(output, len(value))
    output.extend(value)


def text(output, value):
    blob(output, value.encode())


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-reference", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = TARGET / "input-vectors.json"
    if not options.skip_reference:
        result = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "zz-protocol", "--example", "input_port_vectors"], cwd=ROOT, capture_output=True)
        if result.returncode:
            raise SystemExit(result.stderr.decode())
        current = json.loads(result.stdout)
        reference.write_text(json.dumps(current, indent=2) + "\n")
    else:
        current = json.loads(reference.read_text())
    routing_reference = TARGET / "input-routing-vectors.json"
    if not options.skip_reference:
        result = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "zz-client", "--example", "input_routing_vectors"], cwd=ROOT, capture_output=True)
        if result.returncode:
            raise SystemExit(result.stderr.decode())
        routing = json.loads(result.stdout)
        routing_reference.write_text(json.dumps(routing, indent=2) + "\n")
    else:
        routing = json.loads(routing_reference.read_text())
    probe = TARGET / "input-probe"
    if not options.skip_build:
        subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/input_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    counts = {"encoders": 0, "roundtrips": 0, "mappings": 0, "rejections": 0, "accepted_boundaries": 0, "routing_states": 0, "routing_claims": 0, "routing_boundaries": 0}
    with tempfile.TemporaryDirectory(prefix="zz-input-") as directory:
        temporary = Path(directory)
        source = temporary / "source.bin"
        actual = temporary / "actual.bin"

        def check(mode, case, value):
            source.write_bytes(bytes(value))
            subprocess.run([str(probe), mode, str(source), str(actual)], cwd=ROOT, check=True, timeout=10)
            if mode not in ("reject", "routing-reject") and actual.read_bytes() != bytes(case["bytes"]):
                raise AssertionError("native input " + mode + " differs: " + case["name"])

        for case in current["cases"]:
            check("build", case, case["request"])
            counts["encoders"] += 1
            check("roundtrip", case, case["bytes"])
            counts["roundtrips"] += 1
        for case in current["mappings"]:
            check("mapping", case, case["request"])
            counts["mappings"] += 1
        for case in current["rejected"]:
            check("reject", case, case["bytes"])
            counts["rejections"] += 1
        for case in current.get("accepted", []):
            check("roundtrip", case, case["source"])
            counts["accepted_boundaries"] += 1
        for case in current.get("routing_boundaries", []):
            if case["accepted"]:
                expected = bytearray()
                for value in (case["kind"], case["sequence"], 0, 0, 0, 0, len(case["options"])):
                    uint(expected, value)
                for key, value, origin in case["options"]:
                    uint(expected, key)
                    text(expected, value)
                    uint(expected, origin)
                check("routing-event", {"name": case["name"], "bytes": expected}, case["bytes"])
            else:
                check("routing-reject", case, case["bytes"])
            counts["routing_boundaries"] += 1
        for case in routing["cases"]:
            request = bytearray()
            uint(request, len(case["events"]))
            for event in case["events"]:
                blob(request, event)
            uint(request, len(case["queries"]))
            for query in case["queries"]:
                blob(request, query)
            expected = bytearray((int(case["armed"]), int(case["table"] is not None)))
            if case["table"] is not None:
                text(expected, case["table"])
            expected.append(int(case["repeat"]))
            for name, claim in zip(case["names"], case["claims"]):
                text(expected, name)
                expected.append(int(claim))
            check("routing", {"name": case["name"], "bytes": expected}, request)
            counts["routing_states"] += 1
            counts["routing_claims"] += len(case["queries"])
    report = {"passed": True, "reference": current["reference"], **counts}
    (TARGET / "input-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
