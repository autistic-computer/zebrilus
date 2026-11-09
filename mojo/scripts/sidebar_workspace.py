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
    binary = TARGET / "sidebar-workspace-smoke"
    report_path = TARGET / "sidebar-workspace.json"
    report_path.unlink(missing_ok=True)
    if not options.skip_build:
        subprocess.run(["pixi", "run", "bash", "-c", 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/sidebar_workspace_smoke.mojo" -o "$2"', "_", str(PORT), str(binary)], cwd=MJUI, check=True)
    subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    native = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-sidebar-workspace-", dir="/tmp") as directory:
        root = Path(directory)
        socket = str(root / "s")

        def zz(*arguments):
            subprocess.run([str(native), "-S", socket, *arguments], cwd=root, env=environment, check=True, capture_output=True, timeout=15)

        try:
            zz("-f", "/dev/null", "new-session", "-d", "-s", "alpha", "-n", "first", "-c", str(root), "cat")
            zz("new-window", "-d", "-t", "alpha", "-n", "second", "cat")
            zz("new-session", "-d", "-s", "beta", "-n", "third", "-c", str(root), "cat")
            subprocess.run([str(binary), socket, str(TARGET / "sidebar-workspace.png")], cwd=root, env=environment, check=True, timeout=120)
        finally:
            subprocess.run([str(native), "-S", socket, "kill-server"], cwd=root, env=environment, capture_output=True, timeout=10)
    report = {"passed": True, "checks": ["actual private daemon with two sessions and three windows", "sidebar rows from the fleet controller: host, sessions in source order", "Ctrl+1, Ctrl+2 and Ctrl+3 select windows across sessions through staged batches, releases swallowed", "session and window row clicks attach and select through the daemon", "New window, Split right, Split down and New session toolbar buttons send the staged command batches", "picker pane drawn for a picker split, t turns it into a terminal, Escape closes it"]}
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
