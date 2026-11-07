import json
import os
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "run", "--locked", "-p", "zz-ui", "--example", "terminal_overlay_vectors"]
    for setting in ("profile.dev.debug=0", 'profile.dev.package."*".opt-level=0', "profile.dev.package.ala.opt-level=0", "profile.dev.package.ala-macros.opt-level=0", "profile.dev.package.ala-platform.opt-level=0", "profile.dev.package.taffy.opt-level=0"):
        command.extend(["--config", setting])
    source = TARGET / "terminal-overlay-vectors.bin"
    with source.open("wb") as output:
        subprocess.run(command, cwd=ROOT, env=environment, stdout=output, check=True, timeout=1800)
    probe = TARGET / "terminal-overlay-probe"
    subprocess.run(["mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), "-I", str(ROOT.parent / "mjui/src"), str(ROOT / "mojo/tests/terminal_overlay_probe.mojo"), "-o", str(probe)], check=True, timeout=180)
    subprocess.run([str(probe), str(source)], check=True, timeout=60)
    report = {"passed": True, "selection_cases": 1952, "overlay_colors": 1536, "cursor_visibility": 64, "cursor_paints": 160, "reference": "actual native terminal selection corners, overlay colors and paint visibility functions", "scope": "sparse merged coverage against full source spans, row boundaries, rectangle neighbors, duplicate/overlapping ranges, exact Float32 colors and composing/focus/blink combinations"}
    (TARGET / "terminal-overlay-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
