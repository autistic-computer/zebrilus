import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
MJUI = ROOT.parent / "mjui"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    binary = TARGET / "palette-backend-probe"
    subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src -I "$1" $MJ_LINK "$1/tests/palette_backend_probe.mojo" -o "$2"', "_", str(PORT), str(binary)], cwd=MJUI, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    native = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zzpalette-worker-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            subprocess.run([str(native), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "palette-worker-proof", "sleep 60"], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket], cwd=ROOT, env=environment, check=True, timeout=30)
        finally:
            subprocess.run([str(native), "-S", socket, "kill-server"], cwd=ROOT, env=environment, capture_output=True, timeout=10)
    report = {"passed": True, "source": "actual private daemon through asynchronous native backend and compiled Bend gate", "checks": ["typed split and quoted Unicode rename", "daemon-context raw command parsing", "captured source identity rejects stale RPC", "command error preserves connection", "native prefix cancellation", "native prompt and chooser closure", "outputless mutations do not create command output", "detach stays unattached across repeated refreshes", "explicit session selection reattaches on the same connection"]}
    (TARGET / "palette-backend.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
