import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-reference", action="store_true")
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = TARGET / "control-vectors.json"
    if not args.skip_reference:
        encoded = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "zz-protocol", "--example", "control_port_vectors"], cwd=ROOT, capture_output=True, check=True)
        current = json.loads(encoded.stdout)
        reference.write_text(json.dumps(current, indent=2) + "\n")
    else:
        current = json.loads(reference.read_text())
    if current["protocol_version"] != 120:
        raise SystemExit("reference protocol version changed")
    probe = TARGET / "control-probe"
    if not args.skip_build:
        subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "-I", str(PORT), str(PORT / "tests/control_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    counts = {"roundtrips": 0, "outbound": 0, "tree_reductions": 0, "atomic_rejections": 0, "view_reductions": 0, "malformed_frames": 0, "accepted_boundaries": 0}
    outbound = {"hello", "request-full", "tree-sync", "resize-terminal-v2", "client-size-v2", "client-focus", "text-input", "pong"}
    with tempfile.TemporaryDirectory(prefix="zz-control-") as directory:
        temporary = Path(directory)
        source = temporary / "source.bin"
        update = temporary / "update.bin"
        actual = temporary / "actual.bin"

        def invoke(command, *arguments):
            subprocess.run([str(probe), command, *map(str, arguments)], cwd=ROOT, check=True, timeout=30)

        for item in current["messages"]:
            name = item["name"]
            value = item["value"]
            if isinstance(value, dict) and next(iter(value)) in {"Welcome", "Event", "Batch", "Ping", "Pong"}:
                source.write_bytes(bytes(item["bytes"]))
                invoke("roundtrip", source, actual)
                if actual.read_bytes() != source.read_bytes():
                    raise AssertionError("native control roundtrip disagrees: " + name)
                counts["roundtrips"] += 1
            if name in outbound:
                invoke(name, actual)
                if actual.read_bytes() != bytes(item["bytes"]):
                    raise AssertionError("native control encoder disagrees: " + name)
                counts["outbound"] += 1
        for item in current["reductions"]:
            source.write_bytes(bytes(item["before_bytes"]))
            update.write_bytes(bytes(item["delta_bytes"]))
            invoke("reduce", source, update, actual)
            if actual.read_bytes() != bytes(item["after_bytes"]):
                raise AssertionError("native tree reduction disagrees: " + item["name"])
            counts["tree_reductions"] += 1
        for item in current["rejected_reductions"]:
            source.write_bytes(bytes(item["before_bytes"]))
            update.write_bytes(bytes(item["delta_bytes"]))
            invoke("reject", source, update, actual)
            if actual.read_bytes() != bytes(item["after_bytes"]):
                raise AssertionError("native tree rejection changed state: " + item["name"])
            counts["atomic_rejections"] += 1
        for item in current["view_reductions"]:
            source.write_bytes(bytes(item["before_bytes"]))
            update.write_bytes(bytes(item["view_bytes"]))
            invoke("reduce", source, update, actual)
            if actual.read_bytes() != bytes(item["after_bytes"]):
                raise AssertionError("native client view reduction disagrees")
            counts["view_reductions"] += 1
        for item in current.get("accepted_frames", []):
            source.write_bytes(bytes(item["bytes"]))
            invoke("roundtrip", source, actual)
            if actual.read_bytes() != bytes(item["canonical_bytes"]):
                raise AssertionError("native accepted boundary disagrees: " + item["name"])
            counts["accepted_boundaries"] += 1
        for item in current.get("rejected_frames", []):
            rejected_source = temporary / ("rejected-" + item["name"] + ".bin")
            rejected_source.write_bytes(bytes(item["bytes"]))
            invoke("reject-frame", rejected_source)
            counts["malformed_frames"] += 1
    report = {"protocol_version": 120, "reference": "actual zz-protocol encoder/decoder/tree reducer", "passed": True, **counts}
    (TARGET / "control-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
