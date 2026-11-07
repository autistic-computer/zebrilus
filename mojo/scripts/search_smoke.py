import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    binary = ROOT / "target/port/terminal-search-smoke"
    subprocess.run([
        "pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"),
        "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"),
        str(ROOT / "mojo/tests/terminal_search_smoke.mojo"), "-o", str(binary),
    ], cwd=ROOT, check=True)
    cli = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-search-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            source = "printf 'Alpha needle\\nBeta NEEDLE\\nGamma needle\\nSEARCH-READY\\n'; IFS= read -r line; printf 'INPUT:%s\\n' \"$line\"; sleep 30"
            subprocess.run([
                str(cli), "-S", socket, "-f", "/dev/null", "new-session", "-d",
                "-s", "search-proof", "-x", "80", "-y", "24", source,
            ], check=True, timeout=20)
            environment = os.environ.copy()
            environment["ZZ_BEND"] = str(ROOT / "target/port/zz-bend")
            subprocess.run([str(binary), socket], cwd=ROOT, env=environment, check=True, timeout=60)
            result = {"passed": True, "source": "actual private daemon terminal search and copy mode", "checks": ["case-sensitive match count", "next/previous", "invalid regex", "hidden copy position", "accept without close", "return to live", "PTY input isolation", "Bend tree and cells active"]}
            (ROOT / "target/port/terminal-search-smoke.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result))
        finally:
            subprocess.run([str(cli), "-S", socket, "kill-server"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


if __name__ == "__main__":
    main()
