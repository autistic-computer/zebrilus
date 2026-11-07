import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
MJUI = ROOT.parent / "mjui"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    binary = TARGET / "palette-files-probe"
    subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src -I "$1" $MJ_LINK "$1/tests/palette_files_probe.mojo" -o "$2"', "_", str(PORT), str(binary)], cwd=MJUI, check=True)
    subprocess.run([str(binary)], check=True, timeout=30)
    report = {"passed": True, "checks": ["persistent actual Dispatch rank worker", "typed root/entry metadata", "native directory navigation and copy/insert effects", "pending retirement preserves cached file rows", "request identifiers survive close"], "scope": "focused controller/worker check; live UI adapter remains separate"}
    (TARGET / "palette-files-worker.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
