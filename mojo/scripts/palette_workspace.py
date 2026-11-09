import json
import argparse
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
    parser.add_argument("--sdk-source", type=Path)
    parser.add_argument("--native-scene", action="store_true")
    options = parser.parse_args()
    binary = TARGET / "palette-workspace-smoke"
    if not options.skip_build:
        subprocess.run(["pixi", "run", "bash", "-c", 'mojo build -j 1 --fp-mode=contract=off -I "$3" -I "$1" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/palette_workspace_smoke.mojo" -o "$2"', "_", str(PORT), str(binary), str(options.sdk_source or MJUI / "src")], cwd=MJUI, check=True)
    subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    if options.native_scene:
        environment["ZZ_PALETTE_NATIVE_SCENE"] = "1"
    native = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-palette-workspace-", dir="/tmp") as directory:
        temporary = Path(directory)
        socket = str(temporary / "s")
        captured = temporary / "input"
        captured.write_bytes(b"")
        recorder = temporary / "recorder.py"
        recorder.write_text("import os,sys,tty\ntty.setraw(0)\nos.write(1,b'PALETTE_READY\\r\\n')\nwith open(sys.argv[1],'ab',buffering=0) as out:\n while True:\n  data=os.read(0,4096)\n  if not data: break\n  out.write(data)\n")
        command = "python3 " + shlex.quote(str(recorder)) + " " + shlex.quote(str(captured))
        try:
            subprocess.run([str(native), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "palette-workspace-proof", command], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket, str(TARGET / "palette-workspace.png")], cwd=temporary, env=environment, check=True, timeout=90)
            actual = captured.read_bytes()
            if actual != b"abTAILcd\re":
                raise AssertionError("private palette PTY order differs: " + repr(actual))
        finally:
            subprocess.run([str(native), "-S", socket, "kill-server"], cwd=ROOT, env=environment, capture_output=True, timeout=10)
    report = {"passed": True, "host": "actual WorkspaceView/input timeline/private daemon/PTY and isolated bounded clipboard adapter", "checks": ["physical Ctrl+Shift+P consumes associated text once", "earlier terminal input precedes palette activation", "post-close text/paste/text exact PTY order", "Escape then text and physical Enter in one batch", "native query and composed commit", "Shift+Arrow navigates rows", "oversized PRIMARY preserves selection", "6 MiB paste refused before shaping", "quoted Unicode rename and daemon-context if-shell", "native split with no command output modal", "row activation precedes same-frame terminal suffix", "source-kit palette overlay and terminal focus restoration", "active output focus survives palette dismissal", "place activation chooses its new input owner", "parked output stays parked after palette dismissal", "native terminal search remains functional"], "pty_hex": b"abTAILcd\re".hex()}
    (TARGET / "palette-workspace.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
