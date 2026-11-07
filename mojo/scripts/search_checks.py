import argparse
import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
MOJO = ROOT / "mojo"
SDK = ROOT.parent / "mjui"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--ui", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    name = "editor" if args.ui else "controller"
    source = MOJO / f"tests/test_terminal_search_{name}.mojo"
    binary = TARGET / f"search-{name}-probe"
    if not args.skip_build:
        if args.ui:
            subprocess.run([
                "pixi", "run", "--manifest-path", str(SDK / "pixi.toml"),
                "bash", "-c",
                'mojo build --fp-mode=contract=off -I "$1/src" -I "$2" $MJ_LINK "$3" -o "$4"',
                "_", str(SDK), str(MOJO), str(source), str(binary),
            ], cwd=SDK, check=True)
            subprocess.run(["python3", str(MOJO / "scripts/resources.py"), str(binary)], cwd=ROOT, check=True)
        else:
            subprocess.run([
                "pixi", "run", "--manifest-path", str(MOJO / "pixi.toml"),
                "mojo", "build", "--fp-mode=contract=off", "-I", str(MOJO),
                "-I", str(SDK / "src"), str(source), "-o", str(binary),
            ], cwd=ROOT, check=True)
    command = [str(binary)]
    if args.ui:
        command.append(str(TARGET / "terminal-search-chrome.png"))
    subprocess.run(command, cwd=ROOT, check=True, timeout=60)
    result = {"passed": True, "component": name}
    (TARGET / f"terminal-search-{name}.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
