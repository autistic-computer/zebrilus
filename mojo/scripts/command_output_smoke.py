import json
import argparse
import os
from pathlib import Path
import runpy
import shlex
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    if not options.skip_build:
        subprocess.run(["bash", "mojo/scripts/ui.sh", "build-output-smoke"], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    rust = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zzoutput-", dir="/tmp") as directory:
        temporary = Path(directory)
        socket = str(temporary / "s")
        recorder = temporary / "recorder.py"
        recorder.write_text(
            "import os, sys, tty\n"
            "tty.setraw(0)\n"
            "os.write(1, b'UNDERLYING_PANE_READY\\r\\n')\n"
            "with open(sys.argv[1], 'ab', buffering=0) as output:\n"
            "    while True:\n"
            "        data = os.read(0, 4096)\n"
            "        if not data:\n"
            "            break\n"
            "        output.write(data)\n"
        )
        captures = [temporary / "first.bin", temporary / "second.bin"]
        for path in captures:
            path.write_bytes(b"")
        commands = ["python3 " + shlex.quote(str(recorder)) + " " + shlex.quote(str(path)) for path in captures]
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "output-proof", commands[0]], cwd=ROOT, env=environment, capture_output=True, check=True, timeout=15)
            subprocess.run([str(rust), "-S", socket, "split-window", "-h", commands[1]], cwd=ROOT, env=environment, capture_output=True, check=True, timeout=15)
            subprocess.run([str(TARGET / "command-output-smoke"), socket, str(TARGET / "command-output-workspace.png")], cwd=ROOT.parent / "mjui", env=environment, check=True, timeout=90)
            actual = [path.read_bytes() for path in captures]
            if actual != [b"", b"q"]:
                raise AssertionError("command-output input leaked or parked text was lost: " + repr(actual))
            report = {"passed": True, "source": "actual native workspace and private daemon command-output terminal", "checks": ["output replaces only owner pane", "output focus does not select daemon pane", "native global output resize", "output pointer surface remap", "focus survives deactivation", "normal pane parks output key table", "uncomposed multi-scalar commit uses Paste", "composed commit uses Text", "native copy-mode dismissal", "new output id recreates presentation", "physical Ctrl-c and captured release", "underlying PTYs receive no output input"], "pty_hex": [data.hex() for data in actual]}
            (TARGET / "command-output-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], cwd=ROOT, env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
