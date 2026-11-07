import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    rust = ROOT / "target/debug/zz_cli"
    if not rust.exists() or not (TARGET / "zz-bend").exists():
        raise SystemExit("build the native backend and Bend gate before palette-live-smoke")
    binary = TARGET / "palette-live-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/palette_live_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    with tempfile.TemporaryDirectory(prefix="zzpalette-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "palette-proof", "sleep 60"], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket], cwd=ROOT, env=environment, check=True, timeout=30)
            report = {"passed": True, "source": "actual private daemon with native interactive stream and compiled Bend gate", "checks": ["live source snapshot projection", "curated split target slot", "quoted structured rename slot", "raw command daemon parsing", "published dynamic alias and slot inference", "structured alias execution", "raw alias daemon parsing", "source window activation", "outputless mutations do not open command output"]}
            (TARGET / "palette-live-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], cwd=ROOT, env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
