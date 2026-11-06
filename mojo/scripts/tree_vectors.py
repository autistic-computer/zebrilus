import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def identifier(value):
    return value["high"] * 2**32 + value["low"]


def error_fields(value):
    if value["$"] == "None":
        return [0, 0, 0, 0]
    value = value["value"]
    if value["$"] == "BaseMismatch":
        return [1, identifier(value["expected"]), identifier(value["received"]), 0]
    return [["", "", "MissingSession", "MissingWindow", "MissingPane"].index(value["$"]),
            0, 0, identifier(value["id"])]


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-reference", action="store_true")
    parser.add_argument("--bend", type=Path, default=TARGET / "zz-bend")
    args = parser.parse_args()
    reference = TARGET / "tree-reducer-oracle.json"
    if not args.skip_reference:
        with reference.open("w") as output:
            subprocess.run(["cargo", "run", "--locked", "-p", "zz-protocol", "--example",
                            "tree_reducer_oracle"], cwd=ROOT, stdout=output, check=True)
    vectors = json.loads(reference.read_text())["vectors"]
    probe = TARGET / "tree-gate-probe"
    if not args.skip_build:
        subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"),
                        "mojo", "build", "-I", str(ROOT / "mojo"),
                        str(ROOT / "mojo/tests/tree_gate_probe.mojo"), "-o", str(probe)],
                       cwd=ROOT, check=True)
    reports = []
    with tempfile.TemporaryDirectory(prefix="zz-mojo-tree-") as directory:
        directory = Path(directory)
        (directory / "modes.bin").write_bytes(bytes(["delta", "overlay", "view"].index(row["kind"])
                                                   for row in vectors))
        for index, row in enumerate(vectors):
            (directory / f"before-{index}.bin").write_bytes(bytes(row["before_wire"]))
            (directory / f"update-{index}.bin").write_bytes(bytes(row["update_wire"]))
        for phase in ("codec", "native"):
            started = time.monotonic()
            result = subprocess.run([str(probe), phase, str(directory), str(args.bend)],
                                    cwd=ROOT, capture_output=True, check=True, timeout=300)
            statuses = [list(map(int, line.split())) for line in result.stdout.decode().splitlines()]
            assert len(statuses) == len(vectors), result.stdout.decode()[-2000:]
            compared = 0
            for index, row in enumerate(vectors):
                expected = row["before_wire"] if phase == "codec" else row["after_wire"]
                assert (directory / f"after-{index}.bin").read_bytes() == bytes(expected), (phase, row["name"])
                assert statuses[index] == ([0, 0, 0, 0] if phase == "codec" else error_fields(row["outcome"]["error"])), (phase, row["name"])
                compared += len(expected)
            reports.append({"phase": phase, "cases": len(vectors), "snapshot_bytes_compared": compared,
                            "seconds": round(time.monotonic() - started, 6)})
            print(json.dumps(reports[-1]), flush=True)
    report = {"reference": "actual Rust full snapshot and error IDs", "phases": reports,
              "native_combined_input_budget_bytes": 2 * 1024 * 1024,
              "maximum_io_chunk_bytes": 65536, "passed": True}
    (TARGET / "tree-mojo-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
