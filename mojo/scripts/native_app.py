import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    rust = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-native-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "printf 'native renderer\\n'; sleep 60"], env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(TARGET / "zz-mojo-app"), "--socket", socket, "--frames", "6"], cwd=ROOT.parent / "mjui", env=environment, check=True, timeout=30)
            report = {"passed": True, "host": "native mjui application", "frames": 6, "scope": "window, renderer, asynchronous native stream and clean finite shutdown"}
            (TARGET / "native-app-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
