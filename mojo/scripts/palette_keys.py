import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_places_vectors import uint


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-client", "--example", "palette_key_vectors"], cwd=ROOT, check=True)
    cases = json.loads(subprocess.check_output([str(ROOT / "target/debug/examples/palette_key_vectors")]))
    binary = TARGET / "palette-key-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/palette_key_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    payload = bytearray()
    uint(payload, len(cases))
    for case in cases:
        for field in ("apple", "overlay", "capture", "opened"):
            uint(payload, case[field])
        uint(payload, len(case["frame"]))
        payload.extend(case["frame"])
    with tempfile.TemporaryDirectory(prefix="zz-palette-keys-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        source.write_bytes(payload)
        subprocess.run([str(binary), str(source)], check=True)
    report = {"passed": True, "cases": len(cases), "bytes": len(payload), "source": "actual InputRouter with Desktop/DesktopApple ChromeKeymap", "scope": "default palette open keys, prefix precedence, overlay/capture ownership, all16modifier combinations, press/repeat/release, native text fallback and Unicode/uppercase boundaries"}
    (TARGET / "palette-key-vectors.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
