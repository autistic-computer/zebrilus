import json
from pathlib import Path
import runpy
import struct
import subprocess


PORT = Path(__file__).resolve().parents[1]
ROOT = PORT.parent
MJUI = ROOT.parent / "mjui"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    fixture = TARGET / "terminal-color-vectors.bin"
    with fixture.open("wb") as output:
        subprocess.run(["cargo", "run", "--locked", "-p", "zz-client", "--example", "terminal_color_vectors"], cwd=ROOT, stdout=output, check=True)
    binary = TARGET / "terminal-color-probe"
    subprocess.run(["mojo", "build", "--fp-mode=contract=off", "-I", str(MJUI / "src"), str(PORT / "tests/terminal_color_probe.mojo"), "-o", str(binary)], check=True)
    subprocess.run([str(binary), str(fixture)], check=True)
    with fixture.open("rb") as source:
        magic, faint, resolutions, scalars = struct.unpack("<4sIII", source.read(16))
    if magic != b"ZCLR":
        raise ValueError("invalid terminal color oracle")
    (TARGET / "terminal-color-differential.json").write_text(json.dumps({"reference": "actual zz-client terminal_color", "passed": True, "faint_pairs": faint, "resolutions": resolutions, "unicode_scalars": scalars}, indent=2) + "\n")


if __name__ == "__main__":
    main()
