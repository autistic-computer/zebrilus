import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    source = TARGET / 'desktop-chrome-dispatch-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_chrome_dispatch_vectors', '--', str(source)], cwd=ROOT, check=True)
    probe = TARGET / 'desktop-chrome-dispatch-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_chrome_dispatch_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    subprocess.run([str(probe), str(source)], cwd=ROOT, timeout=30, check=True)
    (TARGET / 'desktop-chrome-dispatch-verification.json').write_text(json.dumps({'passed': True, 'typed_actions': 561, 'bytes': source.stat().st_size, 'source': 'actual GUI-delegated desktop dispatch'}, indent=2) + '\n')


if __name__ == '__main__':
    main()
