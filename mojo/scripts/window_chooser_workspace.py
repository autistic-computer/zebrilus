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
    binary = TARGET / "window-chooser-workspace-smoke"
    report_path = TARGET / "window-chooser-workspace.json"
    report_path.unlink(missing_ok=True)
    if not options.skip_build:
        subprocess.run(["pixi", "run", "bash", "-c", 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/window_chooser_workspace_smoke.mojo" -o "$2"', "_", str(PORT), str(binary)], cwd=MJUI, check=True)
    subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    native = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-window-chooser-workspace-", dir="/tmp") as directory:
        root = Path(directory)
        (root / "config").mkdir()
        environment["XDG_CONFIG_HOME"] = str(root / "config")
        socket = str(root / "s")
        try:
            for arguments in (["-f", "/dev/null", "new-session", "-d", "-s", "dev", "-n", "first", "-c", str(root), "cat"], ["new-window", "-d", "-t", "dev", "-n", "second", "cat"], ["new-window", "-d", "-t", "dev", "-n", "third", "cat"], ["split-window", "-d", "-t", "dev:third", "cat"]):
                subprocess.run([str(native), "-S", socket, *arguments], cwd=root, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket], cwd=root, env=environment, check=True, timeout=120)
        finally:
            subprocess.run([str(native), "-S", socket, "kill-server"], cwd=root, env=environment, capture_output=True, timeout=10)
    report = {"passed": True, "checks": ["actual private daemon with three windows, one split", "choose-tree -w opens the Windows chooser with native_chooser on and takes field focus", "a disclosure click selects the row and expands it to its two panes", "typed text and a middle-click PRIMARY paste at the end of the field build the query second", "Enter selects window second, closes the chooser and returns focus to the terminal"]}
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
