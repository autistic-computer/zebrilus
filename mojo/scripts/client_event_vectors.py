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


def optional(output, value):
    output.append(int(value is not None))
    if value is not None:
        uint(output, value)


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-reference", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    references = {}
    for name, package in (("client_event_vectors", "zz-protocol"), ("client_event_reducer_vectors", "zz-client")):
        path = TARGET / (name.replace("_", "-") + ".json")
        if not options.skip_reference:
            result = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", package, "--example", name], cwd=ROOT, capture_output=True)
            if result.returncode:
                raise SystemExit(result.stderr.decode())
            references[name] = json.loads(result.stdout)
            path.write_text(json.dumps(references[name], indent=2) + "\n")
        else:
            references[name] = json.loads(path.read_text())
    probe = TARGET / "client-event-probe"
    if not options.skip_build:
        subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/client_event_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    current = references["client_event_vectors"]
    reducers = references["client_event_reducer_vectors"]
    counts = {"roundtrips": 0, "rejections": 0, "styles": len(current["styles"]), "output_sequences": 0, "output_steps": 0}
    with tempfile.TemporaryDirectory(prefix="zz-client-event-") as directory:
        temporary = Path(directory)
        source = temporary / "source.bin"
        actual = temporary / "actual.bin"

        def check(mode, name, value, expected=None):
            source.write_bytes(bytes(value))
            subprocess.run([str(probe), mode, str(source), str(actual)], cwd=ROOT, check=True, timeout=20)
            if expected is not None and actual.read_bytes() != bytes(expected):
                raise AssertionError("native client event " + mode + " differs: " + name)

        for case in current["cases"]:
            check("roundtrip", case["name"], case["bytes"], case["bytes"])
            counts["roundtrips"] += 1
        for case in current["rejections"]:
            check("reject", case["name"], case["bytes"])
            counts["rejections"] += 1
        request = bytearray()
        uint(request, len(current["styles"]))
        for style in current["styles"]:
            blob(request, style["value"].encode())
        check("styles", "source style parser corpus", request, [int(style["valid"]) for style in current["styles"]])
        for case in reducers["cases"]:
            request = bytearray()
            expected = bytearray()
            uint(request, len(case["steps"]))
            for step in case["steps"]:
                blob(request, step["bytes"])
                for key in ("id", "pane", "generation", "view_generation"):
                    optional(expected, step[key])
                uint(expected, step["revision"])
                uint(expected, step["watermark"])
            check("reduce", case["name"], request, expected)
            counts["output_sequences"] += 1
            counts["output_steps"] += len(case["steps"])
    report = {"passed": True, "reference": current["reference"], "reducer": reducers["reference"], **counts}
    (TARGET / "client-event-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
