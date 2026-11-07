import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--ui", action="store_true")
    options = parser.parse_args()
    binary = ROOT / ("target/port/history-workspace-smoke" if options.ui else "target/port/terminal-history-smoke")
    if options.ui:
        subprocess.run(["bash", "mojo/scripts/ui.sh", "build-history-smoke"], cwd=ROOT, check=True)
    else:
        subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"),
                        "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"),
                        str(ROOT / "mojo/tests/terminal_history_smoke.mojo"), "-o", str(binary)],
                       cwd=ROOT, check=True)
    cli = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-history-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            command = "i=0; while [ $i -lt 640 ]; do printf 'HISTORY-%04d\\n' $i; i=$((i+1)); done; printf 'LIVE-END\\n'; IFS= read -r line; sleep 30"
            subprocess.run([str(cli), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s",
                            "history-proof", "-x", "80", "-y", "24", command], check=True, timeout=20)
            command = [str(binary), socket]
            if options.ui:
                command.append(str(ROOT / "target/port/history-workspace.png"))
            environment = os.environ.copy()
            environment["ZZ_BEND"] = str(ROOT / "target/port/zz-bend")
            subprocess.run(command, cwd=ROOT.parent / "mjui" if options.ui else ROOT,
                           env=environment, check=True, timeout=90 if options.ui else 45)
            report = {"passed": True, "source": "actual native workspace and private daemon history" if options.ui else "actual client history port and private daemon", "checks": ["source history payloads", "bounded shared cache", "stable row identities", "fractional local scrolling", "server convergence"]}
            if options.ui:
                report["checks"] += ["terminal focus retained", "120ms debounce", "selection flush", "typed input cancellation", "sidebar focus preserved", "reconnect terminal focus"]
                report["image"] = "target/port/history-workspace.png"
            name = "history-workspace-smoke.json" if options.ui else "terminal-history-smoke.json"
            (ROOT / "target/port" / name).write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(cli), "-S", socket, "kill-server"], stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=10)


if __name__ == "__main__":
    main()
