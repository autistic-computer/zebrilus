import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    reference = TARGET / "chooser-scroll-vectors.json"
    subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-ui", "--example", "chooser_scroll_vectors", "--", str(reference)], cwd=ROOT, check=True)
    cases = json.loads(reference.read_text())
    output = bytearray()
    uint(output, len(cases))
    for case in cases:
        kind = case["kind"]
        output.append(kind)
        uint(output, case["before"])
        fields = ("row", "viewport", "count", "selected") if kind == 0 else ("maximum", "line", "x", "y")
        for field in fields:
            uint(output, case[field])
        if kind == 1:
            output.append(case["precise"])
        uint(output, case["after"])
    binary = TARGET / "chooser-scroll-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/chooser_scroll_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-chooser-scroll-") as directory:
        path = Path(directory) / "vectors.bin"
        path.write_bytes(output)
        result = subprocess.run([str(binary), str(path)], cwd=ROOT, capture_output=True, check=True, text=True)
        assert int(result.stdout) == len(cases), result.stdout
    report = {"passed": True, "reference": "actual ala UniformListScrollHandle layout and ScrollWheelEvent dispatch", "cases": len(cases), "center": sum(row["kind"] == 0 for row in cases), "wheel": sum(row["kind"] == 1 for row in cases), "bytes": len(output), "scales": [1, 1.25, 2]}
    (TARGET / "chooser-scroll-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
