import json
import os
from pathlib import Path
import runpy
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / "mjui"
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    if sys.platform != "linux":
        raise SystemExit("run the native AppKit clipboard assertions on this host")
    TARGET.mkdir(parents=True, exist_ok=True)
    binary = TARGET / "clipboard-host-smoke"
    subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src -I "$1" $MJ_LINK -D MJ_SDL=1 -Xlinker "$2/build/lib/libSDL3.a" -Xlinker -lm -Xlinker -ldl "$1/clipboard_host_smoke.mojo" -o "$3"', "_", str(PORT), str(MJUI), str(binary)], cwd=MJUI, check=True)
    environment = os.environ.copy()
    environment["SDL_VIDEODRIVER"] = "dummy"
    environment["SDL_AUDIODRIVER"] = "dummy"
    subprocess.run([str(binary)], cwd=MJUI, env=environment, check=True, timeout=20)
    report = {"passed": True, "host": "isolated SDL dummy video driver", "checks": ["binary-safe UTF-8 text", "bounded reads preserve ownership", "server PRIMARY mirror", "application PRIMARY remains distinct", "set-clipboard off", "query reply retains client identity", "PNG ownership and size limits", "replacement retires image offer"]}
    (TARGET / "clipboard-host-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
