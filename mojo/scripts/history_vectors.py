import argparse
from collections import Counter
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
KINDS = ["pixel", "follow", "fraction", "prefetch", "local", "pacer", "history", "wire", "range", "budget"]


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-reference", action="store_true")
    parser.add_argument("--kinds", nargs="+", choices=KINDS)
    args = parser.parse_args()
    reference = TARGET / "terminal-history-vectors.json"
    if not args.skip_reference:
        with reference.open("w") as output:
            subprocess.run(["cargo", "run", "--locked", "-p", "zz-client", "--example",
                            "terminal_history_vectors"], cwd=ROOT, stdout=output, check=True)
    rows = json.loads(reference.read_text())["vectors"]
    if args.kinds:
        rows = [row for row in rows if row["kind"] in args.kinds]
    probe = TARGET / "terminal-history-probe"
    if not args.skip_build:
        subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"),
                        "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"),
                        str(ROOT / "mojo/tests/terminal_history_probe.mojo"), "-o", str(probe)],
                       cwd=ROOT, check=True)
    started = time.monotonic()
    compared = 0
    with tempfile.TemporaryDirectory(prefix="zz-history-") as directory:
        directory = Path(directory)
        (directory / "kinds.bin").write_bytes(bytes(KINDS.index(row["kind"]) for row in rows))
        for index, row in enumerate(rows):
            (directory / f"request-{index}.bin").write_bytes(bytes(row["request"]))
        result = subprocess.run([str(probe), str(directory)], cwd=ROOT,
                                capture_output=True, timeout=300)
        if result.returncode:
            raise RuntimeError(result.stdout.decode() + result.stderr.decode())
        for index, row in enumerate(rows):
            actual = (directory / f"actual-{index}.bin").read_bytes()
            expected = bytes(row["expected"])
            if actual != expected:
                mismatch = next((index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
                raise AssertionError((row["name"], row["kind"], index, mismatch,
                                      list(actual[max(0, mismatch - 8):mismatch + 24]),
                                      list(expected[max(0, mismatch - 8):mismatch + 24])))
            compared += len(expected)
    report = {"reference": "actual client retained history/local scroll reducers",
              "cases": len(rows), "kinds": dict(Counter(row["kind"] for row in rows)),
              "bytes_compared": compared, "seconds": round(time.monotonic() - started, 6),
              "passed": True}
    (TARGET / "terminal-history-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
