import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = subprocess.run(["cargo", "run", "--locked", "-p", "zz-protocol", "--example", "appearance_port_vectors"], cwd=ROOT, capture_output=True, check=True, timeout=180)
    cases = json.loads(reference.stdout)
    probe = TARGET / "appearance-probe"
    subprocess.run(["mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/appearance_probe.mojo"), "-o", str(probe)], check=True, timeout=180)
    accepted = 0
    with tempfile.TemporaryDirectory(prefix="zz-appearance-") as directory:
        source = Path(directory) / "before.bin"
        result = Path(directory) / "after.bin"
        subprocess.run([str(probe), "default", str(source), str(result)], check=True, timeout=15)
        assert result.read_bytes() == bytes(cases[0]["canonical"]), "native default appearance differs"
        for case in cases:
            source.write_bytes(bytes(case["bytes"]))
            subprocess.run([str(probe), "roundtrip" if case["accepted"] else "reject", str(source), str(result)], check=True, timeout=15)
            if case["accepted"]:
                accepted += 1
                assert result.read_bytes() == bytes(case["canonical"]), case["name"]
    report = {"passed": True, "cases": len(cases), "accepted": accepted, "rejected": len(cases)-accepted, "reference": "actual Rust appearance encoder, decoder and validator", "scope": "complete wire fields, raw float bits, defaults, provenance and validation bounds"}
    (TARGET / "appearance-protocol-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
