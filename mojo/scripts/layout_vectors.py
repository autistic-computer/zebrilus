import argparse
import json
from pathlib import Path
import runpy
import struct
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def append_uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def append_bounds(output, bounds):
    for value in bounds:
        append_uint(output, value)


def append_optional_bounds(output, bounds):
    append_uint(output, int(bounds is not None))
    if bounds is not None:
        append_bounds(output, bounds)


def expected_bytes(case):
    output = bytearray()
    for category in ("normalized", "pixels"):
        append_uint(output, len(case[category]))
        for pane, bounds, insets, floating, modal in case[category]:
            append_uint(output, pane)
            append_bounds(output, bounds)
            append_bounds(output, insets)
            append_uint(output, int(floating))
            append_uint(output, int(modal))
    append_uint(output, len(case["floats"]))
    for float_case in case["floats"]:
        for category in ("clipped", "placed"):
            value = float_case[category]
            append_uint(output, int(value is not None))
            if value is not None:
                append_bounds(output, value[0])
                if category == "clipped":
                    append_optional_bounds(output, value[1])
                else:
                    append_bounds(output, value[1])
        append_bounds(output, float_case["pip"])
    return bytes(output)


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-reference", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = TARGET / "layout-vectors.json"
    if not options.skip_reference:
        result = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "zz-client", "--example", "layout_port_vectors"], cwd=ROOT, capture_output=True, check=True)
        current = json.loads(result.stdout)
        reference.write_text(json.dumps(current, indent=2) + "\n")
    else:
        current = json.loads(reference.read_text())
    probe = TARGET / "layout-probe"
    if not options.skip_build:
        subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/layout_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-layout-") as directory:
        temporary = Path(directory)
        source = temporary / "source.bin"
        arguments = temporary / "arguments.bin"
        actual = temporary / "actual.bin"
        for case in current["cases"]:
            source.write_bytes(bytes(case["frame"]))
            arguments.write_bytes(struct.pack("<6I", *case["args"]))
            subprocess.run([str(probe), str(source), str(arguments), str(actual)], cwd=ROOT, check=True, timeout=30)
            expected = expected_bytes(case)
            if actual.read_bytes() != expected:
                (TARGET / "layout-mismatch-actual.bin").write_bytes(actual.read_bytes())
                (TARGET / "layout-mismatch-expected.bin").write_bytes(expected)
                raise AssertionError("native pane geometry differs: " + case["name"])
    report = {"passed": True, "reference": current["reference"], "comparison": "exact f32 bits, stable pane IDs, insets, visibility and paint order", "cases": len(current["cases"]), "normalized_rects": sum(len(case["normalized"]) for case in current["cases"]), "pixel_rects": sum(len(case["pixels"]) for case in current["cases"]), "float_geometries": sum(len(case["floats"]) for case in current["cases"])}
    (TARGET / "layout-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
