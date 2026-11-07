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
    reference = subprocess.run(["cargo", "run", "--locked", "-p", "zz-protocol", "--example", "exec_vectors"], cwd=ROOT, capture_output=True, check=True, timeout=180)
    cases = json.loads(reference.stdout)
    probe = TARGET / "exec-probe"
    subprocess.run(["mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/exec_probe.mojo"), "-o", str(probe)], check=True, timeout=180)
    accepted = 0
    with tempfile.TemporaryDirectory(prefix="zz-exec-codec-") as directory:
        source = Path(directory) / "before.bin"
        result = Path(directory) / "after.bin"
        words = Path(directory) / "words.bin"
        for case in cases:
            source.write_bytes(bytes(case["bytes"]))
            extra = []
            if case["kind"] == "words":
                words.write_bytes(bytes(case["words"]))
                extra.append(str(words))
            subprocess.run([str(probe), case["kind"], str(source), str(result), "accept" if case["accepted"] else "reject", *extra], check=True, timeout=15)
            if case["accepted"]:
                accepted += 1
                assert result.read_bytes() == bytes(case["canonical"]), case
    report = {"passed": True, "cases": len(cases), "accepted": accepted, "rejected": len(cases)-accepted, "reference": "actual Rust execution request/exit codecs and command-word splitter", "scope": "all flags/optional facts, raw command/stdin/environment bytes, prepared/resume metadata, errors, bounds, truncation and trailing extensions"}
    (TARGET / "exec-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
