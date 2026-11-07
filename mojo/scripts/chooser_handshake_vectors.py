import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_dialog_vectors import uint, blob


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    reference = TARGET / "chooser-handshake-vectors.json"
    subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-protocol", "--example", "chooser_handshake_vectors", "--", str(reference)], cwd=ROOT, check=True)
    cases = json.loads(reference.read_text())
    output = bytearray()
    uint(output, len(cases))
    for case in cases:
        output.extend((case["search"], case["paths"], case["chooser"], case["origin"] is not None))
        if case["origin"] is not None:
            uint(output, case["origin"])
        blob(output, case["bytes"])
    binary = TARGET / "chooser-handshake-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/chooser_handshake_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-chooser-hello-") as directory:
        source = Path(directory) / "input.bin"
        source.write_bytes(output)
        result = subprocess.run([str(binary), str(source)], cwd=ROOT, capture_output=True, text=True, check=True)
        assert int(result.stdout) == len(cases), result.stdout
    report = {"passed": True, "cases": len(cases), "default_unchanged": 8, "bytes": len(output), "capability": "client-native-chooser-v1", "default": False}
    (TARGET / "chooser-handshake-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
