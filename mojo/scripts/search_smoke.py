import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--ui", action="store_true")
    options = parser.parse_args()
    binary = ROOT / ("target/port/search-workspace-smoke" if options.ui else "target/port/terminal-search-smoke")
    if options.ui:
        sdk = ROOT.parent / "mjui"
        subprocess.run([
            "pixi", "run", "--manifest-path", str(sdk / "pixi.toml"), "bash", "-c",
            'mojo build --fp-mode=contract=off -I "$1/src" -I "$2/mojo" $MJ_LINK "$2/mojo/search_workspace_smoke.mojo" -o "$3"',
            "_", str(sdk), str(ROOT), str(binary),
        ], cwd=sdk, check=True)
        subprocess.run(["python3", str(ROOT / "mojo/scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
    else:
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
            if options.ui:
                source = "i=0; while [ $i -lt 80 ]; do printf 'HISTORY-%04d\\n' $i; i=$((i+1)); done; printf 'Alpha needle\\nBeta NEEDLE\\nGamma needle\\nUI-SEARCH-READY\\n'; IFS= read -r line; printf 'INPUT:%s\\n' \"$line\"; sleep 30"
            subprocess.run([
                str(cli), "-S", socket, "-f", "/dev/null", "new-session", "-d",
                "-s", "search-proof", "-x", "80", "-y", "24", source,
            ], check=True, timeout=20)
            if options.ui:
                subprocess.run([str(cli), "-S", socket, "set-window-option", "-g", "mode-keys", "vi"], check=True, timeout=20)
            environment = os.environ.copy()
            environment["ZZ_BEND"] = str(ROOT / "target/port/zz-bend")
            command = [str(binary), socket]
            if options.ui:
                command.append(str(ROOT / "target/port/search-workspace.png"))
            subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=120 if options.ui else 60)
            result = {"passed": True, "source": "actual private daemon terminal search and copy mode", "checks": ["case-sensitive match count", "next/previous", "invalid regex", "hidden copy position", "accept without close", "return to live", "PTY input isolation", "Bend tree and cells active"]}
            if options.ui:
                result = {"passed": True, "source": "actual native workspace/input timeline and private daemon", "checks": ["ordered Find/text/Alt frames", "exact daemon matches", "query clipboard", "toolbar click", "invalid regex", "post-close paste/raw Enter", "swallowed Escape", "Escape/keypad release before dispatch", "daemon copy-mode prompt", "accept without close", "command-output search", "output replacement retirement", "stale output owner", "sidebar focus"], "image": "target/port/search-workspace.png"}
            report = "search-workspace-smoke.json" if options.ui else "terminal-search-smoke.json"
            (ROOT / "target/port" / report).write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result))
        finally:
            subprocess.run([str(cli), "-S", socket, "kill-server"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


if __name__ == "__main__":
    main()
