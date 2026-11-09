import json
import os
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
BASE = runpy.run_path(str(Path(__file__).with_name("window_chooser_vectors.py")))


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "build", "--locked", "--offline", "-p", "zz-ui", "--example", "chooser_scroll_vectors"]
    for setting in BASE["SETTINGS"]:
        command.extend(["--config", setting])
    subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=600)
    reference = TARGET / "window-chooser-scroll-vectors.json"
    subprocess.run([str(ROOT / "target/penna-native/debug/examples/chooser_scroll_vectors"), str(reference), "--window"], cwd=ROOT, check=True, timeout=120)
    cases = json.loads(reference.read_text())
    output = bytearray()
    BASE["uint"](output, len(cases))
    for case in cases:
        kind = case["kind"]
        output.append(kind)
        BASE["uint"](output, case["before"])
        for field in (("maximum", "line", "x", "y") if kind == 1 else ("row", "viewport", "count", "selected")):
            BASE["uint"](output, case[field])
        if kind == 1:
            output.append(case["precise"])
        BASE["uint"](output, case["after"])
    request = TARGET / "window-chooser-scroll-input.bin"
    request.write_bytes(output)
    binary = TARGET / "window-chooser-scroll-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/window_chooser_scroll_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request)], cwd=ROOT, check=True, timeout=60)
    report = {"passed": True, "reference": "actual UniformList layout/non-strict Top and Nearest/ScrollWheelEvent dispatch", "cases": len(cases), "top": sum(row["kind"] == 2 for row in cases), "nearest": sum(row["kind"] == 3 for row in cases), "wheel": sum(row["kind"] == 1 for row in cases), "bytes": len(output), "scales": [1, 1.25, 2]}
    (TARGET / "window-chooser-scroll-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
