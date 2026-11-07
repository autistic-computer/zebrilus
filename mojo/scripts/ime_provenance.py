import json
import os
from pathlib import Path
import runpy
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / "mjui"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    if sys.platform != "linux":
        raise SystemExit("run the native mjui test_textinput suite on this host")
    TARGET.mkdir(parents=True, exist_ok=True)
    binary = TARGET / "ime-provenance"
    subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src $MJ_LINK -D MJ_SDL=1 -Xlinker "$1/build/lib/libSDL3.a" -Xlinker -lm -Xlinker -ldl "$1/tests/ime_provenance_probe.mojo" -o "$2"', "_", str(MJUI), str(binary)], cwd=MJUI, check=True)
    environment = os.environ.copy()
    environment["SDL_VIDEODRIVER"] = "dummy"
    subprocess.run([str(binary)], cwd=MJUI, env=environment, check=True, timeout=20)
    report = {"passed": True, "host": "actual SDL text editing and text input events", "checks": ["same-frame compose+commit retains provenance", "ordinary multi-scalar/newline insert remains uncomposed", "take retains provenance after marked text clears"]}
    (TARGET / "ime-provenance.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
