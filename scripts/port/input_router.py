#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(Path(__file__).with_name("guard.py")))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-proof", action="store_true")
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    tools = runpy.run_path(str(ROOT / "bend2/toolchain.py"))["ensure"]()
    environment = os.environ.copy()
    environment["PATH"] = os.pathsep.join([str(Path(tools["bend"]).parent), str(Path(tools["lean"]).parent), environment["PATH"]])
    stages = []

    def run(name, command, capture=False):
        started = time.monotonic()
        result = subprocess.run(command, cwd=ROOT, env=environment, check=True, capture_output=capture, text=True, timeout=1800)
        stages.append({"name": name, "seconds": round(time.monotonic() - started, 3)})
        return result

    if not args.skip_proof:
        run("proof", [tools["bend"], "bend2/PROOF.bend"])
        run("independent-verdict", [tools["bend"], "bend2/PROOF.bend", "--verdict"])
    run("source-oracle", ["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-client", "--example", "input_router_oracle", "--features", "input-router-fixtures", "--", str(TARGET / "input-router-oracle.json")])
    for name in ("input_router", "input_router_data"):
        run(name, [tools["bend"], f"bend2/{name}.bend", "-o", str(TARGET / (name + ".mjs"))])
    result = run("source-differential", ["node", "bend2/input_router_check.mjs"], capture=True)
    report = json.loads(result.stdout)
    report["stages"] = stages
    report["source_bytes"] = (TARGET / "input-router-oracle.json").stat().st_size
    report["runtime_bridge"] = False
    (TARGET / "input-router-verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
