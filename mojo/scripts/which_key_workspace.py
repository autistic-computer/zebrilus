import argparse
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
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    options = parser.parse_args()
    binary = TARGET / "which-key-workspace-smoke"
    report_path = TARGET / "which-key-workspace.json"
    report_path.unlink(missing_ok=True)
    if not options.skip_build:
        subprocess.run(["pixi", "run", "bash", "-c", 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/which_key_workspace_smoke.mojo" -o "$2"', "_", str(PORT), str(binary)], cwd=MJUI, check=True)
    subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    native = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-which-key-workspace-", dir="/tmp") as directory:
        root = Path(directory)
        (root / "config").mkdir()
        environment["XDG_CONFIG_HOME"] = str(root / "config")
        socket = str(root / "s")
        try:
            subprocess.run([str(native), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "which-key", "-c", str(root), "cat"], cwd=root, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket], cwd=root, env=environment, check=True, timeout=120)
        finally:
            subprocess.run([str(native), "-S", socket, "kill-server"], cwd=root, env=environment, capture_output=True, timeout=10)
    report = {"passed": True, "checks": ["actual private daemon and the default 400 ms delay", "C-b arms the prefix and the short sheet shows after the pause with the all-keys hint", "? while armed shows the complete prefix sheet, is not forwarded and leaves the daemon's prefix armed", "Escape hides the sheet and the daemon clears the table", "? without the prefix reaches the pane"]}
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
