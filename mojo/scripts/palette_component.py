import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_places_vectors import fixtures, packet


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
MJUI = ROOT.parent / "mjui"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    retained = TARGET / "retained-input-probe"
    subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src $MJ_LINK tests/retained_input_probe.mojo -o "$1"', "_", str(retained)], cwd=MJUI, check=True)
    subprocess.run([str(retained)], cwd=MJUI, check=True, timeout=20)
    binary = TARGET / "palette-component-probe"
    subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src -I "$1" -I "$1/tests" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/tests/palette_component_probe.mojo" -o "$2"', "_", str(PORT), str(binary)], cwd=MJUI, check=True)
    subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-palette-component-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        fleets, _, _ = fixtures()
        source.write_bytes(packet(fleets, []))
        subprocess.run([str(binary), str(source), str(TARGET / "palette-component.png")], cwd=MJUI, check=True, timeout=30)
    report = {"passed": True, "checks": ["explicit retained layer input scope and previous-frame pointer", "closed, closing, mismatched and outside-modal owner rejection", "first-open focus and scope/frame reset", "ordered interleaved text and paste", "Unicode and newline cleaning", "IME composition and commit", "typed place chips and backtracking", "daemon-context raw command action", "post-close text remains unconsumed", "exact input replacement boundary", "oversized paste and composition atomic refusal", "actual retained source-kit rendering, focus, list navigation and placement", "pointer selection precedes same-frame paste", "outside-click dismissal restores prior focus without activating background"], "input_bytes": 65536, "undo_bytes": 1048576}
    (TARGET / "palette-component.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
