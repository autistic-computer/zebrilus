import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
MJUI = ROOT.parent / "mjui"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    binary = TARGET / "keyboard-smoke"
    rust = ROOT / "target/debug/zz_cli"
    if not rust.exists():
        raise SystemExit("build the Rust reference first: cargo build -p zz-cli")
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(MJUI / "src"), str(PORT / "tests/keyboard_smoke.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    recorder = "stty -echo; printf 'KEYBOARD_READY\\n'; IFS= read -r line; printf 'KEYBOARD_RESULT:%s:' \"${#line}\"; printf '%s' \"$line\" | od -An -tx1 | tr -d ' \\n'; printf '\\n'; sleep 60"
    with tempfile.TemporaryDirectory(prefix="zzkeyboard-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "keyboard-proof", recorder], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(rust), "-S", socket, "set-option", "-g", "default-command", recorder], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket], cwd=ROOT, env=environment, check=True, timeout=30)
            report = {"passed": True, "source": "native Input::Key through actual daemon key tables and two private PTYs", "checks": ["physical Ctrl-b arms prefix", "physical Shift-5 splits through native Key", "captured old-pane releases do not change selection", "associated committed text consumed once", "both PTY input buffers contain zero duplicate bytes"]}
            (TARGET / "keyboard-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], cwd=ROOT, env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
