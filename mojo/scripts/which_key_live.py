from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port/which-key-live'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    binary = TARGET / 'test'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), '-I', str(ROOT.parent / 'mjui/src'), str(ROOT / 'mojo/tests/which_key_live_test.mojo'), '-o', str(binary)], cwd=ROOT, check=True)
    subprocess.run([str(binary)], cwd=ROOT, check=True, timeout=120)


if __name__ == '__main__':
    main()
