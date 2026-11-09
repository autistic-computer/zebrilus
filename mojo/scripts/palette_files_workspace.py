import argparse
import json
import os
from pathlib import Path
import runpy
import shlex
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
    binary = TARGET / "palette-files-workspace-smoke"
    if not options.skip_build:
        subprocess.run(["pixi", "run", "bash", "-c", 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/palette_files_workspace_smoke.mojo" -o "$2"', "_", str(PORT), str(binary)], cwd=MJUI, check=True)
    subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    native = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-files-bridge-", dir="/tmp") as directory:
        root = Path(directory)
        files = root / "files"
        files.mkdir()
        (files / "alpha").write_text("alpha\n")
        (files / "dir").mkdir()
        (files / "dir/nested").write_text("nested\n")
        (files / "λ file").write_text("unicode\n")
        (files / "link").symlink_to("alpha")
        captured = root / "input"
        captured.write_bytes(b"")
        recorder = root / "recorder.py"
        recorder.write_text("import os,sys,tty\ntty.setraw(0)\nos.write(1,b'\\x1b[?2004hFILES_READY\\r\\n')\nwith open(sys.argv[1],'ab',buffering=0) as out:\n while True:\n  value=os.read(0,4096)\n  if not value: break\n  out.write(value)\n")
        socket = str(root / "s")
        command = "python3 " + shlex.quote(str(recorder)) + " " + shlex.quote(str(captured))
        try:
            subprocess.run([str(native), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "files-bridge", "-c", str(files), command], cwd=files, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(native), "-S", socket, "split-window", "-h", "-t", "%0", "sleep 120"], cwd=files, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket, str(files), str(TARGET / "palette-files-workspace.png")], cwd=files, env=environment, check=True, timeout=60)
            actual = captured.read_bytes()
            expected = b"\x1b[200~alpha \x1b[201~"
            if actual != expected:
                raise AssertionError("captured-pane paste differs: " + repr(actual))
        finally:
            subprocess.run([str(native), "-S", socket, "kill-server"], cwd=files, env=environment, capture_output=True, timeout=10)
    report = {"passed": True, "checks": ["actual private daemon and compiled Bend gate", "source listing and immutable chunk handoff", "real background matcher", "source root insertion metadata", "native bracketed paste to captured pane", "active pane remains unchanged", "request IDs survive close", "real WorkspaceView and ordered input timeline", "native choose-path opens captured pane", "directory Tab/ShiftTab and pointer actions", "Alt+Enter copy stays open and canonical toast survives closure", "Linux source Scene frame completes"], "pty_hex": expected.hex()}
    (TARGET / "palette-files-workspace.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
