from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    binary = TARGET / "window-chooser-admission-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/window_chooser_admission_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary)], cwd=ROOT, check=True, timeout=30)


if __name__ == "__main__":
    main()
