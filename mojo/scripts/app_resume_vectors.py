import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
ORACLE = ROOT / "target/debug/examples/app_resume_vectors"
PROBE = TARGET / "app-resume-probe"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-protocol", "--example", "app_resume_vectors"], cwd=ROOT, check=True)
    rows = json.loads(subprocess.check_output([str(ORACLE)], cwd=ROOT))
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/app_resume_probe.mojo"), "-o", str(PROBE)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-resume-codec-", dir="/tmp") as directory:
        directory = Path(directory)
        for index, row in enumerate(rows):
            source = directory / "exit"
            target = directory / "hello"
            source.write_bytes(bytes(row["exit"]))
            subprocess.run([str(PROBE), "codec", str(source), "1" if row["search"] else "0", str(target)], check=True)
            actual = target.read_bytes()
            expected = bytes(row["hello"])
            assert actual == expected, (index, actual.hex(), expected.hex())
    result = {"passed": True, "cases": len(rows), "reference": "actual protocol encoder", "scope": "prepared AttachOperation::Commands, source spans, typed blocks, raw bytes, canonical and alias metadata, ready/error variants, native-search capability"}
    (TARGET / "app-resume-vectors.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
