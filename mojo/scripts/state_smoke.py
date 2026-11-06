import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    binary = TARGET / "state-probe"
    rust = ROOT / "target/debug/zz_cli"
    if not rust.exists():
        raise SystemExit("build the Rust reference first: cargo build -p zz-cli")
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "-I", str(PORT), str(PORT / "tests/state_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    with tempfile.TemporaryDirectory(prefix="zzstate-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "state-proof", "sleep 60"], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket], cwd=ROOT, env=environment, check=True, timeout=30)
            report = {"passed": True, "source": "native interactive Hello/Batch/tree and packed terminal lane", "checks": ["default attach", "split without command output modal", "create and switch session", "terminal glyph projection after executed input"]}
            (TARGET / "state-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], cwd=ROOT, env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
