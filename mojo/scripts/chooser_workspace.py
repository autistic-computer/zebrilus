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
    binary = TARGET / "chooser-workspace-smoke"
    report_path = TARGET / "chooser-workspace.json"
    report_path.unlink(missing_ok=True)
    if not options.skip_build:
        subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src -I "$1" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/chooser_workspace_smoke.mojo" -o "$2"', "_", str(PORT), str(binary)], cwd=MJUI, check=True)
    subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    native = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-chooser-workspace-", dir="/tmp") as directory:
        root = Path(directory)
        for name in ("pane0", "pane1", "reads"):
            (root / name).write_bytes(b"")
        recorder = root / "recorder.py"
        recorder.write_text("import os,sys,tty\ntty.setraw(0)\nos.write(1,b'CHOOSER_READY\\r\\n')\nwith open(sys.argv[1],'ab',buffering=0) as out:\n while True:\n  value=os.read(0,4096)\n  if not value: break\n  out.write(value)\n")
        socket = str(root / "s")
        first = "python3 " + shlex.quote(str(recorder)) + " " + shlex.quote(str(root / "pane0"))
        second = "python3 " + shlex.quote(str(recorder)) + " " + shlex.quote(str(root / "pane1"))
        try:
            subprocess.run([str(native), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "chooser-workspace", "-c", str(root), first], cwd=root, env=environment, capture_output=True, check=True, timeout=15)
            subprocess.run([str(native), "-S", socket, "split-window", "-h", "-t", "%0", second], cwd=root, env=environment, capture_output=True, check=True, timeout=15)
            subprocess.run([str(binary), socket, str(root), str(TARGET / "chooser-workspace.png")], cwd=root, env=environment, check=True, timeout=90)
        finally:
            subprocess.run([str(native), "-S", socket, "kill-server"], cwd=root, env=environment, capture_output=True, timeout=10)
    report = {"passed": True, "checks": ["actual private daemon and Bend gate", "Panes and Clients native modal lifecycle", "full typed presentation retention", "key+text search-start suppression", "implicit full-range UTF16 composition and source revision reset", "composed multi-scalar/newline SearchAppend", "inactive search commit ignored", "atomic oversized preedit and commit preserve scope/query/composition", "PRIMARY and clipboard commands occluded before isolated host reads", "ordered row Select before physical Down", "key-up does not repeat navigation", "double-click pane activation and focus restoration", "close button fires on matching UP", "zero-size modal-origin IME anchor", "complete source Scene replay"], "scope": "experimental explicit constructor opt-in only; default chooser capability remains false until Windows and Buffer frontends are ready"}
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
