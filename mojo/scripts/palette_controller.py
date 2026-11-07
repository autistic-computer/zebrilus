import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_places_vectors import fixtures, packet


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    binary = TARGET / "palette-controller-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(PORT / "tests"), str(PORT / "tests/palette_controller_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-palette-controller-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        fleets, _, _ = fixtures()
        source.write_bytes(packet(fleets, []))
        subprocess.run([str(binary), str(source)], check=True)
    report = {"passed": True, "scope": "source-derived palette activation, chip/draft ownership, auto target removal, query clearing, default places/colon entry, exact command/daemon parser routing, offline refusal and input/fleet bounds"}
    (TARGET / "palette-controller.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
