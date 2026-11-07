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
    reference = subprocess.run(["cargo", "run", "--locked", "-p", "zz-protocol", "--example", "client_file_vectors"], cwd=ROOT, capture_output=True, check=True, timeout=180)
    cases = json.loads(reference.stdout)
    probe = TARGET / "client-file-probe"
    subprocess.run(["mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/client_file_probe.mojo"), "-o", str(probe)], check=True, timeout=180)
    accepted = 0
    large_cases = 0
    with tempfile.TemporaryDirectory(prefix="zz-file-codec-") as directory:
        source = Path(directory) / "before.bin"
        result = Path(directory) / "after.bin"
        for case in cases:
            source.write_bytes(bytes(case["bytes"]))
            expected = [case["error"], str(case["code"])] if case["kind"] == "cli-error" else []
            subprocess.run([str(probe), case["kind"], str(source), str(result), "accept" if case["accepted"] else "reject", *expected], check=True, timeout=15)
            if case["accepted"]:
                accepted += 1
                assert result.read_bytes() == bytes(case["canonical"]), case
        large = subprocess.run([str(ROOT / "target/debug/examples/client_file_vectors"), "--large", directory], capture_output=True, check=True, timeout=60)
        for case in json.loads(large.stdout):
            source = Path(directory) / case["path"]
            subprocess.run([str(probe), case["kind"], str(source), str(result), "accept" if case["accepted"] else "reject"], check=True, timeout=30)
            if case["accepted"]:
                accepted += 1
                assert result.read_bytes() == source.read_bytes(), case
            large_cases += 1
    total = len(cases) + large_cases
    report = {"passed": True, "cases": total, "accepted": accepted, "rejected": total-accepted, "reference": "actual Rust client-file request/response codecs", "scope": "all operations, raw binary data, errors, paths, 32MiB data boundaries, invalid UTF-8/enums/booleans and control extensions"}
    (TARGET / "client-file-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
