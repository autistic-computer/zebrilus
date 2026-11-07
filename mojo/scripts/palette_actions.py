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
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-protocol", "--example", "palette_action_vectors"], cwd=ROOT, check=True)
    cases = json.loads(subprocess.check_output([str(ROOT / "target/debug/examples/palette_action_vectors")]))
    binary = TARGET / "palette-action-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/palette_action_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    payload = bytearray()
    expected = bytearray()
    uint(payload, len(cases))
    uint(expected, len(cases))
    for case in cases:
        uint(payload, case["kind"])
        uint(payload, case.get("request", 0))
        uint(payload, len(case["frame"]))
        payload.extend(case["frame"])
        uint(expected, len(case["frame"]))
        expected.extend(case["frame"])
    with tempfile.TemporaryDirectory(prefix="zz-palette-actions-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        target = Path(directory) / "out"
        source.write_bytes(payload)
        subprocess.run([str(binary), str(source), str(target)], check=True)
        assert target.read_bytes() == expected
    report = {"passed": True, "cases": len(cases), "bytes": len(expected), "source": "actual native CommandRequest and CancelPrefix/CommandPrompt::Close/ChooseTree::Close", "scope": "full invocation raw args/source/blocks/expanded alias/stdin metadata, request boundaries and exact native palette open controls"}
    (TARGET / "palette-action-vectors.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
